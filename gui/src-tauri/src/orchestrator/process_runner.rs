use std::path::Path;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use super::secrets::SecretRedactor;
use super::types::{CapturedOutput, ProcessResult};

pub const VALIDATION_SAFETY_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const CLI_AGENT_SAFETY_TIMEOUT: Duration = Duration::from_secs(60 * 60);
pub const HTTP_INFERENCE_SAFETY_TIMEOUT: Duration = Duration::from_secs(180);
pub const GIT_COMMAND_SAFETY_TIMEOUT: Duration = Duration::from_secs(60);
pub const DRAIN_GRACE_PERIOD: Duration = Duration::from_secs(5);
pub const MAX_CAPTURE_BYTES: usize = 1024 * 1024; // 1 MiB

#[derive(Debug, Clone)]
pub struct ProcessRunner;

impl Default for ProcessRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessRunner {
    pub fn new() -> Self {
        Self
    }

    /// Runs a process with cancellation, timeout, output cap, and Windows Job Object process-tree management.
    pub async fn run(
        &self,
        executable: &str,
        args: &[String],
        working_dir: Option<&Path>,
        timeout: Duration,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ProcessResult, String> {
        self.run_with_stdin(executable, args, working_dir, None, timeout, cancel_token)
            .await
    }

    /// Runs a process and optionally streams a fixed input payload to its stdin.
    pub async fn run_with_stdin(
        &self,
        executable: &str,
        args: &[String],
        working_dir: Option<&Path>,
        stdin_input: Option<&[u8]>,
        timeout: Duration,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ProcessResult, String> {
        let start = Instant::now();
        let redactor = SecretRedactor::new();

        #[cfg(windows)]
        {
            self.run_windows(
                executable,
                args,
                working_dir,
                stdin_input,
                timeout,
                cancel_token,
                &redactor,
                start,
            )
            .await
        }

        #[cfg(not(windows))]
        {
            self.run_unix(
                executable,
                args,
                working_dir,
                stdin_input,
                timeout,
                cancel_token,
                &redactor,
                start,
            )
            .await
        }
    }

    #[cfg(windows)]
    async fn run_windows(
        &self,
        executable: &str,
        args: &[String],
        working_dir: Option<&Path>,
        stdin_input: Option<&[u8]>,
        timeout: Duration,
        cancel_token: Option<&CancellationToken>,
        redactor: &SecretRedactor,
        start: Instant,
    ) -> Result<ProcessResult, String> {
        use windows_sys::Win32::Foundation::HANDLE;
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, WaitForSingleObject, INFINITE,
        };

        // All Win32 handle operations that involve raw pointers are isolated in spawn_win32_child
        let handles = spawn_win32_child(executable, args, working_dir, stdin_input.is_some())?;

        let _job_guard = SendHandleGuard(handles.job);
        let _proc_guard = SendHandleGuard(handles.proc);
        let stdout_val = handles.stdout_rd.0;
        let stderr_val = handles.stderr_rd.0;
        let proc_val = handles.proc.0;
        let job_val = handles.job.0;

        let mut stdout_task = tokio::task::spawn_blocking(move || {
            drain_pipe_handle(stdout_val as HANDLE, MAX_CAPTURE_BYTES)
        });

        let mut stderr_task = tokio::task::spawn_blocking(move || {
            drain_pipe_handle(stderr_val as HANDLE, MAX_CAPTURE_BYTES)
        });

        let mut wait_proc_task = tokio::task::spawn_blocking(move || unsafe {
            WaitForSingleObject(proc_val as HANDLE, INFINITE)
        });

        let mut stdin_task = match (stdin_input.map(|bytes| bytes.to_vec()), handles.stdin_wr) {
            (Some(payload), Some(handle)) => Some(tokio::task::spawn_blocking(move || {
                use std::io::Write;
                use std::os::windows::io::FromRawHandle;
                let mut file = unsafe { std::fs::File::from_raw_handle(handle.0 as *mut _) };
                file.write_all(&payload)
            })),
            _ => None,
        };

        let mut timed_out = false;
        let mut cancelled = false;

        let cancel_fut = async {
            if let Some(token) = cancel_token {
                token.cancelled().await;
            } else {
                std::future::pending::<()>().await;
            }
        };

        tokio::select! {
            _ = tokio::time::sleep(timeout) => {
                timed_out = true;
                unsafe {
                    let _ = TerminateJobObject(job_val as HANDLE, 1);
                }
            }
            _ = cancel_fut => {
                cancelled = true;
                unsafe {
                    let _ = TerminateJobObject(job_val as HANDLE, 1);
                }
            }
            res = &mut wait_proc_task => {
                let _ = res;
            }
        }

        if timed_out || cancelled {
            // Job termination is synchronous at the kernel boundary. Reap the
            // owned root process before proceeding to pipe drains/handle close.
            let _ = wait_proc_task.await;
        } else {
            // The root process has exited. Do not leave background descendants
            // attached to the run merely because they closed their stdio handles.
            unsafe {
                let _ = TerminateJobObject(job_val as HANDLE, 0);
            }
        }

        if let Some(ref mut task) = stdin_task {
            if tokio::time::timeout(DRAIN_GRACE_PERIOD, &mut *task)
                .await
                .is_err()
            {
                task.abort();
            }
        }

        // Collect exit code
        let mut exit_code_val: u32 = 0;
        let exit_code = unsafe {
            if GetExitCodeProcess(proc_val as HANDLE, &mut exit_code_val) != 0 {
                if exit_code_val == 259 {
                    // STILL_ACTIVE
                    None
                } else {
                    Some(exit_code_val as i32)
                }
            } else {
                None
            }
        };

        let drain = async {
            let (out, err) = tokio::join!(&mut stdout_task, &mut stderr_task);
            (out, err)
        };
        let drained = match tokio::time::timeout(DRAIN_GRACE_PERIOD, drain).await {
            Ok(results) => Some(results),
            Err(_) => {
                // A descendant may have inherited a pipe. Terminate the full
                // Job on normal exit as well as after cancel/timeout.
                unsafe {
                    let _ = TerminateJobObject(job_val as HANDLE, 1);
                }
                tokio::time::timeout(Duration::from_secs(1), async {
                    let (out, err) = tokio::join!(&mut stdout_task, &mut stderr_task);
                    (out, err)
                })
                .await
                .ok()
            }
        };
        let (raw_stdout, raw_stderr) = if let Some((out, err)) = drained {
            (out.ok(), err.ok())
        } else {
            stdout_task.abort();
            stderr_task.abort();
            (None, None)
        };
        let raw_stdout = raw_stdout.unwrap_or(CapturedOutput {
            text: String::new(),
            is_truncated: false,
            total_bytes: 0,
        });
        let raw_stderr = raw_stderr.unwrap_or(CapturedOutput {
            text: String::new(),
            is_truncated: false,
            total_bytes: 0,
        });

        let stdout = CapturedOutput {
            text: redactor.redact_secrets(&raw_stdout.text),
            is_truncated: raw_stdout.is_truncated,
            total_bytes: raw_stdout.total_bytes,
        };

        let stderr = CapturedOutput {
            text: redactor.redact_secrets(&raw_stderr.text),
            is_truncated: raw_stderr.is_truncated,
            total_bytes: raw_stderr.total_bytes,
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ProcessResult {
            exit_code,
            stdout,
            stderr,
            timed_out,
            cancelled,
            duration_ms,
        })
    }

    #[cfg(not(windows))]
    async fn run_unix(
        &self,
        executable: &str,
        args: &[String],
        working_dir: Option<&Path>,
        stdin_input: Option<&[u8]>,
        timeout: Duration,
        cancel_token: Option<&CancellationToken>,
        redactor: &SecretRedactor,
        start: Instant,
    ) -> Result<ProcessResult, String> {
        use std::process::Stdio;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::process::Command;

        let mut cmd = Command::new(executable);
        cmd.args(args);
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(if stdin_input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        cmd.process_group(0);
        cmd.kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn process '{}': {}", executable, e))?;
        let child_pid = child
            .id()
            .ok_or_else(|| "Spawned process has no PID".to_string())?
            as i32;
        let mut process_group_guard = UnixProcessGroupGuard::new(child_pid);

        let mut stdout_pipe = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout".to_string())?;
        let mut stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| "Failed to capture stderr".to_string())?;

        let mut stdout_task =
            tokio::spawn(
                async move { drain_async_pipe(&mut stdout_pipe, MAX_CAPTURE_BYTES).await },
            );

        let stderr_task =
            tokio::spawn(
                async move { drain_async_pipe(&mut stderr_pipe, MAX_CAPTURE_BYTES).await },
            );

        let mut stdin_task =
            if let (Some(payload), Some(mut stdin)) = (stdin_input, child.stdin.take()) {
                let payload = payload.to_vec();
                Some(tokio::spawn(async move {
                    stdin.write_all(&payload).await?;
                    stdin.shutdown().await
                }))
            } else {
                None
            };

        let mut timed_out = false;
        let mut cancelled = false;

        let cancel_fut = async {
            if let Some(token) = cancel_token {
                token.cancelled().await;
            } else {
                std::future::pending::<()>().await;
            }
        };

        let status = tokio::select! {
            _ = tokio::time::sleep(timeout) => {
                timed_out = true;
                terminate_unix_process_group(child_pid);
                let _ = child.kill().await;
                child.wait().await.ok()
            }
            _ = cancel_fut => {
                cancelled = true;
                terminate_unix_process_group(child_pid);
                let _ = child.kill().await;
                child.wait().await.ok()
            }
            res = child.wait() => {
                res.ok()
            }
        };

        let exit_code = status.and_then(|s| s.code());

        if let Some(ref mut input_task) = stdin_task {
            if tokio::time::timeout(DRAIN_GRACE_PERIOD, input_task)
                .await
                .is_err()
            {
                input_task.abort();
            }
        }

        let drain = async {
            let (out, err) = tokio::join!(&mut stdout_task, &mut stderr_task);
            (out, err)
        };
        let drained = match tokio::time::timeout(DRAIN_GRACE_PERIOD, drain).await {
            Ok(results) => Some(results),
            Err(_) => {
                // The owned process exited but a descendant may still hold a
                // pipe open. Terminate its process group before final drain.
                terminate_unix_process_group(child_pid);
                tokio::time::timeout(Duration::from_secs(1), async {
                    let (out, err) = tokio::join!(&mut stdout_task, &mut stderr_task);
                    (out, err)
                })
                .await
                .ok()
            }
        };
        let (raw_stdout, raw_stderr) = if let Some((out, err)) = drained {
            (out.ok().and_then(Result::ok), err.ok().and_then(Result::ok))
        } else {
            stdout_task.abort();
            stderr_task.abort();
            (None, None)
        };
        let raw_stdout = raw_stdout.unwrap_or(CapturedOutput {
            text: String::new(),
            is_truncated: false,
            total_bytes: 0,
        });
        let raw_stderr = raw_stderr.unwrap_or(CapturedOutput {
            text: String::new(),
            is_truncated: false,
            total_bytes: 0,
        });

        // A child may detach its stdio handles and outlive the root process.
        // Terminate any remaining group members on normal completion too, as
        // the Windows Job Object path does, then disarm the unwind guard.
        terminate_unix_process_group(child_pid);
        process_group_guard.disarm();

        let stdout = CapturedOutput {
            text: redactor.redact_secrets(&raw_stdout.text),
            is_truncated: raw_stdout.is_truncated,
            total_bytes: raw_stdout.total_bytes,
        };

        let stderr = CapturedOutput {
            text: redactor.redact_secrets(&raw_stderr.text),
            is_truncated: raw_stderr.is_truncated,
            total_bytes: raw_stderr.total_bytes,
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ProcessResult {
            exit_code,
            stdout,
            stderr,
            timed_out,
            cancelled,
            duration_ms,
        })
    }
}

#[cfg(unix)]
fn terminate_unix_process_group(process_id: i32) {
    const SIGKILL: i32 = 9;
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    // A negative PID addresses the dedicated process group created above.
    unsafe {
        let _ = kill(-process_id, SIGKILL);
    }
}

#[cfg(unix)]
struct UnixProcessGroupGuard {
    process_id: i32,
    armed: bool,
}

#[cfg(unix)]
impl UnixProcessGroupGuard {
    fn new(process_id: i32) -> Self {
        Self {
            process_id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(unix)]
impl Drop for UnixProcessGroupGuard {
    fn drop(&mut self) {
        if self.armed {
            terminate_unix_process_group(self.process_id);
        }
    }
}

#[derive(Clone, Copy)]
struct SendHandle(usize);
unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

#[cfg(windows)]
impl SendHandle {
    fn from_raw(h: windows_sys::Win32::Foundation::HANDLE) -> Self {
        Self(h as usize)
    }
    fn as_raw(self) -> windows_sys::Win32::Foundation::HANDLE {
        self.0 as windows_sys::Win32::Foundation::HANDLE
    }
}

struct SendHandleGuard(SendHandle);
unsafe impl Send for SendHandleGuard {}
unsafe impl Sync for SendHandleGuard {}

#[cfg(windows)]
impl Drop for SendHandleGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        let h = self.0.as_raw();
        if h != std::ptr::null_mut() && h != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(h) };
        }
    }
}

