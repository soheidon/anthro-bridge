use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rand::rngs::OsRng;
use rand::RngCore;
use subtle::ConstantTimeEq;
use tokio::sync::{mpsc, Mutex, Notify};

use super::types::{
    ClaimTaskRequest, MailboxSessionDescriptor, OrchestratorTaskEnvelope, ReportProgressRequest,
    SubmitTaskRequest, WorkflowState,
};

/// Location of the ephemeral session descriptor file.
pub fn get_session_descriptor_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata)
            .join("Anthro Bridge")
            .join("orchestrator_session.json")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".config")
            .join("anthro-bridge")
            .join("orchestrator_session.json")
    } else {
        std::env::temp_dir()
            .join("anthro-bridge")
            .join("orchestrator_session.json")
    }
}

/// Shared internal state for the Mailbox HTTP server.
pub struct MailboxInner {
    pub run_id: String,
    pub project_path: String,
    pub token: String,
    pub epoch: u64,
    pub is_claimed: bool,
    pub last_progress_at: Option<std::time::Instant>,
    pub lease_timeout_duration: std::time::Duration,
    pub active_task: Option<OrchestratorTaskEnvelope>,
    pub current_state: WorkflowState,
    pub task_notify: Arc<Notify>,
    pub submit_tx: mpsc::Sender<SubmitTaskRequest>,
    pub progress_tx: mpsc::Sender<ReportProgressRequest>,
}

#[derive(Clone)]
pub struct MailboxState {
    pub inner: Arc<Mutex<MailboxInner>>,
}

/// The running Mailbox server instance.
pub struct MailboxServer {
    pub port: u16,
    pub token: String,
    pub session_path: PathBuf,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Restricts file permissions so only the current user/owner has access.
pub fn restrict_session_file_permissions(path: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to set permissions on {:?}: {}", path, e))?;
        Ok(())
    }
    #[cfg(windows)]
    {
        let username = std::env::var("USERNAME").map_err(|_| {
            "Failed to determine current Windows user identity: USERNAME environment variable is not set."
                .to_string()
        })?;
        restrict_session_file_permissions_windows_user(path, &username)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err("Unsupported OS platform: cannot enforce user-only ACL.".to_string())
    }
}

/// Restricts file permissions on Windows for a specific username.
pub fn restrict_session_file_permissions_windows_user(
    path: &std::path::Path,
    username: &str,
) -> Result<(), String> {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        return Err(
            "Failed to determine Windows user identity: USERNAME is empty or invalid.".to_string(),
        );
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("icacls")
            .arg(path)
            .args([
                "/inheritance:r",
                "/grant:r",
                &format!("{}:(R,W,D)", trimmed),
            ])
            .output()
            .map_err(|e| format!("Failed to execute icacls on {:?}: {}", path, e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "icacls failed to restrict permissions: {} {}",
                stdout, stderr
            ));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
    Ok(())
}

impl MailboxServer {
    /// Starts the Mailbox HTTP server bound to 127.0.0.1:0 (ephemeral port), writes session descriptor,
    /// verifies ACL permissions, and returns the handle along with submit/progress receiver channels.
    pub async fn start(
        run_id: String,
        project_path: String,
    ) -> Result<
        (
            Self,
            MailboxState,
            mpsc::Receiver<SubmitTaskRequest>,
            mpsc::Receiver<ReportProgressRequest>,
        ),
        String,
    > {
        Self::start_with_path_and_perm_fn(
            run_id,
            project_path,
            get_session_descriptor_path(),
            restrict_session_file_permissions,
        )
        .await
    }

    /// Starts Mailbox with a custom permission verification function (used for deterministic ACL failure testing).
    pub async fn start_with_perm_fn<F>(
        run_id: String,
        project_path: String,
        perm_fn: F,
    ) -> Result<
        (
            Self,
            MailboxState,
            mpsc::Receiver<SubmitTaskRequest>,
            mpsc::Receiver<ReportProgressRequest>,
        ),
        String,
    >
    where
        F: FnOnce(&std::path::Path) -> Result<(), String>,
    {
        Self::start_with_path_and_perm_fn(
            run_id,
            project_path,
            get_session_descriptor_path(),
            perm_fn,
        )
        .await
    }

