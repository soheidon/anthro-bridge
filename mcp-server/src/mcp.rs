//! MCP tool layer: defines the `plan` tool, validates its arguments, builds the
//! planning prompts, and converts provider results/errors into MCP types.
//!
//! Prompt ownership lives here; provider HTTP details live in
//! [`crate::provider`].

use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::provider::{PlannerProvider, ProviderError};

/// Arguments accepted by the `plan` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PlanParams {
    /// A concise statement of what the user wants changed or implemented.
    #[schemars(description = "A concise statement of what to change or implement")]
    pub task: String,

    /// Relevant repository information collected by the calling agent.
    #[schemars(description = "Relevant repository context collected by the calling agent")]
    pub context: String,

    /// Explicit limitations or constraints (optional).
    #[schemars(description = "Explicit limitations or constraints (optional)")]
    pub constraints: Option<String>,

    /// Authoritative structured Plan Context provided by the Orchestrator (optional).
    #[schemars(description = "Authoritative structured Plan Context provided by the Orchestrator (optional)")]
    pub plan_context: Option<String>,

    /// Authoritative structured FrozenPlanPayload provided by the Orchestrator (required for plan-bound requests).
    #[schemars(description = "Structured frozen plan payload. Required whenever plan_context identifies a resolved plan.")]
    pub frozen_plan: Option<StructuredFrozenPlanPayload>,
}

/// Arguments accepted by the `review` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReviewParams {
    /// Concise description of the overarching goal or task.
    #[schemars(description = "Concise summary of the task being reviewed")]
    pub task: String,

    /// The exact text of the approved implementation plan.
    #[schemars(description = "The approved implementation plan against which the diff is judged")]
    pub approved_plan: String,

    /// Complete diff of tracked changes (git diff HEAD) plus review-relevant untracked file contents.
    #[schemars(description = "Complete diff containing tracked changes (git diff HEAD) and review-relevant untracked file contents")]
    pub git_diff: String,

    /// Current working tree status (`git status --short`).
    #[schemars(description = "Summary of modified, added, untracked, and deleted files (git status --short)")]
    pub git_status: String,

    /// Automated test results and validation logs (optional for doc-only changes).
    #[schemars(description = "Automated test execution results, pass/fail counts, and validation logs")]
    pub test_results: Option<String>,

    /// Review depth mode: "standard" (default) or "deep".
    #[schemars(description = "Review depth: 'standard' or 'deep'")]
    pub review_mode: Option<String>,

    /// Supplementary repository context, surrounding contracts, call sites, or constraints collected by the calling agent.
    #[schemars(description = "Supplementary repository context, surrounding contracts, call sites, or constraints collected by the calling agent")]
    pub additional_context: Option<String>,
}

/// Arguments accepted by the `orchestrator_claim_task` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaimTaskParams {
    /// Optional run ID to match. If omitted, claims the current active run.
    #[schemars(description = "Optional run ID to match. If omitted, claims the current active run.")]
    #[serde(alias = "run_id")]
    pub run_id: Option<String>,

    /// Timeout in seconds to wait (long-poll) if the orchestrator is in automated validation/review (default: 30)
    #[schemars(description = "Timeout in seconds to wait (long-poll) if the orchestrator is in automated validation/review (default: 30)")]
    #[serde(alias = "wait_seconds")]
    pub wait_seconds: Option<u64>,
}

/// Arguments accepted by the `orchestrator_report_progress` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportProgressParams {
    /// The active run ID
    #[schemars(description = "The active run ID")]
    #[serde(alias = "run_id")]
    pub run_id: String,

    /// The active task ID being executed
    #[schemars(description = "The active task ID being executed")]
    #[serde(alias = "task_id")]
    pub task_id: String,

    /// The claim epoch granted when claiming the task
    #[schemars(description = "The claim epoch granted when claiming the task")]
    pub epoch: u64,

    /// Human-readable progress message to display on the timeline
    #[schemars(description = "Human-readable progress message to display on the timeline")]
    pub message: String,

    /// Optional percentage (0-100)
    #[schemars(description = "Optional percentage (0-100)")]
    pub percent: Option<u32>,
}