#[cfg(windows)]
struct Win32ChildHandles {
    job: SendHandle,
    proc: SendHandle,
    stdout_rd: SendHandle,
    stderr_rd: SendHandle,
    stdin_wr: Option<SendHandle>,
}

#[cfg(windows)]
fn spawn_win32_child(
    executable: &str,
    args: &[String],
    working_dir: Option<&Path>,
    with_stdin: bool,
) -> Result<Win32ChildHandles, String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{
        CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, TRUE,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
        ResumeThread, UpdateProcThreadAttribute, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
        EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
        STARTF_USESTDHANDLES, STARTUPINFOEXW,
    };

    // 1. Create Job Object
    let job = unsafe { CreateJobObjectW(null_mut(), null()) };
    if job == null_mut() || job == INVALID_HANDLE_VALUE {
        return Err("Failed to create Windows Job Object".to_string());
    }

    // Configure Job Object to kill all child processes when the handle is closed
    unsafe {
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let res = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if res == 0 {
            CloseHandle(job);
            return Err("Failed to set Job Object information".to_string());
        }
    }

    // 2. Create pipes for stdout and stderr
    let mut sa: SECURITY_ATTRIBUTES = unsafe { std::mem::zeroed() };
    sa.nLength = std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
    sa.bInheritHandle = TRUE;

    let mut stdout_rd: HANDLE = null_mut();
    let mut stdout_wr: HANDLE = null_mut();
    let mut stderr_rd: HANDLE = null_mut();
    let mut stderr_wr: HANDLE = null_mut();
    let mut stdin_rd: HANDLE = null_mut();
    let mut stdin_wr: HANDLE = null_mut();

    unsafe {
        if CreatePipe(&mut stdout_rd, &mut stdout_wr, &sa, 0) == 0 {
            CloseHandle(job);
            return Err("Failed to create stdout pipe".to_string());
        }
        if CreatePipe(&mut stderr_rd, &mut stderr_wr, &sa, 0) == 0 {
            CloseHandle(stdout_rd);
            CloseHandle(stdout_wr);
            CloseHandle(job);
            return Err("Failed to create stderr pipe".to_string());
        }
        if with_stdin && CreatePipe(&mut stdin_rd, &mut stdin_wr, &sa, 0) == 0 {
            CloseHandle(stdout_rd);
            CloseHandle(stdout_wr);
            CloseHandle(stderr_rd);
            CloseHandle(stderr_wr);
            CloseHandle(job);
            return Err("Failed to create stdin pipe".to_string());
        }
        if with_stdin && SetHandleInformation(stdin_wr, HANDLE_FLAG_INHERIT, 0) == 0 {
            CloseHandle(stdin_rd);
            CloseHandle(stdin_wr);
            CloseHandle(stdout_rd);
            CloseHandle(stdout_wr);
            CloseHandle(stderr_rd);
            CloseHandle(stderr_wr);
            CloseHandle(job);
            return Err("Failed to protect parent stdin pipe handle".to_string());
        }
    }

    // 3. Build a Windows CRT-compatible argv command line.
    let mut cmd_line_str = quote_windows_arg(executable);
    for arg in args {
        cmd_line_str.push(' ');
        cmd_line_str.push_str(&quote_windows_arg(arg));
    }

    let mut cmd_line_w: Vec<u16> = OsStr::new(&cmd_line_str)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let workdir_w: Option<Vec<u16>> = working_dir.map(|d| {
        d.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    });

    // 4. Create suspended child process
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdOutput = stdout_wr;
    startup.StartupInfo.hStdError = stderr_wr;
    startup.StartupInfo.hStdInput = if with_stdin { stdin_rd } else { null_mut() };

    // Restrict inheritance to the exact stdio handles assigned above.
    let mut allowed_handles = Vec::<HANDLE>::new();
    if with_stdin {
        allowed_handles.push(stdin_rd);
    }
    allowed_handles.push(stdout_wr);
    allowed_handles.push(stderr_wr);
    let mut attribute_bytes = 0usize;
    unsafe {
        InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut attribute_bytes);
    }
    let word_len =
        (attribute_bytes + std::mem::size_of::<usize>() - 1) / std::mem::size_of::<usize>();
    let mut attribute_storage = vec![0usize; word_len];
    startup.lpAttributeList = attribute_storage.as_mut_ptr() as _;
    if unsafe {
        InitializeProcThreadAttributeList(startup.lpAttributeList, 1, 0, &mut attribute_bytes)
    } == 0
    {
        unsafe {
            CloseHandle(stdout_rd);
            CloseHandle(stdout_wr);
            CloseHandle(stderr_rd);
            CloseHandle(stderr_wr);
            if with_stdin {
                CloseHandle(stdin_rd);
                CloseHandle(stdin_wr);
            }
            CloseHandle(job);
        }
        return Err("Failed to initialize process handle allow-list".to_string());
    }
    let attr_ok = unsafe {
        UpdateProcThreadAttribute(
            startup.lpAttributeList,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            allowed_handles.as_ptr() as _,
            allowed_handles.len() * std::mem::size_of::<HANDLE>(),
            null_mut(),
            null_mut(),
        )
    } != 0;
    if !attr_ok {
        unsafe {
            DeleteProcThreadAttributeList(startup.lpAttributeList);
            CloseHandle(stdout_rd);
            CloseHandle(stdout_wr);
            CloseHandle(stderr_rd);
            CloseHandle(stderr_wr);
            if with_stdin {
                CloseHandle(stdin_rd);
                CloseHandle(stdin_wr);
            }
            CloseHandle(job);
        }
        return Err("Failed to set process handle allow-list".to_string());
    }

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    let create_res = unsafe {
        CreateProcessW(
            null(),
            cmd_line_w.as_mut_ptr(),
            null_mut(),
            null_mut(),
            TRUE,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            null_mut(),
            workdir_w.as_ref().map_or(null(), |w| w.as_ptr()),
            &startup as *const STARTUPINFOEXW as _,
            &mut pi,
        )
    };

    // Close write pipe handles in parent regardless of create success
    unsafe {
        DeleteProcThreadAttributeList(startup.lpAttributeList);
        CloseHandle(stdout_wr);
        CloseHandle(stderr_wr);
        if with_stdin {
            CloseHandle(stdin_rd);
        }
    }

    if create_res == 0 {
        unsafe {
            CloseHandle(stdout_rd);
            CloseHandle(stderr_rd);
            if with_stdin {
                CloseHandle(stdin_wr);
            }
            CloseHandle(job);
        }
        return Err(format!(
            "Failed to spawn process '{}': Windows error code {}",
            executable,
            std::io::Error::last_os_error()
        ));
    }

    // Assign to Job Object
    let assign_res = unsafe { AssignProcessToJobObject(job, pi.hProcess) };
    if assign_res == 0 {
        unsafe {
            TerminateJobObject(job, 1);
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
            CloseHandle(stdout_rd);
            CloseHandle(stderr_rd);
            if with_stdin {
                CloseHandle(stdin_wr);
            }
            CloseHandle(job);
        }
        return Err("Failed to assign child process to Job Object".to_string());
    }

    // Resume main thread
    let resume_res = unsafe { ResumeThread(pi.hThread) };
    unsafe {
        CloseHandle(pi.hThread);
    }

    if resume_res == u32::MAX {
        unsafe {
            TerminateJobObject(job, 1);
            CloseHandle(pi.hProcess);
            CloseHandle(stdout_rd);
            CloseHandle(stderr_rd);
            if with_stdin {
                CloseHandle(stdin_wr);
            }
            CloseHandle(job);
        }
        return Err("Failed to resume child thread".to_string());
    }

    Ok(Win32ChildHandles {
        job: SendHandle::from_raw(job),
        proc: SendHandle::from_raw(pi.hProcess),
        stdout_rd: SendHandle::from_raw(stdout_rd),
        stderr_rd: SendHandle::from_raw(stderr_rd),
        stdin_wr: if with_stdin {
            Some(SendHandle::from_raw(stdin_wr))
        } else {
            None
        },
    })
}

