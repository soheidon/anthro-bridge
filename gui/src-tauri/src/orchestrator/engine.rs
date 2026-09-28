use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
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
    active_roles_for_workflow, validate_workflow_role_capabilities, AgentRole,
    AuthorizedCustomGate, ExecutionAdapterType, LoopIterationLimits, OrchestratorProfile,
    ReviewFinding, ReviewResult, ReviewVerdict, RunConfigurationSnapshot, RunControlState,
    WorkflowState, PlanOutputOptions,
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

fn validate_workflow_iteration_limits(
    workflow_type: &str,
    limits: &LoopIterationLimits,
) -> Result<(), String> {
    let required_limits: &[(&str, u32)] = match workflow_type {
        "full_loop" => &[
            ("plan_review", limits.max_plan_review_iterations),
            ("fix", limits.max_fix_iterations),
            ("code_review", limits.max_code_review_iterations),
        ],
        "plan_only" => &[("plan_review", limits.max_plan_review_iterations)],
        "implement_only" => &[("fix", limits.max_fix_iterations)],
        // Review Only performs exactly one code review and has no configurable loop.
        "review_only" => &[],
        _ => return Err(format!("Unsupported Orchestrator workflow '{workflow_type}'.")),
    };

    for (name, value) in required_limits {
        if *value == 0 {
            return Err(format!(
                "Preflight failure: {name} iteration limit for workflow '{workflow_type}' must be greater than zero."
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ValidatedPlanOutput {
    requested_path: PathBuf,
    project_root_at_start: PathBuf,
    destination_at_start: PathBuf,
    overwrite_existing: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedPlanTarget {
    project_root: PathBuf,
    destination: PathBuf,
    pub exists: bool,
}

pub fn resolve_plan_target(project_path: &str, plan_path: &str) -> Result<ResolvedPlanTarget, String> {
    let project_input = PathBuf::from(project_path);
    let project_root = fs::canonicalize(&project_input)
        .map_err(|e| format!("Could not resolve project directory: {e}"))?;
    if !project_root.is_dir() {
        return Err("Project path is not a directory.".to_string());
    }

    let raw_path = PathBuf::from(plan_path);
    let target = if raw_path.is_absolute() {
        raw_path
    } else if let Ok(relative) = raw_path.strip_prefix(&project_input) {
        project_root.join(relative)
    } else {
        project_root.join(raw_path)
    };
    if target.file_name().is_none() {
        return Err("Plan file path must include a filename.".to_string());
    }
    if !target
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("md"))
    {
        return Err("Plan output must be a Markdown (.md) file.".to_string());
    }

    let parent = target
        .parent()
        .ok_or_else(|| "Plan file path has no parent directory.".to_string())?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|e| format!("Could not resolve plan file parent directory: {e}"))?;
    if !path_is_within(&project_root, &canonical_parent) {
        return Err("Plan file must remain inside the selected project directory.".to_string());
    }
    let file_name = target
        .file_name()
        .ok_or_else(|| "Plan file path must include a filename.".to_string())?;
    let path_in_parent = canonical_parent.join(file_name);

    match fs::symlink_metadata(&path_in_parent) {
        Ok(metadata) => {
            if metadata.is_dir() {
                return Err("Plan output target is a directory, not a file.".to_string());
            }
            let destination = fs::canonicalize(&path_in_parent)
                .map_err(|e| format!("Could not resolve existing plan file: {e}"))?;
            if !path_is_within(&project_root, &destination) {
                return Err("Plan file must remain inside the selected project directory.".to_string());
            }
            if !destination.is_file() {
                return Err("Plan output target is not a regular file.".to_string());
            }
            Ok(ResolvedPlanTarget {
                project_root,
                destination,
                exists: true,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ResolvedPlanTarget {
            project_root,
            destination: path_in_parent,
            exists: false,
        }),
        Err(error) => Err(format!("Could not inspect plan output target: {error}")),
    }
}

fn path_is_within(root: &Path, candidate: &Path) -> bool {
    #[cfg(windows)]
    {
        let root = root.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
        let candidate = candidate.to_string_lossy().to_lowercase();
        candidate == root
            || candidate
                .strip_prefix(&root)
                .is_some_and(|suffix| suffix.starts_with(['\\', '/']))
    }
    #[cfg(not(windows))]
    {
        candidate.starts_with(root)
    }
}

pub fn validate_plan_output_options(
    project_path: &str,
    options: PlanOutputOptions,
) -> Result<ValidatedPlanOutput, String> {
    let resolved = resolve_plan_target(project_path, &options.path)?;
    if resolved.exists && !options.overwrite_existing {
        return Err("Plan file already exists; explicit overwrite confirmation is required.".to_string());
    }
    Ok(ValidatedPlanOutput {
        requested_path: PathBuf::from(options.path),
        project_root_at_start: resolved.project_root,
        destination_at_start: resolved.destination,
        overwrite_existing: options.overwrite_existing,
    })
}

fn persist_approved_plan(
    project_path: &str,
    output: &ValidatedPlanOutput,
    content: &str,
) -> Result<(), String> {
    persist_approved_plan_with_installer(project_path, output, content, install_plan_file_atomically)
}

fn persist_approved_plan_with_installer<F>(
    project_path: &str,
    output: &ValidatedPlanOutput,
    content: &str,
    installer: F,
) -> Result<(), String>
where
    F: Fn(&Path, &Path, bool) -> std::io::Result<()>,
{
    let initial = resolve_plan_target(project_path, &output.requested_path.to_string_lossy())?;
    if initial.project_root != output.project_root_at_start
        || !paths_equal(&initial.destination, &output.destination_at_start)
    {
        return Err("Project or plan-file path changed during the run; plan was not saved.".to_string());
    }
    if initial.exists && !output.overwrite_existing {
        return Err("Plan file already exists and overwrite was not confirmed.".to_string());
    }

    let parent = output
        .destination_at_start
        .parent()
        .ok_or_else(|| "Plan output target has no parent directory.".to_string())?;
    let filename = output
        .destination_at_start
        .file_name()
        .ok_or_else(|| "Plan output target has no filename.".to_string())?
        .to_string_lossy();
    let temp_path = parent.join(format!(".{filename}.{}.tmp", uuid::Uuid::new_v4()));
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| format!("Could not create temporary plan file: {e}"))?;
    let write_result = (|| {
        temp_file.write_all(content.as_bytes())?;
        temp_file.sync_all()?;
        Ok::<(), std::io::Error>(())
    })();
    drop(temp_file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("Could not write temporary plan file: {error}"));
    }

    // Re-resolve immediately before installation to catch project-root or parent
    // symlink/reparse-point changes that occurred while agents were running.
    let before_install = resolve_plan_target(project_path, &output.requested_path.to_string_lossy());
    let before_install = match before_install {
        Ok(resolved)
            if resolved.project_root == output.project_root_at_start
                && paths_equal(&resolved.destination, &output.destination_at_start)
                && (!resolved.exists || output.overwrite_existing) => resolved,
        Ok(_) => {
            let _ = fs::remove_file(&temp_path);
            return Err("Project or plan-file path changed before saving; plan was not saved.".to_string());
        }
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    };

    let install_result = installer(
        &temp_path,
        &before_install.destination,
        before_install.exists,
    );
    if install_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    install_result.map_err(|e| format!("Could not safely install approved plan: {e}"))
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy().eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn install_plan_file_atomically(
    temp_path: &Path,
    destination: &Path,
    destination_exists: bool,
) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::ptr;
        let temp: Vec<u16> = temp_path.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            if destination_exists {
                ReplaceFileW(target.as_ptr(), temp.as_ptr(), ptr::null(), 0, ptr::null_mut(), ptr::null_mut())
            } else {
                MoveFileExW(temp.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH)
            }
        };
        if result == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        if destination_exists {
            fs::rename(temp_path, destination)
        } else {
            fs::hard_link(temp_path, destination)?;
            fs::remove_file(temp_path)
        }
    }
}

#[cfg(windows)]
const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

#[cfg(windows)]
#[link(name = "Kernel32")]
unsafe extern "system" {
    fn ReplaceFileW(
        replaced_file_name: *const u16,
        replacement_file_name: *const u16,
        backup_file_name: *const u16,
        replace_flags: u32,
        exclude: *mut std::ffi::c_void,
        reserved: *mut std::ffi::c_void,
    ) -> i32;
    fn MoveFileExW(existing_file_name: *const u16, new_file_name: *const u16, flags: u32) -> i32;
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
    #[cfg(test)]
    scripted_validation_executor: Option<Arc<ScriptedValidationExecutor>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct ScriptedAdapterExecutor {
    outputs: Mutex<VecDeque<(AgentRole, AdapterExecutionOutput)>>,
    calls: Mutex<Vec<AgentRole>>,
    before_role_symlink_swap: Mutex<Option<(AgentRole, PathBuf, PathBuf)>>,
    implementer_prompt: Mutex<Option<String>>,
    plan_file_to_observe: Option<PathBuf>,
    plan_exists_at_implementer: Mutex<Option<bool>>,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct ScriptedValidationExecutor {
    summaries: Mutex<VecDeque<ValidationRunSummary>>,
    runs: Mutex<usize>,
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
            #[cfg(test)]
            scripted_validation_executor: None,
        }
    }

    /// Executes the development review workflow adhering to exact terminal conditions, independent counters, and fail-closed reviews.
    pub async fn run_workflow(
        &self,
        run_id: String,
        snapshot: RunConfigurationSnapshot,
        task_prompt: String,
        workflow_type: String, // "full_loop" | "plan_only" | "implement_only" | "review_only"
        plan_output: Option<ValidatedPlanOutput>,
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

        let active_roles = active_roles_for_workflow(&workflow_type)?;
        validate_workflow_iteration_limits(&workflow_type, &snapshot.iteration_limits)?;

        // Validate active role capabilities before starting
        for role in &active_roles {
            let profile = snapshot
                .assignments
                .get(role)
                .ok_or_else(|| format!("Role '{:?}' is not assigned.", role))?;
            // Reject MCP adapter
            if profile.adapter == ExecutionAdapterType::Mcp {
                return Err(format!(
                    "MCP adapter is currently unavailable for role '{:?}'.",
                    role
                ));
            }
            validate_workflow_role_capabilities(&workflow_type, role, Some(profile))
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
        // Review-Only Dedicated Path (Read-Only)
        // ==========================================
        if is_review_only {
            log(format!(
                "[Engine] Starting read-only code review run {} (validation gates and fixer bypassed)",
                run_id
            ));
            let reviewer_profile = snapshot
                .assignments
                .get(&AgentRole::CodeReviewer)
                .ok_or_else(|| "Code Reviewer role is not assigned.".to_string())?;

            let user_injected_guidance = String::new();
            loop {
                self.check_run_control(&mut pause_rx, &cancel_token).await?;

                code_review_count += 1;
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::CodeReview,
                    iteration_info: Some("Single review pass".to_string()),
                    message: format!(
                        "Running read-only code review on git changes (Call {})...",
                        code_review_count
                    ),
                    review_result: None,
                    validation_summary: None,
                    plan_text: None,
                });

                let cr_system = "You are an elite code reviewer. Audit the git diff against the requirements and specifications. Output your verdict as JSON with schema:\n{\n  \"verdict\": \"approved\" | \"changes_required\" | \"needs_clarification\",\n  \"summary\": \"...\",\n  \"findings\": [{\"id\": \"F-01\", \"severity\": \"critical\"|\"high\"|\"medium\"|\"low\", \"file\": \"src/...\", \"line\": 10, \"issue\": \"...\", \"recommendation\": \"...\", \"is_blocking\": true}]\n}";
                let review_task = format!(
                    "## Review Request\n{}\n{}\nEvaluate the current worktree changes and project requirements.",
                    task_prompt, user_injected_guidance
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
                    "[Engine] Review-only verdict: {:?} ({} findings)",
                    cr_result.verdict,
                    cr_result.findings.len()
                ));

                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::CodeReview,
                    iteration_info: Some("Single review pass".to_string()),
                    message: format!("Code review verdict: {:?}", cr_result.verdict),
                    review_result: Some(cr_result.clone()),
                    validation_summary: None,
                    plan_text: None,
                });

                match classify_review_verdict(cr_result.verdict) {
                    ReviewAction::Approved => {
                        log("[Engine] Review-only APPROVED! Completing workflow with READY.".to_string());
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::Complete,
                            iteration_info: None,
                            message: "Review-only workflow completed with approved verdict (READY).".to_string(),
                            review_result: Some(cr_result),
                            validation_summary: None,
                            plan_text: None,
                        });
                        return Ok(WorkflowState::Complete);
                    }
                    ReviewAction::ChangesRequired => {
                        log("[Engine] Review-only completed with ChangesRequired findings (NOT READY).".to_string());
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::Complete,
                            iteration_info: None,
                            message: "Review-only workflow completed with findings (NOT READY).".to_string(),
                            review_result: Some(cr_result),
                            validation_summary: None,
                            plan_text: None,
                        });
                        return Ok(WorkflowState::Complete);
                    }
                    ReviewAction::NeedsClarification => {
                        log("[Engine] Code reviewer requested clarification; review-only is single-pass and will stop without a final verdict.".to_string());
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::Failed,
                            iteration_info: Some("Single-pass review requires clarification".to_string()),
                            message: format!("Review-only could not complete because clarification is required: {}", cr_result.summary),
                            review_result: Some(cr_result.clone()),
                            validation_summary: None,
                            plan_text: None,
                        });
                        return Ok(WorkflowState::Failed);
                    }
                    ReviewAction::Failed => {
                        log("[Engine] Code review failed in review-only mode.".to_string());
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::Failed,
                            iteration_info: Some(format!("Code review call {} failed", code_review_count)),
                            message: "Code review failed in review-only mode.".to_string(),
                            review_result: Some(cr_result),
                            validation_summary: None,
                            plan_text: None,
                        });
                        return Ok(WorkflowState::Failed);
                    }
                }
            }
        }

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
                    if let Some(output) = &plan_output {
                        let safe_plan = super::secrets::SecretRedactor::new().redact_secrets(&current_plan);
                        if let Err(error) = persist_approved_plan(&snapshot.project_path, output, &safe_plan) {
                            let safe_error = super::secrets::SecretRedactor::new().redact_secrets(&error);
                            log(format!("[Engine] Approved plan could not be saved: {safe_error}"));
                            on_event(StepProgressEvent {
                                run_id: run_id.clone(),
                                step: WorkflowState::Failed,
                                iteration_info: Some("plan_save_failed".to_string()),
                                message: format!("Approved plan could not be saved: {safe_error}"),
                                review_result: None,
                                validation_summary: None,
                                plan_text: Some(current_plan),
                            });
                            return Ok(WorkflowState::Failed);
                        }
                    }
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

        // In Full Loop, do not begin implementation until the final reviewed plan
        // is safely persisted. Other workflows never consume this output option.
        if workflow_type == "full_loop" {
            if let Some(output) = &plan_output {
                let safe_plan = super::secrets::SecretRedactor::new().redact_secrets(&current_plan);
                if let Err(error) = persist_approved_plan(&snapshot.project_path, output, &safe_plan) {
                    let safe_error = super::secrets::SecretRedactor::new().redact_secrets(&error);
                    log(format!("[Engine] Approved plan could not be saved: {safe_error}"));
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::Failed,
                        iteration_info: Some("plan_save_failed".to_string()),
                        message: format!("Approved plan could not be saved: {safe_error}"),
                        review_result: None,
                        validation_summary: None,
                        plan_text: Some(current_plan),
                    });
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

            #[cfg(test)]
            let val_summary = if let Some(ref executor) = self.scripted_validation_executor {
                *executor.runs.lock().unwrap() += 1;
                executor
                    .summaries
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(ValidationRunSummary {
                        passed: true,
                        total_gates_run: snapshot.validation_gates.len(),
                        failed_gate_names: vec![],
                        results: vec![],
                        formatted_diagnostics: String::new(),
                    })
            } else {
                self.validation_runner
                    .run_gates(
                        &snapshot.validation_gates,
                        &project_path,
                        &authorized_custom_gates,
                        Some(&cancel_token),
                    )
                    .await?
            };

            #[cfg(not(test))]
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
            let symlink_swap = {
                let mut pending = scripted_executor.before_role_symlink_swap.lock().unwrap();
                if pending.as_ref().is_some_and(|(expected_role, _, _)| expected_role == &role) {
                    pending.take()
                } else {
                    None
                }
            };
            if let Some((_, link_path, new_target)) = symlink_swap {
                #[cfg(windows)]
                {
                    std::fs::remove_dir(&link_path)
                        .map_err(|e| format!("test symlink removal failed: {e}"))?;
                    std::os::windows::fs::symlink_dir(&new_target, &link_path)
                        .map_err(|e| format!("test symlink replacement failed: {e}"))?;
                }
                #[cfg(unix)]
                {
                    std::fs::remove_file(&link_path)
                        .map_err(|e| format!("test symlink removal failed: {e}"))?;
                    std::os::unix::fs::symlink(&new_target, &link_path)
                        .map_err(|e| format!("test symlink replacement failed: {e}"))?;
                }
            }
            let Some((expected_role, output)) = scripted_executor.outputs.lock().unwrap().pop_front() else {
                return Err("Scripted test adapter has no response for the requested role.".to_string());
            };
            scripted_executor.calls.lock().unwrap().push(role.clone());
            if role == AgentRole::Implementer {
                *scripted_executor.implementer_prompt.lock().unwrap() = Some(user_prompt.to_string());
                *scripted_executor.plan_exists_at_implementer.lock().unwrap() = scripted_executor
                    .plan_file_to_observe
                    .as_ref()
                    .map(|path| path.is_file());
            }
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
    use crate::orchestrator::types::ProfileCapability;
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

    fn readonly_reviewer_profile() -> OrchestratorProfile {
        OrchestratorProfile {
            id: "readonly-reviewer".to_string(),
            display_name: "Read-Only Provider Reviewer".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: Some("deepseek".to_string()),
            provider_profile_id: None,
            model: Some("deepseek-v4.1-flash".to_string()),
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(131_072),
        }
    }

    fn mutating_cli_profile() -> OrchestratorProfile {
        OrchestratorProfile {
            id: "mutating-cli".to_string(),
            display_name: "Mutating CLI Adapter".to_string(),
            adapter: ExecutionAdapterType::Cli,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
                ProfileCapability::WorkspaceWrite,
                ProfileCapability::CommandExecution,
            ],
            provider_id: None,
            provider_profile_id: None,
            model: None,
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: Some("codex".to_string()),
            args: Some(vec![]),
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(131_072),
        }
    }

    fn workflow_snapshot(project_path: String) -> RunConfigurationSnapshot {
        let cli = mutating_cli_profile();
        let reviewer = readonly_reviewer_profile();
        let assignments = [
            (AgentRole::Planner, cli.clone()),
            (AgentRole::PlanReviewer, cli.clone()),
            (AgentRole::Implementer, cli.clone()),
            (AgentRole::Fixer, cli.clone()),
            (AgentRole::CodeReviewer, reviewer),
        ]
        .into_iter()
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

    async fn run_scripted_workflow_full(
        workflow_type: &str,
        snapshot: RunConfigurationSnapshot,
        scripted_adapters: Vec<(AgentRole, AdapterExecutionOutput)>,
        scripted_validations: Vec<ValidationRunSummary>,
    ) -> Result<(WorkflowState, Vec<StepProgressEvent>, Vec<AgentRole>, usize), String> {
        run_scripted_workflow_with_plan_output(
            workflow_type,
            snapshot,
            scripted_adapters,
            scripted_validations,
            None,
            None,
        )
        .await
        .map(|(state, events, calls, validation_runs, _, _)| (state, events, calls, validation_runs))
    }

    async fn run_scripted_workflow_with_plan_output(
        workflow_type: &str,
        snapshot: RunConfigurationSnapshot,
        scripted_adapters: Vec<(AgentRole, AdapterExecutionOutput)>,
        scripted_validations: Vec<ValidationRunSummary>,
        plan_output: Option<ValidatedPlanOutput>,
        before_role_symlink_swap: Option<(AgentRole, PathBuf, PathBuf)>,
    ) -> Result<(WorkflowState, Vec<StepProgressEvent>, Vec<AgentRole>, usize, Option<String>, Option<bool>), String> {
        let plan_file_to_observe = plan_output
            .as_ref()
            .map(|output| output.destination_at_start.clone());
        let mut engine = OrchestratorEngine::new();
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(scripted_adapters.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
            before_role_symlink_swap: Mutex::new(before_role_symlink_swap),
            implementer_prompt: Mutex::new(None),
            plan_file_to_observe,
            plan_exists_at_implementer: Mutex::new(None),
        });
        let scripted_val_executor = Arc::new(ScriptedValidationExecutor {
            summaries: Mutex::new(scripted_validations.into_iter().collect()),
            runs: Mutex::new(0),
        });
        engine.scripted_adapter_executor = Some(Arc::clone(&scripted_executor));
        engine.scripted_validation_executor = Some(Arc::clone(&scripted_val_executor));
        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);

        let events_clone = Arc::clone(&events);
        tokio::spawn(async move {
            for _ in 0..50 {
                tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;
                let is_waiting = events_clone
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|e| e.step == WorkflowState::WaitingForUser);
                if is_waiting {
                    let _ = clarification_tx
                        .send("User clarification response".to_string())
                        .await;
                    break;
                }
            }
        });

        let state = engine
            .run_workflow(
                "run-engine-test".into(),
                snapshot,
                "Exercise workflow behavior".into(),
                workflow_type.to_string(),
                plan_output,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                Arc::new(move |event| event_sink.lock().unwrap().push(event)),
                Arc::new(|_| {}),
            )
            .await?;
        drop(pause_tx);
        let events = events.lock().unwrap().clone();
        let calls = scripted_executor.calls.lock().unwrap().clone();
        let val_runs = *scripted_val_executor.runs.lock().unwrap();
        let implementer_prompt = scripted_executor.implementer_prompt.lock().unwrap().clone();
        let plan_exists_at_implementer = *scripted_executor.plan_exists_at_implementer.lock().unwrap();
        Ok((state, events, calls, val_runs, implementer_prompt, plan_exists_at_implementer))
    }

    fn validated_output(project: &Path, path: &Path, overwrite_existing: bool) -> ValidatedPlanOutput {
        validate_plan_output_options(
            &project.to_string_lossy(),
            PlanOutputOptions {
                path: path.to_string_lossy().into_owned(),
                overwrite_existing,
            },
        )
        .unwrap()
    }

    #[test]
    fn plan_target_validation_rejects_outside_paths_and_non_markdown_targets() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside.md");
        std::fs::create_dir(&project).unwrap();
        assert!(resolve_plan_target(
            &project.to_string_lossy(),
            &outside.to_string_lossy()
        )
        .unwrap_err()
        .contains("inside"));
        assert!(resolve_plan_target(
            &project.to_string_lossy(),
            &project.join("plan.txt").to_string_lossy()
        )
        .unwrap_err()
        .contains("Markdown"));
        assert!(resolve_plan_target(
            &project.to_string_lossy(),
            &project.join("missing-parent").join("plan.md").to_string_lossy()
        )
        .is_err());
    }

    #[test]
    fn approved_plan_atomic_save_installs_new_and_overwrites_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let target = project.join("IMPLEMENTATION_PLAN.md");

        let new_output = validated_output(&project, &target, false);
        persist_approved_plan(&project.to_string_lossy(), &new_output, "approved plan ✓").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "approved plan ✓");

        let overwrite = validated_output(&project, &target, true);
        persist_approved_plan(&project.to_string_lossy(), &overwrite, "replacement plan").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "replacement plan");
        assert_eq!(
            std::fs::read_dir(&project)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
                .count(),
            0
        );
    }

    #[test]
    fn existing_plan_requires_explicit_overwrite_authorization() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let target = project.join("IMPLEMENTATION_PLAN.md");
        std::fs::write(&target, b"preserve these bytes").unwrap();

        let result = validate_plan_output_options(
            &project.to_string_lossy(),
            PlanOutputOptions {
                path: target.to_string_lossy().into_owned(),
                overwrite_existing: false,
            },
        );
        assert!(result.unwrap_err().contains("overwrite confirmation"));
        assert_eq!(std::fs::read(&target).unwrap(), b"preserve these bytes");
    }

    #[test]
    fn failed_atomic_replace_preserves_existing_target_and_cleans_temp_file() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let target = project.join("IMPLEMENTATION_PLAN.md");
        std::fs::write(&target, b"original bytes").unwrap();
        let output = validated_output(&project, &target, true);

        let result = persist_approved_plan_with_installer(
            &project.to_string_lossy(),
            &output,
            "replacement",
            |_temp, _target, _exists| Err(std::io::Error::other("injected replace failure")),
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original bytes");
        assert_eq!(std::fs::read_dir(&project).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn plan_only_saves_only_the_final_reviewer_approved_revision() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let target = project.join("IMPLEMENTATION_PLAN.md");
        let output = validated_output(&project, &target, false);
        let (state, _, calls, _, _, _) = run_scripted_workflow_with_plan_output(
            "plan_only",
            workflow_snapshot(project.to_string_lossy().into_owned()),
            vec![
                (AgentRole::Planner, scripted_output("draft plan")),
                (AgentRole::PlanReviewer, scripted_output(r#"{"verdict":"changes_required","summary":"add tests","findings":[]}"#)),
                (AgentRole::Planner, scripted_output("final approved plan")),
                (AgentRole::PlanReviewer, scripted_output(r#"{"verdict":"approved","summary":"ready","findings":[]}"#)),
            ],
            vec![],
            Some(output),
            None,
        )
        .await
        .unwrap();
        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer, AgentRole::Planner, AgentRole::PlanReviewer]);
        assert_eq!(std::fs::read_to_string(target).unwrap(), "final approved plan");
    }

    #[tokio::test]
    async fn plan_only_review_failure_does_not_save_or_complete() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let target = project.join("IMPLEMENTATION_PLAN.md");
        std::fs::write(&target, b"previous approved plan").unwrap();
        let output = validated_output(&project, &target, true);

        let (state, events, calls, _, _, _) = run_scripted_workflow_with_plan_output(
            "plan_only",
            workflow_snapshot(project.to_string_lossy().into_owned()),
            vec![
                (AgentRole::Planner, scripted_output("unapproved draft")),
                (AgentRole::PlanReviewer, scripted_output(r#"{"verdict":"failed","summary":"cannot review","findings":[]}"#)),
            ],
            vec![],
            Some(output),
            None,
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert_eq!(std::fs::read(&target).unwrap(), b"previous approved plan");
        assert!(!events.iter().any(|event| event.step == WorkflowState::Complete));
    }

    #[tokio::test]
    async fn full_loop_persists_plan_before_implementer_and_keeps_same_plan_in_prompt() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let target = project.join("IMPLEMENTATION_PLAN.md");
        std::fs::write(&target, b"prior plan").unwrap();
        let output = validated_output(&project, &target, true);
        let (state, _, calls, _, implementer_prompt, plan_existed_at_implementer) =
            run_scripted_workflow_with_plan_output(
                "full_loop",
                workflow_snapshot(project.to_string_lossy().into_owned()),
                vec![
                    (AgentRole::Planner, scripted_output("approved plan body")),
                    (AgentRole::PlanReviewer, scripted_output(r#"{"verdict":"approved","summary":"ready","findings":[]}"#)),
                    (AgentRole::Implementer, scripted_output("implementation")),
                    (AgentRole::CodeReviewer, scripted_output(r#"{"verdict":"approved","summary":"done","findings":[]}"#)),
                ],
                vec![],
                Some(output),
                None,
            )
            .await
            .unwrap();
        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls[2], AgentRole::Implementer);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "approved plan body");
        assert_eq!(plan_existed_at_implementer, Some(true));
        assert!(implementer_prompt.unwrap().contains("approved plan body"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn save_boundary_rejects_project_symlink_retargeted_during_review_before_implementer() {
        let temp = tempfile::tempdir().unwrap();
        let original_project = temp.path().join("original-project");
        let replacement_project = temp.path().join("replacement-project");
        let project_link = temp.path().join("project-link");
        std::fs::create_dir(&original_project).unwrap();
        std::fs::create_dir(&replacement_project).unwrap();
        std::os::windows::fs::symlink_dir(&original_project, &project_link).unwrap();
        let target = project_link.join("IMPLEMENTATION_PLAN.md");
        let output = validated_output(&project_link, &target, false);

        let (state, _, calls, _, _, _) = run_scripted_workflow_with_plan_output(
            "full_loop",
            workflow_snapshot(project_link.to_string_lossy().into_owned()),
            vec![
                (AgentRole::Planner, scripted_output("approved plan")),
                (AgentRole::PlanReviewer, scripted_output(r#"{"verdict":"approved","summary":"ready","findings":[]}"#)),
            ],
            vec![],
            Some(output),
            Some((AgentRole::PlanReviewer, project_link.clone(), replacement_project.clone())),
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert!(!replacement_project.join("IMPLEMENTATION_PLAN.md").exists());
        assert!(!original_project.join("IMPLEMENTATION_PLAN.md").exists());
    }

    #[tokio::test]
    async fn plan_only_success_completes_with_approved_plan_and_no_downstream_roles() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "plan_only",
            snapshot,
            vec![
                (AgentRole::Planner, scripted_output("Generated architecture plan")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"plan approved","findings":[]}"#),
                ),
            ],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert_eq!(val_runs, 0);
        assert!(events.iter().any(|e| e.step == WorkflowState::Complete && e.plan_text.is_some()));
        assert!(!events.iter().any(|e| matches!(
            e.step,
            WorkflowState::Implementation | WorkflowState::Validation | WorkflowState::CodeReview | WorkflowState::Fix
        )));
    }

    #[tokio::test]
    async fn plan_only_changes_required_then_approved_revises_and_completes() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "plan_only",
            snapshot,
            vec![
                (AgentRole::Planner, scripted_output("Plan draft 1")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"changes_required","summary":"add error handling","findings":[{"id":"F-1","severity":"medium","issue":"missing error handling","is_blocking":true}]}"#),
                ),
                (AgentRole::Planner, scripted_output("Plan draft 2 with error handling")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"plan approved","findings":[]}"#),
                ),
            ],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(
            calls,
            vec![
                AgentRole::Planner,
                AgentRole::PlanReviewer,
                AgentRole::Planner,
                AgentRole::PlanReviewer,
            ]
        );
        assert_eq!(val_runs, 0);
        assert!(events.iter().any(|e| e.step == WorkflowState::PlanRevision));
        assert!(events.iter().any(|e| e.step == WorkflowState::Complete));
    }

    #[tokio::test]
    async fn plan_only_failed_verdict_is_terminal() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "plan_only",
            snapshot,
            vec![
                (AgentRole::Planner, scripted_output("Plan draft")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"failed","summary":"spec violation","findings":[]}"#),
                ),
            ],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert_eq!(val_runs, 0);
        assert!(events.iter().any(|e| e.step == WorkflowState::Failed));
        assert!(!events.iter().any(|e| matches!(
            e.step,
            WorkflowState::Implementation | WorkflowState::Validation | WorkflowState::CodeReview | WorkflowState::Complete
        )));
    }

    #[tokio::test]
    async fn implement_only_success_completes_without_planning_or_code_review() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "implement_only",
            snapshot,
            vec![(AgentRole::Implementer, scripted_output("Code changes applied"))],
            vec![ValidationRunSummary {
                passed: true,
                total_gates_run: 1,
                failed_gate_names: vec![],
                results: vec![],
                formatted_diagnostics: String::new(),
            }],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::Implementer]);
        assert_eq!(val_runs, 1);
        assert!(events.iter().any(|e| e.step == WorkflowState::Implementation));
        assert!(events.iter().any(|e| e.step == WorkflowState::Validation));
        assert!(events.iter().any(|e| e.step == WorkflowState::Complete));
        assert!(!events.iter().any(|e| matches!(
            e.step,
            WorkflowState::PlanGeneration | WorkflowState::PlanReview | WorkflowState::CodeReview
        )));
    }

    #[tokio::test]
    async fn implement_only_validation_failure_then_fixer_success() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "implement_only",
            snapshot,
            vec![
                (AgentRole::Implementer, scripted_output("Initial implementation")),
                (AgentRole::Fixer, scripted_output("Fixed type errors")),
            ],
            vec![
                ValidationRunSummary {
                    passed: false,
                    total_gates_run: 1,
                    failed_gate_names: vec!["cargo check".to_string()],
                    results: vec![],
                    formatted_diagnostics: "type error".to_string(),
                },
                ValidationRunSummary {
                    passed: true,
                    total_gates_run: 1,
                    failed_gate_names: vec![],
                    results: vec![],
                    formatted_diagnostics: String::new(),
                },
            ],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::Implementer, AgentRole::Fixer]);
        assert_eq!(val_runs, 2);
        assert!(events.iter().any(|e| e.step == WorkflowState::Fix));
        assert!(events.iter().any(|e| e.step == WorkflowState::Complete));
        assert!(!events.iter().any(|e| matches!(
            e.step,
            WorkflowState::PlanGeneration | WorkflowState::PlanReview | WorkflowState::CodeReview
        )));
    }

    #[tokio::test]
    async fn implement_only_exhausted_fix_iterations_is_terminal() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        snapshot.iteration_limits.max_fix_iterations = 1;

        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "implement_only",
            snapshot,
            vec![
                (AgentRole::Implementer, scripted_output("Initial implementation")),
                (AgentRole::Fixer, scripted_output("Fix attempt 1")),
            ],
            vec![
                ValidationRunSummary {
                    passed: false,
                    total_gates_run: 1,
                    failed_gate_names: vec!["cargo test".to_string()],
                    results: vec![],
                    formatted_diagnostics: "test failed".to_string(),
                },
                ValidationRunSummary {
                    passed: false,
                    total_gates_run: 1,
                    failed_gate_names: vec!["cargo test".to_string()],
                    results: vec![],
                    formatted_diagnostics: "test still failed".to_string(),
                },
            ],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::Implementer, AgentRole::Fixer]);
        assert_eq!(val_runs, 2);
        assert!(events.iter().any(|e| e.step == WorkflowState::Failed));
        assert!(!events.iter().any(|e| e.step == WorkflowState::CodeReview));
    }

    #[tokio::test]
    async fn review_only_approved_completes_ready_with_no_validation_or_fixer() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "review_only",
            snapshot,
            vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"diff looks clean","findings":[]}"#),
            )],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::CodeReviewer]);
        assert_eq!(val_runs, 0);
        assert!(events.iter().any(|e| e.step == WorkflowState::CodeReview));
        assert!(events.iter().any(|e| e.step == WorkflowState::Complete));
        assert!(!events.iter().any(|e| matches!(
            e.step,
            WorkflowState::PlanGeneration | WorkflowState::Implementation | WorkflowState::Validation | WorkflowState::Fix
        )));
    }

    #[tokio::test]
    async fn review_only_changes_required_completes_not_ready_with_findings_and_no_fixer() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "review_only",
            snapshot,
            vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"changes_required","summary":"found edge cases","findings":[{"id":"F-1","severity":"high","issue":"unchecked index","is_blocking":true}]}"#),
            )],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::CodeReviewer]);
        assert_eq!(val_runs, 0);
        let complete_event = events
            .iter()
            .find(|e| e.step == WorkflowState::Complete)
            .expect("Expected Complete step for review-only changes-required");
        let review_res = complete_event.review_result.as_ref().unwrap();
        assert_eq!(review_res.verdict, ReviewVerdict::ChangesRequired);
        assert_eq!(review_res.findings.len(), 1);
        assert!(!events.iter().any(|e| matches!(
            e.step,
            WorkflowState::Fix | WorkflowState::Validation | WorkflowState::Failed
        )));
    }

    #[tokio::test]
    async fn review_only_failed_verdict_is_terminal() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "review_only",
            snapshot,
            vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"failed","summary":"cannot evaluate malformed diff","findings":[]}"#),
            )],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::CodeReviewer]);
        assert_eq!(val_runs, 0);
        assert!(events.iter().any(|e| e.step == WorkflowState::Failed));
        assert!(!events.iter().any(|e| matches!(e.step, WorkflowState::Fix | WorkflowState::Validation)));
    }

    #[tokio::test]
    async fn review_only_needs_clarification_fails_without_a_second_review_pass() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "review_only",
            snapshot,
            vec![
                (
                    AgentRole::CodeReviewer,
                    scripted_output(r#"{"verdict":"needs_clarification","summary":"is auth optional?","findings":[]}"#),
                ),
                (
                    AgentRole::CodeReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"diff approved with clarification","findings":[]}"#),
                ),
            ],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::CodeReviewer]);
        assert_eq!(val_runs, 0);
        assert!(events.iter().any(|e| e.step == WorkflowState::Failed));
        assert!(!events.iter().any(|e| e.step == WorkflowState::WaitingForUser));
    }

    #[tokio::test]
    async fn review_only_rejects_cli_adapter_without_mutating_capabilities() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let mut cli_profile = mutating_cli_profile();
        cli_profile.capabilities = vec![ProfileCapability::Review, ProfileCapability::WorkspaceRead];
        snapshot.assignments.insert(AgentRole::CodeReviewer, cli_profile);

        let result = run_scripted_workflow_full("review_only", snapshot, vec![], vec![]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires a read-only adapter"));
    }

    #[tokio::test]
    async fn review_only_rejects_provider_with_workspace_write_alone() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let mut profile = readonly_reviewer_profile();
        profile.capabilities.push(ProfileCapability::WorkspaceWrite);
        snapshot.assignments.insert(AgentRole::CodeReviewer, profile);

        let result = run_scripted_workflow_full("review_only", snapshot, vec![], vec![]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("workspace_write"));
    }

    #[tokio::test]
    async fn review_only_rejects_provider_with_command_execution_alone() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let mut profile = readonly_reviewer_profile();
        profile.capabilities.push(ProfileCapability::CommandExecution);
        snapshot.assignments.insert(AgentRole::CodeReviewer, profile);

        let result = run_scripted_workflow_full("review_only", snapshot, vec![], vec![]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("command_execution"));
    }

    #[tokio::test]
    async fn review_only_accepts_safe_provider_and_ollama_profiles() {
        for adapter in [ExecutionAdapterType::Provider, ExecutionAdapterType::Ollama] {
            let directory = tempfile::tempdir().unwrap();
            let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
            let mut profile = readonly_reviewer_profile();
            profile.capabilities.push(ProfileCapability::Reasoning);
            if adapter == ExecutionAdapterType::Ollama {
                profile.id = "readonly-ollama-reviewer".to_string();
                profile.display_name = "Read-Only Ollama Reviewer".to_string();
                profile.adapter = ExecutionAdapterType::Ollama;
                profile.provider_id = None;
                profile.model = None;
                profile.ollama_model = Some("qwen2.5:7b".to_string());
                profile.ollama_endpoint = Some("http://127.0.0.1:11434".to_string());
            }
            snapshot.assignments.insert(AgentRole::CodeReviewer, profile);

            let (state, _events, calls, val_runs) = run_scripted_workflow_full(
                "review_only",
                snapshot,
                vec![(
                    AgentRole::CodeReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"read-only review passed","findings":[]}"#),
                )],
                vec![],
            )
            .await
            .unwrap();

            assert_eq!(state, WorkflowState::Complete);
            assert_eq!(calls, vec![AgentRole::CodeReviewer]);
            assert_eq!(val_runs, 0);
        }
    }

    #[tokio::test]
    async fn workflow_iteration_limits_ignore_unused_limits_and_reject_required_zero_limits() {
        let directory = tempfile::tempdir().unwrap();

        let mut plan = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        plan.iteration_limits.max_plan_review_iterations = 1;
        plan.iteration_limits.max_fix_iterations = 0;
        plan.iteration_limits.max_code_review_iterations = 0;
        let (state, _, calls, _) = run_scripted_workflow_full(
            "plan_only",
            plan,
            vec![
                (AgentRole::Planner, scripted_output("Plan")),
                (AgentRole::PlanReviewer, scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#)),
            ],
            vec![],
        )
        .await
        .unwrap();
        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);

        let mut implement = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        implement.iteration_limits.max_plan_review_iterations = 0;
        implement.iteration_limits.max_fix_iterations = 1;
        implement.iteration_limits.max_code_review_iterations = 0;
        let (state, _, calls, _) = run_scripted_workflow_full(
            "implement_only",
            implement,
            vec![(AgentRole::Implementer, scripted_output("Implemented"))],
            vec![ValidationRunSummary {
                passed: true,
                total_gates_run: 0,
                failed_gate_names: vec![],
                results: vec![],
                formatted_diagnostics: String::new(),
            }],
        )
        .await
        .unwrap();
        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::Implementer]);

        let mut review = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        review.iteration_limits.max_plan_review_iterations = 0;
        review.iteration_limits.max_fix_iterations = 0;
        review.iteration_limits.max_code_review_iterations = 0;
        let (state, _, calls, validations) = run_scripted_workflow_full(
            "review_only",
            review,
            vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )],
            vec![],
        )
        .await
        .unwrap();
        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::CodeReviewer]);
        assert_eq!(validations, 0);

        let mut plan_required_zero = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        plan_required_zero.iteration_limits.max_plan_review_iterations = 0;
        let err = run_scripted_workflow_full("plan_only", plan_required_zero, vec![], vec![])
            .await
            .unwrap_err();
        assert!(err.contains("plan_review iteration limit"));

        let mut implement_required_zero = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        implement_required_zero.iteration_limits.max_fix_iterations = 0;
        let err = run_scripted_workflow_full("implement_only", implement_required_zero, vec![], vec![])
            .await
            .unwrap_err();
        assert!(err.contains("fix iteration limit"));

        for (field, name) in [
            ("plan", "plan_review"),
            ("fix", "fix"),
            ("code", "code_review"),
        ] {
            let mut full = workflow_snapshot(directory.path().to_string_lossy().into_owned());
            match field {
                "plan" => full.iteration_limits.max_plan_review_iterations = 0,
                "fix" => full.iteration_limits.max_fix_iterations = 0,
                _ => full.iteration_limits.max_code_review_iterations = 0,
            }
            let err = run_scripted_workflow_full("full_loop", full, vec![], vec![])
                .await
                .unwrap_err();
            assert!(err.contains(&format!("{name} iteration limit")), "{err}");
        }
    }

    #[tokio::test]
    async fn active_role_scoping_ignores_inactive_assignments_but_enforces_active_assignments() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());

        // Corrupt Planner and PlanReviewer (inactive for implement_only)
        snapshot.assignments.remove(&AgentRole::Planner);
        snapshot.assignments.remove(&AgentRole::PlanReviewer);

        // implement_only should succeed since Implementer and Fixer are valid
        let (state, _events, calls, _val_runs) = run_scripted_workflow_full(
            "implement_only",
            snapshot.clone(),
            vec![(AgentRole::Implementer, scripted_output("Implemented"))],
            vec![ValidationRunSummary {
                passed: true,
                total_gates_run: 0,
                failed_gate_names: vec![],
                results: vec![],
                formatted_diagnostics: String::new(),
            }],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::Implementer]);

        // plan_only with missing Planner should fail preflight
        let plan_res = run_scripted_workflow_full("plan_only", snapshot, vec![], vec![]).await;
        assert!(plan_res.is_err());
        assert!(plan_res.unwrap_err().contains("Planner"));
    }

    #[tokio::test]
    async fn full_loop_preserves_role_sequence_gates_and_review_loop() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "full_loop",
            snapshot,
            vec![
                (AgentRole::Planner, scripted_output("Full plan")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"plan ok","findings":[]}"#),
                ),
                (AgentRole::Implementer, scripted_output("Full implementation")),
                (
                    AgentRole::CodeReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"diff ok","findings":[]}"#),
                ),
            ],
            vec![ValidationRunSummary {
                passed: true,
                total_gates_run: 1,
                failed_gate_names: vec![],
                results: vec![],
                formatted_diagnostics: String::new(),
            }],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(
            calls,
            vec![
                AgentRole::Planner,
                AgentRole::PlanReviewer,
                AgentRole::Implementer,
                AgentRole::CodeReviewer,
            ]
        );
        assert_eq!(val_runs, 1);
        assert!(events.iter().any(|e| e.step == WorkflowState::PlanGeneration));
        assert!(events.iter().any(|e| e.step == WorkflowState::PlanReview));
        assert!(events.iter().any(|e| e.step == WorkflowState::Implementation));
        assert!(events.iter().any(|e| e.step == WorkflowState::Validation));
        assert!(events.iter().any(|e| e.step == WorkflowState::CodeReview));
        assert!(events.iter().any(|e| e.step == WorkflowState::Complete));
    }
}
