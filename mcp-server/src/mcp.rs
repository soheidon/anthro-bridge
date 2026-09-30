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
use serde::Deserialize;

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
        let user_prompt =
            build_user_prompt(&params.task, &params.context, params.constraints.as_deref());

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
    let mut sections = vec![
        format!("## Task\n\n{}", task),
        format!("## Repository context\n\n{}", context),
    ];

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