/// Arguments accepted by the `orchestrator_submit_result` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTaskParams {
    /// The active run ID
    #[schemars(description = "The active run ID")]
    #[serde(alias = "run_id")]
    pub run_id: String,

    /// The active task ID being submitted
    #[schemars(description = "The active task ID being submitted")]
    #[serde(alias = "task_id")]
    pub task_id: String,

    /// The claim epoch granted when claiming the task
    #[schemars(description = "The claim epoch granted when claiming the task")]
    pub epoch: u64,

    /// Optional idempotency key to prevent duplicate submissions
    #[schemars(description = "Optional idempotency key to prevent duplicate submissions")]
    #[serde(alias = "idempotency_key")]
    pub idempotency_key: Option<String>,

    /// Task completion status (e.g. 'success' or 'failed')
    #[schemars(description = "Task completion status (e.g. 'success' or 'failed')")]
    pub status: String,

    /// Summary of changes made or results achieved
    #[schemars(description = "Summary of changes made or results achieved")]
    pub summary: String,

    /// List of modified, created, or touched files
    #[schemars(description = "List of modified, created, or touched files")]
    #[serde(default)]
    #[serde(alias = "modified_files")]
    pub modified_files: Vec<String>,
}

/// Helper to find the session descriptor file.
pub fn get_session_descriptor_path() -> std::path::PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        std::path::PathBuf::from(appdata)
            .join("Anthro Bridge")
            .join("orchestrator_session.json")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".config")
            .join("anthro-bridge")
            .join("orchestrator_session.json")
    } else {
        std::env::temp_dir()
            .join("anthro-bridge")
            .join("orchestrator_session.json")
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EphemeralSession {
    pub run_id: String,
    pub project_path: String,
    pub port: u16,
    pub token: String,
    #[serde(alias = "created_at")]
    pub created_at: u64,
    #[serde(alias = "expires_at")]
    pub expires_at: u64,
}

pub fn read_session_descriptor() -> Result<EphemeralSession, ErrorData> {
    let session_path = get_session_descriptor_path();
    if !session_path.exists() {
        return Err(ErrorData::internal_error(
            format!(
                "No active Anthro Bridge orchestrator session found at '{}'. Please start a run in Anthro Bridge GUI.",
                session_path.display()
            ),
            None,
        ));
    }
    let content = std::fs::read_to_string(&session_path).map_err(|e| {
        ErrorData::internal_error(format!("Failed to read session file: {e}"), None)
    })?;
    let session: EphemeralSession = serde_json::from_str(&content).map_err(|e| {
        ErrorData::internal_error(format!("Invalid session file format: {e}"), None)
    })?;
    Ok(session)
}

/// The MCP planner and reviewer tool handler. Generic over the planner provider so tests can
/// inject a fake provider.
pub struct PlannerTool<P: PlannerProvider> {
    provider: Arc<P>,
}

impl<P: PlannerProvider> PlannerTool<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
        }
    }
}

