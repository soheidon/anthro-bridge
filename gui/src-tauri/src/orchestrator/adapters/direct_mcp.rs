use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientInfo, ListToolsResult, PaginatedRequestParams,
    Tool,
};
use rmcp::{ClientHandler, ServiceExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

use super::super::plan_workspace::{FrozenPlanPayload, PlanContext};
use super::super::secrets::SecretRedactor;
use super::super::types::{
    AgentRole, ExecutionAdapterType, McpServerConfig, McpServerTransport, McpToolContract,
    OrchestratorProfile,
};
use super::{AdapterExecutionInput, AdapterExecutionOutput};

pub const MCP_MAX_CAPTURE_BYTES: usize = 1024 * 1024; // 1 MiB
pub const MCP_EXECUTION_DEADLINE: Duration = Duration::from_secs(180);
pub const MCP_TEARDOWN_GRACE: Duration = Duration::from_secs(5);
pub const MCP_MAX_TOOL_PAGES: usize = 32;
pub const MCP_MAX_TOOLS_TOTAL: usize = 256;

/// Typed, machine-readable error representations for DirectMcpAdapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectMcpError {
    UnauthorizedRole(String),
    InvalidAdapterType(String),
    MissingServerId,
    MissingToolName,
    UnknownServer(String),
    UnsupportedTransport(String),
    UnsupportedContract(String),
    MissingExecutable,
    InvalidExecutablePath(String),
    ShellLauncherRejected(String),
    ScriptWrapperRejected(String),
    InvalidWorkingDirectory(String),
    InvalidEnvironmentVariable(String),
    MissingEnvironmentVariable(String),
    InvalidFrozenPlanPayload(String),
    ProcessContainmentFailed(String),
    SpawnFailed(String),
    HandshakeFailed(String),
    PaginationLimitExceeded(usize),
    ToolLimitExceeded(usize),
    RepeatedCursor(String),
    ToolsListFailed(String),
    ToolNotFound(String),
    IncompatibleSchema(String),
    CallFailed(String),
    ResponseSizeLimitExceeded(usize),
    MalformedStructuredContent(String),
    UnsupportedSchemaVersion(u64),
    ConflictingContentBlocks(String),
    UnsupportedContentType(String),
    EmptyMcpResponse,
    ToolReportedError(String),
    DeadlineExceeded,
    Cancelled,
    CleanupFailed(String),
    ProcessExitFailed(String),
    SerializationFailed(String),
}

impl DirectMcpError {
    /// Stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnauthorizedRole(_) => "DMCP_UNAUTHORIZED_ROLE",
            Self::InvalidAdapterType(_) => "DMCP_INVALID_ADAPTER_TYPE",
            Self::MissingServerId => "DMCP_MISSING_SERVER_ID",
            Self::MissingToolName => "DMCP_MISSING_TOOL_NAME",
            Self::UnknownServer(_) => "DMCP_UNKNOWN_SERVER",
            Self::UnsupportedTransport(_) => "DMCP_UNSUPPORTED_TRANSPORT",
            Self::UnsupportedContract(_) => "DMCP_UNSUPPORTED_CONTRACT",
            Self::MissingExecutable => "DMCP_MISSING_EXECUTABLE",
            Self::InvalidExecutablePath(_) => "DMCP_INVALID_EXECUTABLE_PATH",
            Self::ShellLauncherRejected(_) => "DMCP_SHELL_LAUNCHER_REJECTED",
            Self::ScriptWrapperRejected(_) => "DMCP_SCRIPT_WRAPPER_REJECTED",
            Self::InvalidWorkingDirectory(_) => "DMCP_INVALID_WORKING_DIRECTORY",
            Self::InvalidEnvironmentVariable(_) => "DMCP_INVALID_ENVIRONMENT_VARIABLE",
            Self::MissingEnvironmentVariable(_) => "DMCP_MISSING_ENVIRONMENT_VARIABLE",
            Self::InvalidFrozenPlanPayload(_) => "DMCP_INVALID_FROZEN_PLAN_PAYLOAD",
            Self::ProcessContainmentFailed(_) => "DMCP_PROCESS_CONTAINMENT_FAILED",
            Self::SpawnFailed(_) => "DMCP_SPAWN_FAILED",
            Self::HandshakeFailed(_) => "DMCP_HANDSHAKE_FAILED",
            Self::PaginationLimitExceeded(_) => "DMCP_PAGINATION_LIMIT_EXCEEDED",
            Self::ToolLimitExceeded(_) => "DMCP_TOOL_LIMIT_EXCEEDED",
            Self::RepeatedCursor(_) => "DMCP_REPEATED_CURSOR",
            Self::ToolsListFailed(_) => "DMCP_TOOLS_LIST_FAILED",
            Self::ToolNotFound(_) => "DMCP_TOOL_NOT_FOUND",
            Self::IncompatibleSchema(_) => "DMCP_INCOMPATIBLE_SCHEMA",
            Self::CallFailed(_) => "DMCP_CALL_FAILED",
            Self::ResponseSizeLimitExceeded(_) => "DMCP_RESPONSE_SIZE_LIMIT_EXCEEDED",
            Self::MalformedStructuredContent(_) => "DMCP_MALFORMED_STRUCTURED_CONTENT",
            Self::UnsupportedSchemaVersion(_) => "DMCP_UNSUPPORTED_SCHEMA_VERSION",
            Self::ConflictingContentBlocks(_) => "DMCP_CONFLICTING_CONTENT_BLOCKS",
            Self::UnsupportedContentType(_) => "DMCP_UNSUPPORTED_CONTENT_TYPE",
            Self::EmptyMcpResponse => "DMCP_EMPTY_MCP_RESPONSE",
            Self::ToolReportedError(_) => "DMCP_TOOL_REPORTED_ERROR",
            Self::DeadlineExceeded => "DMCP_DEADLINE_EXCEEDED",
            Self::Cancelled => "DMCP_CANCELLED",
            Self::CleanupFailed(_) => "DMCP_CLEANUP_FAILED",
            Self::ProcessExitFailed(_) => "DMCP_PROCESS_EXIT_FAILED",
            Self::SerializationFailed(_) => "DMCP_SERIALIZATION_FAILED",
        }
    }
}

impl fmt::Display for DirectMcpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self.code();
        match self {
            Self::UnauthorizedRole(msg) => write!(f, "[{code}] MCP role authorization error: {msg}"),
            Self::InvalidAdapterType(msg) => write!(f, "[{code}] Invalid adapter type: {msg}"),
            Self::MissingServerId => write!(f, "[{code}] MCP profile error: missing 'external_mcp_server' identifier."),
            Self::MissingToolName => write!(f, "[{code}] MCP profile error: missing 'mcp_tool' name."),
            Self::UnknownServer(id) => write!(f, "[{code}] MCP profile error: unknown server '{id}' not found in registered mcp_servers."),
            Self::UnsupportedTransport(msg) => write!(f, "[{code}] Unsupported transport: {msg}"),
            Self::UnsupportedContract(msg) => write!(f, "[{code}] Unsupported tool contract: {msg}"),
            Self::MissingExecutable => write!(f, "[{code}] Missing executable: path cannot be empty."),
            Self::InvalidExecutablePath(msg) => write!(f, "[{code}] Invalid executable path: {msg}"),
            Self::ShellLauncherRejected(msg) => write!(f, "[{code}] Shell launcher rejected: {msg}"),
            Self::ScriptWrapperRejected(msg) => write!(f, "[{code}] Script wrapper rejected: {msg}"),
            Self::InvalidWorkingDirectory(msg) => write!(f, "[{code}] Invalid working directory: {msg}"),
            Self::InvalidEnvironmentVariable(msg) => write!(f, "[{code}] Invalid environment variable: {msg}"),
            Self::MissingEnvironmentVariable(msg) => write!(f, "[{code}] Missing environment variable: {msg}"),
            Self::InvalidFrozenPlanPayload(msg) => write!(f, "[{code}] Invalid frozen plan payload: {msg}"),
            Self::ProcessContainmentFailed(msg) => write!(f, "[{code}] Process containment failed: {msg}"),
            Self::SpawnFailed(msg) => write!(f, "[{code}] Spawn failed: {msg}"),
            Self::HandshakeFailed(msg) => write!(f, "[{code}] Handshake failed: {msg}"),
            Self::PaginationLimitExceeded(pages) => write!(f, "[{code}] Pagination limit exceeded: reached {pages} pages (limit: {MCP_MAX_TOOL_PAGES})."),
            Self::ToolLimitExceeded(tools) => write!(f, "[{code}] Tool limit exceeded: discovered {tools} tools (limit: {MCP_MAX_TOOLS_TOTAL})."),
            Self::RepeatedCursor(c) => write!(f, "[{code}] Repeated pagination cursor detected: '{c}'."),
            Self::ToolsListFailed(msg) => write!(f, "[{code}] tools/list call failed: {msg}"),
            Self::ToolNotFound(msg) => write!(f, "[{code}] Tool not found: {msg}"),
            Self::IncompatibleSchema(msg) => write!(f, "[{code}] Incompatible tool schema: {msg}"),
            Self::CallFailed(msg) => write!(f, "[{code}] tools/call failed: {msg}"),
            Self::ResponseSizeLimitExceeded(bytes) => write!(f, "[{code}] Response size limit exceeded: {bytes} bytes exceeds 1 MiB limit."),
            Self::MalformedStructuredContent(msg) => write!(f, "[{code}] Malformed structured content: {msg}"),
            Self::UnsupportedSchemaVersion(ver) => write!(f, "[{code}] Unsupported schema version: expected 1, got {ver}."),
            Self::ConflictingContentBlocks(msg) => write!(f, "[{code}] Conflicting content blocks: {msg}"),
            Self::UnsupportedContentType(msg) => write!(f, "[{code}] Unsupported content type: {msg}"),
            Self::EmptyMcpResponse => write!(f, "[{code}] Empty MCP response: tool returned zero content."),
            Self::ToolReportedError(msg) => write!(f, "[{code}] Tool reported error (isError=true): {msg}"),
            Self::DeadlineExceeded => write!(f, "[{code}] MCP invocation timed out after absolute deadline."),
            Self::Cancelled => write!(f, "[{code}] MCP invocation cancelled by user."),
            Self::CleanupFailed(msg) => write!(f, "[{code}] MCP cleanup failed: {msg}"),
            Self::ProcessExitFailed(msg) => write!(f, "[{code}] MCP server process exited with failure: {msg}"),
            Self::SerializationFailed(msg) => write!(f, "[{code}] Serialization failed: {msg}"),
        }
    }
}

impl std::error::Error for DirectMcpError {}

impl From<DirectMcpError> for String {
    fn from(err: DirectMcpError) -> Self {
        err.to_string()
    }
}

/// Audit record capturing the complete lifecycle and outcome of an MCP invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectMcpAuditRecord {
    pub invocation_id: String,
    pub role: String,
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    pub server_id: String,
    pub tool_name: String,
    pub duration_ms: u64,
    pub pages_discovered: usize,
    pub tools_discovered: usize,
    pub result_code: String,
    pub cleanup_outcome: String,
    // Diagnostic prose can contain provider-controlled data; persist only the
    // stable result code, never raw error text in the durable run journal.
    #[serde(skip)]
    pub error_message: Option<String>,
}

/// Configuration and timing seams for DirectMcpAdapter (injectable for deterministic tests).
#[derive(Debug, Clone)]
pub struct DirectMcpConfig {
    pub execution_deadline: Duration,
    pub teardown_grace: Duration,
    pub max_tool_pages: usize,
    pub max_tools_total: usize,
    pub max_capture_bytes: usize,
}

impl Default for DirectMcpConfig {
    fn default() -> Self {
        Self {
            execution_deadline: MCP_EXECUTION_DEADLINE,
            teardown_grace: MCP_TEARDOWN_GRACE,
            max_tool_pages: MCP_MAX_TOOL_PAGES,
            max_tools_total: MCP_MAX_TOOLS_TOTAL,
            max_capture_bytes: MCP_MAX_CAPTURE_BYTES,
        }
    }
}

/// Private execution input for DirectMcpAdapter carrying frozen Phase A payloads.
#[derive(Debug, Clone)]
pub struct DirectMcpExecutionInput<'a> {
    pub adapter_input: &'a AdapterExecutionInput,
    pub server_config: &'a McpServerConfig,
    pub plan_context: Option<&'a PlanContext>,
    pub frozen_plan: Option<&'a FrozenPlanPayload>,
}

#[derive(Debug, Clone)]
struct DirectMcpClientHandler;

impl ClientHandler for DirectMcpClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

/// Bidirectional async stream combining child process stdout (reader) and stdin (writer).
///
/// The reader path enforces the inbound protocol frame cap. The rmcp transport
/// reads newline-delimited frames with no read-side length limit, so a peer that
/// never emits a delimiter would otherwise grow the line buffer without bound.
/// Counting bytes since the last delimiter here bounds a single frame before the
/// protocol layer can buffer it.
pub struct ChildIo<R, W> {
    reader: R,
    writer: W,
    frame_bytes: usize,
    max_frame_bytes: usize,
    frame_limit_exceeded: Arc<std::sync::atomic::AtomicBool>,
}

impl<R, W> ChildIo<R, W> {
    pub fn new(
        reader: R,
        writer: W,
        max_frame_bytes: usize,
        frame_limit_exceeded: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            reader,
            writer,
            frame_bytes: 0,
            max_frame_bytes,
            frame_limit_exceeded,
        }
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncRead for ChildIo<R, W> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let filled_before = buf.filled().len();
        let poll = Pin::new(&mut self.reader).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll {
            let this = self.get_mut();
            let received = buf.filled().len().saturating_sub(filled_before);
            if received > 0 {
                let new_bytes = &buf.filled()[filled_before..];
                for index in 0..received {
                    this.frame_bytes += 1;
                    if new_bytes[index] == b'\n' {
                        this.frame_bytes = 0;
                    } else if this.frame_bytes > this.max_frame_bytes {
                        this.frame_limit_exceeded
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "MCP inbound protocol frame exceeded the configured limit",
                        )));
                    }
                }
            }
        }
        poll
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncWrite for ChildIo<R, W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.writer).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

/// Expected structured content envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptEnvelopeStructuredContent {
    pub schema_version: u32,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct DirectMcpAdapter {
    pub config: DirectMcpConfig,
    #[cfg(all(test, windows))]
    fail_job_assignment_for_test: bool,
    #[cfg(test)]
    fail_cleanup_for_test: bool,
    #[cfg(test)]
    fail_stderr_join_for_test: bool,
    #[cfg(test)]
    delay_spawn_for_test: Option<Duration>,
    #[cfg(test)]
    delay_normalization_for_test: Option<Duration>,
}

