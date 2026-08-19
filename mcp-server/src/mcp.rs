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

/// The `plan` tool handler. Generic over the planner provider so tests can
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
        validate_params(&params)?;

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
}

#[tool_handler(
    instructions = "Generate implementation plans using a configured external planner model."
)]
impl<P: PlannerProvider> ServerHandler for PlannerTool<P> {}

fn validate_params(params: &PlanParams) -> Result<(), ErrorData> {
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

fn provider_error_to_mcp(err: ProviderError) -> ErrorData {
    ErrorData::internal_error(err.to_string(), None)
}