#[tool_router]
impl<P: PlannerProvider> PlannerTool<P> {
    #[tool(
        description = "Generate an implementation plan for a software-development task using the supplied task description, repository context, and optional constraints."
    )]
    async fn plan(
        &self,
        Parameters(params): Parameters<PlanParams>,
    ) -> Result<CallToolResult, ErrorData> {
        validate_plan_params(&params)?;

        let system_prompt = build_system_prompt();
        let user_prompt = build_user_prompt_with_plan_context_and_frozen_plan(
            &params.task,
            &params.context,
            params.constraints.as_deref(),
            params.plan_context.as_deref(),
            params.frozen_plan.as_ref(),
        );

        match self.provider.plan(&system_prompt, &user_prompt).await {
            Ok(plan) => Ok(CallToolResult::success(vec![ContentBlock::text(plan.text)])),
            Err(err) => {
                tracing::error!(error = %err, "planner provider error");
                Err(provider_error_to_mcp(err))
            }
        }
    }

    #[tool(
        description = "Evaluate whether an implementation accurately, safely, and cleanly fulfills an approved plan, outputting a 3-tier verdict (Approved, Approved with recommendations, Not approved)."
    )]
    async fn review(
        &self,
        Parameters(params): Parameters<ReviewParams>,
    ) -> Result<CallToolResult, ErrorData> {
        validate_review_params(&params)?;

        let system_prompt = build_review_system_prompt();
        let user_prompt = build_review_user_prompt(&params);

        match self.provider.plan(&system_prompt, &user_prompt).await {
            Ok(plan) => Ok(CallToolResult::success(vec![ContentBlock::text(plan.text)])),
            Err(err) => {
                tracing::error!(error = %err, "reviewer provider error");
                Err(provider_error_to_mcp(err))
            }
        }
    }

    #[tool(
        description = "Claim an active orchestrator task or long-poll if the run is in automated validation or review."
    )]
    async fn orchestrator_claim_task(
        &self,
        Parameters(params): Parameters<ClaimTaskParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let session = read_session_descriptor()?;
        let client = reqwest::Client::new();
        let timeout_secs = params.wait_seconds.unwrap_or(30) + 10;
        let url = format!("http://127.0.0.1:{}/mailbox/claim", session.port);
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", session.token))
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .json(&serde_json::json!({
                "runId": params.run_id.unwrap_or(session.run_id),
                "waitSeconds": params.wait_seconds.unwrap_or(30),
            }))
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to reach orchestrator mailbox: {e}"), None))?;

        match resp.status() {
            reqwest::StatusCode::OK => {
                let body = resp.text().await.map_err(|e| ErrorData::internal_error(format!("Failed to read response body: {e}"), None))?;
                Ok(CallToolResult::success(vec![ContentBlock::text(body)]))
            }
            reqwest::StatusCode::NO_CONTENT => {
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    "No task currently available for claim. Long-poll timed out. Orchestrator may still be processing validation/review or waiting for human approval."
                )]))
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(ErrorData::internal_error("Unauthorized: Session token invalid or expired.", None))
            }
            reqwest::StatusCode::CONFLICT => {
                Err(ErrorData::internal_error("Conflict: Stale lease or worker collision detected.", None))
            }
            status => {
                Err(ErrorData::internal_error(format!("Orchestrator returned unexpected HTTP status: {status}"), None))
            }
        }
    }

    #[tool(
        description = "Report step progress and keep active task lease alive during execution."
    )]
    async fn orchestrator_report_progress(
        &self,
        Parameters(params): Parameters<ReportProgressParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let session = read_session_descriptor()?;
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/mailbox/progress", session.port);
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", session.token))
            .json(&serde_json::json!({
                "runId": params.run_id,
                "taskId": params.task_id,
                "epoch": params.epoch,
                "message": params.message,
                "percent": params.percent,
            }))
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to reach orchestrator mailbox: {e}"), None))?;

        match resp.status() {
            reqwest::StatusCode::OK => {
                Ok(CallToolResult::success(vec![ContentBlock::text("Progress reported successfully.")]))
            }
            reqwest::StatusCode::CONFLICT => {
                Err(ErrorData::internal_error("Conflict: Stale lease or worker collision detected (epoch mismatch).", None))
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(ErrorData::internal_error("Unauthorized: Session token invalid or expired.", None))
            }
            status => {
                Err(ErrorData::internal_error(format!("Orchestrator returned unexpected HTTP status: {status}"), None))
            }
        }
    }

    #[tool(
        description = "Submit completed task results to advance the orchestrator state machine to the next stage."
    )]
    async fn orchestrator_submit_result(
        &self,
        Parameters(params): Parameters<SubmitTaskParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let session = read_session_descriptor()?;
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/mailbox/submit", session.port);
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", session.token))
            .json(&serde_json::json!({
                "runId": params.run_id,
                "taskId": params.task_id,
                "epoch": params.epoch,
                "idempotencyKey": params.idempotency_key,
                "status": params.status,
                "summary": params.summary,
                "modifiedFiles": params.modified_files,
            }))
            .send()
            .await
            .map_err(|e| ErrorData::internal_error(format!("Failed to reach orchestrator mailbox: {e}"), None))?;

        match resp.status() {
            reqwest::StatusCode::OK => {
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    format!("Task '{}' submitted successfully. Orchestrator advancing to next stage.", params.task_id)
                )]))
            }
            reqwest::StatusCode::CONFLICT => {
                Err(ErrorData::internal_error("Conflict: Stale lease or worker collision detected (epoch mismatch).", None))
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(ErrorData::internal_error("Unauthorized: Session token invalid or expired.", None))
            }
            status => {
                Err(ErrorData::internal_error(format!("Orchestrator returned unexpected HTTP status: {status}"), None))
            }
        }
    }
}