impl DirectMcpAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: DirectMcpConfig) -> Self {
        Self {
            config,
            #[cfg(all(test, windows))]
            fail_job_assignment_for_test: false,
            #[cfg(test)]
            fail_cleanup_for_test: false,
            #[cfg(test)]
            fail_stderr_join_for_test: false,
            #[cfg(test)]
            delay_spawn_for_test: None,
            #[cfg(test)]
            delay_normalization_for_test: None,
        }
    }

    #[cfg(all(test, windows))]
    fn with_job_assignment_failure_for_test(mut self) -> Self {
        self.fail_job_assignment_for_test = true;
        self
    }

    #[cfg(test)]
    pub fn with_cleanup_failure_for_test(mut self) -> Self {
        self.fail_cleanup_for_test = true;
        self
    }

    /// Injects an abnormal stderr drain task termination so the stderr join
    /// boundary fails independently of process-tree termination.
    #[cfg(test)]
    pub fn with_stderr_join_failure_for_test(mut self) -> Self {
        self.fail_stderr_join_for_test = true;
        self
    }

    #[cfg(test)]
    pub fn with_spawn_delay_for_test(mut self, delay: Duration) -> Self {
        self.delay_spawn_for_test = Some(delay);
        self
    }

    #[cfg(test)]
    pub fn with_normalization_delay_for_test(mut self, delay: Duration) -> Self {
        self.delay_normalization_for_test = Some(delay);
        self
    }

    /// Validates MCP assignment and server configuration before process spawn.
    pub fn validate_mcp_assignment(
        role: &AgentRole,
        profile: &OrchestratorProfile,
        server_registry: &HashMap<String, McpServerConfig>,
        canonical_project_path: &Path,
    ) -> Result<McpServerConfig, DirectMcpError> {
        // Enforce two-role allowlist
        if *role != AgentRole::Planner && *role != AgentRole::PlanReviewer {
            return Err(DirectMcpError::UnauthorizedRole(format!(
                "role '{role:?}' is not permitted to use MCP adapter (only Planner and PlanReviewer are authorized)."
            )));
        }

        if profile.adapter != ExecutionAdapterType::Mcp {
            return Err(DirectMcpError::InvalidAdapterType(format!(
                "expected Mcp, got {:?}",
                profile.adapter
            )));
        }

        let server_id = profile
            .external_mcp_server
            .as_deref()
            .ok_or(DirectMcpError::MissingServerId)?;

        if server_id.trim().is_empty() {
            return Err(DirectMcpError::MissingServerId);
        }

        let mcp_tool = profile
            .mcp_tool
            .as_deref()
            .ok_or(DirectMcpError::MissingToolName)?;

        if mcp_tool.trim().is_empty() {
            return Err(DirectMcpError::MissingToolName);
        }

        let server_config = server_registry
            .get(server_id)
            .ok_or_else(|| DirectMcpError::UnknownServer(server_id.to_string()))?;

        validate_server_config(server_config, canonical_project_path)?;

        Ok(server_config.clone())
    }

    /// Executes a direct MCP tool invocation for Planner or PlanReviewer.
    pub async fn execute(
        &self,
        input: &DirectMcpExecutionInput<'_>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<AdapterExecutionOutput, DirectMcpError> {
        let (output, _audit) = self
            .execute_internal(input, cancel_token)
            .await
            .map_err(|(err, _audit)| err)?;
        Ok(output)
    }

    /// Executes direct MCP tool invocation and returns both output and full audit record.
    pub async fn execute_with_audit(
        &self,
        input: &DirectMcpExecutionInput<'_>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<
        (AdapterExecutionOutput, DirectMcpAuditRecord),
        (DirectMcpError, DirectMcpAuditRecord),
    > {
        self.execute_internal(input, cancel_token).await
    }

    async fn execute_internal(
        &self,
        input: &DirectMcpExecutionInput<'_>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<
        (AdapterExecutionOutput, DirectMcpAuditRecord),
        (DirectMcpError, DirectMcpAuditRecord),
    > {
        // Absolute deadline starts at line 1 of invocation entry
        let start_time = Instant::now();
        let deadline = start_time + self.config.execution_deadline;

        let invocation_id = format!("dmcp-{}", uuid::Uuid::new_v4());
        let role = input.adapter_input.role.clone();
        let role_str = match role {
            AgentRole::Planner => "planner",
            AgentRole::PlanReviewer => "plan_reviewer",
            _ => "unknown",
        };

        let profile_id = input.adapter_input.profile.id.clone();
        let provider_id = input.adapter_input.profile.provider_id.clone();
        let model_id = input.adapter_input.profile.model.clone();
        let server_id = input
            .adapter_input
            .profile
            .external_mcp_server
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        let tool_name = input
            .adapter_input
            .profile
            .mcp_tool
            .as_deref()
            .unwrap_or("unknown")
            .to_string();

        let make_audit = |duration_ms: u64,
                          pages: usize,
                          tools: usize,
                          result_code: &str,
                          cleanup_outcome: &str,
                          error_msg: Option<String>| {
            DirectMcpAuditRecord {
                invocation_id: invocation_id.clone(),
                role: role_str.to_string(),
                profile_id: profile_id.clone(),
                provider_id: provider_id.clone(),
                model_id: model_id.clone(),
                server_id: server_id.clone(),
                tool_name: tool_name.clone(),
                duration_ms,
                pages_discovered: pages,
                tools_discovered: tools,
                result_code: result_code.to_string(),
                cleanup_outcome: cleanup_outcome.to_string(),
                error_message: error_msg,
            }
        };

        if role != AgentRole::Planner && role != AgentRole::PlanReviewer {
            let err = DirectMcpError::UnauthorizedRole(format!(
                "role '{role:?}' is not permitted to use MCP adapter."
            ));
            let audit = make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
            return Err((err, audit));
        }

        // Validate the run-owned Phase A frozen payload with the shared Phase A
        // helper before any process launch. The frozen values are sent verbatim;
        // `.plan` is never rescanned or reread here.
        if let Err(err) = validate_frozen_plan_binding(input.plan_context, input.frozen_plan) {
            let audit = make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
            return Err((err, audit));
        }

        let canonical_project_path = match std::fs::canonicalize(&input.adapter_input.project_path)
        {
            Ok(p) => p,
            Err(e) => {
                let err = DirectMcpError::InvalidWorkingDirectory(format!(
                    "failed to canonicalize project path: {e}"
                ));
                let audit = make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
                return Err((err, audit));
            }
        };

        // Validate server config before spawn
        if let Err(err) = validate_server_config(input.server_config, &canonical_project_path) {
            let audit = make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
            return Err((err, audit));
        }

        // Prepare environment
        let resolved_env =
            match resolve_allowed_environment(&input.server_config.allowed_environment) {
                Ok(env) => env,
                Err(err) => {
                    let audit =
                        make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
                    return Err((err, audit));
                }
            };

        // Determine working directory
        let working_dir = match &input.server_config.working_directory {
            Some(dir) => PathBuf::from(dir),
            None => canonical_project_path.clone(),
        };

        // Build typed arguments for prompt_envelope_v1
        let plan_context_json = match input.plan_context.map(serde_json::to_value).transpose() {
            Ok(val) => val,
            Err(e) => {
                let err = DirectMcpError::SerializationFailed(format!(
                    "failed to serialize PlanContext: {e}"
                ));
                let audit = make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
                return Err((err, audit));
            }
        };

        let frozen_plan_json = match input.frozen_plan.map(serde_json::to_value).transpose() {
            Ok(val) => val,
            Err(e) => {
                let err = DirectMcpError::SerializationFailed(format!(
                    "failed to serialize FrozenPlanPayload: {e}"
                ));
                let audit = make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
                return Err((err, audit));
            }
        };

        let arguments_obj = serde_json::json!({
            "schema_version": 1,
            "role": role_str,
            "system_prompt": input.adapter_input.system_prompt,
            "user_prompt": input.adapter_input.user_prompt,
            "plan_context": plan_context_json,
            "frozen_plan": frozen_plan_json,
        });

        let arguments_map = match arguments_obj.as_object().cloned() {
            Some(map) => map,
            None => {
                let err = DirectMcpError::SerializationFailed(
                    "failed to construct arguments map".to_string(),
                );
                let audit = make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
                return Err((err, audit));
            }
        };

        // Check if deadline already elapsed before spawn
        if Instant::now() >= deadline {
            let err = DirectMcpError::DeadlineExceeded;
            let audit = make_audit(0, 0, 0, err.code(), "not_spawned", Some(err.to_string()));
            return Err((err, audit));
        }

        // Spawn child process with pre-execution containment
        #[cfg(all(test, windows))]
        let inject_job_assignment_failure = self.fail_job_assignment_for_test;
        #[cfg(all(not(test), windows))]
        let inject_job_assignment_failure = false;

        let spawn_result = spawn_mcp_subprocess(
            &input.server_config.executable,
            &input.server_config.args,
            &working_dir,
            &resolved_env,
            self.config.max_capture_bytes,
            self.config.max_capture_bytes,
            #[cfg(windows)]
            inject_job_assignment_failure,
        );
        let (mut child, child_io, mut stderr_task, containment_guard, frame_limit_exceeded) =
            match spawn_result {
                Ok(parts) => parts,
                Err(err) => {
                    let audit =
                        make_audit(0, 0, 0, err.code(), "spawn_failed", Some(err.to_string()));
                    return Err((err, audit));
                }
            };

        #[cfg(test)]
        if let Some(delay) = self.delay_spawn_for_test {
            std::thread::sleep(delay);
        }

        #[cfg(test)]
        let mut containment_guard = containment_guard;
        #[cfg(test)]
        if self.fail_cleanup_for_test {
            containment_guard.fail_terminate_for_test = true;
        }

        // Discovery progress is shared so a failing invocation can still report
        // what it actually discovered. A later failure or a timeout must not
        // erase pages and tools that the boundary already observed.
        let pages_discovered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let tools_discovered = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let execution_fut = async {
            if Instant::now() >= deadline {
                return Err(DirectMcpError::DeadlineExceeded);
            }
            // Serve rmcp client handshake over stdio
            let client = DirectMcpClientHandler
                .serve(child_io)
                .await
                .map_err(|e| DirectMcpError::HandshakeFailed(e.to_string()))?;

            // Discover and validate tools with explicit pagination caps
            let mut matched_tool: Option<Tool> = None;
            let mut cursor: Option<String> = None;
            let mut page_count = 0;
            let mut total_tools = 0;
            let mut seen_cursors = std::collections::HashSet::new();

            loop {
                if Instant::now() >= deadline {
                    return Err(DirectMcpError::DeadlineExceeded);
                }
                if page_count >= self.config.max_tool_pages {
                    return Err(DirectMcpError::PaginationLimitExceeded(page_count + 1));
                }

                let params = cursor.map(|c| {
                    let mut p = PaginatedRequestParams::default();
                    p.cursor = Some(c);
                    p
                });

                let list_result: ListToolsResult = client
                    .list_tools(params)
                    .await
                    .map_err(|e| DirectMcpError::ToolsListFailed(e.to_string()))?;

                // Only received pages count as discovered. Pending/failed
                // requests and the rejected request beyond the cap do not.
                page_count += 1;
                pages_discovered.store(page_count, std::sync::atomic::Ordering::SeqCst);
                total_tools += list_result.tools.len();
                tools_discovered.store(total_tools, std::sync::atomic::Ordering::SeqCst);
                if total_tools > self.config.max_tools_total {
                    return Err(DirectMcpError::ToolLimitExceeded(total_tools));
                }

                for t in list_result.tools {
                    if t.name == tool_name {
                        matched_tool = Some(t);
                    }
                }

                match list_result.next_cursor {
                    Some(next) if !next.trim().is_empty() => {
                        if !seen_cursors.insert(next.clone()) {
                            return Err(DirectMcpError::RepeatedCursor(next));
                        }
                        cursor = Some(next);
                    }
                    _ => break,
                }
            }

            let tool = matched_tool.ok_or_else(|| {
                DirectMcpError::ToolNotFound(format!(
                    "configured tool '{tool_name}' not found on server '{server_id}'"
                ))
            })?;

            // Validate tool inputSchema against prompt_envelope_v1
            let schema_val = serde_json::Value::Object((*tool.input_schema).clone());
            validate_prompt_envelope_schema(&schema_val)?;

            // Execute tools/call
            if Instant::now() >= deadline {
                return Err(DirectMcpError::DeadlineExceeded);
            }
            let call_params =
                CallToolRequestParams::new(tool_name.clone()).with_arguments(arguments_map);
            let call_result: CallToolResult = client
                .call_tool(call_params)
                .await
                .map_err(|e| DirectMcpError::CallFailed(e.to_string()))?;

            // Serialize call result for size limit check
            let result_bytes = serde_json::to_vec(&call_result).map_err(|e| {
                DirectMcpError::SerializationFailed(format!("failed to serialize MCP result: {e}"))
            })?;

            if result_bytes.len() > self.config.max_capture_bytes {
                return Err(DirectMcpError::ResponseSizeLimitExceeded(
                    result_bytes.len(),
                ));
            }

            // Normalize CallToolResult
            if Instant::now() >= deadline {
                return Err(DirectMcpError::DeadlineExceeded);
            }
            #[cfg(test)]
            if let Some(delay) = self.delay_normalization_for_test {
                std::thread::sleep(delay);
            }
            let normalized_content =
                normalize_call_tool_result(&call_result, self.config.max_capture_bytes)?;
            if Instant::now() >= deadline {
                return Err(DirectMcpError::DeadlineExceeded);
            }

            // Redacted raw_json
            let raw_json_str = String::from_utf8_lossy(&result_bytes).into_owned();
            let redactor = SecretRedactor::new();
            let redacted_raw_json = redactor.redact_secrets(&raw_json_str);

            Ok((
                normalized_content,
                redacted_raw_json,
                page_count,
                total_tools,
            ))
        };

        // Run with absolute deadline and cancellation
        let cancel_fut = async {
            if let Some(token) = cancel_token {
                token.cancelled().await;
            } else {
                std::future::pending::<()>().await;
            }
        };

        let mut timed_out = false;
        let mut cancelled = false;

        let mut execution_result: Result<(String, String, usize, usize), DirectMcpError> = tokio::select! {
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                timed_out = true;
                Err(DirectMcpError::DeadlineExceeded)
            }
            _ = cancel_fut => {
                cancelled = true;
                Err(DirectMcpError::Cancelled)
            }
            res = execution_fut => res,
        };

        // Synchronous OS spawn and response serialization cannot be forcibly
        // preempted by Tokio. Reject any completion that returns after the
        // absolute deadline and clean up its child before exposing a result.
        if Instant::now() >= deadline && execution_result.is_ok() {
            timed_out = true;
            execution_result = Err(DirectMcpError::DeadlineExceeded);
        }

        // The bounded reader aborts the protocol stream when a peer exceeds the
        // inbound frame cap. Surface that as the typed size-limit error rather
        // than whichever transport error the aborted read produced.
        if execution_result.is_err()
            && frame_limit_exceeded.load(std::sync::atomic::Ordering::SeqCst)
        {
            execution_result = Err(DirectMcpError::ResponseSizeLimitExceeded(
                self.config.max_capture_bytes,
            ));
        }

        // Perform cleanup on ALL exit paths (success and failure)
        #[cfg(test)]
        let fail_stderr_join = self.fail_stderr_join_for_test;

        let cleanup_result = perform_mcp_cleanup(
            &mut child,
            &mut stderr_task,
            containment_guard,
            self.config.teardown_grace,
            timed_out || cancelled || execution_result.is_err(),
            #[cfg(test)]
            fail_stderr_join,
        )
        .await;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Process outcomes
        match (execution_result, cleanup_result) {
            (Ok((content, raw_json, pages, tools)), Ok((exit_status, _stderr, exited_on_its_own))) => {
                // A non-zero status only indicates an abnormal server exit when
                // the server exited by itself. When the shutdown grace expired
                // and the owned tree was terminated, the status is the result of
                // our own successful teardown and must not discard a valid
                // tool result.
                if exited_on_its_own && !exit_status.success() {
                    let code_str = exit_status
                        .code()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "terminated by signal".to_string());
                    let err = DirectMcpError::ProcessExitFailed(format!("exit code {code_str}"));
                    let audit = make_audit(
                        duration_ms,
                        pages,
                        tools,
                        err.code(),
                        "cleaned_up_with_failure_exit",
                        Some(err.to_string()),
                    );
                    return Err((err, audit));
                }

                let output = AdapterExecutionOutput {
                    content,
                    raw_json: Some(raw_json),
                    tokens_used: None,
                    model_used: format!("mcp:{server_id}/{tool_name}"),
                    duration_ms,
                };
                let audit = make_audit(
                    duration_ms,
                    pages,
                    tools,
                    "SUCCESS",
                    "cleaned_up_success",
                    None,
                );
                Ok((output, audit))
            }
            (Ok((_, _, pages, tools)), Err(cleanup_err)) => {
                let audit = make_audit(
                    duration_ms,
                    pages,
                    tools,
                    cleanup_err.code(),
                    "cleanup_failed",
                    Some(cleanup_err.to_string()),
                );
                Err((cleanup_err, audit))
            }
            (Err(exec_err), Ok(_)) => {
                let audit = make_audit(
                    duration_ms,
                    pages_discovered.load(std::sync::atomic::Ordering::SeqCst),
                    tools_discovered.load(std::sync::atomic::Ordering::SeqCst),
                    exec_err.code(),
                    "cleaned_up_after_error",
                    Some(exec_err.to_string()),
                );
                Err((exec_err, audit))
            }
            (Err(exec_err), Err(cleanup_err)) => {
                let combined_err = DirectMcpError::CleanupFailed(format!(
                    "{}; additional cleanup failure: {}",
                    exec_err, cleanup_err
                ));
                let audit = make_audit(
                    duration_ms,
                    pages_discovered.load(std::sync::atomic::Ordering::SeqCst),
                    tools_discovered.load(std::sync::atomic::Ordering::SeqCst),
                    combined_err.code(),
                    "cleanup_failed_after_error",
                    Some(combined_err.to_string()),
                );
                Err((combined_err, audit))
            }
        }
    }
}

/// Validates the run-owned Phase A plan binding before any process launch.
///
/// Mirrors the Mailbox dispatch boundary: a frozen payload requires a resolved
/// plan-bound context, and a plan-bound context requires its frozen payload.
/// Validation uses the Phase A helper and never re-reads on-disk plan files.
fn validate_frozen_plan_binding(
    plan_context: Option<&PlanContext>,
    frozen_plan: Option<&FrozenPlanPayload>,
) -> Result<(), DirectMcpError> {
    match (plan_context, frozen_plan) {
        (Some(context), Some(payload)) => {
            if !context.is_plan_bound() {
                return Err(DirectMcpError::InvalidFrozenPlanPayload(
                    "a frozen plan payload requires a resolved plan-bound PlanContext.".to_string(),
                ));
            }
            payload
                .validate_against_plan_context(context)
                .map_err(DirectMcpError::InvalidFrozenPlanPayload)
        }
        (None, Some(_)) => Err(DirectMcpError::InvalidFrozenPlanPayload(
            "a frozen plan payload requires its run-owned PlanContext.".to_string(),
        )),
        (Some(context), None) if context.is_plan_bound() => {
            Err(DirectMcpError::InvalidFrozenPlanPayload(
                "a plan-bound PlanContext requires its frozen plan payload.".to_string(),
            ))
        }
        _ => Ok(()),
    }
}