    /// Starts Mailbox with a custom session path and permission verification function.
    pub async fn start_with_path_and_perm_fn<F>(
        run_id: String,
        project_path: String,
        session_path: PathBuf,
        perm_fn: F,
    ) -> Result<
        (
            Self,
            MailboxState,
            mpsc::Receiver<SubmitTaskRequest>,
            mpsc::Receiver<ReportProgressRequest>,
        ),
        String,
    >
    where
        F: FnOnce(&std::path::Path) -> Result<(), String>,
    {
        if let Some(parent) = session_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Generate 256-bit CSPRNG token (32 bytes hex-encoded = 64 hex characters)
        let mut token_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut token_bytes);
        let token: String = token_bytes.iter().map(|b| format!("{:02x}", b)).collect();

        // 1. Bind listener first to determine port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind mailbox TCP listener: {}", e))?;

        let local_addr = listener
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {}", e))?;
        let port = local_addr.port();

        // 2. Write session descriptor to AppData
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expires_at = created_at + 86400; // 24 hours

        let descriptor = MailboxSessionDescriptor {
            run_id: run_id.clone(),
            project_path: project_path.clone(),
            port,
            token: token.clone(),
            created_at,
            expires_at,
        };

        let json = serde_json::to_string_pretty(&descriptor)
            .map_err(|e| format!("Failed to serialize session descriptor: {}", e))?;

        std::fs::write(&session_path, json.as_bytes())
            .map_err(|e| format!("Failed to write session descriptor to {:?}: {}", session_path, e))?;

        // 3. Enforce user-only ACL and FAIL CLOSED if permission restriction fails
        if let Err(acl_err) = perm_fn(&session_path) {
            let _ = std::fs::remove_file(&session_path);
            return Err(format!(
                "Security failure: Failed to enforce user-only ACL on session descriptor at {:?}: {}",
                session_path, acl_err
            ));
        }

        // 4. Only after ACL is verified, launch HTTP server
        let (submit_tx, submit_rx) = mpsc::channel::<SubmitTaskRequest>(32);
        let (progress_tx, progress_rx) = mpsc::channel::<ReportProgressRequest>(128);
        let task_notify = Arc::new(Notify::new());

        let inner = MailboxInner {
            run_id: run_id.clone(),
            project_path: project_path.clone(),
            token: token.clone(),
            epoch: 1,
            is_claimed: false,
            last_progress_at: None,
            lease_timeout_duration: Duration::from_secs(60),
            active_task: None,
            current_state: WorkflowState::PlanDraft,
            task_notify: task_notify.clone(),
            submit_tx,
            progress_tx,
        };

        let state = MailboxState {
            inner: Arc::new(Mutex::new(inner)),
        };

        let app = Router::new()
            .route("/health", get(handle_health))
            .route("/mailbox/claim", post(handle_claim))
            .route("/mailbox/progress", post(handle_progress))
            .route("/mailbox/submit", post(handle_submit))
            .with_state(state.clone());

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let server_handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        Ok((
            Self {
                port,
                token,
                session_path,
                shutdown_tx: Some(shutdown_tx),
                server_handle: Some(server_handle),
            },
            state,
            submit_rx,
            progress_rx,
        ))
    }

    /// Stops the server and deletes the session descriptor.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.server_handle.take() {
            let _ = handle.await;
        }
        if self.session_path.exists() {
            let _ = std::fs::remove_file(&self.session_path);
        }
    }
}

impl Drop for MailboxServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if self.session_path.exists() {
            let _ = std::fs::remove_file(&self.session_path);
        }
    }
}

// Authentication helper using constant-time comparison
fn verify_bearer_token(headers: &HeaderMap, expected_token: &str) -> bool {
    if let Some(auth_header) = headers.get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(provided_token) = auth_str.strip_prefix("Bearer ") {
                let provided_token = provided_token.trim();
                return provided_token.as_bytes().ct_eq(expected_token.as_bytes()).into();
            }
        }
    }
    false
}

async fn handle_health(
    headers: HeaderMap,
    State(state): State<MailboxState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let guard = state.inner.lock().await;
    if !verify_bearer_token(&headers, &guard.token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(serde_json::json!({ "status": "ok", "runId": guard.run_id })))
}