#[tool_handler(
    instructions = "Generate implementation plans and evaluate code implementations using a configured external model."
)]
impl<P: PlannerProvider> ServerHandler for PlannerTool<P> {}

/// Structured PlanContext serialization schema for authoritative MCP forwarding.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredPlanContext {
    pub project_root_identity: String,
    pub plan_directory: String,
    pub application_version: String,
    pub plan_series_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_primary_plan: Option<StructuredPlanFileRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_supplemental_plans: Vec<StructuredSupplementalPlanRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_leaf_plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_plan_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_primary_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_primary_revision: Option<u64>,
    pub resolver_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved_reason_code: Option<String>,
}

impl StructuredPlanContext {
    fn is_plan_bound(&self) -> bool {
        self.resolver_status.eq_ignore_ascii_case("resolved")
            && (self.current_leaf_plan_id.is_some()
                || self.current_primary_plan.is_some()
                || !self.active_supplemental_plans.is_empty()
                || self.effective_plan_digest.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredPlanFileRecord {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredSupplementalPlanRecord {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub suffix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredPlanFileIdentityAndDigest {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredSupplementalPlanIdentityAndDigest {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub suffix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StructuredFrozenPlanSourceKind {
    Primary,
    Supplemental,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredFrozenPlanSource {
    pub kind: StructuredFrozenPlanSourceKind,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    pub path: String,
    pub source_digest: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredFrozenPlanPayload {
    pub schema_version: u32,
    pub effective_plan_content: String,
    pub content_digest: String,
    pub effective_plan_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_plan_identity_and_digest: Option<StructuredPlanFileIdentityAndDigest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordered_supplemental_plan_identities_and_digests: Vec<StructuredSupplementalPlanIdentityAndDigest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordered_sources: Vec<StructuredFrozenPlanSource>,
}

pub fn canonical_assemble_sources(sources: &[StructuredFrozenPlanSource]) -> Result<String, String> {
    let mut out = String::new();
    for (i, source) in sources.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        if source.id.contains('"') || source.id.contains('\n') || source.id.contains('\r') {
            return Err(format!("Invalid plan source id '{}'", source.id));
        }
        if let Some(ref suffix) = source.suffix {
            if suffix.contains('"') || suffix.contains('\n') || suffix.contains('\r') {
                return Err(format!("Invalid plan source suffix '{}'", suffix));
            }
        }

        match source.kind {
            StructuredFrozenPlanSourceKind::Primary => {
                out.push_str(&format!(
                    "<<<PLAN_SOURCE kind=\"primary\" id=\"{}\">>>\n{}\n<<<END_PLAN_SOURCE>>>",
                    source.id, source.content
                ));
            }
            StructuredFrozenPlanSourceKind::Supplemental => {
                let suffix_attr = match source.suffix {
                    Some(ref s) => format!(" suffix=\"{}\"", s),
                    None => String::new(),
                };
                out.push_str(&format!(
                    "<<<PLAN_SOURCE kind=\"supplemental\" id=\"{}\"{}>\n{}\n<<<END_PLAN_SOURCE>>>",
                    source.id, suffix_attr, source.content
                ));
            }
        }
    }
    Ok(out)
}

pub fn format_structured_plan_context(ctx: &StructuredPlanContext) -> String {
    let mut s = String::new();
    s.push_str("## Plan Workspace Context\n");
    s.push_str(&format!("- **Status**: {}\n", ctx.resolver_status));
    s.push_str(&format!("- **Application Version**: {}\n", ctx.application_version));
    s.push_str(&format!("- **Plan Series Version**: {}\n", ctx.plan_series_version));
    if let Some(ref p) = ctx.current_primary_plan {
        s.push_str(&format!("- **Primary Plan**: {} (revision: {}, digest: {})\n", p.id, p.revision, p.digest));
    }
    if !ctx.active_supplemental_plans.is_empty() {
        s.push_str("- **Active Supplemental Plans**:\n");
        for supp in &ctx.active_supplemental_plans {
            s.push_str(&format!("  - {} (suffix: {}, digest: {})\n", supp.id, supp.suffix, supp.digest));
        }
    }
    if let Some(ref leaf_id) = ctx.current_leaf_plan_id {
        s.push_str(&format!("- **Current Leaf Plan**: {}\n", leaf_id));
    }
    if let Some(ref digest) = ctx.effective_plan_digest {
        s.push_str(&format!("- **Effective Plan Digest**: {}\n", digest));
    }
    s
}

fn validate_plan_params(params: &PlanParams) -> Result<(), ErrorData> {
    if params.task.trim().is_empty() {
        return Err(ErrorData::invalid_params("`task` must not be empty", None));
    }
    if params.context.trim().is_empty() {
        return Err(ErrorData::invalid_params(
            "`context` must not be empty",
            None,
        ));
    }

    let parsed_plan_ctx = if let Some(ref raw_ctx) = params.plan_context {
        let trimmed = raw_ctx.trim();
        if !trimmed.is_empty() {
            let parsed: Result<StructuredPlanContext, _> = serde_json::from_str(trimmed);
            match parsed {
                Ok(ctx) => Some(ctx),
                Err(err) => {
                    return Err(ErrorData::invalid_params(
                        format!("`plan_context` must be a valid serialized PlanContext JSON object: {err}"),
                        None,
                    ));
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let is_plan_bound = parsed_plan_ctx.as_ref().is_some_and(StructuredPlanContext::is_plan_bound);

    if is_plan_bound && params.frozen_plan.is_none() {
        return Err(ErrorData::invalid_params(
            "`frozen_plan` is required when `plan_context` identifies a resolved plan",
            None,
        ));
    }

    if let Some(payload) = params.frozen_plan.as_ref() {
        if payload.schema_version != 1 {
            return Err(ErrorData::invalid_params(
                format!("`frozen_plan.schema_version` is unsupported: {}", payload.schema_version),
                None,
            ));
        }

        let plan_ctx = parsed_plan_ctx.as_ref().ok_or_else(|| {
            ErrorData::invalid_params(
                "`frozen_plan` requires its matching `plan_context`",
                None,
            )
        })?;
        if !plan_ctx.is_plan_bound() {
            return Err(ErrorData::invalid_params(
                "`frozen_plan` requires a resolved plan-bound `plan_context`",
                None,
            ));
        }

        // Rule 1: Validate each source content digest
        for src in &payload.ordered_sources {
            let computed_src_digest = format!("{:x}", Sha256::digest(src.content.as_bytes()));
            if src.source_digest != computed_src_digest {
                return Err(ErrorData::invalid_params(
                    format!(
                        "`frozen_plan` source '{}' digest mismatch: recorded '{}', computed '{}'",
                        src.id, src.source_digest, computed_src_digest
                    ),
                    None,
                ));
            }
        }

        // Rule 2: Validate ordered_sources against plan_context primary + supplementals
        let expected_source_count = (if plan_ctx.current_primary_plan.is_some() { 1 } else { 0 })
            + plan_ctx.active_supplemental_plans.len();
        if payload.ordered_sources.len() != expected_source_count {
            return Err(ErrorData::invalid_params(
                format!(
                    "`frozen_plan.ordered_sources` count mismatch with `plan_context`: {} vs {}",
                    payload.ordered_sources.len(),
                    expected_source_count
                ),
                None,
            ));
        }

        let mut src_idx = 0;
        if let Some(ref primary) = plan_ctx.current_primary_plan {
            let primary_src = &payload.ordered_sources[src_idx];
            if primary_src.kind != StructuredFrozenPlanSourceKind::Primary
                || primary_src.id != primary.id
                || primary_src.path != primary.path
                || primary_src.source_digest != primary.digest
                || primary_src.suffix.is_some()
            {
                return Err(ErrorData::invalid_params(
                    "`frozen_plan` primary source mismatch with `plan_context`",
                    None,
                ));
            }
            src_idx += 1;
        }

        for supp in &plan_ctx.active_supplemental_plans {
            let supp_src = &payload.ordered_sources[src_idx];
            if supp_src.kind != StructuredFrozenPlanSourceKind::Supplemental
                || supp_src.id != supp.id
                || supp_src.path != supp.path
                || supp_src.source_digest != supp.digest
                || supp_src.suffix.as_deref() != Some(&supp.suffix)
            {
                return Err(ErrorData::invalid_params(
                    format!(
                        "`frozen_plan` supplemental source mismatch at index {} with `plan_context`",
                        src_idx
                    ),
                    None,
                ));
            }
            src_idx += 1;
        }

        // Rule 3: Validate canonical assembly of ordered_sources == effective_plan_content
        let assembled = canonical_assemble_sources(&payload.ordered_sources)
            .map_err(|e| ErrorData::invalid_params(format!("Canonical assembly failure: {e}"), None))?;
        if payload.effective_plan_content != assembled {
            return Err(ErrorData::invalid_params(
                "`frozen_plan.effective_plan_content` does not match canonical assembly of `ordered_sources`",
                None,
            ));
        }

        // Rule 4: Validate content_digest == sha256(effective_plan_content)
        let computed_content_digest = format!("{:x}", Sha256::digest(payload.effective_plan_content.as_bytes()));
        if payload.content_digest != computed_content_digest {
            return Err(ErrorData::invalid_params(
                format!(
                    "`frozen_plan.content_digest` mismatch: expected '{}', got '{}'",
                    computed_content_digest, payload.content_digest
                ),
                None,
            ));
        }

        // Rule 5: Validate effective_plan_digest == plan_context.effective_plan_digest
        let ctx_digest = plan_ctx.effective_plan_digest.as_ref().ok_or_else(|| {
            ErrorData::invalid_params(
                "`plan_context.effective_plan_digest` is required with `frozen_plan`",
                None,
            )
        })?;
        if &payload.effective_plan_digest != ctx_digest {
            return Err(ErrorData::invalid_params(
                format!(
                    "`frozen_plan.effective_plan_digest` '{}' does not match `plan_context.effective_plan_digest` '{}'",
                    payload.effective_plan_digest, ctx_digest
                ),
                None,
            ));
        }

        match (&payload.primary_plan_identity_and_digest, &plan_ctx.current_primary_plan) {
            (Some(p_id), Some(ctx_primary)) => {
                if p_id.id != ctx_primary.id
                    || p_id.path != ctx_primary.path
                    || p_id.digest != ctx_primary.digest
                    || p_id.revision != ctx_primary.revision
                {
                    return Err(ErrorData::invalid_params(
                        "`frozen_plan` primary identity does not match `plan_context`",
                        None,
                    ));
                }
            }
            (None, None) => {}
            _ => {
                return Err(ErrorData::invalid_params(
                    "`frozen_plan` primary presence does not match `plan_context`",
                    None,
                ));
            }
        }

        if payload.ordered_supplemental_plan_identities_and_digests.len()
            != plan_ctx.active_supplemental_plans.len()
        {
            return Err(ErrorData::invalid_params(
                "`frozen_plan` supplemental source count does not match `plan_context`",
                None,
            ));
        }
        for (payload_supp, ctx_supp) in payload
            .ordered_supplemental_plan_identities_and_digests
            .iter()
            .zip(&plan_ctx.active_supplemental_plans)
        {
            if payload_supp.id != ctx_supp.id
                || payload_supp.path != ctx_supp.path
                || payload_supp.digest != ctx_supp.digest
                || payload_supp.suffix != ctx_supp.suffix
            {
                return Err(ErrorData::invalid_params(
                    "`frozen_plan` supplemental source identity does not match `plan_context`",
                    None,
                ));
            }
        }
    }

    Ok(())
}

pub fn resolve_review_mode(mode: Option<&str>) -> Result<&'static str, ErrorData> {
    match mode {
        None => Ok("standard"),
        Some(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed == "standard" {
                Ok("standard")
            } else if trimmed == "deep" {
                Ok("deep")
            } else {
                Err(ErrorData::invalid_params(
                    format!(
                        "Invalid `review_mode`: '{trimmed}'. Only 'standard' and 'deep' are supported."
                    ),
                    None,
                ))
            }
        }
    }
}

fn validate_review_params(params: &ReviewParams) -> Result<(), ErrorData> {
    if params.task.trim().is_empty() {
        return Err(ErrorData::invalid_params("`task` must not be empty", None));
    }
    if params.approved_plan.trim().is_empty() {
        return Err(ErrorData::invalid_params(
            "`approved_plan` must not be empty",
            None,
        ));
    }
    if params.git_diff.trim().is_empty() {
        return Err(ErrorData::invalid_params(
            "`git_diff` must not be empty",
            None,
        ));
    }
    if params.git_status.trim().is_empty() {
        return Err(ErrorData::invalid_params(
            "`git_status` must not be empty",
            None,
        ));
    }
    resolve_review_mode(params.review_mode.as_deref())?;
    Ok(())
}

/// Builds the planner system prompt (constant role instructions).
pub fn build_system_prompt() -> String {
    [
        "You are a software implementation planner, not an implementation agent.",
        "You produce a concrete, ordered implementation plan that will be handed to an autonomous coding agent, which will perform the actual repository reads, edits, builds, and tests.",
        "",
        "Follow these rules strictly:",
        "",
        "- Base every conclusion only on the context supplied in the user message. Do not invent files, functions, types, or behavior that are not mentioned there.",
        "- Explicitly distinguish confirmed facts (stated in the context) from assumptions (inferred or guessed). Label assumptions clearly.",
        "- Prefer minimal, targeted changes. Avoid broad refactors or unrelated cleanup unless clearly required by the task.",
        "- Preserve existing behavior outside the requested scope.",
        "- Where the supplied context allows, identify the exact files and functions or components likely to change.",
        "- Provide an ordered sequence of implementation steps.",
        "- Provide concrete verification steps, including relevant build and test commands where you can infer them.",
        "- When the supplied context is insufficient, state explicitly what is missing rather than guessing.",
        "- Never claim that a change has already been made.",
        "- Never invent or fabricate test results.",
        "- Never assume access to the repository beyond the supplied context; you cannot read, edit, or execute anything yourself.",
        "",
        "Return a plan that can be followed directly by an autonomous coding agent.",
    ]
    .join("\n")
}

/// Builds the user prompt from the task, context, and optional constraints.
pub fn build_user_prompt(task: &str, context: &str, constraints: Option<&str>) -> String {
    build_user_prompt_with_plan_context_and_frozen_plan(task, context, constraints, None, None)
}

/// Builds the user prompt from the task, context, optional constraints, and optional plan context.
pub fn build_user_prompt_with_plan_context(
    task: &str,
    context: &str,
    constraints: Option<&str>,
    plan_context: Option<&str>,
) -> String {
    build_user_prompt_with_plan_context_and_frozen_plan(task, context, constraints, plan_context, None)
}

/// Builds the user prompt from the task, context, optional constraints, optional plan context, and optional frozen plan.
pub fn build_user_prompt_with_plan_context_and_frozen_plan(
    task: &str,
    context: &str,
    constraints: Option<&str>,
    plan_context: Option<&str>,
    frozen_plan: Option<&StructuredFrozenPlanPayload>,
) -> String {
    let mut sections = vec![
        format!("## Task\n\n{}", task),
        format!("## Repository context\n\n{}", context),
    ];

    if let Some(payload) = frozen_plan {
        sections.push(format!(
            "## Approved Frozen Implementation Plan\n\n{}",
            payload.effective_plan_content
        ));
    }

    if let Some(pc) = plan_context {
        if !pc.trim().is_empty() {
            if let Ok(ctx) = serde_json::from_str::<StructuredPlanContext>(pc.trim()) {
                sections.push(format_structured_plan_context(&ctx));
            } else {
                sections.push(format!("## Plan Context\n\n{}", pc.trim()));
            }
        }
    }

    if let Some(constraints) = constraints {
        if !constraints.trim().is_empty() {
            sections.push(format!("## Constraints\n\n{}", constraints));
        }
    }

    sections.join("\n\n")
}

/// Builds the review system prompt (constant reviewer role instructions).
pub fn build_review_system_prompt() -> String {
    [
        "You are a strict, read-only software implementation reviewer, not an implementation agent.",
        "Your task is to independently evaluate whether the supplied implementation changes accurately, safely, and cleanly fulfill the approved plan.",
        "",
        "Follow these rules strictly:",
        "",
        "1. Core Principles:",
        "- You are read-only. Do not output patches, do not write replacement code, and do not execute modifications.",
        "- Tests passing are necessary but not sufficient for approval. You must actively inspect the diff, boundary conditions, contracts, and repository context.",
        "- If test_results are absent: determine whether automated tests were reasonably required for the changes. If code logic changed but no test evidence is provided, this may be a blocking issue. If the change is documentation-only or otherwise does not reasonably require tests, absence of test results is not itself a blocker.",
        "- Do not guess or assume repository contents beyond what is provided in the prompt. Base your evaluation strictly on the approved plan, git diff, git status, test results, and additional context provided.",
        "",
        "2. Review Dimensions:",
        "Always evaluate:",
        "- Scope compliance: verify no unauthorized files or unrelated subsystems were modified (cross-referencing git_status and untracked files).",
        "- Plan compliance: map every requirement from the approved plan to concrete implementation evidence.",
        "- Contract preservation: verify public APIs, return types, schemas, and backward-compatible fallbacks remain intact.",
        "- Algorithmic correctness: check logic branches, boundary conditions, indexing, nullability, and state invariants.",
        "- Failure & error handling: verify error propagation, message sanitization, and graceful degradation.",
        "- Test adequacy: check whether tests cover edge cases and failure modes, not just happy paths.",
        "- Diff hygiene: detect leftover debug code, temporary artifacts, formatting churn, or unintentional EOL changes.",
        "Evaluate when relevant (or in deep review mode):",
        "- Determinism & RNG: execution ordering, seed handling, reproducibility.",
        "- Concurrency & parallelism: lock contention, async lifetimes, race conditions, deadlock risks.",
        "- Performance & memory: expensive allocations in hot paths, quadratic loops, unnecessary cloning.",
        "",
        "3. Output Format & Verdict Rules:",
        "Your response MUST begin with the following decision header:",
        "",
        "# Review Summary",
        "",
        "Decision: <Approved | Approved with recommendations | Not approved>",
        "Commit readiness: <READY | NOT READY>",
        "",
        "Verdict definitions:",
        "- 'Decision: Approved' with 'Commit readiness: READY': all plan requirements, contracts, and test obligations are verified. No blocking issues.",
        "- 'Decision: Approved with recommendations' with 'Commit readiness: READY': the implementation is safe to commit as-is. All recommendations are strictly optional enhancements or non-functional polish that do not block committing.",
        "- 'Decision: Not approved' with 'Commit readiness: NOT READY': one or more blocking issues exist (broken contract, omitted requirement, missing required tests, regressions, or unauthorized file changes).",
        "",
        "Follow the header with:",
        "",
        "## Plan Compliance Matrix",
        "A Markdown table mapping each requirement from the approved plan to implementation location, evidence, and PASS/FAIL status.",
        "",
        "## Blocking Issues",
        "If none: '- None'. Otherwise, for each blocker include:",
        "### <Number>. <Title>",
        "- Severity: Blocking",
        "- Location: <file:line>",
        "- Problem: <clear defect description>",
        "- Why it matters: <impact on safety, correctness, or contracts>",
        "- Required fix: <concrete guidance for the build agent>",
        "",
        "## Recommendations & Non-Blocking Notes",
        "If none: '- None'. List any optional suggestions or future considerations.",
        "",
        "## Verification & Test Assessment",
        "A concise assessment of test execution status and coverage adequacy.",
    ]
    .join("\n")
}

/// Builds the review user prompt from `ReviewParams`.
pub fn build_review_user_prompt(params: &ReviewParams) -> String {
    let mut sections = vec![
        format!("## Task Summary\n\n{}", params.task),
        format!("## Approved Implementation Plan\n\n{}", params.approved_plan),
        format!("## Working Tree Status (git status)\n\n{}", params.git_status),
        format!("## Implementation Diff\n\n{}", params.git_diff),
    ];

    if let Some(ref tests) = params.test_results {
        if !tests.trim().is_empty() {
            sections.push(format!("## Test Results\n\n{}", tests));
        }
    } else {
        sections.push("## Test Results\n\n[No test results provided]".to_string());
    }

    let effective_mode = resolve_review_mode(params.review_mode.as_deref()).unwrap_or("standard");
    sections.push(format!("## Review Mode\n\n{}", effective_mode));

    if let Some(ref ctx) = params.additional_context {
        if !ctx.trim().is_empty() {
            sections.push(format!("## Additional Repository Context & Contracts\n\n{}", ctx));
        }
    }

    sections.join("\n\n")
}

fn provider_error_to_mcp(err: ProviderError) -> ErrorData {
    ErrorData::internal_error(err.to_string(), None)
}
