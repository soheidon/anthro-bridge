use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(test)]
use std::collections::VecDeque;
#[cfg(test)]
use std::sync::Mutex;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::adapters::codex_cli::CodexCliAdapter;
use super::adapters::ollama::OllamaAdapter;
use super::adapters::provider::ProviderAdapter;
use super::adapters::{AdapterExecutionInput, AdapterExecutionOutput};
use super::context_builder::{BuiltContext, ContextBuilder};
use super::finding_aggregator::FindingAggregator;
use super::token_estimator::TokenCountQuality;
use super::types::{
    validate_role_capabilities, AgentRole, AuthorizedCustomGate, ExecutionAdapterType,
    OrchestratorProfile, ReviewFinding, ReviewResult, ReviewVerdict, RunConfigurationSnapshot,
    RunControlState, WorkflowState,
};
use super::validation::{ValidationRunSummary, ValidationRunner};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepProgressEvent {
    #[serde(alias = "run_id")]
    pub run_id: String,
    pub step: WorkflowState,
    #[serde(alias = "iteration_info")]
    pub iteration_info: Option<String>,
    pub message: String,
    #[serde(alias = "review_result")]
    pub review_result: Option<ReviewResult>,
    #[serde(alias = "validation_summary")]
    pub validation_summary: Option<ValidationRunSummary>,
    #[serde(alias = "plan_text")]
    pub plan_text: Option<String>,
}

#[derive(Debug, Clone)]
pub enum BlockingResolution {
    Retry { guidance: Option<String> },
    Abort,
}