/// Encodes one argument using the quoting rules used by the Windows C runtime.
/// Quoting every argument also makes empty strings and trailing backslashes safe.
fn quote_windows_arg(arg: &str) -> String {
    let mut encoded = String::with_capacity(arg.len() + 2);
    encoded.push('"');
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        if ch == '\\' {
            backslashes += 1;
        } else if ch == '"' {
            encoded.extend(
                std::iter::repeat('\\').take(backslashes.saturating_mul(2).saturating_add(1)),
            );
            encoded.push('"');
            backslashes = 0;
        } else {
            encoded.extend(std::iter::repeat('\\').take(backslashes));
            encoded.push(ch);
            backslashes = 0;
        }
    }
    encoded.extend(std::iter::repeat('\\').take(backslashes.saturating_mul(2)));
    encoded.push('"');
    encoded
}

#[cfg(windows)]
fn drain_pipe_handle(
    handle: windows_sys::Win32::Foundation::HANDLE,
    max_bytes: usize,
) -> CapturedOutput {
    use std::io::Read;
    use std::os::windows::io::FromRawHandle;

    let mut file = unsafe { std::fs::File::from_raw_handle(handle as _) };
    let mut captured = Vec::new();
    let mut total_bytes = 0;
    let mut is_truncated = false;
    let mut buf = [0u8; 8192];

    while let Ok(n) = file.read(&mut buf) {
        if n == 0 {
            break;
        }
        total_bytes += n;
        if captured.len() < max_bytes {
            let to_take = n.min(max_bytes - captured.len());
            captured.extend_from_slice(&buf[..to_take]);
            if captured.len() >= max_bytes && total_bytes > max_bytes {
                is_truncated = true;
            }
        } else {
            is_truncated = true;
        }
    }

    CapturedOutput {
        text: String::from_utf8_lossy(&captured).to_string(),
        is_truncated,
        total_bytes,
    }
}

