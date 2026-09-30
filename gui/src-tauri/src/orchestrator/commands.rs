use super::engine::{
    preview_plan_archive, validate_plan_archive_options, BlockingResolution, OrchestratorEngine,
    StepProgressEvent, ValidatedPlanArchive,
};
use super::presets;
use super::types::*;
use super::validation::compute_gate_command_hash;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct ActiveRun {
    pub run_id: String,
    pub control_tx: watch::Sender<RunControlState>,
    pub cancel_token: CancellationToken,
    pub clarification_tx: mpsc::Sender<String>,
    pub blocking_resolution_tx: mpsc::Sender<BlockingResolution>,
    pub human_gate_tx: mpsc::Sender<HumanGateDecision>,
    pub worker_reclaim_tx: mpsc::Sender<()>,
    pub current_step: Arc<Mutex<StepProgressEvent>>,
}

#[derive(Default)]
pub struct OrchestratorState {
    pub active_run: Mutex<Option<ActiveRun>>,
    pub authorized_custom_gates: Mutex<Vec<AuthorizedCustomGate>>,
}

/// Execution boundary used by the production run starter and its side-effect-free tests.
pub trait RunStartRuntime: Clone + Send + Sync + 'static {
    fn emit_step(&self, event: StepProgressEvent);
    fn emit_log(&self, event: super::types::RunLogEvent);
    fn spawn(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);
}

impl RunStartRuntime for AppHandle {
    fn emit_step(&self, event: StepProgressEvent) {
        let _ = self.emit("orchestrator:step_update", event);
    }

    fn emit_log(&self, event: super::types::RunLogEvent) {
        let _ = self.emit("orchestrator:log", event);
    }

    fn spawn(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
        tokio::spawn(task);
    }
}

impl OrchestratorState {
    pub fn new() -> Self {
        Self {
            active_run: Mutex::new(None),
            authorized_custom_gates: Mutex::new(Vec::new()),
        }
    }
}

/// Detects project metadata, configuration files, and language environment.
pub fn detect_project_metadata_impl(project_path: &str) -> ProjectMetadataResponse {
    let path = Path::new(project_path);
    if !path.exists() {
        return ProjectMetadataResponse {
            path: project_path.to_string(),
            exists: false,
            is_directory: false,
            project_type: "Unknown".to_string(),
            detected_files: DetectedFiles {
                spec_md: false,
                implementation_plan_md: false,
                agents_md: false,
                readme_md: false,
                package_json: false,
                tsconfig_json: false,
                pyproject_toml: false,
                requirements_txt: false,
                cargo_toml: false,
                go_mod: false,
                description: false,
                renv_lock: false,
                clasp_json: false,
                git: false,
            },
            suggested_gates: vec![],
        };
    }

    let is_directory = path.is_dir();
    let spec_md = path.join("SPEC.md").exists();
    let implementation_plan_md = path.join("IMPLEMENTATION_PLAN.md").exists();
    let agents_md = path.join("AGENTS.md").exists();
    let readme_md = path.join("README.md").exists();
    let git = path.join(".git").exists();

    let locations = presets::discover_project_locations(path, 2);

    let mut package_json = false;
    let mut tsconfig_json = false;
    let mut pyproject_toml = false;
    let mut requirements_txt = false;
    let mut cargo_toml = false;
    let mut go_mod = false;
    let mut description = false;
    let mut renv_lock = false;
    let mut clasp_json = false;

    let mut detected_types = Vec::new();

    for loc in &locations {
        let p = &loc.full_path;
        let has_pkg = p.join("package.json").exists();
        let has_tsc = p.join("tsconfig.json").exists();
        let has_pyproject = p.join("pyproject.toml").exists();
        let has_reqs = p.join("requirements.txt").exists();
        let has_setup_py = p.join("setup.py").exists();
        let has_setup_cfg = p.join("setup.cfg").exists();
        let has_cargo = p.join("Cargo.toml").exists();
        let has_go = p.join("go.mod").exists();
        let has_desc = p.join("DESCRIPTION").exists();
        let has_renv = p.join("renv.lock").exists();
        let has_clasp = p.join(".clasp.json").exists();

        if has_pkg {
            package_json = true;
        }
        if has_tsc {
            tsconfig_json = true;
        }
        if has_pyproject {
            pyproject_toml = true;
        }
        if has_reqs {
            requirements_txt = true;
        }
        if has_cargo {
            cargo_toml = true;
        }
        if has_go {
            go_mod = true;
        }
        if has_desc {
            description = true;
        }
        if has_renv {
            renv_lock = true;
        }
        if has_clasp {
            clasp_json = true;
        }

        if has_pkg || has_tsc {
            if has_clasp {
                if !detected_types.contains(&"Google Apps Script (clasp)") {
                    detected_types.push("Google Apps Script (clasp)");
                }
            } else if !detected_types.contains(&"TypeScript / Node") {
                detected_types.push("TypeScript / Node");
            }
        } else if has_clasp && !detected_types.contains(&"Google Apps Script (clasp)") {
            detected_types.push("Google Apps Script (clasp)");
        }

        if has_cargo && !detected_types.contains(&"Rust") {
            detected_types.push("Rust");
        }
        if (has_pyproject || has_reqs || has_setup_py || has_setup_cfg)
            && !detected_types.contains(&"Python")
        {
            detected_types.push("Python");
        }
        if has_go && !detected_types.contains(&"Go") {
            detected_types.push("Go");
        }
        if (has_desc || has_renv) && !detected_types.contains(&"R") {
            detected_types.push("R");
        }
    }

    let project_type = match detected_types.len() {
        0 => "Unknown".to_string(),
        1 => detected_types[0].to_string(),
        _ => format!("Mixed ({})", detected_types.join(", ")),
    };

    let suggested_gates = presets::generate_project_validation_gates(path);

    ProjectMetadataResponse {
        path: project_path.to_string(),
        exists: true,
        is_directory,
        project_type,
        detected_files: DetectedFiles {
            spec_md,
            implementation_plan_md,
            agents_md,
            readme_md,
            package_json,
            tsconfig_json,
            pyproject_toml,
            requirements_txt,
            cargo_toml,
            go_mod,
            description,
            renv_lock,
            clasp_json,
            git,
        },
        suggested_gates,
    }
}