pub type EventCallback = Arc<dyn Fn(StepProgressEvent) + Send + Sync>;
pub type LogCallback = Arc<dyn Fn(super::types::RunLogEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewAction {
    Approved,
    ChangesRequired,
    NeedsClarification,
    Failed,
}

fn classify_review_verdict(verdict: ReviewVerdict) -> ReviewAction {
    match verdict {
        ReviewVerdict::Approved => ReviewAction::Approved,
        ReviewVerdict::ChangesRequired => ReviewAction::ChangesRequired,
        ReviewVerdict::NeedsClarification => ReviewAction::NeedsClarification,
        ReviewVerdict::Failed => ReviewAction::Failed,
    }
}

#[derive(Debug, Clone)]
pub struct OrchestratorEngine {
    context_builder: ContextBuilder,
    validation_runner: ValidationRunner,
    finding_aggregator: FindingAggregator,
    provider_adapter: ProviderAdapter,
    ollama_adapter: OllamaAdapter,
    codex_cli_adapter: CodexCliAdapter,
    #[cfg(test)]
    scripted_adapter_executor: Option<Arc<ScriptedAdapterExecutor>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct ScriptedAdapterExecutor {
    outputs: Mutex<VecDeque<(AgentRole, AdapterExecutionOutput)>>,
    calls: Mutex<Vec<AgentRole>>,
}

impl Default for OrchestratorEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestratorEngine {
    pub fn new() -> Self {
        Self {
            context_builder: ContextBuilder::new(),
            validation_runner: ValidationRunner::new(),
            finding_aggregator: FindingAggregator::new(),
            provider_adapter: ProviderAdapter::new(),
            ollama_adapter: OllamaAdapter::new(),
            codex_cli_adapter: CodexCliAdapter::new(),
            #[cfg(test)]
            scripted_adapter_executor: None,
        }
    }

    /// Executes the development review workflow adhering to exact terminal conditions, independent counters, and fail-closed reviews.
    pub async fn run_workflow(
        &self,
        run_id: String,
        snapshot: RunConfigurationSnapshot,
        task_prompt: String,
        workflow_type: String, // "full_loop" | "plan_only" | "implement_only" | "review_only"
        authorized_custom_gates: Vec<AuthorizedCustomGate>,
        mut pause_rx: watch::Receiver<RunControlState>,
        cancel_token: CancellationToken,
        mut clarification_rx: mpsc::Receiver<String>,
        mut blocking_resolution_rx: mpsc::Receiver<BlockingResolution>,
        on_event: EventCallback,
        on_log: LogCallback,
    ) -> Result<WorkflowState, String> {
        let log_run_id = run_id.clone();
        let log = move |message: String| {
            on_log(super::types::RunLogEvent {
                run_id: log_run_id.clone(),
                message,
            });
        };
        let project_path = PathBuf::from(&snapshot.project_path);
        if !project_path.exists() {
            return Err(format!(
                "Project path '{}' does not exist.",
                snapshot.project_path
            ));
        }

        // Preflight validation: Reject iteration limits of zero
        if snapshot.iteration_limits.max_plan_review_iterations == 0
            || snapshot.iteration_limits.max_fix_iterations == 0
            || snapshot.iteration_limits.max_code_review_iterations == 0
        {
            return Err("Preflight failure: iteration limits (plan_review, fix, code_review) must all be greater than zero.".to_string());
        }

        // Validate all role capabilities before starting
        for (role, profile) in &snapshot.assignments {
            // Reject MCP adapter
            if profile.adapter == ExecutionAdapterType::Mcp {
                return Err(format!(
                    "MCP adapter is currently unavailable for role '{:?}'.",
                    role
                ));
            }
            validate_role_capabilities(role, Some(profile))
                .map_err(|e| format!("Capability check failed: {}", e.message))?;
        }

        log(format!(
            "[Engine] Starting Orchestrator run {} for workflow '{}'",
            run_id, workflow_type
        ));

        let mut current_plan = String::new();
        let mut prev_code_findings = Vec::<ReviewFinding>::new();

        // Independent invocation counters
        let mut plan_review_count = 0;
        let mut code_review_count = 0;
        let mut fix_count = 0;

        let is_plan_only = workflow_type == "plan_only";
        let is_implement_only = workflow_type == "implement_only";
        let is_review_only = workflow_type == "review_only";

        // ==========================================
        // Phase 1: Planning (if applicable)
        // ==========================================
        if !is_implement_only && !is_review_only {
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::PlanGeneration,
                iteration_info: None,
                message: "Generating implementation plan...".to_string(),
                review_result: None,
                validation_summary: None,
                plan_text: None,
            });

            let planner_profile = snapshot
                .assignments
                .get(&AgentRole::Planner)
                .ok_or_else(|| "Planner role is not assigned.".to_string())?;

            let system_prompt = "You are an expert software architect and planner. Analyze the user request, project specifications, and codebase structure. Output a detailed, actionable, step-by-step implementation plan.";

            let ctx = build_role_context(
                &self.context_builder,
                planner_profile,
                &project_path,
                system_prompt,
                &task_prompt,
                None,
                Some(&cancel_token),
            )
            .await?;

            let output = self
                .execute_adapter(
                    AgentRole::Planner,
                    planner_profile,
                    system_prompt,
                    &ctx.prompt,
                    &project_path,
                    Some(&cancel_token),
                )
                .await?;

            current_plan = output.content;
            log(format!(
                "[Engine] Plan generated ({} chars)",
                current_plan.len()
            ));

            // ==========================================
            // Phase 2: Plan Review Loop
            // ==========================================
            let mut plan_approved = false;
            while !plan_approved {
                self.check_run_control(&mut pause_rx, &cancel_token).await?;

                // Check limit and increment immediately before invocation
                if plan_review_count >= snapshot.iteration_limits.max_plan_review_iterations {
                    log(format!("[Engine] Plan review limit ({} calls) reached without approval. Workflow ending Failed.", plan_review_count));
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::Failed,
                        iteration_info: Some(format!(
                            "Plan review limit reached ({}/{})",
                            plan_review_count, snapshot.iteration_limits.max_plan_review_iterations
                        )),
                        message: "Plan review iteration limit reached without approval."
                            .to_string(),
                        review_result: None,
                        validation_summary: None,
                        plan_text: Some(current_plan),
                    });
                    return Ok(WorkflowState::Failed);
                }

                plan_review_count += 1;
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::PlanReview,
                    iteration_info: Some(format!(
                        "Call {}/{}",
                        plan_review_count, snapshot.iteration_limits.max_plan_review_iterations
                    )),
                    message: format!(
                        "Reviewing implementation plan (Call {})...",
                        plan_review_count
                    ),
                    review_result: None,
                    validation_summary: None,
                    plan_text: Some(current_plan.clone()),
                });

                let reviewer_profile = snapshot
                    .assignments
                    .get(&AgentRole::PlanReviewer)
                    .ok_or_else(|| "Plan Reviewer role is not assigned.".to_string())?;

                let reviewer_system = "You are a rigorous technical reviewer. Audit the proposed implementation plan against the requirements. Output your verdict as JSON with schema:\n{\n  \"verdict\": \"approved\" | \"changes_required\" | \"needs_clarification\",\n  \"summary\": \"...\",\n  \"findings\": [{\"id\": \"F-01\", \"severity\": \"critical\"|\"high\"|\"medium\"|\"low\", \"issue\": \"...\", \"recommendation\": \"...\", \"is_blocking\": true}]\n}";

                let review_input = format!(
                    "## Proposed Plan\n{}\n\n## Original Task\n{}",
                    current_plan, task_prompt
                );
                let review_ctx = build_role_context(
                    &self.context_builder,
                    reviewer_profile,
                    &project_path,
                    reviewer_system,
                    &review_input,
                    None,
                    Some(&cancel_token),
                )
                .await?;

                let review_output = self
                    .execute_adapter(
                        AgentRole::PlanReviewer,
                        reviewer_profile,
                        reviewer_system,
                        &review_ctx.prompt,
                        &project_path,
                        Some(&cancel_token),
                    )
                    .await?;

                let review_res = self
                    .finding_aggregator
                    .parse_review_output(&review_output.content);
                log(format!(
                    "[Engine] Plan review result: {:?} ({} findings)",
                    review_res.verdict,
                    review_res.findings.len()
                ));

                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::PlanReview,
                    iteration_info: Some(format!(
                        "Call {}/{}",
                        plan_review_count, snapshot.iteration_limits.max_plan_review_iterations
                    )),
                    message: format!("Plan review verdict: {:?}", review_res.verdict),
                    review_result: Some(review_res.clone()),
                    validation_summary: None,
                    plan_text: Some(current_plan.clone()),
                });

                match classify_review_verdict(review_res.verdict) {
                    ReviewAction::Approved => {
                        log("[Engine] Plan approved!".to_string());
                        plan_approved = true;
                    }
                    ReviewAction::NeedsClarification => {
                        log("[Engine] Plan reviewer requested user clarification.".to_string());
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::WaitingForUser,
                            iteration_info: None,
                            message: format!("Clarification needed: {}", review_res.summary),
                            review_result: Some(review_res.clone()),
                            validation_summary: None,
                            plan_text: Some(current_plan.clone()),
                        });

                        // Block until clarification is submitted or run cancelled
                        let clarification = tokio::select! {
                            _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                            msg = clarification_rx.recv() => msg.ok_or_else(|| "Clarification channel closed.".to_string())?,
                        };

                        log(
                            "[Engine] Clarification received from user. Revising plan..."
                                .to_string(),
                        );

                        // Plan revision with user clarification
                        self.check_run_control(&mut pause_rx, &cancel_token).await?;
                        let revision_prompt = format!(
                            "## Previous Plan\n{}\n\n## User Clarification\n{}\n\n## Review Findings\n{}",
                            current_plan, clarification, review_res.summary
                        );
                        let revision_ctx = build_role_context(
                            &self.context_builder,
                            planner_profile,
                            &project_path,
                            system_prompt,
                            &revision_prompt,
                            None,
                            Some(&cancel_token),
                        )
                        .await?;
                        let rev_output = self
                            .execute_adapter(
                                AgentRole::Planner,
                                planner_profile,
                                system_prompt,
                                &revision_ctx.prompt,
                                &project_path,
                                Some(&cancel_token),
                            )
                            .await?;
                        current_plan = rev_output.content;
                    }
                    ReviewAction::Failed => {
                        log(
                            "[Engine] Plan review failed; ending workflow without implementation."
                                .to_string(),
                        );
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::Failed,
                            iteration_info: Some(format!(
                                "Plan review call {} failed",
                                plan_review_count
                            )),
                            message: "Plan review failed; workflow stopped fail-closed."
                                .to_string(),
                            review_result: Some(review_res),
                            validation_summary: None,
                            plan_text: Some(current_plan),
                        });
                        return Ok(WorkflowState::Failed);
                    }
                    ReviewAction::ChangesRequired => {
                        if plan_review_count >= snapshot.iteration_limits.max_plan_review_iterations
                        {
                            log(format!("[Engine] Max plan review calls ({}) reached with ChangesRequired. Workflow ending Failed.", plan_review_count));
                            on_event(StepProgressEvent {
                                run_id: run_id.clone(),
                                step: WorkflowState::Failed,
                                iteration_info: Some(format!(
                                    "Plan review limit reached ({}/{})",
                                    plan_review_count,
                                    snapshot.iteration_limits.max_plan_review_iterations
                                )),
                                message: "Plan review rejected at iteration limit.".to_string(),
                                review_result: Some(review_res),
                                validation_summary: None,
                                plan_text: Some(current_plan),
                            });
                            return Ok(WorkflowState::Failed);
                        }

                        // Plan revision
                        self.check_run_control(&mut pause_rx, &cancel_token).await?;
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::PlanRevision,
                            iteration_info: Some(format!(
                                "Revision after call {}",
                                plan_review_count
                            )),
                            message: "Revising plan based on review findings...".to_string(),
                            review_result: None,
                            validation_summary: None,
                            plan_text: Some(current_plan.clone()),
                        });

                        let revision_prompt = format!(
                            "## Previous Plan\n{}\n\n## Please address the review findings and update the plan:\nSummary: {}\nFindings: {:#?}\n\n## Original Task\n{}",
                            current_plan, review_res.summary, review_res.findings, task_prompt
                        );
                        let revision_ctx = build_role_context(
                            &self.context_builder,
                            planner_profile,
                            &project_path,
                            system_prompt,
                            &revision_prompt,
                            Some(&review_res.findings),
                            Some(&cancel_token),
                        )
                        .await?;

                        let rev_output = self
                            .execute_adapter(
                                AgentRole::Planner,
                                planner_profile,
                                system_prompt,
                                &revision_ctx.prompt,
                                &project_path,
                                Some(&cancel_token),
                            )
                            .await?;

                        current_plan = rev_output.content;
                    }
                }
            }

            if is_plan_only {
                if plan_approved {
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::Complete,
                        iteration_info: None,
                        message: "Plan-only workflow completed successfully with approved plan."
                            .to_string(),
                        review_result: None,
                        validation_summary: None,
                        plan_text: Some(current_plan),
                    });
                    return Ok(WorkflowState::Complete);
                } else {
                    return Ok(WorkflowState::Failed);
                }
            }
        }

        // ==========================================
        // Phase 3: Implementation (if applicable)
        // ==========================================
        if !is_review_only {
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::Implementation,
                iteration_info: None,
                message: "Implementing changes in codebase...".to_string(),
                review_result: None,
                validation_summary: None,
                plan_text: if current_plan.is_empty() {
                    None
                } else {
                    Some(current_plan.clone())
                },
            });

            let implementer_profile = snapshot
                .assignments
                .get(&AgentRole::Implementer)
                .ok_or_else(|| "Implementer role is not assigned.".to_string())?;

            let impl_system = "You are the autonomous Implementer agent. Implement the required code changes, create files, and update existing modules according to the plan.";
            let impl_user = format!(
                "## Approved Implementation Plan\n{}\n\n## Task Instructions\n{}",
                if current_plan.is_empty() {
                    "Follow task instructions directly."
                } else {
                    &current_plan
                },
                task_prompt
            );
            let impl_ctx = build_role_context(
                &self.context_builder,
                implementer_profile,
                &project_path,
                impl_system,
                &impl_user,
                None,
                Some(&cancel_token),
            )
            .await?;

            let impl_out = self
                .execute_adapter(
                    AgentRole::Implementer,
                    implementer_profile,
                    impl_system,
                    &impl_ctx.prompt,
                    &project_path,
                    Some(&cancel_token),
                )
                .await?;

            log(format!(
                "[Engine] Implementation output received ({} chars)",
                impl_out.content.len()
            ));
        }

        // ==========================================
        // Phase 4: Validation & Review Loop
        // ==========================================
        let mut user_injected_guidance = String::new();
        loop {
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

            // Step 4.1: Validation Gates
            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::Validation,
                iteration_info: Some(format!("Fix call {}", fix_count)),
                message: "Running validation gates (typecheck, tests, lint)...".to_string(),
                review_result: None,
                validation_summary: None,
                plan_text: None,
            });

            let val_summary = self
                .validation_runner
                .run_gates(
                    &snapshot.validation_gates,
                    &project_path,
                    &authorized_custom_gates,
                    Some(&cancel_token),
                )
                .await?;

            log(format!(
                "[Engine] Validation run completed: passed={}",
                val_summary.passed
            ));

            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::Validation,
                iteration_info: Some(format!("Fix call {}", fix_count)),
                message: if val_summary.passed {
                    "All validation gates passed!".to_string()
                } else {
                    format!(
                        "Validation failed: {}",
                        val_summary.failed_gate_names.join(", ")
                    )
                },
                review_result: None,
                validation_summary: Some(val_summary.clone()),
                plan_text: None,
            });

            if !val_summary.passed {
                // Check fix limit before invoking fixer
                if fix_count >= snapshot.iteration_limits.max_fix_iterations {
                    log(format!("[Engine] Max fix calls ({}) reached with validation failures. Workflow ending Failed.", fix_count));
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::Failed,
                        iteration_info: Some(format!(
                            "Fix limit reached ({}/{})",
                            fix_count, snapshot.iteration_limits.max_fix_iterations
                        )),
                        message: "Validation failed at maximum fix limit.".to_string(),
                        review_result: None,
                        validation_summary: Some(val_summary),
                        plan_text: None,
                    });
                    return Ok(WorkflowState::Failed);
                }

                // Run Fixer
                fix_count += 1;
                self.check_run_control(&mut pause_rx, &cancel_token).await?;

                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::Fix,
                    iteration_info: Some(format!(
                        "Fix call {}/{}",
                        fix_count, snapshot.iteration_limits.max_fix_iterations
                    )),
                    message: format!("Fixing validation errors (Call {})...", fix_count),
                    review_result: None,
                    validation_summary: Some(val_summary.clone()),
                    plan_text: None,
                });

                let fixer_profile = snapshot
                    .assignments
                    .get(&AgentRole::Fixer)
                    .ok_or_else(|| "Fixer role is not assigned.".to_string())?;

                let fix_system = "You are the Fixer agent. Analyze the validation errors and fix the source code to resolve all failures.";
                let fix_user = format!(
                    "## Validation Errors\n{}\n\n## Task Context\n{}\n{}",
                    val_summary.formatted_diagnostics, task_prompt, user_injected_guidance
                );
                let fix_ctx = build_role_context(
                    &self.context_builder,
                    fixer_profile,
                    &project_path,
                    fix_system,
                    &fix_user,
                    None,
                    Some(&cancel_token),
                )
                .await?;

                self.execute_adapter(
                    AgentRole::Fixer,
                    fixer_profile,
                    fix_system,
                    &fix_ctx.prompt,
                    &project_path,
                    Some(&cancel_token),
                )
                .await?;

                continue;
            }

            if is_implement_only {
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::Complete,
                    iteration_info: None,
                    message: "Implementation and validation completed successfully.".to_string(),
                    review_result: None,
                    validation_summary: Some(val_summary),
                    plan_text: None,
                });
                return Ok(WorkflowState::Complete);
            }

            // Step 4.2: Code Review
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

            // Check code review limit before invocation
            if code_review_count >= snapshot.iteration_limits.max_code_review_iterations {
                log(format!("[Engine] Code review limit ({}) reached without approval. Workflow ending Failed.", code_review_count));
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::Failed,
                    iteration_info: Some(format!(
                        "Code review limit reached ({}/{})",
                        code_review_count, snapshot.iteration_limits.max_code_review_iterations
                    )),
                    message: "Code review limit reached without approval.".to_string(),
                    review_result: None,
                    validation_summary: Some(val_summary),
                    plan_text: None,
                });
                return Ok(WorkflowState::Failed);
            }

            code_review_count += 1;
            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::CodeReview,
                iteration_info: Some(format!(
                    "Call {}/{}",
                    code_review_count, snapshot.iteration_limits.max_code_review_iterations
                )),
                message: format!(
                    "Running code review on git changes (Call {})...",
                    code_review_count
                ),
                review_result: None,
                validation_summary: Some(val_summary.clone()),
                plan_text: None,
            });

            let reviewer_profile = snapshot
                .assignments
                .get(&AgentRole::CodeReviewer)
                .ok_or_else(|| "Code Reviewer role is not assigned.".to_string())?;

            let cr_system = "You are an elite code reviewer. Audit the git diff against the requirements and specifications. Output your verdict as JSON with schema:\n{\n  \"verdict\": \"approved\" | \"changes_required\" | \"needs_clarification\",\n  \"summary\": \"...\",\n  \"findings\": [{\"id\": \"F-01\", \"severity\": \"critical\"|\"high\"|\"medium\"|\"low\", \"file\": \"src/...\", \"line\": 10, \"issue\": \"...\", \"recommendation\": \"...\", \"is_blocking\": true}]\n}";
            let review_task = format!(
                "## Review Request\n{}\n\nEvaluate the current worktree changes and project requirements.",
                task_prompt
            );
            let cr_ctx = build_role_context(
                &self.context_builder,
                reviewer_profile,
                &project_path,
                cr_system,
                &review_task,
                None,
                Some(&cancel_token),
            )
            .await?;

            let cr_out = self
                .execute_adapter(
                    AgentRole::CodeReviewer,
                    reviewer_profile,
                    cr_system,
                    &cr_ctx.prompt,
                    &project_path,
                    Some(&cancel_token),
                )
                .await?;

            let cr_result = self.finding_aggregator.parse_review_output(&cr_out.content);
            log(format!(
                "[Engine] Code review verdict: {:?} ({} findings)",
                cr_result.verdict,
                cr_result.findings.len()
            ));

            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::CodeReview,
                iteration_info: Some(format!(
                    "Call {}/{}",
                    code_review_count, snapshot.iteration_limits.max_code_review_iterations
                )),
                message: format!("Code review verdict: {:?}", cr_result.verdict),
                review_result: Some(cr_result.clone()),
                validation_summary: Some(val_summary.clone()),
                plan_text: None,
            });

            if classify_review_verdict(cr_result.verdict) == ReviewAction::Approved {
                log("[Engine] Code review APPROVED!".to_string());
                break;
            }

            if classify_review_verdict(cr_result.verdict) == ReviewAction::NeedsClarification {
                log("[Engine] Code reviewer requested clarification.".to_string());
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::WaitingForUser,
                    iteration_info: None,
                    message: format!("Clarification needed: {}", cr_result.summary),
                    review_result: Some(cr_result.clone()),
                    validation_summary: Some(val_summary.clone()),
                    plan_text: None,
                });

                let clarification = tokio::select! {
                    _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                    msg = clarification_rx.recv() => msg.ok_or_else(|| "Clarification channel closed.".to_string())?,
                };

                user_injected_guidance = format!(
                    "\n## User Clarification for Code Review\n{}\n",
                    clarification
                );
            }

            if classify_review_verdict(cr_result.verdict) == ReviewAction::Failed {
                log(
                    "[Engine] Code review failed; ending workflow without invoking the fixer."
                        .to_string(),
                );
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::Failed,
                    iteration_info: Some(format!("Code review call {} failed", code_review_count)),
                    message: "Code review failed; workflow stopped fail-closed.".to_string(),
                    review_result: Some(cr_result),
                    validation_summary: Some(val_summary),
                    plan_text: None,
                });
                return Ok(WorkflowState::Failed);
            }

            // Check for repeated blocking findings
            if self
                .finding_aggregator
                .has_repeated_blocking_findings(&prev_code_findings, &cr_result.findings)
            {
                log("[Engine] Repeated blocking findings detected. Entering WaitingForBlockingResolution.".to_string());
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::WaitingForBlockingResolution,
                    iteration_info: None,
                    message: "Repeated blocking findings encountered. Requires explicit resolution (retry with guidance or abort).".to_string(),
                    review_result: Some(cr_result.clone()),
                    validation_summary: Some(val_summary.clone()),
                    plan_text: None,
                });

                let resolution = tokio::select! {
                    _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                    res = blocking_resolution_rx.recv() => res.ok_or_else(|| "Blocking resolution channel closed.".to_string())?,
                };

                match resolution {
                    BlockingResolution::Retry { guidance } => {
                        log(
                            "[Engine] Retrying with user guidance for blocking findings."
                                .to_string(),
                        );
                        if let Some(g) = guidance {
                            user_injected_guidance =
                                format!("\n## Guidance for Blocking Findings\n{}\n", g);
                        }
                        // Reset detector but preserve global review/fix counters
                        prev_code_findings.clear();
                    }
                    BlockingResolution::Abort => {
                        log("[Engine] Aborted by user at blocking resolution.".to_string());
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::Failed,
                            iteration_info: None,
                            message: "Aborted by user on repeated blocking findings.".to_string(),
                            review_result: Some(cr_result),
                            validation_summary: Some(val_summary),
                            plan_text: None,
                        });
                        return Ok(WorkflowState::Failed);
                    }
                }
            } else {
                prev_code_findings = cr_result.findings.clone();
            }

            // Check limits before triggering Fixer
            if fix_count >= snapshot.iteration_limits.max_fix_iterations
                || code_review_count >= snapshot.iteration_limits.max_code_review_iterations
            {
                log(format!("[Engine] Max review/fix limit reached (fix: {}/{}, review: {}/{}). Workflow ending Failed.", fix_count, snapshot.iteration_limits.max_fix_iterations, code_review_count, snapshot.iteration_limits.max_code_review_iterations));
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::Failed,
                    iteration_info: Some(format!(
                        "Fix limit: {}/{}, Review limit: {}/{}",
                        fix_count,
                        snapshot.iteration_limits.max_fix_iterations,
                        code_review_count,
                        snapshot.iteration_limits.max_code_review_iterations
                    )),
                    message: "Code review ChangesRequired reached iteration limit.".to_string(),
                    review_result: Some(cr_result),
                    validation_summary: Some(val_summary),
                    plan_text: None,
                });
                return Ok(WorkflowState::Failed);
            }

            // Trigger Fixer for Code Review findings
            fix_count += 1;
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::Fix,
                iteration_info: Some(format!(
                    "Fix call {}/{}",
                    fix_count, snapshot.iteration_limits.max_fix_iterations
                )),
                message: format!("Fixing code review findings (Call {})...", fix_count),
                review_result: Some(cr_result.clone()),
                validation_summary: None,
                plan_text: None,
            });

            let fixer_profile = snapshot
                .assignments
                .get(&AgentRole::Fixer)
                .ok_or_else(|| "Fixer role is not assigned.".to_string())?;

            let fix_system = "You are the Fixer agent. Modify the codebase to resolve all issues identified in the code review.";
            let fix_task = format!(
                "## Original Task\n{}\n\n## Review Findings\n{:#?}\n{}",
                task_prompt, cr_result.findings, user_injected_guidance
            );
            let fix_ctx = build_role_context(
                &self.context_builder,
                fixer_profile,
                &project_path,
                fix_system,
                &fix_task,
                Some(&cr_result.findings),
                Some(&cancel_token),
            )
            .await?;

            self.execute_adapter(
                AgentRole::Fixer,
                fixer_profile,
                fix_system,
                &fix_ctx.prompt,
                &project_path,
                Some(&cancel_token),
            )
            .await?;
        }

        on_event(StepProgressEvent {
            run_id: run_id.clone(),
            step: WorkflowState::Complete,
            iteration_info: None,
            message: "Autonomous development loop completed successfully!".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: if current_plan.is_empty() {
                None
            } else {
                Some(current_plan)
            },
        });

        Ok(WorkflowState::Complete)
    }

    async fn execute_adapter(
        &self,
        role: AgentRole,
        profile: &OrchestratorProfile,
        system_prompt: &str,
        user_prompt: &str,
        project_path: &PathBuf,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<AdapterExecutionOutput, String> {
        #[cfg(test)]
        if let Some(scripted_executor) = &self.scripted_adapter_executor {
            let Some((expected_role, output)) = scripted_executor.outputs.lock().unwrap().pop_front() else {
                return Err("Scripted test adapter has no response for the requested role.".to_string());
            };
            scripted_executor.calls.lock().unwrap().push(role.clone());
            if expected_role != role {
                return Err(format!(
                    "Scripted test adapter expected role {:?}, received {:?}.",
                    expected_role, role
                ));
            }
            return Ok(output);
        }

        let input = AdapterExecutionInput {
            role,
            profile: profile.clone(),
            system_prompt: system_prompt.to_string(),
            user_prompt: user_prompt.to_string(),
            project_path: project_path.clone(),
            temperature: None,
        };

        match profile.adapter {
            ExecutionAdapterType::Provider => {
                self.provider_adapter.execute(&input, cancel_token).await
            }
            ExecutionAdapterType::Ollama => self.ollama_adapter.execute(&input, cancel_token).await,
            ExecutionAdapterType::Cli => self.codex_cli_adapter.execute(&input, cancel_token).await,
            ExecutionAdapterType::Mcp => {
                Err("MCP adapter is reserved and currently disabled.".to_string())
            }
        }
    }

    async fn check_run_control(
        &self,
        rx: &mut watch::Receiver<RunControlState>,
        cancel_token: &CancellationToken,
    ) -> Result<(), String> {
        if cancel_token.is_cancelled() || *rx.borrow() == RunControlState::Cancelled {
            return Err("Execution cancelled by user.".to_string());
        }
        if *rx.borrow() == RunControlState::Paused {
            while *rx.borrow() == RunControlState::Paused {
                tokio::select! {
                    _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                    res = rx.changed() => {
                        if res.is_err() {
                            break;
                        }
                    }
                }
            }
            if cancel_token.is_cancelled() || *rx.borrow() == RunControlState::Cancelled {
                return Err("Execution cancelled by user.".to_string());
            }
        }
        Ok(())
    }
}