async fn handle_claim(
    headers: HeaderMap,
    State(state): State<MailboxState>,
    Json(payload): Json<ClaimTaskRequest>,
) -> Result<Response, StatusCode> {
    let wait_seconds = payload.wait_seconds.unwrap_or(30).min(60);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(wait_seconds);

    loop {
        let notify = {
            let mut guard = state.inner.lock().await;
            if !verify_bearer_token(&headers, &guard.token) {
                return Err(StatusCode::UNAUTHORIZED);
            }

            // Check if specific runId requested
            if !payload.run_id.is_empty() && payload.run_id != guard.run_id {
                return Err(StatusCode::BAD_REQUEST);
            }

            // Disallow claim while waiting for human confirmation
            if guard.current_state == WorkflowState::WaitingForUser {
                guard.task_notify.clone()
            } else if !guard.is_claimed && guard.active_task.is_some() {
                guard.is_claimed = true;
                guard.last_progress_at = Some(std::time::Instant::now());
                let current_epoch = guard.epoch;
                if let Some(ref mut task) = guard.active_task {
                    task.epoch = current_epoch;
                    return Ok(Json(task.clone()).into_response());
                }
                guard.task_notify.clone()
            } else {
                guard.task_notify.clone()
            }
        };

        // Long-poll wait until task is available or timeout reached
        let now = tokio::time::Instant::now();
        if now >= deadline {
            // Long poll timeout with 204 No Content
            return Ok(StatusCode::NO_CONTENT.into_response());
        }

        let time_left = deadline - now;
        tokio::select! {
            _ = notify.notified() => {
                // Task became available or state changed, re-check loop
                continue;
            }
            _ = tokio::time::sleep(time_left) => {
                return Ok(StatusCode::NO_CONTENT.into_response());
            }
        }
    }
}