/// Atomically starts an orchestrator run, installs ActiveRun, spawns the async workflow, and returns StartRunResponse immediately.
pub fn start_orchestrator_run_impl<R: RunStartRuntime>(
    runtime: R,
    state: Arc<OrchestratorState>,
    snapshot: RunConfigurationSnapshot,
    task_prompt: String,
    workflow_type: String,
    transient_overrides: Option<RunTransientOverrides>,
    plan_archive_options: Option<PlanArchiveOptions>,
) -> Result<StartRunResponse, String> {
    let snapshot = prepare_run_snapshot(&workflow_type, snapshot, transient_overrides)?;
    let plan_archive: Option<ValidatedPlanArchive> = match workflow_type.as_str() {
        "full_loop" | "plan_only" | "human_gated_loop" => Some(validate_plan_archive_options(
            &snapshot.project_path,
            plan_archive_options.ok_or_else(|| {
                "A plan archive folder is required for this workflow.".to_string()
            })?,
        )?),
        "implement_only" | "review_only" => {
            if plan_archive_options.is_some() {
                return Err("This workflow does not accept a plan archive folder.".to_string());
            }
            None
        }
        _ => return Err(format!("Unsupported Orchestrator workflow '{workflow_type}'.")),
    };
    let run_id = Uuid::new_v4().to_string();

    let (control_tx, control_rx) = watch::channel(RunControlState::Running);
    let cancel_token = CancellationToken::new();
    let (clarification_tx, clarification_rx) = mpsc::channel::<String>(16);
    let (blocking_tx, blocking_rx) = mpsc::channel::<BlockingResolution>(16);
    let (human_gate_tx, human_gate_rx) = mpsc::channel::<HumanGateDecision>(16);
    let (worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel::<()>(16);

    // Snapshot authorized custom gates
    let authorized_gates = {
        let auth_lock = state
            .authorized_custom_gates
            .lock()
            .map_err(|e| e.to_string())?;
        auth_lock.clone()
    };

    let current_step = Arc::new(Mutex::new(StepProgressEvent {
        run_id: run_id.clone(),
        step: WorkflowState::BuildingContext,
        iteration_info: None,
        message: String::new(),
        review_result: None,
        validation_summary: None,
        plan_text: None,
    }));

    // Atomically check that no active run exists and install the new active run
    {
        let mut active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
        if active_lock.is_some() {
            return Err(
                "An orchestrator run is already active. Stop or cancel the current run first."
                    .to_string(),
            );
        }
        *active_lock = Some(ActiveRun {
            run_id: run_id.clone(),
            control_tx,
            cancel_token: cancel_token.clone(),
            clarification_tx,
            blocking_resolution_tx: blocking_tx,
            human_gate_tx,
            worker_reclaim_tx,
            current_step: current_step.clone(),
        });
    }

    let spawned_run_id = run_id.clone();
    let state_clone = Arc::clone(&state);

    let event_runtime = runtime.clone();
    let step_tracker = current_step.clone();
    let on_event: super::engine::EventCallback = Arc::new(move |evt: StepProgressEvent| {
        if let Ok(mut lock) = step_tracker.lock() {
            *lock = evt.clone();
        }
        event_runtime.emit_step(evt);
    });

    let log_runtime = runtime.clone();
    let on_log: super::engine::LogCallback =
        Arc::new(move |log_event: super::types::RunLogEvent| {
            log_runtime.emit_log(log_event);
        });

    let run_id_for_task = run_id.clone();
    let supervisor_run_id = run_id_for_task.clone();
    let engine = OrchestratorEngine::new();
    let workflow_cancel_token = cancel_token.clone();
    let workflow_on_event = on_event.clone();
    let workflow_on_log = on_log.clone();

    runtime.spawn(Box::pin(async move {
        let workflow = async move {
            engine
                .run_workflow(
                    run_id_for_task.clone(),
                    snapshot,
                    task_prompt,
                    workflow_type,
                    plan_archive,
                    authorized_gates,
                    control_rx,
                    workflow_cancel_token,
                    clarification_rx,
                    blocking_rx,
                    human_gate_rx,
                    worker_reclaim_rx,
                    workflow_on_event,
                    workflow_on_log,
                )
                .await
        };
        supervise_run(workflow, supervisor_run_id, state_clone, on_event, on_log).await;
    }));

    Ok(StartRunResponse {
        run_id: spawned_run_id,
    })
}

pub fn preview_plan_archive_impl(
    project_path: &str,
    archive_directory: &str,
) -> Result<PlanArchivePreview, String> {
    preview_plan_archive(project_path, archive_directory)
}

fn validate_workflow_type(workflow_type: &str) -> Result<(), String> {
    match workflow_type {
        "full_loop" | "plan_only" | "implement_only" | "review_only" | "human_gated_loop" => Ok(()),
        _ => Err(format!(
            "Unsupported Orchestrator workflow '{workflow_type}'. Supported workflows are: full_loop, plan_only, implement_only, review_only, human_gated_loop."
        )),
    }
}

fn prepare_run_snapshot(
    workflow_type: &str,
    snapshot: RunConfigurationSnapshot,
    overrides: Option<RunTransientOverrides>,
) -> Result<RunConfigurationSnapshot, String> {
    let active_roles = active_roles_for_workflow(workflow_type)?;
    for role in &active_roles {
        let profile = snapshot.assignments.get(role);
        validate_workflow_role_capabilities(workflow_type, role, profile)
            .map_err(|e| format!("Preflight validation failure for role '{:?}': {}", role, e.message))?;
    }
    apply_transient_overrides(snapshot, overrides)
}

fn apply_transient_overrides(
    mut snapshot: RunConfigurationSnapshot,
    overrides: Option<RunTransientOverrides>,
) -> Result<RunConfigurationSnapshot, String> {
    let Some(overrides) = overrides else {
        return Ok(snapshot);
    };

    if let Some(gate_overrides) = overrides.gate_overrides {
        for gate_id in gate_overrides.keys() {
            if !snapshot.validation_gates.iter().any(|gate| gate.id == *gate_id) {
                return Err(format!("Transient override references unknown validation gate '{gate_id}'."));
            }
        }
        for gate in &mut snapshot.validation_gates {
            if let Some(enabled) = gate_overrides.get(&gate.id) {
                gate.enabled = *enabled;
            }
        }
    }

    if let Some(limits) = overrides.limit_overrides {
        for value in [
            limits.max_plan_review_iterations,
            limits.max_fix_iterations,
            limits.max_code_review_iterations,
        ]
        .into_iter()
        .flatten()
        {
            if !(1..=100).contains(&value) {
                return Err("Transient iteration limits must be between 1 and 100.".to_string());
            }
        }
        if let Some(value) = limits.max_plan_review_iterations {
            snapshot.iteration_limits.max_plan_review_iterations = value;
        }
        if let Some(value) = limits.max_fix_iterations {
            snapshot.iteration_limits.max_fix_iterations = value;
        }
        if let Some(value) = limits.max_code_review_iterations {
            snapshot.iteration_limits.max_code_review_iterations = value;
        }
    }

    Ok(snapshot)
}

async fn supervise_run<F>(
    workflow: F,
    run_id: String,
    state: Arc<OrchestratorState>,
    on_event: super::engine::EventCallback,
    on_log: super::engine::LogCallback,
) where
    F: Future<Output = Result<WorkflowState, String>> + Send + 'static,
{
    // The supervisor observes the workflow JoinHandle so panic unwinding is
    // converted into a failure result and cleanup always follows.
    let result = tokio::spawn(workflow).await;
    let (error, panicked) = match result {
        Ok(Ok(_)) => (None, false),
        Ok(Err(error)) => (Some(error), false),
        Err(join_error) => {
            let panicked = join_error.is_panic();
            (
                Some(if panicked {
                    format!("Orchestrator workflow panicked: {join_error}")
                } else {
                    format!("Orchestrator workflow task failed: {join_error}")
                }),
                panicked,
            )
        }
    };
    if panicked {
        cancel_active_run_for_run(&state, &run_id);
    }
    if let Some(error) = error {
        on_log(super::types::RunLogEvent {
            run_id: run_id.clone(),
            message: format!("[Engine] Run {run_id} ended with error: {error}"),
        });
        on_event(StepProgressEvent {
            run_id: run_id.clone(),
            step: WorkflowState::Failed,
            iteration_info: None,
            message: format!("Workflow execution failed: {error}"),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        });
    }
    clear_active_run_for_run(&state, &run_id);
}

fn clear_active_run_for_run(state: &OrchestratorState, finishing_run_id: &str) {
    if let Ok(mut active_lock) = state.active_run.lock() {
        if active_lock
            .as_ref()
            .is_some_and(|current| current.run_id == finishing_run_id)
        {
            *active_lock = None;
        }
    }
}

fn cancel_active_run_for_run(state: &OrchestratorState, run_id: &str) {
    if let Ok(active_lock) = state.active_run.lock() {
        if let Some(active) = active_lock
            .as_ref()
            .filter(|active| active.run_id == run_id)
        {
            active.cancel_token.cancel();
        }
    }
}

pub fn pause_orchestrator_run_impl(state: &OrchestratorState, run_id: &str) -> Result<(), String> {
    let active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
    if let Some(ref active) = *active_lock {
        if active.run_id != run_id {
            return Err(format!(
                "Run ID mismatch: expected active run '{}', got '{}'",
                active.run_id, run_id
            ));
        }
        let _ = active.control_tx.send(RunControlState::Paused);
        Ok(())
    } else {
        Err("No active run to pause.".to_string())
    }
}

pub fn resume_orchestrator_run_impl(state: &OrchestratorState, run_id: &str) -> Result<(), String> {
    let active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
    if let Some(ref active) = *active_lock {
        if active.run_id != run_id {
            return Err(format!(
                "Run ID mismatch: expected active run '{}', got '{}'",
                active.run_id, run_id
            ));
        }
        let _ = active.control_tx.send(RunControlState::Running);
        Ok(())
    } else {
        Err("No active run to resume.".to_string())
    }
}

pub fn cancel_orchestrator_run_impl(state: &OrchestratorState, run_id: &str) -> Result<(), String> {
    let active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
    if let Some(ref active) = *active_lock {
        if active.run_id != run_id {
            return Err(format!(
                "Run ID mismatch: expected active run '{}', got '{}'",
                active.run_id, run_id
            ));
        }
        active.cancel_token.cancel();
        let _ = active.control_tx.send(RunControlState::Cancelled);
        Ok(())
    } else {
        Err("No active run to cancel.".to_string())
    }
}

pub fn submit_clarification_response_impl(
    state: &OrchestratorState,
    run_id: &str,
    response: String,
) -> Result<(), String> {
    let active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
    if let Some(ref active) = *active_lock {
        if active.run_id != run_id {
            return Err(format!(
                "Run ID mismatch: expected active run '{}', got '{}'",
                active.run_id, run_id
            ));
        }
        active
            .clarification_tx
            .try_send(response)
            .map_err(|e| format!("Failed to send clarification response: {}", e))?;
        Ok(())
    } else {
        Err("No active run waiting for clarification.".to_string())
    }
}

pub fn resolve_blocking_finding_impl(
    state: &OrchestratorState,
    run_id: &str,
    action: String, // "retry" | "abort"
    guidance: Option<String>,
) -> Result<(), String> {
    let active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
    if let Some(ref active) = *active_lock {
        if active.run_id != run_id {
            return Err(format!(
                "Run ID mismatch: expected active run '{}', got '{}'",
                active.run_id, run_id
            ));
        }
        let resolution = match action.as_str() {
            "retry" => BlockingResolution::Retry { guidance },
            "abort" => BlockingResolution::Abort,
            _ => {
                return Err(format!(
                    "Invalid blocking resolution action '{}'. Use 'retry' or 'abort'.",
                    action
                ))
            }
        };
        active
            .blocking_resolution_tx
            .try_send(resolution)
            .map_err(|e| format!("Failed to send blocking resolution: {}", e))?;
        Ok(())
    } else {
        Err("No active run waiting for blocking resolution.".to_string())
    }
}

pub fn resolve_human_gate_impl(
    state: &OrchestratorState,
    run_id: &str,
    decision: HumanGateDecision,
) -> Result<(), String> {
    let active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
    if let Some(ref active) = *active_lock {
        if active.run_id != run_id {
            return Err(format!(
                "Run ID mismatch: expected active run '{}', got '{}'",
                active.run_id, run_id
            ));
        }
        active
            .human_gate_tx
            .try_send(decision)
            .map_err(|e| format!("Failed to send human gate decision: {}", e))?;
        Ok(())
    } else {
        Err("No active run waiting for human gate decision.".to_string())
    }
}

pub fn confirm_worker_stopped_and_reclaim_impl(
    state: &OrchestratorState,
    run_id: &str,
) -> Result<(), String> {
    let active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
    if let Some(ref active) = *active_lock {
        if active.run_id != run_id {
            return Err(format!(
                "Run ID mismatch: expected active run '{}', got '{}'",
                active.run_id, run_id
            ));
        }

        let current_event = active.current_step.lock().map_err(|e| e.to_string())?.clone();

        // Verify workflow state is specifically the worker-disconnect WaitingForUser state awaiting operator confirmation
        let is_worker_disconnect_waiting = current_event.step == WorkflowState::WaitingForUser
            && current_event.message.contains("Antigravity worker disconnected or lease timed out");

        if !is_worker_disconnect_waiting {
            return Err(format!(
                "InvalidState: Cannot reclaim worker in state '{:?}'. Worker reclaim confirmation is only allowed when WaitingForUser due to worker disconnect/lease timeout.",
                current_event.step
            ));
        }

        active
            .worker_reclaim_tx
            .try_send(())
            .map_err(|e| format!("Failed to send worker reclaim confirmation: {}", e))?;
        Ok(())
    } else {
        Err("No active run waiting for worker reclaim confirmation.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Default)]
    struct CountingRunStartRuntime {
        spawn_attempts: Arc<AtomicUsize>,
        event_attempts: Arc<AtomicUsize>,
    }

    impl RunStartRuntime for CountingRunStartRuntime {
        fn emit_step(&self, _event: StepProgressEvent) {
            self.event_attempts.fetch_add(1, Ordering::SeqCst);
        }

        fn emit_log(&self, _event: super::super::types::RunLogEvent) {
            self.event_attempts.fetch_add(1, Ordering::SeqCst);
        }

        fn spawn(&self, _task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
            self.spawn_attempts.fetch_add(1, Ordering::SeqCst);
            // Deliberately do not poll the task: this fake observes whether production
            // reaches the execution boundary without starting adapters or processes.
        }
    }

    fn snapshot_for_overrides() -> RunConfigurationSnapshot {
        let planner_profile = OrchestratorProfile {
            id: "mimo-v26-pro".to_string(),
            display_name: "MiMo".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![ProfileCapability::Reasoning],
            provider_id: Some("mimo".to_string()),
            provider_profile_id: None,
            model: Some("mimo-v2.6-pro".to_string()),
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(128_000),
        };
        let reviewer_profile = OrchestratorProfile {
            id: "deepseek-v41-flash".to_string(),
            display_name: "DeepSeek".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![ProfileCapability::Reasoning, ProfileCapability::Review],
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
            context_window_tokens: Some(128_000),
        };
        let implementer_profile = OrchestratorProfile {
            id: "codex-cli".to_string(),
            display_name: "Codex CLI".to_string(),
            adapter: ExecutionAdapterType::Cli,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::WorkspaceRead,
                ProfileCapability::WorkspaceWrite,
                ProfileCapability::CommandExecution,
                ProfileCapability::Review,
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
            context_window_tokens: Some(128_000),
        };
        let assignments = std::collections::HashMap::from([
            (AgentRole::Planner, planner_profile),
            (AgentRole::PlanReviewer, reviewer_profile.clone()),
            (AgentRole::Implementer, implementer_profile.clone()),
            (AgentRole::Fixer, implementer_profile),
            (AgentRole::CodeReviewer, reviewer_profile),
        ]);
        RunConfigurationSnapshot {
            project_path: "C:/project".to_string(),
            assignments,
            iteration_limits: LoopIterationLimits::default(),
            validation_gates: vec![ValidationGateConfig {
                id: "test".to_string(),
                name: "Tests".to_string(),
                category: Some(ValidationCategory::Tests),
                executable: "cargo".to_string(),
                args: vec!["test".to_string()],
                enabled: true,
                working_dir: Some("C:/project".to_string()),
                fail_on_error: true,
                is_advanced_custom: true,
                success_criteria: Some(GateSuccessCriteria::ExitZero),
            }],
            budget_limits: std::collections::HashMap::new(),
            created_at_unix: 1,
        }
    }

    #[test]
    fn production_start_rejects_unknown_workflow_before_any_execution_side_effect() {
        let runtime = CountingRunStartRuntime::default();
        let state = Arc::new(OrchestratorState::new());
        let initial_authorizations = state.authorized_custom_gates.lock().unwrap().clone();

        let result = start_orchestrator_run_impl(
            runtime.clone(),
            Arc::clone(&state),
            snapshot_for_overrides(),
            "must not run".to_string(),
            "future_workflow".to_string(),
            None,
            None,
        );

        let error = result.unwrap_err();
        assert!(error.contains("Unsupported Orchestrator workflow"));
        assert!(state.active_run.lock().unwrap().is_none());
        assert_eq!(*state.authorized_custom_gates.lock().unwrap(), initial_authorizations);
        assert_eq!(runtime.spawn_attempts.load(Ordering::SeqCst), 0);
        assert_eq!(runtime.event_attempts.load(Ordering::SeqCst), 0);
        // Adapter invocation, validation execution, and engine execution are all
        // downstream of the background task; the fake runtime confirms no task was
        // handed off or polled, so none of those effects can begin.
    }

    #[test]
    fn workflow_start_preflight_accepts_only_known_workflows_before_run_setup() {
        for workflow in ["full_loop", "plan_only", "implement_only", "review_only"] {
            assert!(prepare_run_snapshot(workflow, snapshot_for_overrides(), None).is_ok());
        }
    }

    #[test]
    fn plan_workflow_requires_valid_output_before_active_run_or_spawn() {
        let runtime = CountingRunStartRuntime::default();
        let state = Arc::new(OrchestratorState::new());
        let project = tempfile::tempdir().unwrap();
        let mut snapshot = snapshot_for_overrides();
        snapshot.project_path = project.path().to_string_lossy().into_owned();

        let missing = start_orchestrator_run_impl(
            runtime.clone(),
            Arc::clone(&state),
            snapshot.clone(),
            "task".to_string(),
            "plan_only".to_string(),
            None,
            None,
        )
        .unwrap_err();
        assert!(missing.contains("archive folder is required"));

        let outside = tempfile::tempdir().unwrap();
        let invalid = start_orchestrator_run_impl(
            runtime.clone(),
            Arc::clone(&state),
            snapshot,
            "task".to_string(),
            "plan_only".to_string(),
            None,
            Some(PlanArchiveOptions {
                directory: outside.path().to_string_lossy().into_owned(),
            }),
        )
        .unwrap_err();
        assert!(invalid.contains("inside"));
        assert!(state.active_run.lock().unwrap().is_none());
        assert_eq!(runtime.spawn_attempts.load(Ordering::SeqCst), 0);
        assert_eq!(runtime.event_attempts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn plan_archive_preview_uses_same_containment_validation_and_backend_version() {
        let project = tempfile::tempdir().unwrap();
        let archive = project.path().join(".plan");
        assert_eq!(
            preview_plan_archive_impl(
                &project.path().to_string_lossy(),
                &archive.to_string_lossy()
            )
            .unwrap()
            .next_file_name,
            "V0.24.0-r1.md"
        );
        std::fs::create_dir(&archive).unwrap();
        std::fs::write(archive.join("V0.24.0-r1.md"), "existing").unwrap();
        assert_eq!(
            preview_plan_archive_impl(
                &project.path().to_string_lossy(),
                &archive.to_string_lossy()
            )
            .unwrap()
            .next_file_name,
            "V0.24.0-r2.md"
        );
    }

    #[test]
    fn transient_overrides_apply_to_snapshot_only_and_preserve_gate_identity() {
        let original = snapshot_for_overrides();
        let updated = apply_transient_overrides(
            original.clone(),
            Some(RunTransientOverrides {
                gate_overrides: Some(std::collections::HashMap::from([("test".to_string(), false)])),
                limit_overrides: Some(PartialLoopIterationLimits {
                    max_fix_iterations: Some(4),
                    ..Default::default()
                }),
            }),
        )
        .unwrap();

        assert_eq!(original.validation_gates[0].enabled, true);
        assert_eq!(updated.validation_gates[0].enabled, false);
        assert_eq!(updated.validation_gates[0].executable, "cargo");
        assert_eq!(updated.validation_gates[0].args, vec!["test"]);
        assert_eq!(updated.validation_gates[0].working_dir.as_deref(), Some("C:/project"));
        assert!(updated.validation_gates[0].is_advanced_custom);
        assert_eq!(updated.iteration_limits.max_fix_iterations, 4);
        assert_eq!(original.iteration_limits.max_fix_iterations, 3);
    }

    #[test]
    fn transient_overrides_reject_unknown_gate_and_out_of_range_limits() {
        let unknown_gate = apply_transient_overrides(
            snapshot_for_overrides(),
            Some(RunTransientOverrides {
                gate_overrides: Some(std::collections::HashMap::from([("missing".to_string(), false)])),
                ..Default::default()
            }),
        );
        assert!(unknown_gate.unwrap_err().contains("unknown validation gate"));

        let invalid_limit = apply_transient_overrides(
            snapshot_for_overrides(),
            Some(RunTransientOverrides {
                limit_overrides: Some(PartialLoopIterationLimits {
                    max_plan_review_iterations: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        );
        assert!(invalid_limit.unwrap_err().contains("between 1 and 100"));
    }

    fn active_run(run_id: &str) -> ActiveRun {
        let (control_tx, _) = watch::channel(RunControlState::Running);
        let (clarification_tx, _) = mpsc::channel(1);
        let (blocking_resolution_tx, _) = mpsc::channel(1);
        let (human_gate_tx, _) = mpsc::channel(1);
        let (worker_reclaim_tx, _) = mpsc::channel(1);
        let current_step = Arc::new(Mutex::new(StepProgressEvent {
            run_id: run_id.to_string(),
            step: WorkflowState::BuildingContext,
            iteration_info: None,
            message: String::new(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        }));
        ActiveRun {
            run_id: run_id.to_string(),
            control_tx,
            cancel_token: CancellationToken::new(),
            clarification_tx,
            blocking_resolution_tx,
            human_gate_tx,
            worker_reclaim_tx,
            current_step,
        }
    }

    fn callbacks() -> (
        super::super::engine::EventCallback,
        super::super::engine::LogCallback,
    ) {
        (Arc::new(|_| {}), Arc::new(|_| {}))
    }

    #[test]
    fn test_confirm_worker_stopped_and_reclaim_state_enforcement() {
        let state = Arc::new(OrchestratorState::new());
        let (control_tx, _) = watch::channel(RunControlState::Running);
        let (clarification_tx, _) = mpsc::channel(1);
        let (blocking_resolution_tx, _) = mpsc::channel(1);
        let (human_gate_tx, _) = mpsc::channel(1);
        let (worker_reclaim_tx, mut worker_reclaim_rx) = mpsc::channel(4);
        let current_step = Arc::new(Mutex::new(StepProgressEvent {
            run_id: "test-reclaim-state-run".to_string(),
            step: WorkflowState::BuildingContext,
            iteration_info: None,
            message: String::new(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        }));
        let active = ActiveRun {
            run_id: "test-reclaim-state-run".to_string(),
            control_tx,
            cancel_token: CancellationToken::new(),
            clarification_tx,
            blocking_resolution_tx,
            human_gate_tx,
            worker_reclaim_tx,
            current_step: current_step.clone(),
        };
        *state.active_run.lock().unwrap() = Some(active);

        // 1. Reclaim during Implementation must be rejected
        *current_step.lock().unwrap() = StepProgressEvent {
            run_id: "test-reclaim-state-run".to_string(),
            step: WorkflowState::Implementation,
            iteration_info: None,
            message: "Awaiting Antigravity implementation...".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        };
        let res_impl = confirm_worker_stopped_and_reclaim_impl(&state, "test-reclaim-state-run");
        assert!(res_impl.is_err(), "Reclaim during Implementation must be rejected");
        assert!(res_impl.unwrap_err().contains("InvalidState"));
        assert!(worker_reclaim_rx.try_recv().is_err(), "No reclaim signal should be enqueued on rejection");

        // 2. Reclaim during Validation must be rejected
        *current_step.lock().unwrap() = StepProgressEvent {
            run_id: "test-reclaim-state-run".to_string(),
            step: WorkflowState::Validation,
            iteration_info: None,
            message: "Running validation gates...".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        };
        let res_val = confirm_worker_stopped_and_reclaim_impl(&state, "test-reclaim-state-run");
        assert!(res_val.is_err(), "Reclaim during Validation must be rejected");
        assert!(worker_reclaim_rx.try_recv().is_err(), "No reclaim signal should be enqueued on rejection");

        // 3. Reclaim during CodeReview must be rejected
        *current_step.lock().unwrap() = StepProgressEvent {
            run_id: "test-reclaim-state-run".to_string(),
            step: WorkflowState::CodeReview,
            iteration_info: None,
            message: "Reviewing code diff...".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        };
        let res_cr = confirm_worker_stopped_and_reclaim_impl(&state, "test-reclaim-state-run");
        assert!(res_cr.is_err(), "Reclaim during CodeReview must be rejected");
        assert!(worker_reclaim_rx.try_recv().is_err(), "No reclaim signal should be enqueued on rejection");

        // 4. Reclaim during HumanGate must be rejected
        *current_step.lock().unwrap() = StepProgressEvent {
            run_id: "test-reclaim-state-run".to_string(),
            step: WorkflowState::HumanGate,
            iteration_info: None,
            message: "Awaiting human operator approval...".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        };
        let res_hg = confirm_worker_stopped_and_reclaim_impl(&state, "test-reclaim-state-run");
        assert!(res_hg.is_err(), "Reclaim during HumanGate must be rejected");
        assert!(worker_reclaim_rx.try_recv().is_err(), "No reclaim signal should be enqueued on rejection");

        // 5. Reclaim during unrelated WaitingForUser (e.g. Code review limit reached) must be rejected
        *current_step.lock().unwrap() = StepProgressEvent {
            run_id: "test-reclaim-state-run".to_string(),
            step: WorkflowState::WaitingForUser,
            iteration_info: None,
            message: "Code review limit reached without approval.".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        };
        let res_unrelated = confirm_worker_stopped_and_reclaim_impl(&state, "test-reclaim-state-run");
        assert!(res_unrelated.is_err(), "Reclaim during unrelated WaitingForUser reason must be rejected");
        assert!(worker_reclaim_rx.try_recv().is_err(), "No reclaim signal should be enqueued on rejection");

        // 6. Reclaim when in worker-disconnect WaitingForUser MUST SUCCEED and enqueue signal
        *current_step.lock().unwrap() = StepProgressEvent {
            run_id: "test-reclaim-state-run".to_string(),
            step: WorkflowState::WaitingForUser,
            iteration_info: None,
            message: "Antigravity worker disconnected or lease timed out without progress. Waiting for human confirmation to reclaim.".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
        };
        let res_valid = confirm_worker_stopped_and_reclaim_impl(&state, "test-reclaim-state-run");
        assert!(res_valid.is_ok(), "Reclaim during worker-disconnect WaitingForUser must succeed: {:?}", res_valid);
        assert!(worker_reclaim_rx.try_recv().is_ok(), "Reclaim signal must be enqueued on success");
    }

    #[tokio::test]
    async fn panicking_workflow_clears_active_run_and_allows_next_start() {
        let state = Arc::new(OrchestratorState::new());
        let active = active_run("panic-run");
        let cancel_token = active.cancel_token.clone();
        *state.active_run.lock().unwrap() = Some(active);
        let (on_event, on_log) = callbacks();

        supervise_run(
            async {
                panic!("simulated workflow panic");
                #[allow(unreachable_code)]
                Ok(WorkflowState::Complete)
            },
            "panic-run".into(),
            Arc::clone(&state),
            on_event,
            on_log,
        )
        .await;

        assert!(cancel_token.is_cancelled());
        assert!(state.active_run.lock().unwrap().is_none());
        *state.active_run.lock().unwrap() = Some(active_run("next-run"));
        assert_eq!(
            state.active_run.lock().unwrap().as_ref().unwrap().run_id,
            "next-run"
        );
    }

    #[tokio::test]
    async fn stale_workflow_cleanup_does_not_clear_a_newer_run() {
        let state = Arc::new(OrchestratorState::new());
        *state.active_run.lock().unwrap() = Some(active_run("old-run"));
        let (on_event, on_log) = callbacks();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let task_state = Arc::clone(&state);
        let task = tokio::spawn(async move {
            supervise_run(
                async move {
                    let _ = release_rx.await;
                    Ok(WorkflowState::Complete)
                },
                "old-run".into(),
                task_state,
                on_event,
                on_log,
            )
            .await;
        });

        *state.active_run.lock().unwrap() = Some(active_run("new-run"));
        let _ = release_tx.send(());
        task.await.unwrap();
        assert_eq!(
            state.active_run.lock().unwrap().as_ref().unwrap().run_id,
            "new-run"
        );
    }
}

pub fn authorize_custom_validation_gate_impl(
    state: &OrchestratorState,
    gate_id: String,
    executable: String,
    args: Vec<String>,
    working_dir: Option<String>,
    project_root: String,
) -> Result<AuthorizedCustomGate, String> {
    let root_path = PathBuf::from(&project_root);
    let canonical_root = root_path
        .canonicalize()
        .map_err(|e| format!("Invalid project root path '{}': {}", project_root, e))?;

    let target_workdir = if let Some(ref rel) = working_dir {
        if rel.trim().is_empty() {
            canonical_root.clone()
        } else {
            canonical_root.join(rel)
        }
    } else {
        canonical_root.clone()
    };

    let canonical_workdir = target_workdir.canonicalize().map_err(|e| {
        format!(
            "Working directory '{}' does not exist or is inaccessible: {}",
            target_workdir.display(),
            e
        )
    })?;

    if !canonical_workdir.starts_with(&canonical_root) {
        return Err(format!(
            "Security violation: working directory '{}' escapes project root '{}'",
            canonical_workdir.display(),
            canonical_root.display()
        ));
    }

    let can_workdir_str = canonical_workdir.to_string_lossy().to_string();
    let command_hash = compute_gate_command_hash(&gate_id, &executable, &args, &can_workdir_str);

    let auth_record = AuthorizedCustomGate {
        gate_id: gate_id.clone(),
        executable,
        args,
        canonical_working_dir: can_workdir_str,
        command_hash,
    };

    let mut auth_lock = state
        .authorized_custom_gates
        .lock()
        .map_err(|e| e.to_string())?;
    // Replace any existing authorization for the same gate_id
    auth_lock.retain(|g| g.gate_id != gate_id);
    auth_lock.push(auth_record.clone());

    Ok(auth_record)
}