async fn build_role_context(
    builder: &ContextBuilder,
    profile: &OrchestratorProfile,
    project_path: &std::path::Path,
    system_prompt: &str,
    user_prompt: &str,
    findings: Option<&[ReviewFinding]>,
    cancel_token: Option<&CancellationToken>,
) -> Result<BuiltContext, String> {
    let context_window = builder.resolve_profile_context_window(profile)?;
    let project_budget = builder.calculate_project_budget(
        context_window,
        system_prompt,
        user_prompt,
        TokenCountQuality::Estimated,
    )?;
    builder
        .build(
            project_path,
            user_prompt,
            None,
            findings,
            Some(project_budget),
            cancel_token,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::validation::{ValidationGateResult, ValidationRunSummary};
    use serde_json::json;

    #[test]
    fn failed_review_verdict_is_distinct_from_revision_or_fix() {
        assert_eq!(
            classify_review_verdict(ReviewVerdict::Failed),
            ReviewAction::Failed
        );
        assert_ne!(
            classify_review_verdict(ReviewVerdict::Failed),
            ReviewAction::ChangesRequired
        );
        assert_eq!(
            classify_review_verdict(ReviewVerdict::ChangesRequired),
            ReviewAction::ChangesRequired
        );
        assert_eq!(
            classify_review_verdict(ReviewVerdict::NeedsClarification),
            ReviewAction::NeedsClarification
        );
        assert_eq!(
            classify_review_verdict(ReviewVerdict::Approved),
            ReviewAction::Approved
        );
    }

    #[test]
    fn progress_event_serializes_nested_validation_payload_as_camel_case() {
        let event = StepProgressEvent {
            run_id: "run-123".to_string(),
            step: WorkflowState::Validation,
            iteration_info: Some("pass 1".to_string()),
            message: "validated".to_string(),
            review_result: None,
            validation_summary: Some(ValidationRunSummary {
                passed: false,
                total_gates_run: 1,
                failed_gate_names: vec!["cargo test".to_string()],
                results: vec![ValidationGateResult {
                    gate_id: "gate-1".to_string(),
                    gate_name: "Rust tests".to_string(),
                    executable: "cargo".to_string(),
                    args: vec!["test".to_string()],
                    exit_code: 1,
                    success: false,
                    stdout: String::new(),
                    stderr: "failed".to_string(),
                    is_truncated: false,
                    duration_ms: 12,
                    fail_on_error: true,
                    timed_out: false,
                    cancelled: false,
                }],
                formatted_diagnostics: "diagnostics".to_string(),
            }),
            plan_text: Some("plan".to_string()),
        };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["runId"], "run-123");
        assert_eq!(value["step"], "validation");
        assert_eq!(value["validationSummary"]["totalGatesRun"], 1);
        assert_eq!(value["validationSummary"]["failedGateNames"][0], "cargo test");
        assert_eq!(value["validationSummary"]["formattedDiagnostics"], "diagnostics");
        assert_eq!(value["validationSummary"]["results"][0]["gateId"], "gate-1");
        assert_eq!(value["validationSummary"]["results"][0]["gateName"], "Rust tests");
        assert_eq!(value["validationSummary"]["results"][0]["exitCode"], 1);
        assert_eq!(value["validationSummary"]["results"][0]["isTruncated"], false);
        assert_eq!(value["validationSummary"]["results"][0]["durationMs"], 12);
        assert_eq!(value["validationSummary"]["results"][0]["failOnError"], true);
        assert_eq!(value["validationSummary"]["results"][0]["timedOut"], false);
    }

    #[test]
    fn legacy_snake_case_progress_payload_deserializes() {
        let legacy = json!({
            "run_id":"legacy-run","step":"validation","iteration_info":"pass 1","message":"done",
            "review_result":null,"plan_text":null,
            "validation_summary":{"passed":true,"total_gates_run":1,"failed_gate_names":[],"formatted_diagnostics":"",
                "results":[{"gate_id":"g1","gate_name":"test","executable":"cargo","args":["test"],"exit_code":0,"success":true,"stdout":"ok","stderr":"","is_truncated":false,"duration_ms":3,"fail_on_error":true,"timed_out":false,"cancelled":false}]}
        });
        let decoded: StepProgressEvent = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.run_id, "legacy-run");
        let summary = decoded.validation_summary.unwrap();
        assert!(summary.passed);
        assert_eq!(summary.results[0].gate_id, "g1");
        assert_eq!(summary.results[0].duration_ms, 3);
    }

    fn scripted_output(content: &str) -> AdapterExecutionOutput {
        AdapterExecutionOutput {
            content: content.to_string(),
            raw_json: None,
            tokens_used: None,
            model_used: "scripted".to_string(),
            duration_ms: 0,
        }
    }

    fn workflow_snapshot(project_path: String) -> RunConfigurationSnapshot {
        let profile = OrchestratorProfile {
            id: "scripted".to_string(),
            display_name: "Scripted test adapter".to_string(),
            adapter: ExecutionAdapterType::Cli,
            capabilities: vec![
                super::super::types::ProfileCapability::Reasoning,
                super::super::types::ProfileCapability::Review,
                super::super::types::ProfileCapability::WorkspaceRead,
                super::super::types::ProfileCapability::WorkspaceWrite,
                super::super::types::ProfileCapability::CommandExecution,
            ],
            provider_id: None,
            provider_profile_id: None,
            model: None,
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: Some("unused-in-scripted-test".to_string()),
            args: Some(vec![]),
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(131_072),
        };
        let assignments = [
            AgentRole::Planner,
            AgentRole::PlanReviewer,
            AgentRole::Implementer,
            AgentRole::Fixer,
            AgentRole::CodeReviewer,
        ]
        .into_iter()
        .map(|role| (role, profile.clone()))
        .collect();
        RunConfigurationSnapshot {
            project_path,
            assignments,
            iteration_limits: Default::default(),
            validation_gates: vec![],
            budget_limits: Default::default(),
            created_at_unix: 0,
        }
    }

    async fn run_scripted_workflow(
        workflow_type: &str,
        scripted: Vec<(AgentRole, AdapterExecutionOutput)>,
    ) -> (WorkflowState, Vec<StepProgressEvent>, Vec<AgentRole>) {
        let directory = tempfile::tempdir().unwrap();
        let mut engine = OrchestratorEngine::new();
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(scripted.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
        });
        engine.scripted_adapter_executor = Some(Arc::clone(&scripted_executor));
        let events = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let state = engine
            .run_workflow(
                "run-engine-test".into(),
                workflow_snapshot(directory.path().to_string_lossy().into_owned()),
                "Exercise fail-closed workflow behavior".into(),
                workflow_type.to_string(),
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                Arc::new(move |event| event_sink.lock().unwrap().push(event)),
                Arc::new(|_| {}),
            )
            .await
            .unwrap();
        drop(pause_tx);
        let events = Arc::try_unwrap(events).unwrap().into_inner().unwrap();
        let calls = scripted_executor.calls.lock().unwrap().clone();
        (state, events, calls)
    }

    #[tokio::test]
    async fn plan_review_failed_is_terminal_in_actual_engine_workflow() {
        let (state, events, calls) = run_scripted_workflow(
            "plan_only",
            vec![
                (AgentRole::Planner, scripted_output("Generated plan")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"failed","summary":"review unavailable","findings":[]}"#),
                ),
            ],
        )
        .await;

        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert!(events.iter().any(|event| event.step == WorkflowState::Failed));
        assert!(!events.iter().any(|event| matches!(
            event.step,
            WorkflowState::PlanRevision
                | WorkflowState::Implementation
                | WorkflowState::Validation
                | WorkflowState::CodeReview
                | WorkflowState::Complete
        )));
    }

    #[tokio::test]
    async fn code_review_failed_is_terminal_in_actual_engine_workflow() {
        let (state, events, calls) = run_scripted_workflow(
            "full_loop",
            vec![
                (AgentRole::Planner, scripted_output("Generated plan")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"plan accepted","findings":[]}"#),
                ),
                (AgentRole::Implementer, scripted_output("implemented")),
                (
                    AgentRole::CodeReviewer,
                    scripted_output(r#"{"verdict":"failed","summary":"review unavailable","findings":[]}"#),
                ),
            ],
        )
        .await;

        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(
            calls,
            vec![
                AgentRole::Planner,
                AgentRole::PlanReviewer,
                AgentRole::Implementer,
                AgentRole::CodeReviewer,
            ]
        );
        assert!(events.iter().any(|event| event.step == WorkflowState::Failed));
        assert!(!events
            .iter()
            .any(|event| matches!(event.step, WorkflowState::Fix | WorkflowState::Complete)));
    }
}