async fn handle_progress(
    headers: HeaderMap,
    State(state): State<MailboxState>,
    Json(payload): Json<ReportProgressRequest>,
) -> Result<StatusCode, StatusCode> {
    let mut guard = state.inner.lock().await;

    if !verify_bearer_token(&headers, &guard.token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if payload.run_id != guard.run_id {
        return Err(StatusCode::BAD_REQUEST);
    }

    if !guard.is_claimed {
        return Err(StatusCode::CONFLICT);
    }

    if let Some(ref active) = guard.active_task {
        if active.task_id != payload.task_id {
            return Err(StatusCode::BAD_REQUEST);
        }
    } else {
        return Err(StatusCode::CONFLICT);
    }

    // Epoch validation
    if payload.epoch != guard.epoch {
        return Err(StatusCode::CONFLICT);
    }

    guard.last_progress_at = Some(std::time::Instant::now());

    let tx = guard.progress_tx.clone();
    drop(guard);

    let _ = tx.send(payload).await;
    Ok(StatusCode::OK)
}

async fn handle_submit(
    headers: HeaderMap,
    State(state): State<MailboxState>,
    Json(payload): Json<SubmitTaskRequest>,
) -> Result<StatusCode, StatusCode> {
    let mut guard = state.inner.lock().await;

    // 1. Verify authentication
    if !verify_bearer_token(&headers, &guard.token) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // 2. Verify run_id
    if payload.run_id != guard.run_id {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 3. Verify active lease exists
    if !guard.is_claimed {
        return Err(StatusCode::CONFLICT);
    }

    // 4. Verify task_id matches active task
    if let Some(ref active) = guard.active_task {
        if active.task_id != payload.task_id {
            return Err(StatusCode::BAD_REQUEST);
        }
    } else {
        return Err(StatusCode::CONFLICT);
    }

    // 5. Verify epoch matches current epoch
    if payload.epoch != guard.epoch {
        return Err(StatusCode::CONFLICT);
    }

    // All validations passed -> now perform state mutation atomically
    guard.active_task = None;
    guard.is_claimed = false;
    guard.last_progress_at = None;
    let tx = guard.submit_tx.clone();
    drop(guard);

    let _ = tx.send(payload).await;
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mailbox_server_lifecycle_and_endpoints() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_path = temp_dir.path().join("orchestrator_session.json");
        let run_id = "test-run-123".to_string();
        let project_path = "C:/test/path".to_string();

        let (server, state, mut submit_rx, mut progress_rx) =
            MailboxServer::start_with_path_and_perm_fn(
                run_id.clone(),
                project_path.clone(),
                session_path,
                restrict_session_file_permissions,
            )
            .await
            .expect("Failed to start server");

        let base_url = format!("http://127.0.0.1:{}", server.port);
        let client = reqwest::Client::new();

        // 1. Health check without token -> 401
        let resp = client.get(format!("{}/health", base_url)).send().await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

        // 2. Health check with valid token -> 200
        let resp = client
            .get(format!("{}/health", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        // 3. Claim when no active task (1 second timeout) -> 204
        let claim_req = ClaimTaskRequest {
            run_id: run_id.clone(),
            wait_seconds: Some(1),
        };
        let resp = client
            .post(format!("{}/mailbox/claim", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&claim_req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::NO_CONTENT);

        // 4. Set active task and claim -> 200 with task envelope
        {
            let mut guard = state.inner.lock().await;
            guard.active_task = Some(OrchestratorTaskEnvelope {
                run_id: run_id.clone(),
                task_id: "task-impl-1".to_string(),
                stage: WorkflowState::Implementation,
                role: super::super::types::AgentRole::Implementer,
                epoch: 1,
                project_path: project_path.clone(),
                approved_plan: Some("Plan details".to_string()),
                task_prompt: Some("Task prompt".to_string()),
                review_feedback: None,
                validation_summary: None,
            });
            guard.task_notify.notify_waiters();
        }

        let resp = client
            .post(format!("{}/mailbox/claim", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&claim_req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let envelope: OrchestratorTaskEnvelope = resp.json().await.unwrap();
        assert_eq!(envelope.task_id, "task-impl-1");
        assert_eq!(envelope.epoch, 1);

        // 5. Progress report
        let prog_req = ReportProgressRequest {
            run_id: run_id.clone(),
            task_id: "task-impl-1".to_string(),
            epoch: 1,
            message: "50% done".to_string(),
            percent: Some(50),
        };
        let resp = client
            .post(format!("{}/mailbox/progress", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&prog_req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let received_prog = progress_rx.recv().await.unwrap();
        assert_eq!(received_prog.message, "50% done");

        // 6. Submit result
        let submit_req = SubmitTaskRequest {
            run_id: run_id.clone(),
            task_id: "task-impl-1".to_string(),
            epoch: 1,
            idempotency_key: Some("key-1".to_string()),
            status: "success".to_string(),
            summary: "Finished implementation".to_string(),
            modified_files: vec!["src/main.rs".to_string()],
        };
        let resp = client
            .post(format!("{}/mailbox/submit", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&submit_req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let received_submit = submit_rx.recv().await.unwrap();
        assert_eq!(received_submit.summary, "Finished implementation");

        // Cleanup
        server.stop().await;
    }

    #[tokio::test]
    async fn test_submit_unauthorized_and_stale_epoch_does_not_mutate_state() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_path = temp_dir.path().join("orchestrator_session.json");
        let run_id = "test-run-mut-check".to_string();
        let project_path = "C:/test/path".to_string();

        let (server, state, _submit_rx, _progress_rx) =
            MailboxServer::start_with_path_and_perm_fn(
                run_id.clone(),
                project_path.clone(),
                session_path,
                restrict_session_file_permissions,
            )
            .await
            .expect("Failed to start server");

        let base_url = format!("http://127.0.0.1:{}", server.port);
        let client = reqwest::Client::new();

        // Arm active task
        {
            let mut guard = state.inner.lock().await;
            guard.active_task = Some(OrchestratorTaskEnvelope {
                run_id: run_id.clone(),
                task_id: "task-guard-1".to_string(),
                stage: WorkflowState::Implementation,
                role: super::super::types::AgentRole::Implementer,
                epoch: 1,
                project_path: project_path.clone(),
                approved_plan: None,
                task_prompt: None,
                review_feedback: None,
                validation_summary: None,
            });
            guard.task_notify.notify_waiters();
        }

        // Claim task
        let claim_req = ClaimTaskRequest {
            run_id: run_id.clone(),
            wait_seconds: Some(1),
        };
        let claim_resp = client
            .post(format!("{}/mailbox/claim", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&claim_req)
            .send()
            .await
            .unwrap();
        assert_eq!(claim_resp.status(), reqwest::StatusCode::OK);

        // Verify task is claimed
        {
            let guard = state.inner.lock().await;
            assert!(guard.is_claimed);
            assert!(guard.active_task.is_some());
        }

        // 1. Submit with invalid token -> 401 and active_task/is_claimed MUST REMAIN INTACT
        let submit_req = SubmitTaskRequest {
            run_id: run_id.clone(),
            task_id: "task-guard-1".to_string(),
            epoch: 1,
            idempotency_key: None,
            status: "success".to_string(),
            summary: "Malicious submission".to_string(),
            modified_files: vec![],
        };
        let unauth_resp = client
            .post(format!("{}/mailbox/submit", base_url))
            .header("Authorization", "Bearer invalid-token")
            .json(&submit_req)
            .send()
            .await
            .unwrap();
        assert_eq!(unauth_resp.status(), reqwest::StatusCode::UNAUTHORIZED);

        {
            let guard = state.inner.lock().await;
            assert!(guard.is_claimed, "is_claimed must still be true after 401");
            assert!(guard.active_task.is_some(), "active_task must remain Some after 401");
        }

        // 2. Submit with stale epoch (e.g. epoch 999) -> 409 CONFLICT and active_task MUST REMAIN INTACT
        let stale_req = SubmitTaskRequest {
            run_id: run_id.clone(),
            task_id: "task-guard-1".to_string(),
            epoch: 999,
            idempotency_key: None,
            status: "success".to_string(),
            summary: "Stale epoch submission".to_string(),
            modified_files: vec![],
        };
        let conflict_resp = client
            .post(format!("{}/mailbox/submit", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&stale_req)
            .send()
            .await
            .unwrap();
        assert_eq!(conflict_resp.status(), reqwest::StatusCode::CONFLICT);

        {
            let guard = state.inner.lock().await;
            assert!(guard.is_claimed, "is_claimed must still be true after 409");
            assert!(guard.active_task.is_some(), "active_task must remain Some after 409");
        }

        server.stop().await;
    }

    #[tokio::test]
    async fn test_concurrent_claim_exclusivity() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_path = temp_dir.path().join("orchestrator_session.json");
        let run_id = "test-run-concurrent".to_string();
        let project_path = "C:/test/path".to_string();

        let (server, state, _submit_rx, _progress_rx) =
            MailboxServer::start_with_path_and_perm_fn(
                run_id.clone(),
                project_path.clone(),
                session_path,
                restrict_session_file_permissions,
            )
            .await
            .expect("Failed to start server");

        let base_url = format!("http://127.0.0.1:{}", server.port);
        let token = server.token.clone();

        // Arm active task
        {
            let mut guard = state.inner.lock().await;
            guard.active_task = Some(OrchestratorTaskEnvelope {
                run_id: run_id.clone(),
                task_id: "task-concurrent-1".to_string(),
                stage: WorkflowState::Implementation,
                role: super::super::types::AgentRole::Implementer,
                epoch: 1,
                project_path: project_path.clone(),
                approved_plan: None,
                task_prompt: None,
                review_feedback: None,
                validation_summary: None,
            });
            guard.task_notify.notify_waiters();
        }

        let client = reqwest::Client::new();
        let claim_req = ClaimTaskRequest {
            run_id: run_id.clone(),
            wait_seconds: Some(1),
        };

        // Launch 2 concurrent workers attempting to claim
        let c1 = client.clone();
        let u1 = format!("{}/mailbox/claim", base_url);
        let t1 = token.clone();
        let r1 = claim_req.clone();
        let handle1 = tokio::spawn(async move {
            c1.post(&u1)
                .header("Authorization", format!("Bearer {}", t1))
                .json(&r1)
                .send()
                .await
                .unwrap()
                .status()
        });

        let c2 = client.clone();
        let u2 = format!("{}/mailbox/claim", base_url);
        let t2 = token.clone();
        let r2 = claim_req.clone();
        let handle2 = tokio::spawn(async move {
            c2.post(&u2)
                .header("Authorization", format!("Bearer {}", t2))
                .json(&r2)
                .send()
                .await
                .unwrap()
                .status()
        });

        let status1 = handle1.await.unwrap();
        let status2 = handle2.await.unwrap();

        // Exactly one worker must succeed with 200 OK, the other must receive 204 No Content
        let ok_count = (status1 == reqwest::StatusCode::OK) as usize
            + (status2 == reqwest::StatusCode::OK) as usize;
        let no_content_count = (status1 == reqwest::StatusCode::NO_CONTENT) as usize
            + (status2 == reqwest::StatusCode::NO_CONTENT) as usize;

        assert_eq!(ok_count, 1, "Exactly one worker must acquire the task");
        assert_eq!(no_content_count, 1, "The second worker must not acquire the task");

        server.stop().await;
    }

    #[test]
    fn test_restrict_session_file_permissions_helper() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_session.json");
        std::fs::write(&file_path, b"{}").unwrap();

        let res = restrict_session_file_permissions(&file_path);
        assert!(res.is_ok(), "Permission restriction helper should succeed: {:?}", res);
    }

    #[test]
    fn test_restrict_session_file_permissions_windows_user_valid_and_invalid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_session_user.json");
        std::fs::write(&file_path, b"{}").unwrap();

        // 1. Missing or empty user identity must fail closed
        let res_empty = restrict_session_file_permissions_windows_user(&file_path, "");
        assert!(res_empty.is_err(), "Empty username must fail: {:?}", res_empty);
        let res_whitespace = restrict_session_file_permissions_windows_user(&file_path, "   ");
        assert!(res_whitespace.is_err(), "Whitespace username must fail: {:?}", res_whitespace);

        // 2. Valid username (from env or fallback) must succeed
        #[cfg(windows)]
        if let Ok(username) = std::env::var("USERNAME") {
            let res_valid = restrict_session_file_permissions_windows_user(&file_path, &username);
            assert!(res_valid.is_ok(), "Valid username must succeed: {:?}", res_valid);
        }
    }

    #[tokio::test]
    async fn test_mailbox_startup_fails_closed_when_acl_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_path = temp_dir.path().join("test_session_acl_fail.json");
        let run_id = "test-acl-fail-run".to_string();
        let project_path = "C:/test/path".to_string();

        if session_path.exists() {
            let _ = std::fs::remove_file(&session_path);
        }

        // Custom perm_fn simulating ACL failure
        let failing_perm_fn = |_path: &std::path::Path| -> Result<(), String> {
            Err("Simulated ACL permission enforcement failure".to_string())
        };

        let result = MailboxServer::start_with_path_and_perm_fn(
            run_id,
            project_path,
            session_path.clone(),
            failing_perm_fn,
        )
        .await;

        assert!(result.is_err(), "Mailbox startup must fail closed if ACL setup fails");
        let err_msg = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected startup error"),
        };
        assert!(err_msg.contains("Security failure"), "Error must describe security failure: {}", err_msg);
        assert!(!session_path.exists(), "Session descriptor file must be cleaned up on failure");
    }

    #[tokio::test]
    async fn test_mailbox_startup_fails_closed_when_identity_cannot_be_resolved() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_path = temp_dir.path().join("test_session_identity_fail.json");
        let run_id = "test-identity-fail-run".to_string();
        let project_path = "C:/test/path".to_string();

        if session_path.exists() {
            let _ = std::fs::remove_file(&session_path);
        }

        // Perm function simulating unresolved user identity
        let missing_identity_perm_fn = |path: &std::path::Path| -> Result<(), String> {
            restrict_session_file_permissions_windows_user(path, "")
        };

        let result = MailboxServer::start_with_path_and_perm_fn(
            run_id,
            project_path,
            session_path.clone(),
            missing_identity_perm_fn,
        )
        .await;

        assert!(result.is_err(), "Mailbox startup must fail closed if user identity cannot be resolved");
        let err_msg = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected startup error"),
        };
        assert!(err_msg.contains("Security failure"), "Error must describe security failure: {}", err_msg);
        assert!(!session_path.exists(), "Session descriptor file must be cleaned up on failure");
    }
}