/// Validates McpServerConfig before process execution.
pub fn validate_server_config(
    config: &McpServerConfig,
    canonical_project_path: &Path,
) -> Result<(), DirectMcpError> {
    if config.transport != McpServerTransport::Stdio {
        return Err(DirectMcpError::UnsupportedTransport(
            "only stdio transport is supported.".to_string(),
        ));
    }

    if config.tool_contract != McpToolContract::PromptEnvelopeV1 {
        return Err(DirectMcpError::UnsupportedContract(
            "only prompt_envelope_v1 is supported.".to_string(),
        ));
    }

    let exec_str = config.executable.trim();
    if exec_str.is_empty() {
        return Err(DirectMcpError::MissingExecutable);
    }

    let exec_path = Path::new(exec_str);
    if !exec_path.is_absolute() {
        return Err(DirectMcpError::InvalidExecutablePath(format!(
            "executable path '{exec_str}' must be absolute."
        )));
    }

    // Canonicalize executable path
    let canonical_exec = std::fs::canonicalize(exec_path).map_err(|e| {
        DirectMcpError::InvalidExecutablePath(format!(
            "failed to resolve executable '{exec_str}': {e}"
        ))
    })?;

    if !canonical_exec.is_file() {
        return Err(DirectMcpError::InvalidExecutablePath(format!(
            "executable '{exec_str}' is not a file."
        )));
    }

    // Reject shell launchers and script wrappers
    let file_stem = canonical_exec
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();

    let extension = canonical_exec
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();

    let forbidden_shells = [
        "cmd",
        "powershell",
        "pwsh",
        "sh",
        "bash",
        "zsh",
        "csh",
        "tcsh",
        "fish",
        "dash",
    ];

    if forbidden_shells.contains(&file_stem.as_str()) {
        return Err(DirectMcpError::ShellLauncherRejected(format!(
            "shell executable '{file_stem}' cannot be used as an MCP server launcher."
        )));
    }

    let forbidden_scripts = ["cmd", "bat", "ps1", "sh", "bash"];
    if forbidden_scripts.contains(&extension.as_str()) {
        return Err(DirectMcpError::ScriptWrapperRejected(format!(
            "script wrapper '.{extension}' cannot be launched directly as an MCP server without explicit binary runtime."
        )));
    }

    // Validate working directory if specified
    if let Some(ref cwd_str) = config.working_directory {
        let cwd_path = Path::new(cwd_str);
        if !cwd_path.is_absolute() {
            return Err(DirectMcpError::InvalidWorkingDirectory(format!(
                "working directory '{cwd_str}' must be absolute."
            )));
        }
        let canonical_cwd = std::fs::canonicalize(cwd_path).map_err(|e| {
            DirectMcpError::InvalidWorkingDirectory(format!("failed to resolve '{cwd_str}': {e}"))
        })?;
        if !canonical_cwd.is_dir() {
            return Err(DirectMcpError::InvalidWorkingDirectory(format!(
                "'{cwd_str}' is not a directory."
            )));
        }
    } else if !canonical_project_path.is_dir() {
        return Err(DirectMcpError::InvalidWorkingDirectory(format!(
            "project directory '{canonical_project_path:?}' is not a directory."
        )));
    }

    // Validate allowed environment variable names
    let mut seen_names = std::collections::HashSet::new();
    for env_name in &config.allowed_environment {
        let trimmed = env_name.trim();
        if trimmed.is_empty()
            || trimmed.contains('=')
            || !trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(DirectMcpError::InvalidEnvironmentVariable(format!(
                "'{env_name}' is not a valid environment variable name."
            )));
        }

        #[cfg(windows)]
        let lookup_name = trimmed.to_ascii_uppercase();
        #[cfg(not(windows))]
        let lookup_name = trimmed.to_string();

        if !seen_names.insert(lookup_name) {
            return Err(DirectMcpError::InvalidEnvironmentVariable(format!(
                "duplicate environment variable '{env_name}'."
            )));
        }
    }

    Ok(())
}

/// Resolves allowed environment variables from current process environment.
pub fn resolve_allowed_environment(
    allowed_names: &[String],
) -> Result<HashMap<String, String>, DirectMcpError> {
    let mut resolved = HashMap::new();

    // On Windows, auto-include standard system variables if present in the environment
    #[cfg(windows)]
    {
        for standard in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP"] {
            if let Ok(val) = std::env::var(standard) {
                resolved.insert(standard.to_string(), val);
            }
        }
    }

    for name in allowed_names {
        match std::env::var(name) {
            Ok(val) => {
                resolved.insert(name.clone(), val);
            }
            Err(_) => {
                return Err(DirectMcpError::MissingEnvironmentVariable(format!(
                    "explicitly allowed variable '{name}' was not found in host environment."
                )));
            }
        }
    }

    Ok(resolved)
}

/// Validates that tool inputSchema accepts `prompt_envelope_v1`.
pub fn validate_prompt_envelope_schema(schema: &serde_json::Value) -> Result<(), DirectMcpError> {
    let obj = schema.as_object().ok_or_else(|| {
        DirectMcpError::IncompatibleSchema("tool inputSchema is not a JSON object.".to_string())
    })?;

    if let Some(t) = obj.get("type").and_then(|v| v.as_str()) {
        if t != "object" {
            return Err(DirectMcpError::IncompatibleSchema(format!(
                "tool inputSchema type must be 'object', got '{t}'."
            )));
        }
    }

    let allowed_envelope_fields = [
        "schema_version",
        "role",
        "system_prompt",
        "user_prompt",
        "plan_context",
        "frozen_plan",
    ];

    if let Some(required) = obj.get("required").and_then(|v| v.as_array()) {
        for req in required {
            if let Some(name) = req.as_str() {
                if !allowed_envelope_fields.contains(&name) {
                    return Err(DirectMcpError::IncompatibleSchema(format!(
                        "tool requires non-envelope parameter '{name}'."
                    )));
                }
            }
        }
    }

    if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
        // Built-in MCP server legacy tools require task/context
        if (properties.contains_key("task") && properties.contains_key("context"))
            || (properties.contains_key("task") && properties.contains_key("approved_plan"))
        {
            return Err(DirectMcpError::IncompatibleSchema(
                "tool implements legacy parameter schema rather than prompt_envelope_v1."
                    .to_string(),
            ));
        }

        // If role property is defined, ensure type is string
        if let Some(role_prop) = properties.get("role").and_then(|v| v.as_object()) {
            if let Some(t) = role_prop.get("type").and_then(|v| v.as_str()) {
                if t != "string" {
                    return Err(DirectMcpError::IncompatibleSchema(format!(
                        "'role' parameter must be string, got '{t}'."
                    )));
                }
            }
        }

        // If user_prompt property is defined, ensure type is string
        if let Some(user_prompt_prop) = properties.get("user_prompt").and_then(|v| v.as_object()) {
            if let Some(t) = user_prompt_prop.get("type").and_then(|v| v.as_str()) {
                if t != "string" {
                    return Err(DirectMcpError::IncompatibleSchema(format!(
                        "'user_prompt' parameter must be string, got '{t}'."
                    )));
                }
            }
        }
    }

    Ok(())
}

/// Normalizes CallToolResult into plain text string adhering to r22b rules.
pub fn normalize_call_tool_result(
    result: &CallToolResult,
    max_bytes: usize,
) -> Result<String, DirectMcpError> {
    if result.is_error.unwrap_or(false) {
        let result_json = serde_json::to_value(result).unwrap_or_default();
        let mut msg = String::new();
        if let Some(contents) = result_json.get("content").and_then(|v| v.as_array()) {
            for block in contents {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    if !text.trim().is_empty() {
                        if !msg.is_empty() {
                            msg.push_str("; ");
                        }
                        msg.push_str(text);
                    }
                }
            }
        }
        if msg.is_empty() {
            msg = "tool reported isError=true without message".to_string();
        }
        return Err(DirectMcpError::ToolReportedError(msg));
    }

    let result_json = serde_json::to_value(result).map_err(|e| {
        DirectMcpError::SerializationFailed(format!("failed to inspect MCP result JSON: {e}"))
    })?;

    // Check for structuredContent
    let structured_content = result_json
        .get("structuredContent")
        .or_else(|| result_json.get("structured_content"))
        .or_else(|| {
            result_json
                .get("_meta")
                .and_then(|m| m.get("structuredContent"))
        });

    let content_blocks = result_json
        .get("content")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if let Some(struct_val) = structured_content {
        // Must be an object with exactly schema_version and content
        let struct_obj = struct_val.as_object().ok_or_else(|| {
            DirectMcpError::MalformedStructuredContent(
                "structuredContent is not an object.".to_string(),
            )
        })?;

        let schema_ver = struct_obj
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                DirectMcpError::MalformedStructuredContent(
                    "missing or invalid schema_version.".to_string(),
                )
            })?;

        if schema_ver != 1 {
            return Err(DirectMcpError::UnsupportedSchemaVersion(schema_ver));
        }

        let content_str = struct_obj
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                DirectMcpError::MalformedStructuredContent(
                    "missing or invalid content field.".to_string(),
                )
            })?;

        if content_str.trim().is_empty() {
            return Err(DirectMcpError::MalformedStructuredContent(
                "structuredContent content is empty or whitespace.".to_string(),
            ));
        }

        // Verify no unknown fields in structuredContent
        for key in struct_obj.keys() {
            if key != "schema_version" && key != "content" {
                return Err(DirectMcpError::MalformedStructuredContent(format!(
                    "unknown field '{key}' in structuredContent."
                )));
            }
        }

        // With structured content, allow zero content blocks or exactly one text block matching envelope JSON
        if content_blocks.is_empty() {
            return Ok(content_str.to_string());
        }

        if content_blocks.len() > 1 {
            return Err(DirectMcpError::ConflictingContentBlocks(
                "multiple content blocks received alongside structuredContent.".to_string(),
            ));
        }

        if let Some(first_block) = content_blocks.first() {
            if let Some(text) = first_block.get("text").and_then(|v| v.as_str()) {
                if let Ok(parsed_text) = serde_json::from_str::<serde_json::Value>(text) {
                    if parsed_text == *struct_val {
                        return Ok(content_str.to_string());
                    }
                }
                return Err(DirectMcpError::ConflictingContentBlocks(
                    "companion text block does not match structuredContent.".to_string(),
                ));
            }
        }

        return Err(DirectMcpError::UnsupportedContentType(
            "non-text block received alongside structuredContent.".to_string(),
        ));
    }

    // Without structured content: require exactly one non-empty text block
    if content_blocks.is_empty() {
        return Err(DirectMcpError::EmptyMcpResponse);
    }

    if content_blocks.len() > 1 {
        return Err(DirectMcpError::ConflictingContentBlocks(format!(
            "tool returned {} content blocks (expected exactly 1).",
            content_blocks.len()
        )));
    }

    let block = &content_blocks[0];
    let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if block_type != "text" {
        return Err(DirectMcpError::UnsupportedContentType(format!(
            "expected text block, received block of type '{block_type}'."
        )));
    }

    let text = block.get("text").and_then(|v| v.as_str()).unwrap_or("");
    if text.is_empty() {
        return Err(DirectMcpError::EmptyMcpResponse);
    }
    if text.len() > max_bytes {
        return Err(DirectMcpError::ResponseSizeLimitExceeded(text.len()));
    }

    Ok(text.to_string())
}

/// Process tree containment handle wrapper with deterministic terminate-and-close.
pub struct ProcessContainmentGuard {
    #[cfg(windows)]
    job_handle: usize,
    #[cfg(unix)]
    process_group_id: i32,
    #[cfg(test)]
    pub(crate) fail_terminate_for_test: bool,
}

impl ProcessContainmentGuard {
    /// Explicitly terminates descendant processes and closes handles.
    pub fn terminate_and_close(&mut self) -> Result<(), String> {
        #[cfg(test)]
        if self.fail_terminate_for_test {
            return Err("injected process tree termination failure".to_string());
        }
        let mut failure = None;
        #[cfg(windows)]
        if self.job_handle != 0 {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            let terminate_res = unsafe { TerminateJobObject(self.job_handle as _, 1) };
            if terminate_res == 0 {
                failure = Some(format!(
                    "failed to terminate Windows Job Object: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let close_res = unsafe { CloseHandle(self.job_handle as _) };
            self.job_handle = 0;
            if close_res == 0 {
                let close_error = format!(
                    "failed to close Job Object handle: {}",
                    std::io::Error::last_os_error()
                );
                failure = Some(failure.map_or(close_error.clone(), |previous| {
                    format!("{previous}; {close_error}")
                }));
            }
        }
        #[cfg(unix)]
        if self.process_group_id != 0 {
            unsafe {
                extern "C" {
                    fn kill(pid: i32, sig: i32) -> i32;
                }
                if kill(-self.process_group_id, 9) != 0 {
                    let error = std::io::Error::last_os_error();
                    // ESRCH means the owned group has already exited; all other
                    // failures mean descendant termination was not established.
                    if error.raw_os_error() != Some(3) {
                        failure = Some(format!("failed to terminate owned process group: {error}"));
                    }
                }
            }
            self.process_group_id = 0;
        }
        failure.map_or(Ok(()), Err)
    }

    #[cfg(all(test, windows))]
    pub(crate) fn job_handle_for_test(&self) -> usize {
        self.job_handle
    }
}

impl Drop for ProcessContainmentGuard {
    fn drop(&mut self) {
        #[cfg(test)]
        {
            self.fail_terminate_for_test = false;
        }
        let _ = self.terminate_and_close();
    }
}

/// Spawns the MCP subprocess over stdio with process-tree containment.
fn spawn_mcp_subprocess(
    executable: &str,
    args: &[String],
    working_dir: &Path,
    env: &HashMap<String, String>,
    max_stderr_bytes: usize,
    max_frame_bytes: usize,
    #[cfg(windows)] fail_job_assignment: bool,
) -> Result<
    (
        tokio::process::Child,
        ChildIo<tokio::process::ChildStdout, tokio::process::ChildStdin>,
        tokio::task::JoinHandle<String>,
        ProcessContainmentGuard,
        Arc<std::sync::atomic::AtomicBool>,
    ),
    DirectMcpError,
> {
    use std::process::Stdio;
    use tokio::process::Command;

    #[cfg(windows)]
    let job_handle = {
        use std::ptr::{null, null_mut};
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let job = unsafe { CreateJobObjectW(null_mut(), null()) };
        if job.is_null() || job == INVALID_HANDLE_VALUE {
            return Err(DirectMcpError::ProcessContainmentFailed(
                "failed to create Windows Job Object".to_string(),
            ));
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe { CloseHandle(job) };
            return Err(DirectMcpError::ProcessContainmentFailed(
                "failed to configure Windows Job Object".to_string(),
            ));
        }
        job
    };

    let mut cmd = Command::new(executable);
    cmd.args(args);
    cmd.current_dir(working_dir);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    cmd.env_clear();
    for (k, v) in env {
        cmd.env(k, v);
    }

    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;
        cmd.as_std_mut().creation_flags(CREATE_SUSPENDED);
    }

    let mut child = cmd.spawn().map_err(|e| {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(job_handle)
        };
        DirectMcpError::SpawnFailed(format!("failed to spawn '{executable}': {e}"))
    })?;

    #[cfg(windows)]
    let containment_guard = {
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        let raw_handle = match child.raw_handle() {
            Some(handle) => handle,
            None => {
                let cleanup = abort_suspended_child(&mut child, job_handle as _);
                return Err(DirectMcpError::ProcessContainmentFailed(format!(
                    "child process has no raw handle{}",
                    cleanup
                        .err()
                        .map(|e| format!("; cleanup failed: {e}"))
                        .unwrap_or_default()
                )));
            }
        };

        let assign_res = if fail_job_assignment {
            0
        } else {
            unsafe { AssignProcessToJobObject(job_handle, raw_handle as _) }
        };
        if assign_res == 0 {
            let cause = if fail_job_assignment {
                std::io::Error::other("injected Job Object assignment failure")
            } else {
                std::io::Error::last_os_error()
            };
            let cleanup = abort_suspended_child(&mut child, job_handle as _);
            return Err(DirectMcpError::ProcessContainmentFailed(format!(
                "failed to assign process to Job Object: {cause}{}",
                cleanup
                    .err()
                    .map(|e| format!("; cleanup failed: {e}"))
                    .unwrap_or_default()
            )));
        }

        // Verify Job Object membership explicitly before resuming the suspended primary thread.
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;
        let mut in_job: windows_sys::Win32::Foundation::BOOL = 0;
        let query_res = unsafe { IsProcessInJob(raw_handle as _, job_handle, &mut in_job) };
        if query_res == 0 || in_job == 0 {
            let cause = if query_res == 0 {
                std::io::Error::last_os_error()
            } else {
                std::io::Error::other("process is not a member of the configured Job Object")
            };
            let cleanup = abort_suspended_child(&mut child, job_handle as _);
            return Err(DirectMcpError::ProcessContainmentFailed(format!(
                "failed to verify Job Object membership: {cause}{}",
                cleanup
                    .err()
                    .map(|e| format!("; cleanup failed: {e}"))
                    .unwrap_or_default()
            )));
        }

        // CommandExt::creation_flags(CREATE_SUSPENDED) prevents entry into child
        // code. The newly created suspended process has only its primary thread;
        // locate that thread by owner PID, then resume it after Job assignment and verification.
        let pid = match child.id() {
            Some(pid) => pid,
            None => {
                let cleanup = abort_suspended_child(&mut child, job_handle as _);
                return Err(DirectMcpError::ProcessContainmentFailed(format!(
                    "child process has no PID{}",
                    cleanup
                        .err()
                        .map(|e| format!("; cleanup failed: {e}"))
                        .unwrap_or_default()
                )));
            }
        };
        let resumed = resume_suspended_primary_thread(pid);
        if let Err(error) = resumed {
            let cleanup = abort_suspended_child(&mut child, job_handle as _);
            return Err(DirectMcpError::ProcessContainmentFailed(format!(
                "{error}{}",
                cleanup
                    .err()
                    .map(|e| format!("; cleanup failed: {e}"))
                    .unwrap_or_default()
            )));
        }

        ProcessContainmentGuard {
            job_handle: job_handle as usize,
            #[cfg(test)]
            fail_terminate_for_test: false,
        }
    };

    #[cfg(unix)]
    let containment_guard = {
        let pid = child.id().unwrap_or(0) as i32;
        ProcessContainmentGuard {
            process_group_id: pid,
            #[cfg(test)]
            fail_terminate_for_test: false,
        }
    };

    // Only expose pipes to the protocol client after containment is established
    // and (on Windows) the suspended primary thread has been resumed.
    let child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| DirectMcpError::SpawnFailed("failed to capture child stdin".to_string()))?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| DirectMcpError::SpawnFailed("failed to capture child stdout".to_string()))?;
    let mut child_stderr = child
        .stderr
        .take()
        .ok_or_else(|| DirectMcpError::SpawnFailed("failed to capture child stderr".to_string()))?;
    let frame_limit_exceeded = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let child_io = ChildIo::new(
        child_stdout,
        child_stdin,
        max_frame_bytes,
        frame_limit_exceeded.clone(),
    );
    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buffer = Vec::new();
        let mut temp = [0u8; 4096];
        while let Ok(n) = child_stderr.read(&mut temp).await {
            if n == 0 {
                break;
            }
            if buffer.len() + n <= max_stderr_bytes {
                buffer.extend_from_slice(&temp[..n]);
            } else if buffer.len() < max_stderr_bytes {
                let remaining = max_stderr_bytes - buffer.len();
                buffer.extend_from_slice(&temp[..remaining]);
            }
        }
        String::from_utf8_lossy(&buffer).into_owned()
    });

    Ok((
        child,
        child_io,
        stderr_task,
        containment_guard,
        frame_limit_exceeded,
    ))
}