#[cfg(not(windows))]
async fn drain_async_pipe<R: tokio::io::AsyncRead + Unpin>(
    pipe: &mut R,
    max_bytes: usize,
) -> CapturedOutput {
    use tokio::io::AsyncReadExt;
    let mut captured = Vec::new();
    let mut total_bytes = 0;
    let mut is_truncated = false;
    let mut buf = [0u8; 8192];

    while let Ok(n) = pipe.read(&mut buf).await {
        if n == 0 {
            break;
        }
        total_bytes += n;
        if captured.len() < max_bytes {
            let to_take = n.min(max_bytes - captured.len());
            captured.extend_from_slice(&buf[..to_take]);
            if captured.len() >= max_bytes && total_bytes > max_bytes {
                is_truncated = true;
            }
        } else {
            is_truncated = true;
        }
    }

    CapturedOutput {
        text: String::from_utf8_lossy(&captured).to_string(),
        is_truncated,
        total_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_process_runner_echo() {
        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();

        #[cfg(windows)]
        let (exe, args) = (
            "powershell",
            vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Write-Output 'hello world'".to_string(),
            ],
        );
        #[cfg(not(windows))]
        let (exe, args) = ("echo", vec!["hello world".to_string()]);

        let res = runner
            .run(exe, &args, Some(dir.path()), Duration::from_secs(10), None)
            .await
            .unwrap();

        assert_eq!(res.exit_code, Some(0));
        assert!(res.stdout.text.contains("hello world"));
        assert!(!res.timed_out);
        assert!(!res.cancelled);
        assert!(!res.stdout.is_truncated);
    }

    #[tokio::test]
    async fn output_capture_is_bounded_and_drains_stdout_and_stderr_independently() {
        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let script =
            "process.stdout.write('x'.repeat(1100000)); process.stderr.write('y'.repeat(1100000));";
        let args = vec!["-e".to_string(), script.to_string()];
        let result = runner
            .run(
                "node",
                &args,
                Some(dir.path()),
                Duration::from_secs(15),
                None,
            )
            .await
            .unwrap();

        assert_eq!(result.exit_code, Some(0), "stderr: {}", result.stderr.text);
        assert_eq!(result.stdout.text.len(), MAX_CAPTURE_BYTES);
        assert_eq!(result.stderr.text.len(), MAX_CAPTURE_BYTES);
        assert!(result.stdout.is_truncated);
        assert!(result.stderr.is_truncated);
        assert_eq!(result.stdout.total_bytes, 1_100_000);
        assert_eq!(result.stderr.total_bytes, 1_100_000);
    }

    #[tokio::test]
    async fn test_process_runner_cancellation() {
        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let cancel_token = CancellationToken::new();

        let cancel_token_clone = cancel_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel_token_clone.cancel();
        });

        #[cfg(windows)]
        let (exe, args) = (
            "powershell",
            vec![
                "-Command".to_string(),
                "Start-Sleep -Seconds 10".to_string(),
            ],
        );
        #[cfg(not(windows))]
        let (exe, args) = ("sleep", vec!["10".to_string()]);

        let res = runner
            .run(
                exe,
                &args,
                Some(dir.path()),
                Duration::from_secs(10),
                Some(&cancel_token),
            )
            .await
            .unwrap();

        assert!(res.cancelled);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_stdin_prompt_is_delivered_and_closed() {
        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let input = b"system prompt\nuser prompt\n";
        let args = vec![
            "-NoProfile".to_string(),
            "-Command".to_string(),
            "[Console]::In.ReadToEnd()".to_string(),
        ];
        let result = runner
            .run_with_stdin(
                "powershell",
                &args,
                Some(dir.path()),
                Some(input),
                Duration::from_secs(10),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.text.contains("system prompt"));
        assert!(result.stdout.text.contains("user prompt"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_job_cancellation_terminates_descendant_tree() {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        };

        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let pid_file = dir.path().join("child.pid");
        let script = format!(
            "$c=Start-Process -FilePath $env:COMSPEC -ArgumentList '/c','ping 127.0.0.1 -n 30 > nul' -PassThru; [IO.File]::WriteAllText('{}',[string]$c.Id); Start-Sleep -Seconds 30",
            pid_file.display()
        );
        let args = vec!["-NoProfile".into(), "-Command".into(), script];
        let cancel = CancellationToken::new();
        let cancel_child = cancel.clone();
        let dir_path = dir.path().to_path_buf();
        let runner_task = tokio::spawn(async move {
            runner
                .run(
                    "powershell",
                    &args,
                    Some(&dir_path),
                    Duration::from_secs(20),
                    Some(&cancel_child),
                )
                .await
        });

        tokio::time::timeout(Duration::from_secs(5), async {
            while !pid_file.exists() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("child PID should be written before cancellation");
        let child_pid: u32 = std::fs::read_to_string(&pid_file).unwrap().parse().unwrap();
        cancel.cancel();
        assert!(runner_task.await.unwrap().unwrap().cancelled);

        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, child_pid) };
        if !process.is_null() {
            let wait = unsafe { WaitForSingleObject(process, 1_000) };
            unsafe {
                CloseHandle(process);
            }
            assert_eq!(wait, 0, "descendant must be terminated by the Job Object");
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_normal_exit_terminates_descendant_holding_inherited_pipes() {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        };

        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let pid_file = dir.path().join("normal-child.pid");
        let pid_file_js = serde_json::to_string(&pid_file.to_string_lossy().to_string()).unwrap();
        let script = format!(
            "const {{spawn}}=require('node:child_process');const fs=require('node:fs');const c=spawn(process.execPath,['-e','setTimeout(()=>{{}},30000)'],{{stdio:'inherit'}});c.unref();fs.writeFileSync({},String(c.pid));",
            pid_file_js
        );
        let args = vec!["-e".to_string(), script];
        let start = Instant::now();
        let result = runner
            .run(
                "node",
                &args,
                Some(dir.path()),
                Duration::from_secs(15),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.exit_code, Some(0), "stderr: {}", result.stderr.text);
        assert!(!result.timed_out);
        assert!(start.elapsed() < DRAIN_GRACE_PERIOD + Duration::from_secs(2));

        let child_pid: u32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, child_pid) };
        if !process.is_null() {
            let wait = unsafe { WaitForSingleObject(process, 1_000) };
            unsafe { CloseHandle(process) };
            assert_eq!(wait, 0, "normal completion must terminate job descendants");
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_timeout_terminates_descendant_tree() {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        };

        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let pid_file = dir.path().join("timeout-child.pid");
        let pid_file_js = serde_json::to_string(&pid_file.to_string_lossy().to_string()).unwrap();
        let script = format!(
            "const {{spawn}}=require('node:child_process');const fs=require('node:fs');const c=spawn(process.execPath,['-e','setTimeout(()=>{{}},30000)'],{{stdio:'inherit'}});fs.writeFileSync({},String(c.pid));setTimeout(()=>{{}},30000);",
            pid_file_js
        );
        let args = vec!["-e".to_string(), script];
        let result = runner
            .run(
                "node",
                &args,
                Some(dir.path()),
                Duration::from_secs(2),
                None,
            )
            .await
            .unwrap();
        assert!(result.timed_out, "process result: {result:?}");

        let child_pid: u32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, child_pid) };
        if !process.is_null() {
            let wait = unsafe { WaitForSingleObject(process, 1_000) };
            unsafe { CloseHandle(process) };
            assert_eq!(wait, 0, "timeout must terminate job descendants");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdin_prompt_reaches_child_without_loss() {
        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let input = "## System\nKeep this exact.\n\n## User\n日本語 🚀\n";
        let res = runner
            .run_with_stdin(
                "cat",
                &[],
                Some(dir.path()),
                Some(input.as_bytes()),
                Duration::from_secs(5),
                None,
            )
            .await
            .unwrap();
        assert_eq!(res.exit_code, Some(0));
        assert_eq!(res.stdout.text, input);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn parent_exit_with_descendant_pipe_is_bounded_and_tree_is_terminated() {
        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let start = Instant::now();
        let res = tokio::time::timeout(
            DRAIN_GRACE_PERIOD + Duration::from_secs(3),
            runner.run(
                "sh",
                &["-c".into(), "sleep 30 & exit 0".into()],
                Some(dir.path()),
                Duration::from_secs(10),
                None,
            ),
        )
        .await
        .expect("runner must not wait indefinitely for inherited pipe handles")
        .unwrap();
        assert_eq!(res.exit_code, Some(0));
        assert!(start.elapsed() < DRAIN_GRACE_PERIOD + Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_terminates_process_group_while_stdin_is_being_written() {
        let runner = ProcessRunner::new();
        let dir = tempdir().unwrap();
        let token = CancellationToken::new();
        let cancel = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel.cancel();
        });
        let payload = vec![b'x'; 8 * 1024 * 1024];
        let res = tokio::time::timeout(
            Duration::from_secs(4),
            runner.run_with_stdin(
                "sh",
                &["-c".into(), "sleep 30 & wait".into()],
                Some(dir.path()),
                Some(&payload),
                Duration::from_secs(20),
                Some(&token),
            ),
        )
        .await
        .expect("cancelled process group and blocked stdin writer must stop")
        .unwrap();
        assert!(res.cancelled);
    }

    #[test]
    fn windows_argument_encoding_handles_crt_edge_cases() {
        let cases = [
            ("", "\"\""),
            ("simple", "\"simple\""),
            ("with spaces", "\"with spaces\""),
            ("path\\ending\\", "\"path\\ending\\\\\""),
            ("quote\"inside", "\"quote\\\"inside\""),
            (
                "C:\\Program Files\\Test\\",
                "\"C:\\Program Files\\Test\\\\\"",
            ),
            ("日本語 🚀", "\"日本語 🚀\""),
        ];
        for (input, expected) in cases {
            assert_eq!(quote_windows_arg(input), expected, "input: {input:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_argument_encoding_round_trips_with_command_line_parser() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

        let args = [
            "".to_string(),
            "simple".to_string(),
            "with spaces".to_string(),
            "backslash\\".to_string(),
            "quote\"inside".to_string(),
            "path\\ending\\".to_string(),
            "C:\\Program Files\\Test\\".to_string(),
            "mixed \\\" sequence".to_string(),
            "日本語 🚀".to_string(),
        ];
        let cmd = format!(
            "{}",
            std::iter::once(quote_windows_arg("runner.exe"))
                .chain(args.iter().map(|arg| quote_windows_arg(arg)))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let wide: Vec<u16> = std::ffi::OsStr::new(&cmd)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut argc = 0i32;
        let argv = unsafe { CommandLineToArgvW(wide.as_ptr(), &mut argc) };
        assert!(!argv.is_null());
        let parsed = unsafe { std::slice::from_raw_parts(argv, argc as usize) }
            .iter()
            .map(|ptr| {
                let ptr = *ptr;
                let mut len = 0usize;
                unsafe {
                    while *ptr.add(len) != 0 {
                        len += 1;
                    }
                }
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) })
            })
            .collect::<Vec<_>>();
        unsafe {
            LocalFree(argv as _);
        }
        assert_eq!(parsed.first().map(String::as_str), Some("runner.exe"));
        assert_eq!(&parsed[1..], args.as_slice());
    }
}