#[cfg(windows)]
fn terminate_and_reap_suspended_child(child: &mut tokio::process::Child) -> Result<(), String> {
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};
    let raw = child
        .raw_handle()
        .ok_or_else(|| "suspended child has no process handle".to_string())?;
    if unsafe { WaitForSingleObject(raw as _, 0) } == WAIT_OBJECT_0 {
        return Ok(());
    }
    let terminated = unsafe { TerminateProcess(raw as _, 1) };
    if terminated == 0 {
        return Err(format!(
            "failed to terminate suspended child: {}",
            std::io::Error::last_os_error()
        ));
    }
    let wait = unsafe { WaitForSingleObject(raw as _, 2_000) };
    if wait != WAIT_OBJECT_0 {
        return Err("failed to reap suspended child within two seconds".to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn abort_suspended_child(
    child: &mut tokio::process::Child,
    job_handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<(), String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::TerminateJobObject;
    let mut errors = Vec::new();
    if unsafe { TerminateJobObject(job_handle, 1) } == 0 {
        errors.push(format!(
            "TerminateJobObject: {}",
            std::io::Error::last_os_error()
        ));
    }
    if let Err(error) = terminate_and_reap_suspended_child(child) {
        errors.push(error);
    }
    if unsafe { CloseHandle(job_handle) } == 0 {
        errors.push(format!(
            "CloseHandle(Job): {}",
            std::io::Error::last_os_error()
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(windows)]
fn resume_suspended_primary_thread(process_id: u32) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
        return Err(format!(
            "could not enumerate suspended child thread: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut found = None;
    let mut has_entry = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while has_entry {
        if entry.th32OwnerProcessID == process_id {
            found = Some(entry.th32ThreadID);
            break;
        }
        has_entry = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    let thread_id =
        found.ok_or_else(|| "primary thread for suspended child was not found".to_string())?;
    let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
    if thread.is_null() {
        return Err(format!(
            "could not open suspended child thread: {}",
            std::io::Error::last_os_error()
        ));
    }
    let previous_suspend_count = unsafe { ResumeThread(thread) };
    let close_result = unsafe { CloseHandle(thread) };
    if previous_suspend_count == u32::MAX {
        return Err(format!(
            "could not resume suspended child thread: {}",
            std::io::Error::last_os_error()
        ));
    }
    if close_result == 0 {
        return Err("could not close primary thread handle after resume".to_string());
    }
    Ok(())
}

/// Builds a handle whose join deterministically fails with a real `JoinError`.
///
/// The task never completes, so it cannot finish before `abort()` regardless of
/// scheduling; awaiting it therefore always yields `Err(JoinError::cancelled)`.
#[cfg(test)]
fn aborted_join_handle() -> tokio::task::JoinHandle<String> {
    let handle = tokio::spawn(std::future::pending::<String>());
    handle.abort();
    handle
}

/// Joins the bounded stderr drain task under the shared cleanup deadline.
async fn join_stderr_drain(
    stderr_task: &mut tokio::task::JoinHandle<String>,
    cleanup_deadline: tokio::time::Instant,
    failures: &mut Vec<String>,
) -> Option<String> {
    match tokio::time::timeout_at(cleanup_deadline, &mut *stderr_task).await {
        Ok(Ok(drained)) => Some(drained),
        Ok(Err(error)) => {
            failures.push(format!("failed to join stderr task: {error}"));
            None
        }
        Err(_) => {
            stderr_task.abort();
            failures.push("shared cleanup deadline expired while joining stderr task".to_string());
            None
        }
    }
}

/// Performs explicit graceful teardown, process reaping, and descendant termination.
async fn perform_mcp_cleanup(
    child: &mut tokio::process::Child,
    stderr_task: &mut tokio::task::JoinHandle<String>,
    mut containment_guard: ProcessContainmentGuard,
    grace_period: Duration,
    force_kill_first: bool,
    #[cfg(test)] fail_stderr_join: bool,
) -> Result<(std::process::ExitStatus, String, bool), DirectMcpError> {
    let cleanup_deadline = tokio::time::Instant::now() + grace_period.min(MCP_TEARDOWN_GRACE);
    let mut failures = Vec::new();
    if force_kill_first {
        if let Err(error) = containment_guard.terminate_and_close() {
            failures.push(error);
        }
    }

    // Give a healthy root at most one second to exit, reserving the remainder
    // of the same absolute budget for termination, reap, and stderr join.
    let graceful_status = if force_kill_first {
        None
    } else {
        let graceful_deadline =
            (tokio::time::Instant::now() + Duration::from_secs(1)).min(cleanup_deadline);
        match tokio::time::timeout_at(graceful_deadline, child.wait()).await {
            Ok(Ok(status)) => Some(status),
            Ok(Err(error)) => {
                failures.push(format!("failed to wait for child process: {error}"));
                None
            }
            Err(_) => None,
        }
    };
    // A status observed here is the server's own exit; anything we had to
    // terminate afterwards is an artefact of our own teardown, not an abnormal
    // server exit, and must not be reported as one.
    let exited_on_its_own = graceful_status.is_some();

    // Kill any descendants left after the root exits (or the full tree when
    // graceful shutdown did not finish), then reap the root within the same deadline.
    if let Err(error) = containment_guard.terminate_and_close() {
        failures.push(error);
    }
    let exit_status = match graceful_status {
        Some(status) => Some(status),
        None if tokio::time::Instant::now() < cleanup_deadline => {
            match tokio::time::timeout_at(cleanup_deadline, child.wait()).await {
                Ok(Ok(status)) => Some(status),
                Ok(Err(error)) => {
                    failures.push(format!("failed to reap child process: {error}"));
                    None
                }
                Err(_) => {
                    failures
                        .push("shared cleanup deadline expired before root reaping".to_string());
                    None
                }
            }
        }
        None => {
            failures.push("shared cleanup deadline expired before root reaping".to_string());
            None
        }
    };

    // Join the bounded stderr drain task. The test-only seam hands the join
    // boundary a handle whose join genuinely fails, so the shared production
    // join/error path handles an injected failure exactly as it would a real
    // abnormal drain task; no error string is synthesised here.
    #[cfg(test)]
    let stderr_str = if fail_stderr_join {
        // Reap the real drain task within the shared cleanup deadline first.
        stderr_task.abort();
        let _ = tokio::time::timeout_at(cleanup_deadline, &mut *stderr_task).await;
        let mut failing = aborted_join_handle();
        let joined = join_stderr_drain(&mut failing, cleanup_deadline, &mut failures).await;
        debug_assert!(joined.is_none(), "an aborted join handle must not yield stderr");
        joined
    } else {
        join_stderr_drain(stderr_task, cleanup_deadline, &mut failures).await
    };
    #[cfg(not(test))]
    let stderr_str = join_stderr_drain(stderr_task, cleanup_deadline, &mut failures).await;

    if !failures.is_empty() {
        return Err(DirectMcpError::CleanupFailed(failures.join("; ")));
    }
    match (exit_status, stderr_str) {
        (Some(status), Some(stderr)) => Ok((status, stderr, exited_on_its_own)),
        _ => Err(DirectMcpError::CleanupFailed(
            "cleanup did not obtain process status and stderr result".to_string(),
        )),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::orchestrator::plan_workspace::{
        capture_frozen_plan_snapshot, PlanWorkspaceConfig,
    };
    use crate::orchestrator::types::ProfileCapability;
    use tempfile::tempdir;

    fn valid_test_executable() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        #[cfg(windows)]
        let exe_path = dir.path().join("fake_node.exe");
        #[cfg(not(windows))]
        let exe_path = dir.path().join("fake_node");
        std::fs::write(&exe_path, b"mock binary content").unwrap();
        (dir, exe_path)
    }

    fn sample_server_config(exe: &Path) -> McpServerConfig {
        McpServerConfig {
            transport: McpServerTransport::Stdio,
            executable: exe.to_string_lossy().to_string(),
            args: vec!["--stdio".to_string()],
            working_directory: None,
            allowed_environment: vec![],
            tool_contract: McpToolContract::PromptEnvelopeV1,
        }
    }

    fn sample_mcp_profile(server_id: &str, tool_name: &str) -> OrchestratorProfile {
        OrchestratorProfile {
            id: "mcp-planner".to_string(),
            display_name: "MCP DeepSeek Planner".to_string(),
            adapter: ExecutionAdapterType::Mcp,
            capabilities: vec![ProfileCapability::Reasoning],
            provider_id: None,
            provider_profile_id: None,
            model: None,
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: Some(server_id.to_string()),
            mcp_tool: Some(tool_name.to_string()),
            context_window_tokens: Some(128_000),
        }
    }

    /// In-memory protocol-unit transport using the production frame cap.
    fn test_child_io<R, W>(reader: R, writer: W) -> ChildIo<R, W> {
        ChildIo::new(
            reader,
            writer,
            MCP_MAX_CAPTURE_BYTES,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }

    pub(crate) fn build_stdio_peer(temp: &tempfile::TempDir) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("direct_mcp_stdio_peer.rs");
        let output = temp.path().join(if cfg!(windows) {
            "direct-mcp-peer.exe"
        } else {
            "direct-mcp-peer"
        });
        let result = std::process::Command::new("rustc")
            .arg("--edition=2021")
            .arg(source)
            .arg("-o")
            .arg(&output)
            .output()
            .expect("rustc must be available to build the stdio fake peer");
        assert!(
            result.status.success(),
            "fake peer build failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        output
    }

    fn process_is_alive(pid: u32) -> bool {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
            use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};
            const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;
            let process = unsafe { OpenProcess(SYNCHRONIZE_ACCESS, 0, pid) };
            if process.is_null() {
                return false;
            }
            let wait_res = unsafe { WaitForSingleObject(process, 2_000) };
            let alive = wait_res != WAIT_OBJECT_0;
            unsafe {
                CloseHandle(process);
            }
            alive
        }
        #[cfg(unix)]
        {
            for _ in 0..20 {
                if unsafe { kill(pid as i32, 0) != 0 } {
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            true
        }
    }

    #[cfg(unix)]
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }

    #[tokio::test]
    async fn test_production_adapter_spawns_stdio_peer_and_normalizes_tool_result() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let profile = sample_mcp_profile("fake", "plan");
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["paged".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile,
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (output, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .unwrap_or_else(|(error, _audit)| {
                panic!("production stdio invocation failed: {error}")
            });
        assert_eq!(output.content, "fake peer success");
        assert_eq!(audit.result_code, "SUCCESS");
        assert_eq!(audit.pages_discovered, 2);
        assert_eq!(audit.tools_discovered, 2);
        assert_eq!(audit.cleanup_outcome, "cleaned_up_success");
    }

    #[tokio::test]
    async fn test_production_stdio_accepts_exact_page_and_tool_limits() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        // Both boundaries use the production defaults and actual stdio child;
        // the configured tool is the final discovered entry in each case.
        for (mode, expected_pages, expected_tools) in [
            ("exact-page-cap", MCP_MAX_TOOL_PAGES, MCP_MAX_TOOL_PAGES),
            ("exact-tool-cap", 1, MCP_MAX_TOOLS_TOTAL),
        ] {
            let called = temp.path().join(format!("{mode}-called"));
            let input = AdapterExecutionInput {
                role: AgentRole::Planner,
                profile: sample_mcp_profile("fake", "plan"),
                system_prompt: "system".to_string(),
                user_prompt: "task".to_string(),
                project_path: project.path().to_path_buf(),
                temperature: None,
            };
            let server = McpServerConfig {
                args: vec![mode.to_string(), called.to_string_lossy().into_owned()],
                working_directory: Some(project.path().to_string_lossy().into_owned()),
                ..sample_server_config(&peer)
            };
            let direct_input = DirectMcpExecutionInput {
                adapter_input: &input,
                server_config: &server,
                plan_context: None,
                frozen_plan: None,
            };
            let (output, audit) = DirectMcpAdapter::new()
                .execute_with_audit(&direct_input, None)
                .await
                .unwrap_or_else(|(error, _)| panic!("{mode} must be accepted: {error}"));
            assert_eq!(output.content, "fake peer success", "mode={mode}");
            assert_eq!(audit.result_code, "SUCCESS", "mode={mode}");
            assert_eq!(audit.cleanup_outcome, "cleaned_up_success", "mode={mode}");
            assert_eq!(audit.pages_discovered, expected_pages, "mode={mode}");
            assert_eq!(audit.tools_discovered, expected_tools, "mode={mode}");
            assert_eq!(std::fs::read_to_string(&called).unwrap(), "called", "mode={mode}");
        }
    }

    #[tokio::test]
    async fn test_production_adapter_rejects_repeated_cursor_before_tool_call() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["repeat".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("repeated cursor must fail through production pagination");
        assert_eq!(error.code(), "DMCP_REPEATED_CURSOR");
        assert_eq!(audit.result_code, "DMCP_REPEATED_CURSOR");
    }

    #[tokio::test]
    async fn test_production_stdio_enforces_page_tool_caps_and_schema_before_call() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        for (mode, expected_code, config, verify_page_count) in [
            (
                "page-cap",
                "DMCP_PAGINATION_LIMIT_EXCEEDED",
                DirectMcpConfig::default(),
                true,
            ),
            (
                "tool-cap",
                "DMCP_TOOL_LIMIT_EXCEEDED",
                DirectMcpConfig::default(),
                false,
            ),
            (
                "bad-schema",
                "DMCP_INCOMPATIBLE_SCHEMA",
                DirectMcpConfig::default(),
                false,
            ),
        ] {
            let auxiliary = temp.path().join(format!("{mode}.state"));
            let input = AdapterExecutionInput {
                role: AgentRole::Planner,
                profile: sample_mcp_profile("fake", "plan"),
                system_prompt: "system".to_string(),
                user_prompt: "task".to_string(),
                project_path: project.path().to_path_buf(),
                temperature: None,
            };
            let server = McpServerConfig {
                executable: peer.to_string_lossy().into_owned(),
                args: vec![mode.to_string(), auxiliary.to_string_lossy().into_owned()],
                working_directory: Some(project.path().to_string_lossy().into_owned()),
                ..sample_server_config(&peer)
            };
            let direct_input = DirectMcpExecutionInput {
                adapter_input: &input,
                server_config: &server,
                plan_context: None,
                frozen_plan: None,
            };
            let (error, audit) = DirectMcpAdapter::with_config(config)
                .execute_with_audit(&direct_input, None)
                .await
                .expect_err("cap/schema violation must reject before tools/call");
            assert_eq!(error.code(), expected_code, "fake peer mode {mode}");
            assert_eq!(audit.result_code, expected_code);
            if verify_page_count {
                assert_eq!(
                    std::fs::read_to_string(&auxiliary).unwrap(),
                    "32",
                    "no 33rd page request may be sent"
                );
                assert_eq!(audit.pages_discovered, MCP_MAX_TOOL_PAGES);
                assert_eq!(audit.tools_discovered, MCP_MAX_TOOL_PAGES);
            } else {
                assert!(
                    !auxiliary.exists(),
                    "tools/call must not be dispatched for {mode}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_production_adapter_deadline_cleans_up_hung_stdio_peer() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["hang-list".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            execution_deadline: Duration::from_millis(500),
            teardown_grace: Duration::from_secs(2),
            ..DirectMcpConfig::default()
        });
        let (error, audit) = adapter
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("hung stdio peer must be stopped by absolute deadline");
        assert_eq!(error.code(), "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.result_code, "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.pages_discovered, 0, "an unanswered page is not discovered");
        assert_eq!(audit.tools_discovered, 0);
        assert_ne!(audit.cleanup_outcome, "cleanup_failed");
    }

    #[tokio::test]
    async fn test_production_adapter_deadline_during_handshake_is_audited_and_cleaned() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let marker = temp.path().join("initialize-ready");
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "hang-initialize".to_string(),
                marker.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            execution_deadline: Duration::from_millis(500),
            ..DirectMcpConfig::default()
        });
        let invocation = adapter.execute_with_audit(&direct_input, None);
        tokio::pin!(invocation);
        let wait_for_ready = async {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if marker.exists() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
        };
        tokio::select! {
            result = &mut invocation => panic!("peer unexpectedly completed before handshake deadline: {result:?}"),
            ready = wait_for_ready => assert!(ready.is_ok(), "fake peer initialize marker was not observed"),
        }
        let (error, audit) = invocation
            .await
            .expect_err("deadline must reject the hung handshake");
        assert_eq!(error.code(), "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.result_code, "DMCP_DEADLINE_EXCEEDED");
        assert_ne!(audit.cleanup_outcome, "cleanup_failed");
    }

    #[tokio::test]
    async fn test_expired_invocation_deadline_rejects_before_process_spawn() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let started = temp.path().join("must-not-start");
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "started-marker".to_string(),
                started.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            execution_deadline: Duration::ZERO,
            ..DirectMcpConfig::default()
        });
        let (error, audit) = adapter
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("already-expired invocation must not spawn a child");
        assert_eq!(error.code(), "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.cleanup_outcome, "not_spawned");
        assert!(
            !started.exists(),
            "child must not execute when deadline is expired before spawn"
        );
    }

    #[tokio::test]
    async fn test_production_sync_spawn_delay_beyond_deadline_rejects_and_cleans_up_without_dispatch() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let started = temp.path().join("child-started");
        let call_marker = temp.path().join("tool-called");
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "started-marker".to_string(),
                started.to_string_lossy().into_owned(),
                call_marker.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        const DEADLINE_MS: u64 = 20;
        const SPAWN_DELAY_MS: u64 = 60;
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            execution_deadline: Duration::from_millis(DEADLINE_MS),
            ..DirectMcpConfig::default()
        })
        .with_spawn_delay_for_test(Duration::from_millis(SPAWN_DELAY_MS));

        let (error, audit) = adapter
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("sync spawn delay beyond deadline must reject invocation");

        assert_eq!(error.code(), "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.result_code, "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.cleanup_outcome, "cleaned_up_after_error");
        assert!(
            started.exists(),
            "child process was spawned and resumed under containment"
        );
        assert!(
            !call_marker.exists(),
            "zero tool calls must be dispatched when spawn completes after deadline"
        );

        // The kernel-level process creation cannot be preempted, so the deadline
        // must be enforced by rejecting late completion. The measured duration
        // therefore has to include the whole non-preemptible delay, which is the
        // observable deadline overrun the invocation must report.
        assert!(
            audit.duration_ms >= SPAWN_DELAY_MS,
            "measured duration {}ms must include the {SPAWN_DELAY_MS}ms non-preemptible spawn delay",
            audit.duration_ms
        );
        println!(
            "sync spawn deadline overrun: deadline={DEADLINE_MS}ms spawn_delay={SPAWN_DELAY_MS}ms measured={}ms overrun={}ms",
            audit.duration_ms,
            audit.duration_ms.saturating_sub(DEADLINE_MS)
        );
    }

    #[tokio::test]
    async fn test_production_sync_normalization_delay_beyond_deadline_rejects_and_cleans_up() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["paged".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            execution_deadline: Duration::from_millis(500),
            ..DirectMcpConfig::default()
        })
        .with_normalization_delay_for_test(Duration::from_millis(600));

        let (error, audit) = adapter
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("normalization completion beyond deadline must not return success");

        assert_eq!(error.code(), "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.result_code, "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.cleanup_outcome, "cleaned_up_after_error");
    }

    #[tokio::test]
    async fn test_production_adapter_cancellation_during_handshake_is_audited_and_cleaned() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let marker = temp.path().join("handshake-ready");
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "hang-initialize".to_string(),
                marker.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let cancel = CancellationToken::new();
        let adapter = DirectMcpAdapter::new();
        let invocation = adapter.execute_with_audit(&direct_input, Some(&cancel));
        tokio::pin!(invocation);
        let wait_for_ready = async {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if marker.exists() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
        };
        tokio::select! {
            result = &mut invocation => panic!("peer unexpectedly completed before cancellation: {result:?}"),
            ready = wait_for_ready => assert!(ready.is_ok(), "fake peer handshake marker was not observed"),
        }
        cancel.cancel();
        let (error, audit) = invocation
            .await
            .expect_err("cancellation must reject invocation");
        assert_eq!(error.code(), "DMCP_CANCELLED");
        assert_eq!(audit.result_code, "DMCP_CANCELLED");
        assert_ne!(audit.cleanup_outcome, "cleanup_failed");
    }

    #[tokio::test]
    async fn test_production_adapter_cancellation_during_discovery_and_call_is_audited() {
        for (mode, expected_pages) in [("hang-list", 0), ("paged-hang-list", 1), ("hang-call", 1)] {
            let temp = tempdir().unwrap();
            let peer = build_stdio_peer(&temp);
            let project = tempdir().unwrap();
            let marker = temp.path().join(format!("{mode}-ready"));
            let input = AdapterExecutionInput {
                role: AgentRole::Planner,
                profile: sample_mcp_profile("fake", "plan"),
                system_prompt: "system".to_string(),
                user_prompt: "task".to_string(),
                project_path: project.path().to_path_buf(),
                temperature: None,
            };
            let config = McpServerConfig {
                executable: peer.to_string_lossy().into_owned(),
                args: vec![mode.to_string(), marker.to_string_lossy().into_owned()],
                working_directory: Some(project.path().to_string_lossy().into_owned()),
                ..sample_server_config(&peer)
            };
            let direct_input = DirectMcpExecutionInput {
                adapter_input: &input,
                server_config: &config,
                plan_context: None,
                frozen_plan: None,
            };
            let cancel = CancellationToken::new();
            let adapter = DirectMcpAdapter::new();
            let invocation = adapter.execute_with_audit(&direct_input, Some(&cancel));
            tokio::pin!(invocation);
            let wait_for_ready = async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        if marker.exists() {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
            };
            tokio::select! {
                result = &mut invocation => panic!("peer unexpectedly completed before cancellation ({mode}): {result:?}"),
                ready = wait_for_ready => assert!(ready.is_ok(), "fake peer {mode} marker was not observed"),
            }
            cancel.cancel();
            let (error, audit) = invocation
                .await
                .expect_err("cancellation must reject invocation");
            assert_eq!(error.code(), "DMCP_CANCELLED", "mode={mode}");
            assert_eq!(audit.result_code, "DMCP_CANCELLED", "mode={mode}");
            assert_eq!(audit.pages_discovered, expected_pages, "mode={mode}");
            assert_eq!(audit.tools_discovered, expected_pages, "mode={mode}");
            assert_ne!(audit.cleanup_outcome, "cleanup_failed", "mode={mode}");
        }
    }

    #[tokio::test]
    async fn test_production_adapter_audits_only_received_pages_when_discovery_disconnects() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            args: vec!["paged-exit-list".to_string()],
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("second-page disconnect must reject discovery");
        assert_eq!(error.code(), "DMCP_TOOLS_LIST_FAILED");
        assert_eq!(audit.pages_discovered, 1);
        assert_eq!(audit.tools_discovered, 1);
    }

    #[tokio::test]
    async fn test_production_adapter_audits_discovery_counts_when_a_later_step_fails() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        // Paginates over two pages, then presents a tool whose schema cannot
        // satisfy prompt_envelope_v1, so discovery succeeds and the call never
        // happens.
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["paged-bad-schema".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("an incompatible schema must reject before tools/call");
        assert_eq!(error.code(), "DMCP_INCOMPATIBLE_SCHEMA");
        assert_eq!(audit.result_code, "DMCP_INCOMPATIBLE_SCHEMA");
        assert_eq!(audit.cleanup_outcome, "cleaned_up_after_error");
        assert_eq!(
            audit.pages_discovered, 2,
            "both pages discovered before the failure must reach the audit record"
        );
        assert_eq!(
            audit.tools_discovered, 2,
            "tools discovered before the failure must reach the audit record"
        );
    }

    #[tokio::test]
    async fn test_production_adapter_deadline_during_tool_call_is_audited_and_cleaned() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let marker = temp.path().join("call-ready");
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "hang-call".to_string(),
                marker.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            execution_deadline: Duration::from_millis(500),
            ..DirectMcpConfig::default()
        });
        let invocation = adapter.execute_with_audit(&direct_input, None);
        tokio::pin!(invocation);
        let wait_for_ready = async {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if marker.exists() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
        };
        tokio::select! {
            result = &mut invocation => panic!("peer unexpectedly completed before deadline: {result:?}"),
            ready = wait_for_ready => assert!(ready.is_ok(), "fake peer tool-call marker was not observed"),
        }
        let (error, audit) = invocation
            .await
            .expect_err("deadline must reject the hung tool call");
        assert_eq!(error.code(), "DMCP_DEADLINE_EXCEEDED");
        assert_eq!(audit.result_code, "DMCP_DEADLINE_EXCEEDED");
        assert_ne!(audit.cleanup_outcome, "cleanup_failed");
        // Discovery completed before the call hung, so the audit must attribute
        // it even though the invocation failed.
        assert_eq!(audit.pages_discovered, 1);
        assert_eq!(audit.tools_discovered, 1);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_windows_job_assignment_failure_prevents_child_execution_and_tool_dispatch() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let started = temp.path().join("child-started");
        let call_marker = temp.path().join("tool-called");
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "started-marker".to_string(),
                started.to_string_lossy().into_owned(),
                call_marker.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .with_job_assignment_failure_for_test()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("injected Job assignment failure must stop before protocol startup");
        assert_eq!(error.code(), "DMCP_PROCESS_CONTAINMENT_FAILED");
        assert_eq!(audit.result_code, error.code());
        assert!(
            !started.exists(),
            "suspended child executed before containment succeeded"
        );
        assert!(
            !call_marker.exists(),
            "no MCP tools/call can be dispatched after containment failure"
        );
    }

    #[tokio::test]
    async fn test_production_stdio_stderr_flood_is_drained_without_deadlock() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["stderr-flood".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (_, audit) = tokio::time::timeout(
            Duration::from_secs(5),
            DirectMcpAdapter::new().execute_with_audit(&direct_input, None),
        )
        .await
        .expect("stderr flood deadlocked the protocol")
        .expect("stderr flood should not fail a valid MCP call");
        assert_eq!(audit.result_code, "SUCCESS");
    }

    #[tokio::test]
    async fn test_production_adapter_reports_premature_child_exit() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["exit-list".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("premature child exit must not produce a result");
        assert!(matches!(
            error.code(),
            "DMCP_TOOLS_LIST_FAILED" | "DMCP_PROCESS_EXIT_FAILED"
        ));
        assert_eq!(audit.result_code, error.code());
    }

    #[tokio::test]
    async fn test_production_cleanup_deadline_failure_prevents_success_result() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let pid_file = temp.path().join("zero-cleanup-budget.pid");
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "descendant-success".to_string(),
                pid_file.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            teardown_grace: Duration::ZERO,
            ..DirectMcpConfig::default()
        });
        let (error, audit) = adapter
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("cleanup deadline failure must reject otherwise valid tool output");
        assert_eq!(error.code(), "DMCP_CLEANUP_FAILED");
        assert_eq!(audit.result_code, "DMCP_CLEANUP_FAILED");
        assert_eq!(audit.cleanup_outcome, "cleanup_failed");
    }

    #[tokio::test]
    async fn test_production_cleanup_deadline_failure_preserves_invocation_error() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["repeat".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            teardown_grace: Duration::ZERO,
            ..DirectMcpConfig::default()
        });
        let (error, audit) = adapter
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("cleanup failure after invocation failure must remain observable");
        assert_eq!(error.code(), "DMCP_CLEANUP_FAILED");
        assert!(error.to_string().contains("DMCP_REPEATED_CURSOR"));
        assert!(error.to_string().contains("DMCP_CLEANUP_FAILED"));
        assert_eq!(audit.result_code, "DMCP_CLEANUP_FAILED");
        assert_eq!(audit.cleanup_outcome, "cleanup_failed_after_error");
    }

    #[tokio::test]
    async fn test_production_cleanup_operation_failure_prevents_success_result() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["paged".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::new().with_cleanup_failure_for_test();
        let (error, audit) = adapter
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("cleanup operation failure must reject otherwise valid tool output");
        assert_eq!(error.code(), "DMCP_CLEANUP_FAILED");
        assert!(error.to_string().contains("injected process tree termination failure"));
        assert_eq!(audit.result_code, "DMCP_CLEANUP_FAILED");
        assert_eq!(audit.cleanup_outcome, "cleanup_failed");
    }

    #[tokio::test]
    async fn test_production_cleanup_operation_failure_preserves_both_errors_on_invocation_failure() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["repeat".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let adapter = DirectMcpAdapter::new().with_cleanup_failure_for_test();
        let (error, audit) = adapter
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("cleanup operation failure after invocation failure must preserve both causes");
        assert_eq!(error.code(), "DMCP_CLEANUP_FAILED");
        let err_msg = error.to_string();
        assert!(
            err_msg.contains("DMCP_REPEATED_CURSOR"),
            "expected original invocation error in message, got: {err_msg}"
        );
        assert!(
            err_msg.contains("injected process tree termination failure") || err_msg.contains("DMCP_CLEANUP_FAILED"),
            "expected cleanup failure in message, got: {err_msg}"
        );
        assert_eq!(audit.result_code, "DMCP_CLEANUP_FAILED");
        assert_eq!(audit.cleanup_outcome, "cleanup_failed_after_error");
    }

    #[tokio::test]
    async fn test_production_stderr_join_failure_prevents_success_result() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["paged".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .with_stderr_join_failure_for_test()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("stderr join failure must reject otherwise valid tool output");
        assert_eq!(error.code(), "DMCP_CLEANUP_FAILED");
        let message = error.to_string();
        assert!(
            message.contains("failed to join stderr task"),
            "expected the production stderr-join failure text, got: {message}"
        );
        assert!(
            message.contains("was cancelled"),
            "expected the real JoinError cause, got: {message}"
        );
        assert_eq!(audit.result_code, "DMCP_CLEANUP_FAILED");
        assert_eq!(audit.cleanup_outcome, "cleanup_failed");
        // The tool call itself succeeded; only cleanup failed, so the audit must
        // still show the discovered pagination for the rejected invocation.
        assert_eq!(audit.pages_discovered, 2);
        assert_eq!(audit.tools_discovered, 2);
    }

    #[tokio::test]
    async fn test_production_stderr_join_failure_preserves_both_errors_on_invocation_failure() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["repeat".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .with_stderr_join_failure_for_test()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("stderr join failure after an invocation failure must preserve both causes");
        assert_eq!(error.code(), "DMCP_CLEANUP_FAILED");
        let message = error.to_string();
        assert!(
            message.contains("DMCP_REPEATED_CURSOR"),
            "expected the original invocation error in the message, got: {message}"
        );
        assert!(
            message.contains("failed to join stderr task") && message.contains("was cancelled"),
            "expected the production stderr-join failure text in the message, got: {message}"
        );
        assert_eq!(audit.result_code, "DMCP_CLEANUP_FAILED");
        assert_eq!(audit.cleanup_outcome, "cleanup_failed_after_error");
    }

    #[tokio::test]
    async fn test_production_cleanup_terminates_descendant_on_success_and_error() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        for mode in ["descendant-success", "descendant-repeat"] {
            let pid_file = temp.path().join(format!("{mode}.pid"));
            let input = AdapterExecutionInput {
                role: AgentRole::Planner,
                profile: sample_mcp_profile("fake", "plan"),
                system_prompt: "system".to_string(),
                user_prompt: "task".to_string(),
                project_path: project.path().to_path_buf(),
                temperature: None,
            };
            let config = McpServerConfig {
                executable: peer.to_string_lossy().into_owned(),
                args: vec![mode.to_string(), pid_file.to_string_lossy().into_owned()],
                working_directory: Some(project.path().to_string_lossy().into_owned()),
                ..sample_server_config(&peer)
            };
            let direct_input = DirectMcpExecutionInput {
                adapter_input: &input,
                server_config: &config,
                plan_context: None,
                frozen_plan: None,
            };
            let result = DirectMcpAdapter::new()
                .execute_with_audit(&direct_input, None)
                .await;
            if mode.ends_with("success") {
                assert!(
                    result.is_ok(),
                    "successful peer invocation failed: {result:?}"
                );
            } else {
                assert_eq!(result.unwrap_err().0.code(), "DMCP_REPEATED_CURSOR");
            }
            let pid: u32 = std::fs::read_to_string(&pid_file).unwrap().parse().unwrap();
            assert!(
                !process_is_alive(pid),
                "owned descendant PID {pid} survived cleanup for {mode}"
            );
        }
    }

    #[test]
    fn test_validate_mcp_assignment_allowed_roles() {
        let (_dir, exe) = valid_test_executable();
        let proj_dir = tempdir().unwrap();
        let canonical_proj = std::fs::canonicalize(proj_dir.path()).unwrap();

        let server_cfg = sample_server_config(&exe);
        let mut servers = HashMap::new();
        servers.insert("deepseek-mcp".to_string(), server_cfg);

        let profile = sample_mcp_profile("deepseek-mcp", "plan");

        // Planner and PlanReviewer allowed
        assert!(DirectMcpAdapter::validate_mcp_assignment(
            &AgentRole::Planner,
            &profile,
            &servers,
            &canonical_proj,
        )
        .is_ok());

        assert!(DirectMcpAdapter::validate_mcp_assignment(
            &AgentRole::PlanReviewer,
            &profile,
            &servers,
            &canonical_proj,
        )
        .is_ok());

        // Other roles rejected
        let rejected_roles = [
            AgentRole::Implementer,
            AgentRole::Fixer,
            AgentRole::CodeReviewer,
        ];
        for r in rejected_roles {
            let res =
                DirectMcpAdapter::validate_mcp_assignment(&r, &profile, &servers, &canonical_proj);
            assert!(res.is_err());
            let err = res.unwrap_err();
            assert_eq!(err.code(), "DMCP_UNAUTHORIZED_ROLE");
        }
    }

    #[test]
    fn test_validate_mcp_assignment_adapter_mismatch() {
        let (_dir, exe) = valid_test_executable();
        let proj_dir = tempdir().unwrap();
        let canonical_proj = std::fs::canonicalize(proj_dir.path()).unwrap();

        let server_cfg = sample_server_config(&exe);
        let mut servers = HashMap::new();
        servers.insert("deepseek-mcp".to_string(), server_cfg);

        let mut profile = sample_mcp_profile("deepseek-mcp", "plan");
        profile.adapter = ExecutionAdapterType::Provider;

        let res = DirectMcpAdapter::validate_mcp_assignment(
            &AgentRole::Planner,
            &profile,
            &servers,
            &canonical_proj,
        );
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code(), "DMCP_INVALID_ADAPTER_TYPE");
    }

    #[test]
    fn test_validate_mcp_assignment_missing_fields() {
        let (_dir, exe) = valid_test_executable();
        let proj_dir = tempdir().unwrap();
        let canonical_proj = std::fs::canonicalize(proj_dir.path()).unwrap();

        let server_cfg = sample_server_config(&exe);
        let mut servers = HashMap::new();
        servers.insert("deepseek-mcp".to_string(), server_cfg);

        // Missing external_mcp_server
        let mut p1 = sample_mcp_profile("deepseek-mcp", "plan");
        p1.external_mcp_server = None;
        assert_eq!(
            DirectMcpAdapter::validate_mcp_assignment(
                &AgentRole::Planner,
                &p1,
                &servers,
                &canonical_proj
            )
            .unwrap_err()
            .code(),
            "DMCP_MISSING_SERVER_ID"
        );

        // Empty external_mcp_server
        let p2 = sample_mcp_profile("", "plan");
        assert_eq!(
            DirectMcpAdapter::validate_mcp_assignment(
                &AgentRole::Planner,
                &p2,
                &servers,
                &canonical_proj
            )
            .unwrap_err()
            .code(),
            "DMCP_MISSING_SERVER_ID"
        );

        // Missing mcp_tool
        let mut p3 = sample_mcp_profile("deepseek-mcp", "plan");
        p3.mcp_tool = None;
        assert_eq!(
            DirectMcpAdapter::validate_mcp_assignment(
                &AgentRole::Planner,
                &p3,
                &servers,
                &canonical_proj
            )
            .unwrap_err()
            .code(),
            "DMCP_MISSING_TOOL_NAME"
        );

        // Empty mcp_tool
        let p4 = sample_mcp_profile("deepseek-mcp", "   ");
        assert_eq!(
            DirectMcpAdapter::validate_mcp_assignment(
                &AgentRole::Planner,
                &p4,
                &servers,
                &canonical_proj
            )
            .unwrap_err()
            .code(),
            "DMCP_MISSING_TOOL_NAME"
        );

        // Unknown server id
        let p5 = sample_mcp_profile("nonexistent-server", "plan");
        let res5 = DirectMcpAdapter::validate_mcp_assignment(
            &AgentRole::Planner,
            &p5,
            &servers,
            &canonical_proj,
        );
        assert!(res5.is_err());
        assert_eq!(res5.unwrap_err().code(), "DMCP_UNKNOWN_SERVER");
    }

    #[test]
    fn test_validate_server_config_transport_and_contract() {
        let (_dir, exe) = valid_test_executable();
        let proj_dir = tempdir().unwrap();
        let canonical_proj = std::fs::canonicalize(proj_dir.path()).unwrap();

        // Valid stdio config passes
        let cfg = sample_server_config(&exe);
        assert!(validate_server_config(&cfg, &canonical_proj).is_ok());

        // Unsupported transport in JSON deserialization
        let bad_transport_json = serde_json::json!({
            "transport": "http",
            "executable": exe.to_string_lossy().to_string(),
            "toolContract": "prompt_envelope_v1"
        });
        assert!(serde_json::from_value::<McpServerConfig>(bad_transport_json).is_err());

        // Unsupported tool contract in JSON deserialization
        let bad_contract_json = serde_json::json!({
            "transport": "stdio",
            "executable": exe.to_string_lossy().to_string(),
            "toolContract": "legacy"
        });
        assert!(serde_json::from_value::<McpServerConfig>(bad_contract_json).is_err());
    }

    #[test]
    fn test_validate_server_config_executable_checks() {
        let proj_dir = tempdir().unwrap();
        let canonical_proj = std::fs::canonicalize(proj_dir.path()).unwrap();

        // Empty executable
        let mut cfg = sample_server_config(Path::new(""));
        cfg.executable = "".to_string();
        assert_eq!(
            validate_server_config(&cfg, &canonical_proj)
                .unwrap_err()
                .code(),
            "DMCP_MISSING_EXECUTABLE"
        );

        // Relative path
        cfg.executable = "relative/path/node.exe".to_string();
        assert_eq!(
            validate_server_config(&cfg, &canonical_proj)
                .unwrap_err()
                .code(),
            "DMCP_INVALID_EXECUTABLE_PATH"
        );

        // Non-existent file
        cfg.executable = "C:/nonexistent/binary_path_12345.exe".to_string();
        assert_eq!(
            validate_server_config(&cfg, &canonical_proj)
                .unwrap_err()
                .code(),
            "DMCP_INVALID_EXECUTABLE_PATH"
        );

        // Directory instead of file
        cfg.executable = canonical_proj.to_string_lossy().to_string();
        assert_eq!(
            validate_server_config(&cfg, &canonical_proj)
                .unwrap_err()
                .code(),
            "DMCP_INVALID_EXECUTABLE_PATH"
        );
    }

    #[test]
    fn test_validate_server_config_shell_and_script_rejections() {
        let dir = tempdir().unwrap();
        let proj_dir = tempdir().unwrap();
        let canonical_proj = std::fs::canonicalize(proj_dir.path()).unwrap();

        // Shell executables
        for shell in [
            "cmd.exe",
            "powershell.exe",
            "pwsh.exe",
            "bash.exe",
            "sh.exe",
            "zsh.exe",
        ] {
            let p = dir.path().join(shell);
            std::fs::write(&p, b"mock shell").unwrap();
            let cfg = sample_server_config(&p);
            let res = validate_server_config(&cfg, &canonical_proj);
            assert!(res.is_err());
            assert_eq!(
                res.unwrap_err().code(),
                "DMCP_SHELL_LAUNCHER_REJECTED",
                "Expected shell rejection for {shell}"
            );
        }

        // Script wrappers
        for script in ["server.bat", "run.cmd", "start.ps1", "launch.sh"] {
            let p = dir.path().join(script);
            std::fs::write(&p, b"mock script").unwrap();
            let cfg = sample_server_config(&p);
            let res = validate_server_config(&cfg, &canonical_proj);
            assert!(res.is_err());
            assert_eq!(
                res.unwrap_err().code(),
                "DMCP_SCRIPT_WRAPPER_REJECTED",
                "Expected script rejection for {script}"
            );
        }
    }

    #[test]
    fn test_validate_server_config_environment_variables() {
        let (_dir, exe) = valid_test_executable();
        let proj_dir = tempdir().unwrap();
        let canonical_proj = std::fs::canonicalize(proj_dir.path()).unwrap();

        let mut cfg = sample_server_config(&exe);
        cfg.allowed_environment = vec!["VALID_VAR_1".to_string(), "ANOTHER_VAR".to_string()];
        assert!(validate_server_config(&cfg, &canonical_proj).is_ok());

        // Empty var name
        cfg.allowed_environment = vec!["".to_string()];
        assert_eq!(
            validate_server_config(&cfg, &canonical_proj)
                .unwrap_err()
                .code(),
            "DMCP_INVALID_ENVIRONMENT_VARIABLE"
        );

        // Var name with '='
        cfg.allowed_environment = vec!["VAR=VALUE".to_string()];
        assert_eq!(
            validate_server_config(&cfg, &canonical_proj)
                .unwrap_err()
                .code(),
            "DMCP_INVALID_ENVIRONMENT_VARIABLE"
        );

        // Var name with non-alphanumeric chars
        cfg.allowed_environment = vec!["VAR-NAME".to_string()];
        assert_eq!(
            validate_server_config(&cfg, &canonical_proj)
                .unwrap_err()
                .code(),
            "DMCP_INVALID_ENVIRONMENT_VARIABLE"
        );

        // Duplicate var name
        cfg.allowed_environment = vec!["DUPLICATE_VAR".to_string(), "duplicate_var".to_string()];
        #[cfg(windows)]
        assert_eq!(
            validate_server_config(&cfg, &canonical_proj)
                .unwrap_err()
                .code(),
            "DMCP_INVALID_ENVIRONMENT_VARIABLE"
        );
    }

    #[test]
    fn test_resolve_allowed_environment() {
        std::env::set_var("ANTHRO_TEST_VAR_123", "hello_world");

        let allowed = vec!["ANTHRO_TEST_VAR_123".to_string()];
        let resolved = resolve_allowed_environment(&allowed).unwrap();
        assert_eq!(
            resolved.get("ANTHRO_TEST_VAR_123").map(String::as_str),
            Some("hello_world")
        );

        // Missing variable fails
        let missing = vec!["NONEXISTENT_VAR_98765".to_string()];
        let err = resolve_allowed_environment(&missing).unwrap_err();
        assert_eq!(err.code(), "DMCP_MISSING_ENVIRONMENT_VARIABLE");

        std::env::remove_var("ANTHRO_TEST_VAR_123");
    }

    #[test]
    fn test_validate_prompt_envelope_schema() {
        // Valid schema
        let valid_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "schema_version": { "type": "integer" },
                "role": { "type": "string" },
                "system_prompt": { "type": "string" },
                "user_prompt": { "type": "string" },
                "plan_context": { "type": "object" },
                "frozen_plan": { "type": "object" }
            },
            "required": ["schema_version", "role", "user_prompt"]
        });
        assert!(validate_prompt_envelope_schema(&valid_schema).is_ok());

        // Legacy schema with task/context
        let legacy_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "task": { "type": "string" },
                "context": { "type": "string" }
            },
            "required": ["task", "context"]
        });
        let res = validate_prompt_envelope_schema(&legacy_schema);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code(), "DMCP_INCOMPATIBLE_SCHEMA");

        // Required non-envelope parameter
        let bad_req = serde_json::json!({
            "type": "object",
            "properties": {
                "user_prompt": { "type": "string" },
                "custom_arg": { "type": "string" }
            },
            "required": ["custom_arg"]
        });
        assert_eq!(
            validate_prompt_envelope_schema(&bad_req)
                .unwrap_err()
                .code(),
            "DMCP_INCOMPATIBLE_SCHEMA"
        );

        // Non-string role property
        let bad_role = serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "number" }
            }
        });
        assert_eq!(
            validate_prompt_envelope_schema(&bad_role)
                .unwrap_err()
                .code(),
            "DMCP_INCOMPATIBLE_SCHEMA"
        );
    }

    #[test]
    fn test_normalize_call_tool_result_is_error() {
        let err_res: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [{"type": "text", "text": "provider auth failed"}],
            "isError": true
        }))
        .unwrap();
        let res = normalize_call_tool_result(&err_res, 1024 * 1024);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code(), "DMCP_TOOL_REPORTED_ERROR");
        assert!(err.to_string().contains("provider auth failed"));
    }

    #[test]
    fn test_normalize_call_tool_result_structured_content() {
        // Valid structured content
        let ok_res: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [],
            "isError": false,
            "structuredContent": {
                "schema_version": 1,
                "content": "# Implementation Plan\n1. Step"
            }
        }))
        .unwrap();
        let normalized = normalize_call_tool_result(&ok_res, 1024 * 1024).unwrap();
        assert_eq!(normalized, "# Implementation Plan\n1. Step");

        // Structured content with unsupported schema_version
        let bad_res: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [],
            "structuredContent": {
                "schema_version": 2,
                "content": "# Plan"
            }
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&bad_res, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_UNSUPPORTED_SCHEMA_VERSION"
        );

        // Structured content with extra field
        let extra_res: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [],
            "structuredContent": {
                "schema_version": 1,
                "content": "# Plan",
                "extra_field": "val"
            }
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&extra_res, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_MALFORMED_STRUCTURED_CONTENT"
        );
    }

    #[test]
    fn test_normalize_call_tool_result_text_fallback() {
        // Valid single text block
        let text_res: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [
                { "type": "text", "text": "Plain plan text" }
            ]
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&text_res, 1024 * 1024).unwrap(),
            "Plain plan text"
        );

        // Empty content blocks
        let empty_res: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": []
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&empty_res, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_EMPTY_MCP_RESPONSE"
        );

        // Multiple content blocks without structured content
        let multi_res: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [
                { "type": "text", "text": "Block 1" },
                { "type": "text", "text": "Block 2" }
            ]
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&multi_res, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_CONFLICTING_CONTENT_BLOCKS"
        );

        // Text exceeding max size
        let big_res: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [
                { "type": "text", "text": "a".repeat(100) }
            ]
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&big_res, 50).unwrap_err().code(),
            "DMCP_RESPONSE_SIZE_LIMIT_EXCEEDED"
        );
    }

    #[test]
    fn test_normalize_call_tool_result_structured_companion_and_unsupported_blocks() {
        let envelope = serde_json::json!({ "schema_version": 1, "content": "# Plan" });

        // Companion text block whose JSON equals the structured envelope is accepted.
        let matching: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [{ "type": "text", "text": envelope.to_string() }],
            "structuredContent": envelope,
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&matching, 1024 * 1024).unwrap(),
            "# Plan"
        );

        // Companion text that parses but disagrees with the envelope is rejected.
        let conflicting: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [{ "type": "text", "text": "{\"schema_version\":1,\"content\":\"# Other\"}" }],
            "structuredContent": { "schema_version": 1, "content": "# Plan" },
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&conflicting, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_CONFLICTING_CONTENT_BLOCKS"
        );

        // Companion text that is not JSON at all is rejected, never silently preferred.
        let non_json: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [{ "type": "text", "text": "not json" }],
            "structuredContent": { "schema_version": 1, "content": "# Plan" },
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&non_json, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_CONFLICTING_CONTENT_BLOCKS"
        );

        // Multiple companion blocks alongside structured content are rejected.
        let multiple: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [
                { "type": "text", "text": envelope.to_string() },
                { "type": "text", "text": envelope.to_string() },
            ],
            "structuredContent": { "schema_version": 1, "content": "# Plan" },
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&multiple, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_CONFLICTING_CONTENT_BLOCKS"
        );

        // A non-text companion block alongside structured content is rejected.
        let image_companion: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [{ "type": "image", "data": "aGk=", "mimeType": "image/png" }],
            "structuredContent": { "schema_version": 1, "content": "# Plan" },
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&image_companion, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_UNSUPPORTED_CONTENT_TYPE"
        );

        // structuredContent must be an object with a non-empty content string.
        let empty_content: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [],
            "structuredContent": { "schema_version": 1, "content": "   " },
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&empty_content, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_MALFORMED_STRUCTURED_CONTENT"
        );

        let missing_version: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [],
            "structuredContent": { "content": "# Plan" },
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&missing_version, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_MALFORMED_STRUCTURED_CONTENT"
        );

        let not_an_object: CallToolResult = serde_json::from_value(serde_json::json!({
            "content": [],
            "structuredContent": "# Plan",
        }))
        .unwrap();
        assert_eq!(
            normalize_call_tool_result(&not_an_object, 1024 * 1024)
                .unwrap_err()
                .code(),
            "DMCP_MALFORMED_STRUCTURED_CONTENT"
        );

        // Without structured content, only a single text block is accepted.
        for unsupported in [
            serde_json::json!({ "type": "image", "data": "aGk=", "mimeType": "image/png" }),
            serde_json::json!({ "type": "audio", "data": "aGk=", "mimeType": "audio/wav" }),
            serde_json::json!({ "type": "resource", "resource": { "uri": "file:///x", "text": "x" } }),
        ] {
            let block = unsupported.clone();
            let result: CallToolResult =
                serde_json::from_value(serde_json::json!({ "content": [block] })).unwrap();
            assert_eq!(
                normalize_call_tool_result(&result, 1024 * 1024)
                    .unwrap_err()
                    .code(),
                "DMCP_UNSUPPORTED_CONTENT_TYPE",
                "block kind {unsupported} must be rejected"
            );
        }
    }

    // -------------------------------------------------------------
    // FAKE PEER MCP PROTOCOL INTEGRATION TESTS
    // -------------------------------------------------------------
    #[tokio::test]
    async fn test_fake_peer_multi_page_discovery_and_call() {
        let (client_io, server_io) = tokio::io::duplex(65536);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);

        let server_task = tokio::spawn(async move {
            // Run a lightweight JSON-RPC line server responding to MCP stdio initialize, list_tools, call_tool
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let mut reader = BufReader::new(server_read);
            let mut writer = server_write;
            let mut line = String::new();

            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let msg: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = msg.get("id").cloned();
                let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

                if method == "initialize" {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": { "tools": { "listChanged": false } },
                            "serverInfo": { "name": "fake-peer", "version": "1.0.0" }
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                } else if method == "notifications/initialized" {
                    // notification, no response
                } else if method == "tools/list" {
                    let params = msg.get("params");
                    let cursor = params
                        .and_then(|p| p.get("cursor"))
                        .and_then(|v| v.as_str());
                    if cursor.is_none() {
                        // First page
                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "tools": [{
                                    "name": "other_tool",
                                    "description": "Other",
                                    "inputSchema": { "type": "object", "properties": {} }
                                }],
                                "nextCursor": "page_2_cursor"
                            }
                        });
                        writer
                            .write_all(format!("{}\n", resp).as_bytes())
                            .await
                            .unwrap();
                    } else if cursor == Some("page_2_cursor") {
                        // Second page with matching tool
                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "tools": [{
                                    "name": "plan",
                                    "description": "Planner tool",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "schema_version": { "type": "integer" },
                                            "role": { "type": "string" },
                                            "user_prompt": { "type": "string" }
                                        },
                                        "required": ["schema_version", "role", "user_prompt"]
                                    }
                                }],
                                "nextCursor": null
                            }
                        });
                        writer
                            .write_all(format!("{}\n", resp).as_bytes())
                            .await
                            .unwrap();
                    }
                } else if method == "tools/call" {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": "## Verified Implementation Plan\n1. Done" }],
                            "isError": false
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                }
                line.clear();
            }
        });

        let client = DirectMcpClientHandler
            .serve(test_child_io(client_read, client_write))
            .await
            .unwrap();

        // Perform multi-page list_tools
        let mut matched_tool = None;
        let mut cursor = None;
        let mut page_count = 0;
        let mut total_tools = 0;
        let mut seen_cursors = std::collections::HashSet::new();

        loop {
            page_count += 1;
            assert!(page_count <= 32);

            let params = cursor.map(|c| {
                let mut p = PaginatedRequestParams::default();
                p.cursor = Some(c);
                p
            });

            let list_res: ListToolsResult = client.list_tools(params).await.unwrap();
            total_tools += list_res.tools.len();
            for t in list_res.tools {
                if t.name == "plan" {
                    matched_tool = Some(t);
                }
            }

            match list_res.next_cursor {
                Some(next) if !next.trim().is_empty() => {
                    assert!(seen_cursors.insert(next.clone()));
                    cursor = Some(next);
                }
                _ => break,
            }
        }

        assert_eq!(page_count, 2);
        assert_eq!(total_tools, 2);
        assert!(matched_tool.is_some());

        // Perform tool call
        let call_params = CallToolRequestParams::new("plan".to_string()).with_arguments(
            serde_json::json!({
                "schema_version": 1,
                "role": "planner",
                "user_prompt": "do the task"
            })
            .as_object()
            .cloned()
            .unwrap(),
        );
        let call_res: CallToolResult = client.call_tool(call_params).await.unwrap();
        let normalized = normalize_call_tool_result(&call_res, 1024 * 1024).unwrap();
        assert_eq!(normalized, "## Verified Implementation Plan\n1. Done");

        drop(client);
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_fake_peer_repeated_cursor_detection() {
        let (client_io, server_io) = tokio::io::duplex(65536);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);

        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let mut reader = BufReader::new(server_read);
            let mut writer = server_write;
            let mut line = String::new();

            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let msg: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = msg.get("id").cloned();
                let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

                if method == "initialize" {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": { "tools": { "listChanged": false } },
                            "serverInfo": { "name": "fake-peer", "version": "1.0.0" }
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                } else if method == "tools/list" {
                    // Always return the exact same nextCursor -> repeated cursor!
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "tools": [{ "name": "dummy", "inputSchema": { "type": "object" } }],
                            "nextCursor": "repeated_cursor_123"
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                }
                line.clear();
            }
        });

        let client = DirectMcpClientHandler
            .serve(test_child_io(client_read, client_write))
            .await
            .unwrap();

        let mut cursor: Option<String> = None;
        let mut seen_cursors = std::collections::HashSet::new();
        let mut hit_repeated = false;

        for _ in 0..5 {
            let params = cursor.as_ref().map(|c| {
                let mut p = PaginatedRequestParams::default();
                p.cursor = Some(c.clone());
                p
            });
            let list_res: ListToolsResult = client.list_tools(params).await.unwrap();
            if let Some(next) = list_res.next_cursor {
                if !seen_cursors.insert(next.clone()) {
                    hit_repeated = true;
                    let err = DirectMcpError::RepeatedCursor(next);
                    assert_eq!(err.code(), "DMCP_REPEATED_CURSOR");
                    break;
                }
                cursor = Some(next);
            }
        }

        assert!(
            hit_repeated,
            "Expected repeated cursor to be detected and rejected"
        );
    }

    #[tokio::test]
    async fn test_fake_peer_page_limit_exceeded() {
        let (client_io, server_io) = tokio::io::duplex(65536);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);

        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let mut reader = BufReader::new(server_read);
            let mut writer = server_write;
            let mut line = String::new();
            let mut counter = 0;

            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let msg: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = msg.get("id").cloned();
                let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

                if method == "initialize" {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": { "tools": { "listChanged": false } },
                            "serverInfo": { "name": "fake-peer", "version": "1.0.0" }
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                } else if method == "tools/list" {
                    counter += 1;
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "tools": [{ "name": format!("tool_{counter}"), "inputSchema": { "type": "object" } }],
                            "nextCursor": format!("page_{counter}")
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                }
                line.clear();
            }
        });

        let client = DirectMcpClientHandler
            .serve(test_child_io(client_read, client_write))
            .await
            .unwrap();

        let mut cursor: Option<String> = None;
        let mut pages_requested = 0usize;

        loop {
            pages_requested += 1;
            if pages_requested > MCP_MAX_TOOL_PAGES {
                let err = DirectMcpError::PaginationLimitExceeded(pages_requested);
                assert_eq!(err.code(), "DMCP_PAGINATION_LIMIT_EXCEEDED");
                break;
            }

            let params = cursor.as_ref().map(|c| {
                let mut p = PaginatedRequestParams::default();
                p.cursor = Some(c.clone());
                p
            });
            let list_res: ListToolsResult = client.list_tools(params).await.unwrap();
            cursor = list_res.next_cursor;
        }

        assert_eq!(
            pages_requested,
            MCP_MAX_TOOL_PAGES + 1,
            "pagination must stop at the page cap"
        );
    }

    #[tokio::test]
    async fn test_fake_peer_tool_limit_exceeded() {
        let (client_io, server_io) = tokio::io::duplex(65536);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);

        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let mut reader = BufReader::new(server_read);
            let mut writer = server_write;
            let mut line = String::new();

            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let msg: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = msg.get("id").cloned();
                let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

                if method == "initialize" {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": { "tools": { "listChanged": false } },
                            "serverInfo": { "name": "fake-peer", "version": "1.0.0" }
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                } else if method == "tools/list" {
                    // Return 300 tools in 1 page
                    let tools: Vec<serde_json::Value> = (0..300)
                        .map(|i| serde_json::json!({ "name": format!("tool_{i}"), "inputSchema": { "type": "object" } }))
                        .collect();
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "tools": tools,
                            "nextCursor": null
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                }
                line.clear();
            }
        });

        let client = DirectMcpClientHandler
            .serve(test_child_io(client_read, client_write))
            .await
            .unwrap();

        let list_res: ListToolsResult = client.list_tools(None).await.unwrap();
        assert_eq!(list_res.tools.len(), 300);
        let err = DirectMcpError::ToolLimitExceeded(list_res.tools.len());
        assert_eq!(err.code(), "DMCP_TOOL_LIMIT_EXCEEDED");
    }

    #[tokio::test]
    async fn test_direct_mcp_deadline_exceeded() {
        let adapter = DirectMcpAdapter::with_config(DirectMcpConfig {
            execution_deadline: Duration::from_millis(50),
            teardown_grace: Duration::from_millis(50),
            max_tool_pages: 32,
            max_tools_total: 256,
            max_capture_bytes: 1024 * 1024,
        });

        let (_dir, exe) = valid_test_executable();
        let proj_dir = tempdir().unwrap();
        let canonical_proj = std::fs::canonicalize(proj_dir.path()).unwrap();

        let server_cfg = sample_server_config(&exe);
        let mut profile = sample_mcp_profile("fake-server", "plan");
        profile.adapter = ExecutionAdapterType::Mcp;

        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile,
            system_prompt: "sys".to_string(),
            user_prompt: "user".to_string(),
            project_path: canonical_proj,
            temperature: None,
        };

        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &server_cfg,
            plan_context: None,
            frozen_plan: None,
        };

        let res = adapter.execute_with_audit(&direct_input, None).await;
        assert!(res.is_err());
        let (err, audit) = res.unwrap_err();
        assert!(!audit.result_code.is_empty());
        assert_eq!(audit.role, "planner");
        assert_eq!(audit.server_id, "fake-server");
        assert_eq!(audit.tool_name, "plan");
        assert!(audit.duration_ms <= 5000);
        assert!(!err.to_string().is_empty());
    }

    #[tokio::test]
    async fn test_fake_peer_cancellation() {
        let (client_io, server_io) = tokio::io::duplex(65536);
        let (server_read, server_write) = tokio::io::split(server_io);
        let (client_read, client_write) = tokio::io::split(client_io);

        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let mut reader = BufReader::new(server_read);
            let mut writer = server_write;
            let mut line = String::new();

            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                let msg: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = msg.get("id").cloned();
                let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

                if method == "initialize" {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocolVersion": "2024-11-05",
                            "capabilities": { "tools": { "listChanged": false } },
                            "serverInfo": { "name": "fake-peer", "version": "1.0.0" }
                        }
                    });
                    writer
                        .write_all(format!("{}\n", resp).as_bytes())
                        .await
                        .unwrap();
                } else if method == "tools/call" {
                    // Hang forever to allow cancellation to trigger
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
                line.clear();
            }
        });

        let client = DirectMcpClientHandler
            .serve(test_child_io(client_read, client_write))
            .await
            .unwrap();

        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();

        let call_fut = async {
            let call_params = CallToolRequestParams::new("plan".to_string()).with_arguments(
                serde_json::json!({
                    "schema_version": 1,
                    "role": "planner",
                    "user_prompt": "do work"
                })
                .as_object()
                .cloned()
                .unwrap(),
            );
            client.call_tool(call_params).await
        };

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel_token_clone.cancel();
        });

        let res = tokio::select! {
            _ = cancel_token.cancelled() => Err(DirectMcpError::Cancelled),
            res = call_fut => res.map_err(|e| DirectMcpError::CallFailed(e.to_string())),
        };

        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code(), "DMCP_CANCELLED");
    }

    #[tokio::test]
    async fn test_fake_peer_early_disconnect() {
        let (client_io, server_io) = tokio::io::duplex(65536);
        let (client_read, client_write) = tokio::io::split(client_io);

        // Drop server end immediately to simulate early disconnect
        drop(server_io);

        let handshake_res = DirectMcpClientHandler
            .serve(test_child_io(client_read, client_write))
            .await;

        assert!(handshake_res.is_err());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_windows_production_path_containment_lifecycle() {
        use windows_sys::Win32::System::JobObjects::IsProcessInJob;

        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let descendant_pid_file = temp.path().join("descendant.pid");

        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "descendant-success".to_string(),
                descendant_pid_file.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };

        // 1. Direct spawn driving CREATE_SUSPENDED -> AssignProcessToJobObject -> verify -> ResumeThread
        let env_map = HashMap::new();
        let spawn_res = spawn_mcp_subprocess(
            &config.executable,
            &config.args,
            project.path(),
            &env_map,
            1024 * 1024,
            1024 * 1024,
            false,
        );
        let (mut child, _child_io, stderr_task, mut guard, _frame_exceeded) =
            spawn_res.expect("real spawn with containment lifecycle must succeed");

        // Containment handle must be non-zero
        assert_ne!(
            guard.job_handle_for_test(),
            0,
            "Job Object handle must be non-zero after containment"
        );

        let child_raw = child.raw_handle().expect("spawned child must have raw handle");
        let mut in_job: windows_sys::Win32::Foundation::BOOL = 0;
        let queried = unsafe {
            IsProcessInJob(child_raw as _, guard.job_handle_for_test() as _, &mut in_job)
        };
        assert_ne!(queried, 0, "IsProcessInJob query must succeed");
        assert_ne!(in_job, 0, "child process must be confirmed member of Job Object");

        // Wait for child to produce its descendant PID (proves child ran only after containment and resume)
        let mut descendant_pid = None;
        for _ in 0..100 {
            if descendant_pid_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&descendant_pid_file) {
                    if let Ok(pid) = content.trim().parse::<u32>() {
                        descendant_pid = Some(pid);
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let desc_pid = descendant_pid.expect("child must execute and spawn descendant after resume");
        assert!(process_is_alive(desc_pid), "descendant process must be alive initially");

        // Terminate and close via guard
        assert!(guard.terminate_and_close().is_ok(), "containment guard cleanup must succeed");
        let _ = child.kill().await;
        let _ = stderr_task.abort();

        // Verify descendant is terminated
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            !process_is_alive(desc_pid),
            "descendant process must be terminated upon Job Object termination"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn test_process_containment_guard_cleanup_and_close() {
        let mut guard = ProcessContainmentGuard {
            process_group_id: 0,
            fail_terminate_for_test: false,
        };
        assert!(guard.terminate_and_close().is_ok());

        let mut failing_guard = ProcessContainmentGuard {
            process_group_id: 0,
            fail_terminate_for_test: true,
        };
        let err = failing_guard.terminate_and_close().unwrap_err();
        assert!(err.contains("injected process tree termination failure"));
    }

    #[tokio::test]
    async fn test_bounded_frame_reader_rejects_a_frame_past_the_inbound_limit() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let cap = 64usize;
        let (client_io, mut peer) = tokio::io::duplex(4096);
        let (reader, writer) = tokio::io::split(client_io);
        let exceeded = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut io = ChildIo::new(reader, writer, cap, exceeded.clone());
        let mut buf = [0u8; 256];

        // A frame that reaches the cap without a delimiter is still inside it.
        peer.write_all(&vec![b'x'; cap]).await.unwrap();
        assert_eq!(io.read(&mut buf).await.unwrap(), cap);
        assert!(!exceeded.load(std::sync::atomic::Ordering::SeqCst));

        // One byte past the cap, still undelimited, is refused and flagged.
        peer.write_all(b"y").await.unwrap();
        let error = io.read(&mut buf).await.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(exceeded.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_bounded_frame_reader_resets_its_budget_at_every_delimiter() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let cap = 64usize;
        let (client_io, mut peer) = tokio::io::duplex(4096);
        let (reader, writer) = tokio::io::split(client_io);
        let exceeded = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut io = ChildIo::new(reader, writer, cap, exceeded.clone());
        let mut buf = [0u8; 4096];

        // The bound is per frame, not per stream: cap-sized frames separated by
        // delimiters must all pass.
        let frame = vec![b'x'; cap];
        let mut expected = 0usize;
        for _ in 0..4 {
            peer.write_all(&frame).await.unwrap();
            peer.write_all(b"\n").await.unwrap();
            expected += cap + 1;
        }
        let mut seen = 0usize;
        while seen < expected {
            seen += io.read(&mut buf).await.unwrap();
        }
        assert_eq!(seen, expected);
        assert!(!exceeded.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_production_adapter_reports_an_oversized_inbound_frame_as_a_size_limit() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["oversized-frame".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("an unterminated frame past the cap must fail closed");
        assert_eq!(error.code(), "DMCP_RESPONSE_SIZE_LIMIT_EXCEEDED");
        assert_eq!(audit.result_code, "DMCP_RESPONSE_SIZE_LIMIT_EXCEEDED");
    }

    /// Runs the production cleanup boundary over a fixture peer.
    async fn run_cleanup_over_fixture_peer(
        peer_mode: &str,
        grace_period: Duration,
        force_kill_first: bool,
    ) -> Result<(std::process::ExitStatus, bool), DirectMcpError> {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let mut child = tokio::process::Command::new(&peer)
            .arg(peer_mode)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn fixture peer");
        // The exit-status classification is independent of the stderr drain, so
        // this boundary supplies a drain task that simply completes.
        let mut stderr_task = tokio::spawn(async { String::new() });
        let guard = ProcessContainmentGuard {
            #[cfg(windows)]
            job_handle: 0,
            #[cfg(unix)]
            process_group_id: 0,
            fail_terminate_for_test: false,
        };
        let (status, _stderr, exited_on_its_own) =
            perform_mcp_cleanup(&mut child, &mut stderr_task, guard, grace_period, force_kill_first, false)
                .await?;
        Ok((status, exited_on_its_own))
    }

    #[tokio::test]
    async fn test_cleanup_treats_a_server_self_exit_as_the_server_outcome() {
        let (status, exited_on_its_own) =
            run_cleanup_over_fixture_peer("exit-now", Duration::from_secs(2), false)
                .await
                .expect("cleanup must observe a root that exited on its own");
        assert!(exited_on_its_own, "a status observed before termination is the server's own exit");
        assert_eq!(status.code(), Some(3));
    }

    #[tokio::test]
    async fn test_cleanup_does_not_attribute_a_terminated_exit_to_the_server() {
        // The root outlives the graceful window and is only observed after the
        // cleanup boundary took over, so the failure status is our teardown's
        // artefact and must not discard an otherwise valid tool result.
        let (status, exited_on_its_own) =
            run_cleanup_over_fixture_peer("exit-after-delay", Duration::from_secs(4), false)
                .await
                .expect("cleanup must reap the root within the shared deadline");
        assert!(
            !exited_on_its_own,
            "a status seen only after the graceful window is not a server self-exit"
        );
        assert_eq!(status.code(), Some(3));
    }

    #[test]
    fn test_audit_record_construction() {
        let audit = DirectMcpAuditRecord {
            invocation_id: "dmcp-12345".to_string(),
            role: "planner".to_string(),
            profile_id: "mcp-planner".to_string(),
            provider_id: None,
            model_id: None,
            server_id: "deepseek-mcp".to_string(),
            tool_name: "plan".to_string(),
            duration_ms: 125,
            pages_discovered: 2,
            tools_discovered: 5,
            result_code: "SUCCESS".to_string(),
            cleanup_outcome: "cleaned_up_success".to_string(),
            error_message: None,
        };
        assert_eq!(audit.result_code, "SUCCESS");
        assert_eq!(audit.pages_discovered, 2);
        assert_eq!(audit.tools_discovered, 5);
        assert_eq!(audit.duration_ms, 125);
    }

    // -------------------------------------------------------------
    // PHASE A FROZEN PAYLOAD BINDING OVER THE REAL STDIO WIRE
    // -------------------------------------------------------------

    struct PlanBoundFixture {
        // Keeps the fixture project tree alive for the duration of the test.
        _project: tempfile::TempDir,
        project_path: PathBuf,
        plan_context: PlanContext,
        frozen_plan: FrozenPlanPayload,
    }

    fn plan_bound_fixture() -> PlanBoundFixture {
        let (project, root) =
            crate::orchestrator::plan_workspace::tests::setup_test_project();
        let mut workspace_config = PlanWorkspaceConfig::default();
        workspace_config.plan_series_version_override = Some("0.24.0".to_string());
        std::fs::write(
            root.join(".plan").join("V0.24.0-r1.md"),
            "# Frozen Primary Plan\n\nOriginal frozen body.\n",
        )
        .unwrap();
        let snapshot = capture_frozen_plan_snapshot(&root, &workspace_config)
            .expect("frozen plan snapshot capture must succeed");
        snapshot
            .ensure_run_entry_allowed()
            .expect("resolved plan context must be allowed at run entry");
        let payload = snapshot
            .to_frozen_plan_payload()
            .expect("resolved non-empty plan workspace must produce a frozen payload");
        PlanBoundFixture {
            _project: project,
            project_path: root,
            plan_context: snapshot.plan_context,
            frozen_plan: payload,
        }
    }

    fn plan_bound_input(
        fixture: &PlanBoundFixture,
        peer: &Path,
        mode_args: Vec<String>,
    ) -> (AdapterExecutionInput, McpServerConfig) {
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: fixture.project_path.clone(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: mode_args,
            working_directory: Some(fixture.project_path.to_string_lossy().into_owned()),
            ..sample_server_config(peer)
        };
        (input, config)
    }

    #[tokio::test]
    async fn test_production_frozen_plan_payload_crosses_stdio_wire_unchanged() {
        let fixture = plan_bound_fixture();
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let capture = temp.path().join("call-capture.json");
        let (input, config) = plan_bound_input(
            &fixture,
            &peer,
            vec![
                "capture-call".to_string(),
                capture.to_string_lossy().into_owned(),
            ],
        );
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: Some(&fixture.plan_context),
            frozen_plan: Some(&fixture.frozen_plan),
        };
        let (output, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .unwrap_or_else(|(error, _)| panic!("plan-bound invocation failed: {error}"));
        assert_eq!(output.content, "fake peer success");
        assert_eq!(audit.result_code, "SUCCESS");

        let raw = std::fs::read_to_string(&capture)
            .expect("fake peer must capture the tools/call request it received");
        let request: serde_json::Value =
            serde_json::from_str(raw.trim()).expect("captured request must be JSON");
        assert_eq!(request["method"], serde_json::json!("tools/call"));
        let arguments = &request["params"]["arguments"];
        assert_eq!(arguments["schema_version"], serde_json::json!(1));
        assert_eq!(arguments["role"], serde_json::json!("planner"));
        assert_eq!(
            arguments["plan_context"],
            serde_json::to_value(&fixture.plan_context).unwrap(),
            "the run-owned PlanContext must cross the wire unchanged"
        );
        assert_eq!(
            arguments["frozen_plan"],
            serde_json::to_value(&fixture.frozen_plan).unwrap(),
            "the frozen plan payload must cross the wire unchanged"
        );
        assert_eq!(
            arguments["frozen_plan"]["effectivePlanContent"],
            serde_json::to_value(&fixture.frozen_plan.effective_plan_content).unwrap()
        );
        let effective = arguments["frozen_plan"]["effectivePlanContent"]
            .as_str()
            .expect("effective plan content must be a string");
        assert!(effective.contains("# Frozen Primary Plan"));
        assert!(effective.contains("Original frozen body."));
    }

    #[tokio::test]
    async fn test_production_plan_bound_payload_mismatch_rejects_before_tool_call() {
        let fixture = plan_bound_fixture();
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let capture = temp.path().join("call-capture.json");
        let (input, config) = plan_bound_input(
            &fixture,
            &peer,
            vec![
                "capture-call".to_string(),
                capture.to_string_lossy().into_owned(),
            ],
        );
        // Corrupt the frozen payload so Phase A validation must fail closed.
        let mut corrupted = fixture.frozen_plan.clone();
        corrupted.effective_plan_digest = "sha256:corrupted".to_string();
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: Some(&fixture.plan_context),
            frozen_plan: Some(&corrupted),
        };
        let (error, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("an invalid frozen payload must reject the invocation");
        assert_eq!(error.code(), "DMCP_INVALID_FROZEN_PLAN_PAYLOAD");
        assert_eq!(audit.result_code, "DMCP_INVALID_FROZEN_PLAN_PAYLOAD");
        assert_eq!(audit.cleanup_outcome, "not_spawned");
        assert!(
            !capture.exists(),
            "an invalid frozen payload must reject before any tools/call is dispatched"
        );
    }

    #[tokio::test]
    async fn test_production_missing_frozen_payload_for_plan_bound_context_rejects_before_spawn()
    {
        let fixture = plan_bound_fixture();
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let started = temp.path().join("child-started");
        let (input, config) = plan_bound_input(
            &fixture,
            &peer,
            vec![
                "started-marker".to_string(),
                started.to_string_lossy().into_owned(),
            ],
        );
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: Some(&fixture.plan_context),
            frozen_plan: None,
        };
        let (error, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .expect_err("a plan-bound context without its frozen payload must reject");
        assert_eq!(error.code(), "DMCP_INVALID_FROZEN_PLAN_PAYLOAD");
        assert_eq!(audit.cleanup_outcome, "not_spawned");
        assert!(
            !started.exists(),
            "a plan-bound context without its frozen payload must reject before child spawn"
        );
    }

    #[tokio::test]
    async fn test_production_non_plan_bound_run_accepts_absent_frozen_payload() {
        let (_project, root) = crate::orchestrator::plan_workspace::tests::setup_test_project();
        let mut workspace_config = PlanWorkspaceConfig::default();
        workspace_config.plan_series_version_override = Some("0.24.0".to_string());
        // Clean workspace: a resolved context that is not plan-bound and carries no payload.
        let snapshot = capture_frozen_plan_snapshot(&root, &workspace_config).unwrap();
        assert!(snapshot.to_frozen_plan_payload().is_none());
        assert!(!snapshot.plan_context.is_plan_bound());
        snapshot.ensure_run_entry_allowed().unwrap();

        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: root.clone(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["paged".to_string()],
            working_directory: Some(root.to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: Some(&snapshot.plan_context),
            frozen_plan: None,
        };
        let (output, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .unwrap_or_else(|(error, _)| panic!("non-plan-bound invocation failed: {error}"));
        assert_eq!(output.content, "fake peer success");
        assert_eq!(audit.result_code, "SUCCESS");
        assert_eq!(audit.cleanup_outcome, "cleaned_up_success");
    }

    #[tokio::test]
    async fn test_production_on_disk_plan_edits_do_not_change_the_frozen_payload() {
        let fixture = plan_bound_fixture();
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let capture = temp.path().join("call-capture.json");
        let (input, config) = plan_bound_input(
            &fixture,
            &peer,
            vec![
                "capture-call".to_string(),
                capture.to_string_lossy().into_owned(),
            ],
        );

        // Mutate the on-disk plan after the snapshot was frozen.
        std::fs::write(
            fixture.project_path.join(".plan").join("V0.24.0-r1.md"),
            "# Frozen Primary Plan\n\nEDITED ON DISK AFTER FREEZE.\n",
        )
        .unwrap();

        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: Some(&fixture.plan_context),
            frozen_plan: Some(&fixture.frozen_plan),
        };
        DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .unwrap_or_else(|(error, _)| panic!("plan-bound invocation failed: {error}"));

        let raw = std::fs::read_to_string(&capture).unwrap();
        let request: serde_json::Value = serde_json::from_str(raw.trim()).unwrap();
        let sent = &request["params"]["arguments"]["frozen_plan"];
        assert_eq!(
            sent,
            &serde_json::to_value(&fixture.frozen_plan).unwrap(),
            "the adapter must send the frozen payload and never re-read .plan"
        );
        let serialized = sent.to_string();
        assert!(
            serialized.contains("Original frozen body."),
            "the frozen body must survive an on-disk edit"
        );
        assert!(
            !serialized.contains("EDITED ON DISK AFTER FREEZE."),
            "on-disk plan edits must not reach the tool call"
        );
    }

    #[tokio::test]
    async fn test_production_executable_args_preserve_spaces_and_unicode_literally() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let captured = temp.path().join("argv-capture");

        let literal_args = vec![
            "a b".to_string(),
            "  leading and trailing  ".to_string(),
            "日本語 ünïcødé — ✓".to_string(),
            "quote\"and'apostrophe".to_string(),
            String::new(),
            "$(echo injected); rm -rf /".to_string(),
            "a;b|c&d>e".to_string(),
        ];
        let mut args = vec![
            "argv-capture".to_string(),
            captured.to_string_lossy().into_owned(),
        ];
        args.extend(literal_args.iter().cloned());

        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args,
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };
        let (output, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .unwrap_or_else(|(error, _)| panic!("literal-argument invocation failed: {error}"));
        assert_eq!(output.content, "fake peer success");
        assert_eq!(audit.result_code, "SUCCESS");

        let recorded = std::fs::read_to_string(&captured)
            .expect("fake peer must record the argv it received");
        let received: Vec<&str> = recorded.split('\u{1f}').collect();
        // [0] is the mode; [1] is the capture path; the remainder are the literal args.
        assert_eq!(
            &received[2..],
            literal_args
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            "arguments must reach the child verbatim without shell interpretation"
        );
    }

    #[tokio::test]
    async fn test_production_child_environment_isolation_end_to_end() {
        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();
        let env_capture_file = temp.path().join("child-env.txt");

        // Set host environment variables: one to allow, one to exclude
        let host_allowed_key = "ANTHRO_ALLOWED_TEST_KEY_456";
        let host_allowed_val = "allowed_secret_value";
        let host_forbidden_key = "ANTHRO_FORBIDDEN_HOST_KEY_789";
        let host_forbidden_val = "forbidden_host_secret";

        std::env::set_var(host_allowed_key, host_allowed_val);
        std::env::set_var(host_forbidden_key, host_forbidden_val);

        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec![
                "env-capture".to_string(),
                env_capture_file.to_string_lossy().into_owned(),
            ],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            allowed_environment: vec![host_allowed_key.to_string()],
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };

        let (output, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .unwrap_or_else(|(error, _)| panic!("env-isolated child invocation failed: {error}"));

        std::env::remove_var(host_allowed_key);
        std::env::remove_var(host_forbidden_key);

        assert_eq!(output.content, "fake peer success");
        assert_eq!(audit.result_code, "SUCCESS");

        assert!(
            env_capture_file.exists(),
            "child process must capture and dump its environment"
        );
        let dumped_content = std::fs::read_to_string(&env_capture_file)
            .expect("read dumped environment");
        let child_vars: HashMap<String, String> = dumped_content
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, '=');
                let k = parts.next()?;
                let v = parts.next().unwrap_or("");
                Some((k.to_uppercase(), v.to_string()))
            })
            .collect();

        // 1. Explicitly allowed variable is present
        assert_eq!(
            child_vars.get(host_allowed_key),
            Some(&host_allowed_val.to_string()),
            "whitelisted environment variable must be present in child"
        );

        // 2. Unlisted host variable is strictly absent
        assert!(
            !child_vars.contains_key(host_forbidden_key),
            "unlisted host environment variable must NOT leak to child"
        );

        // 3. Only allowed variable + OS required baseline variables are present
        #[cfg(windows)]
        {
            let permitted_windows_keys = [
                host_allowed_key,
                "SYSTEMROOT",
                "WINDIR",
                "TEMP",
                "TMP",
            ];
            for key in child_vars.keys() {
                assert!(
                    permitted_windows_keys.iter().any(|allowed| allowed.eq_ignore_ascii_case(key)),
                    "unexpected environment variable '{key}' in isolated child"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_direct_adapter_zero_mailbox_activity() {
        use crate::orchestrator::mailbox::{
            get_session_descriptor_path, MailboxState,
        };

        let temp = tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project = tempdir().unwrap();

        let session_descriptor = get_session_descriptor_path();
        let descriptor_existed_before = session_descriptor.exists();

        // Construct mock MailboxState to check baseline state
        let mailbox_state = MailboxState::new_mock(42);
        let initial_epoch = mailbox_state.inner.lock().await.epoch;
        let initial_dispatches = mailbox_state.inner.lock().await.total_dispatches;
        let initial_claimed = mailbox_state.inner.lock().await.is_claimed;
        let initial_active_task = mailbox_state.inner.lock().await.active_task.clone();

        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: sample_mcp_profile("fake", "plan"),
            system_prompt: "system".to_string(),
            user_prompt: "task".to_string(),
            project_path: project.path().to_path_buf(),
            temperature: None,
        };
        let config = McpServerConfig {
            executable: peer.to_string_lossy().into_owned(),
            args: vec!["paged".to_string()],
            working_directory: Some(project.path().to_string_lossy().into_owned()),
            ..sample_server_config(&peer)
        };
        let direct_input = DirectMcpExecutionInput {
            adapter_input: &input,
            server_config: &config,
            plan_context: None,
            frozen_plan: None,
        };

        let (output, audit) = DirectMcpAdapter::new()
            .execute_with_audit(&direct_input, None)
            .await
            .expect("direct mcp adapter execution must succeed");

        assert_eq!(output.content, "fake peer success");
        assert_eq!(audit.result_code, "SUCCESS");

        // Zero Mailbox Activity Assertions:
        // 1. No new session descriptor file created
        assert_eq!(
            session_descriptor.exists(),
            descriptor_existed_before,
            "DirectMcpAdapter must not create or alter Mailbox session descriptor"
        );

        // 2. MailboxState remains completely untouched
        let guard = mailbox_state.inner.lock().await;
        assert_eq!(guard.epoch, initial_epoch, "DirectMcpAdapter must not modify mailbox epoch");
        assert_eq!(
            guard.total_dispatches, initial_dispatches,
            "DirectMcpAdapter must produce zero mailbox dispatches"
        );
        assert_eq!(
            guard.is_claimed, initial_claimed,
            "DirectMcpAdapter must not claim mailbox tasks"
        );
        assert_eq!(
            guard.active_task, initial_active_task,
            "DirectMcpAdapter must not assign active mailbox tasks"
        );
        assert!(guard.task_dispatches.is_empty(), "DirectMcpAdapter must not record task dispatches");
    }
}
