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

pub struct OrchestratorState {
    pub active_run: Mutex<Option<ActiveRun>>,
    /// Serializes recovery preflight/resolution with the active-run reservation.
    pub recovery_gate: Mutex<()>,
    pub authorized_custom_gates: Mutex<Vec<AuthorizedCustomGate>>,
    pub journal_manager: Option<Arc<super::recovery::JournalManager>>,
}

impl Default for OrchestratorState {
    fn default() -> Self {
        Self::new()
    }
}

/// Execution boundary used by the production run starter and its side-effect-free tests.
pub trait RunStartRuntime: Clone + Send + Sync + 'static {
    fn emit_step(&self, event: StepProgressEvent);
    fn emit_log(&self, event: super::types::RunLogEvent);
    fn spawn(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>);

    /// Supplies the engine used by the production workflow path. Tests may
    /// provide deterministic scripted adapters while retaining the real
    /// command, callback, journal, and supervisor wiring.
    fn create_engine(&self) -> OrchestratorEngine {
        OrchestratorEngine::new()
    }
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
            recovery_gate: Mutex::new(()),
            authorized_custom_gates: Mutex::new(Vec::new()),
            journal_manager: None,
        }
    }

    pub fn with_journal_manager(journal_manager: Arc<super::recovery::JournalManager>) -> Self {
        Self {
            active_run: Mutex::new(None),
            recovery_gate: Mutex::new(()),
            authorized_custom_gates: Mutex::new(Vec::new()),
            journal_manager: Some(journal_manager),
        }
    }
}

fn make_run_event_callback<R: RunStartRuntime>(
    runtime: R,
    step_tracker: Arc<Mutex<StepProgressEvent>>,
    journal_tracker: Arc<Mutex<super::recovery::RunJournal>>,
    journal_manager: Option<Arc<super::recovery::JournalManager>>,
    checkpoint_store: Option<Arc<super::checkpoint::CheckpointStore>>,
    cancel_token: CancellationToken,
) -> super::engine::EventCallback {
    Arc::new(move |evt: StepProgressEvent| -> Result<(), String> {
        let mut journal_guard = journal_tracker
            .lock()
            .map_err(|e| format!("Journal mutex poisoned: {e}"))?;

        let mut candidate = journal_guard.clone();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        candidate.revision += 1;
        candidate.updated_at_unix = now;
        candidate.current_state = evt.step;
        if let Some(plan) = evt.plan_text.as_deref().filter(|plan| !plan.trim().is_empty()) {
            // Keep only the latest non-empty plan sent through the production
            // event path so recovery/adopt can reconstruct the approved plan
            // without relying on a UI-only cache.
            candidate.approved_plan = Some(plan.to_string());
        }

        // A checkpoint pointer is published only after its payload and manifest
        // have been durably written and verified. Capture a completed boundary
        // first, then the entry baseline for a newly entered mutating stage.
        if let Some(store) = &checkpoint_store {
            let capture = |stage: WorkflowState, kind: super::recovery::CheckpointKind| {
                let stage_input = serde_json::json!({
                    "message": evt.message,
                    "planText": evt.plan_text,
                    "reviewResult": evt.review_result,
                    "validationSummary": evt.validation_summary,
                    "iterationInfo": evt.iteration_info,
                }).to_string();
                store.capture(
                    &candidate.run_id,
                    stage,
                    kind,
                    Path::new(&candidate.canonical_project_path),
                    candidate.task_prompt.clone(),
                    Some(stage_input),
                )
            };
            if let Some(completed) = evt.completed_stage {
                if recovery_checkpoint_stage(completed) {
                    match capture(completed, super::recovery::CheckpointKind::StageCheckpoint) {
                        Ok((manifest, reference)) => {
                            candidate.checkpoint_manifest_ref = Some(reference);
                            candidate.checkpoint_digest = Some(manifest.manifest_digest);
                        }
                        Err(error) => {
                            cancel_token.cancel();
                            return Err(format!("[Recovery] Stage checkpoint failed: {error}"));
                        }
                    }
                }
            }
            if recovery_entry_stage(evt.step) && evt.completed_stage != Some(evt.step) {
                match capture(evt.step, super::recovery::CheckpointKind::EntryBaseline) {
                    Ok((manifest, reference)) => {
                        candidate.checkpoint_manifest_ref = Some(reference);
                        candidate.checkpoint_digest = Some(manifest.manifest_digest);
                    }
                    Err(error) => {
                        cancel_token.cancel();
                        return Err(format!("[Recovery] Stage entry baseline failed: {error}"));
                    }
                }
            }
        }

        match evt.step {
            WorkflowState::Complete => {
                candidate.status = super::recovery::RunRecoveryStatus::Complete;
                candidate.last_successful_state = Some(WorkflowState::Complete);
            }
            WorkflowState::Failed => candidate.status = super::recovery::RunRecoveryStatus::Failed,
            WorkflowState::Cancelled => {
                candidate.status = super::recovery::RunRecoveryStatus::Cancelled
            }
            _ => {}
        }
        if let Some(completed) = evt.completed_stage {
            candidate.last_successful_state = Some(completed);
        }
        if let Some(ref info) = evt.iteration_info {
            candidate.stage_entry_info = Some(info.clone());
        }
        if let Some(count) = evt.plan_review_count {
            candidate.iteration_counters.plan_review_count = count;
        }
        if let Some(count) = evt.fix_count {
            candidate.iteration_counters.fix_count = count;
        }
        if let Some(count) = evt.code_review_count {
            candidate.iteration_counters.code_review_count = count;
        }
        if let Some(dispatches) = evt.antigravity_dispatches {
            candidate.iteration_counters.antigravity_dispatches = Some(dispatches);
        }

        if let Some(ref manager) = journal_manager {
            if let Err(error) = manager.write_journal(&candidate) {
                cancel_token.cancel();
                runtime.emit_log(super::types::RunLogEvent {
                    run_id: candidate.run_id.clone(),
                    message: format!("[Recovery] Journal persistence failed: {error}"),
                });
                return Err(format!("[Recovery] Journal persistence failed: {error}"));
            }
        }

        *journal_guard = candidate;
        drop(journal_guard);
        *step_tracker
            .lock()
            .map_err(|e| format!("Step tracker mutex poisoned: {e}"))? = evt.clone();
        runtime.emit_step(evt);
        Ok(())
    })
}

/// Builds the run engine with the durable Direct MCP audit wiring attached.
///
/// Every production run-start path must use this helper: a run whose Planner or
/// PlanReviewer uses the MCP adapter fails closed when the audit callback is
/// absent, so a path that builds an unwired engine silently disables the adapter.
fn build_run_engine<R: RunStartRuntime>(
    runtime: &R,
    journal_state: Arc<Mutex<super::recovery::RunJournal>>,
    journal_manager: Option<Arc<super::recovery::JournalManager>>,
) -> OrchestratorEngine {
    #[cfg(test)]
    AUDITED_ENGINE_BUILDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    runtime
        .create_engine()
        .with_direct_mcp_audit_callback(make_direct_mcp_audit_callback(
            journal_state,
            journal_manager,
        ))
}

/// Counts builds that went through `build_run_engine`, so tests can prove every
/// production run-start path takes the audited route.
#[cfg(test)]
static AUDITED_ENGINE_BUILDS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
fn audited_engine_builds() -> usize {
    AUDITED_ENGINE_BUILDS.load(std::sync::atomic::Ordering::SeqCst)
}

fn make_direct_mcp_audit_callback(
    journal_tracker: Arc<Mutex<super::recovery::RunJournal>>,
    journal_manager: Option<Arc<super::recovery::JournalManager>>,
) -> super::engine::DirectMcpAuditCallback {
    Arc::new(move |audit| {
        let manager = journal_manager.as_ref().ok_or_else(|| {
            "[DirectMcp] Cannot record invocation because the run journal is unavailable.".to_string()
        })?;
        let mut guard = journal_tracker
            .lock()
            .map_err(|e| format!("[DirectMcp] Journal mutex poisoned: {e}"))?;
        let mut candidate = guard.clone();
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            "[DirectMcp] Run journal revision overflow while recording invocation.".to_string()
        })?;
        candidate.updated_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("[DirectMcp] System clock error: {e}"))?
            .as_secs();
        candidate.direct_mcp_invocations.push(audit);
        manager
            .write_journal(&candidate)
            .map_err(|e| format!("[DirectMcp] Invocation audit persistence failed: {e}"))?;
        *guard = candidate;
        Ok(())
    })
}

fn make_dispatch_persistence_callback(
    journal_tracker: Arc<Mutex<super::recovery::RunJournal>>,
    journal_manager: Option<Arc<super::recovery::JournalManager>>,
    cancel_token: CancellationToken,
) -> super::mailbox::DispatchPersistenceCallback {
    Arc::new(move |dispatch: super::mailbox::MailboxDispatchSnapshot| {
        let mut journal = journal_tracker.lock().map_err(|e| e.to_string())?;
        let mut candidate = journal.clone();
        candidate.revision = candidate.revision.checked_add(1)
            .ok_or_else(|| "Run journal revision overflow while recording Worker claim".to_string())?;
        candidate.updated_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?.as_secs();
        candidate.iteration_counters.antigravity_dispatches = Some(dispatch.total_dispatches);
        candidate.iteration_counters.antigravity_task_dispatches = dispatch.task_dispatches;
        candidate.iteration_counters.mailbox_epoch = Some(dispatch.epoch);
        if let Some(manager) = &journal_manager {
            if let Err(error) = manager.write_journal(&candidate) {
                cancel_token.cancel();
                return Err(format!("Could not durably record Worker claim: {error}"));
            }
        }
        *journal = candidate;
        Ok(())
    })
}

fn recovery_entry_stage(stage: WorkflowState) -> bool {
    matches!(stage, WorkflowState::PlanIntegration | WorkflowState::PlanRevision | WorkflowState::Implementation | WorkflowState::Fix | WorkflowState::Validation)
}

fn recovery_checkpoint_stage(stage: WorkflowState) -> bool {
    matches!(stage, WorkflowState::PlanDraft | WorkflowState::PlanIntegration | WorkflowState::PlanReview | WorkflowState::PlanRevision | WorkflowState::Implementation | WorkflowState::Validation | WorkflowState::Fix | WorkflowState::CodeReview | WorkflowState::HumanGate)
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
        antigravity_dispatches: None,
        antigravity_dispatch_limit: None,
        budget_scope: None,
        waiting_reason: None,
        ..Default::default()
            }));

    let initial_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let initial_journal = super::recovery::RunJournal {
        schema_version: super::recovery::JOURNAL_SCHEMA_VERSION,
        run_id: run_id.clone(),
        workflow_type: workflow_type.to_string(),
        canonical_project_path: snapshot.project_path.clone(),
        task_prompt: Some(task_prompt.clone()),
        approved_plan: None,
        snapshot: snapshot.clone(),
        current_state: WorkflowState::BuildingContext,
        last_successful_state: None,
        stage_entry_info: None,
        iteration_counters: super::recovery::RunIterationCounters::default(),
        direct_mcp_invocations: Vec::new(),
        plan_convergence_audit: Vec::new(),
        plan_convergence_confirmation_intent: None,
        revision: 1,
        resume_generation: 0,
        checkpoint_manifest_ref: None,
        checkpoint_digest: None,
        last_shelve_backup_id: None,
        last_shelve_backup_digest: None,
        status: super::recovery::RunRecoveryStatus::Active,
        created_at_unix: initial_now,
        updated_at_unix: initial_now,
    };

    // Atomically check that no active run exists and reserve the active run slot FIRST
    {
        let _recovery_gate = state.recovery_gate.lock().map_err(|e| e.to_string())?;
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

    if let Some(ref jm) = state.journal_manager {
        if let Err(e) = jm.write_journal(&initial_journal) {
            // Release active run reservation on persistence failure
            if let Ok(mut active_lock) = state.active_run.lock() {
                if active_lock
                    .as_ref()
                    .is_some_and(|active| active.run_id == run_id)
                {
                    *active_lock = None;
                }
            }
            return Err(format!("Failed to create initial run journal: {e}"));
        }
    }

    let journal_state = Arc::new(Mutex::new(initial_journal));
    let jm_opt = state.journal_manager.clone();

    let spawned_run_id = run_id.clone();
    let state_clone = Arc::clone(&state);

    let on_event = make_run_event_callback(
        runtime.clone(),
        current_step.clone(),
        journal_state.clone(),
        jm_opt.clone(),
        jm_opt.as_ref().map(|jm| Arc::new(super::checkpoint::CheckpointStore::new(jm.runs_dir().to_path_buf()))),
        cancel_token.clone(),
    );

    let log_runtime = runtime.clone();
    let on_log: super::engine::LogCallback =
        Arc::new(move |log_event: super::types::RunLogEvent| {
            log_runtime.emit_log(log_event);
        });

    let run_id_for_task = run_id.clone();
    let supervisor_run_id = run_id_for_task.clone();
    let engine = build_run_engine(&runtime, journal_state.clone(), jm_opt.clone());
    let workflow_cancel_token = cancel_token.clone();
    let workflow_on_event = on_event.clone();
    let workflow_on_log = on_log.clone();
    let supervisor_journal_state = Some(journal_state.clone());
    let dispatch_persistence = Some(make_dispatch_persistence_callback(
        journal_state.clone(), jm_opt.clone(), workflow_cancel_token.clone(),
    ));

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
                    dispatch_persistence,
                )
                .await
        };
        supervise_run(workflow, supervisor_run_id, state_clone, supervisor_journal_state, on_event, on_log).await;
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
    journal_state: Option<Arc<Mutex<super::recovery::RunJournal>>>,
    on_event: super::engine::EventCallback,
    on_log: super::engine::LogCallback,
) where
    F: Future<Output = Result<WorkflowState, String>> + Send + 'static,
{
    // The supervisor observes the workflow JoinHandle so panic unwinding is
    // converted into a failure result and cleanup always follows.
    let result = tokio::spawn(workflow).await;
    let mut settled_state = None;
    let (error, panicked) = match result {
        Ok(Ok(state)) => {
            settled_state = Some(state);
            (None, false)
        }
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
    if let Some(ref error) = error {
        on_log(super::types::RunLogEvent {
            run_id: run_id.clone(),
            message: format!("[Engine] Run {run_id} ended with error: {error}"),
        });
        let _ = on_event(StepProgressEvent {
            run_id: run_id.clone(),
            step: WorkflowState::Failed,
            iteration_info: None,
            message: format!("Workflow execution failed: {error}"),
            review_result: None,
            validation_summary: None,
            plan_text: None,
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
        });
    }

    if let (Some(ref jm), Some(ref j_state)) = (&state.journal_manager, &journal_state) {
        if let Ok(mut j_lock) = j_state.lock() {
            // A waiting outcome is already durably recorded by the run-event
            // callback. Writing again here would only advance the journal past
            // the revision the operator was just shown, so the revision a
            // confirmation must present would be stale on arrival.
            let awaiting_operator = settled_state == Some(WorkflowState::WaitingForUser)
                && error.is_none()
                && !panicked;
            if j_lock.status == super::recovery::RunRecoveryStatus::Active && !awaiting_operator {
                j_lock.revision += 1;
                j_lock.updated_at_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if panicked || error.is_some() {
                    j_lock.status = super::recovery::RunRecoveryStatus::Failed;
                    j_lock.current_state = WorkflowState::Failed;
                }
                if let Err(err) = jm.write_journal(&j_lock) {
                    on_log(super::types::RunLogEvent {
                        run_id: run_id.clone(),
                        message: format!("[Recovery] Failed to persist terminal journal state: {err}"),
                    });
                }
            }
        }
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

// ---------------------------------------------------------------------------
// Plan convergence — production opt-in entry and human confirmation
// ---------------------------------------------------------------------------

/// Drives one convergence run through the production driver.
pub async fn run_plan_convergence_impl<E: super::convergence::ConvergenceAdapterExecutor>(
    driver: &super::convergence::ConvergenceDriver,
    executor: &E,
    audit_callback: super::convergence::AuditPersistenceFn,
    progress_callback: Option<super::convergence::ConvergenceProgressCallback>,
    cancel_token: Option<&CancellationToken>,
) -> super::convergence::ConvergenceOutcome {
    driver
        .run(executor, audit_callback, progress_callback, cancel_token)
        .await
}

/// Production opt-in entry for the Planner–Reviewer convergence loop.
///
/// Everything the loop depends on is validated and frozen before the active run
/// is reserved: distinct Planner/PlanReviewer assignments, a resolved Plan
/// Workspace context, and a positive review budget. When the feature is not
/// opted in, this path refuses before touching any shared run state.
pub fn start_plan_convergence_run_impl<R: RunStartRuntime>(
    runtime: R,
    state: Arc<OrchestratorState>,
    mut snapshot: RunConfigurationSnapshot,
    task_prompt: String,
    convergence_config: super::convergence::PlanConvergenceConfig,
    workspace_config: Option<super::plan_workspace::PlanWorkspaceConfig>,
) -> Result<StartRunResponse, String> {
    // Convergence may never dispatch with an in-memory-only audit trail.
    let manager = state.journal_manager.clone()
        .ok_or_else(|| "PC_PERSISTENCE_FAILED: journal storage unavailable".to_string())?;
    if !convergence_config.opt_in {
        return Err("Plan convergence is not opted in.".to_string());
    }

    let planner_profile = snapshot
        .assignments
        .get(&AgentRole::Planner)
        .ok_or_else(|| "Planner role is not assigned.".to_string())?
        .clone();

    let reviewer_profile = snapshot
        .assignments
        .get(&AgentRole::PlanReviewer)
        .ok_or_else(|| "PlanReviewer role is not assigned.".to_string())?
        .clone();

    if planner_profile.id.trim().is_empty()
        || reviewer_profile.id.trim().is_empty()
        || planner_profile.id == reviewer_profile.id
    {
        return Err(format!(
            "Planner ('{}') and PlanReviewer ('{}') must have distinct assigned profiles.",
            planner_profile.id, reviewer_profile.id
        ));
    }

    if snapshot.iteration_limits.max_plan_review_iterations == 0 {
        return Err("max_plan_review_iterations must be greater than zero.".to_string());
    }

    let plan_cfg = workspace_config.unwrap_or_else(|| snapshot.plan_workspace.clone());
    // The effective workspace configuration is frozen into the run snapshot so
    // the journal, the frozen context, and any later human confirmation all use
    // the exact same workspace definition.
    snapshot.plan_workspace = plan_cfg.clone();
    let frozen_snapshot = super::plan_workspace::capture_frozen_plan_snapshot(
        Path::new(&snapshot.project_path),
        &plan_cfg,
    )
    .map_err(|e| format!("Failed to capture frozen plan snapshot: {e}"))?;

    if frozen_snapshot.plan_context.resolver_status
        != super::plan_workspace::PlanResolverStatus::Resolved
    {
        return Err(format!(
            "Plan workspace context is not resolved: {:?}",
            frozen_snapshot.plan_context.unresolved_reason_code
        ));
    }

    let run_id = Uuid::new_v4().to_string();
    let (control_tx, _control_rx) = watch::channel(RunControlState::Running);
    let cancel_token = CancellationToken::new();
    let (clarification_tx, _clarification_rx) = mpsc::channel::<String>(16);
    let (blocking_tx, _blocking_rx) = mpsc::channel::<BlockingResolution>(16);
    let (human_gate_tx, _human_gate_rx) = mpsc::channel::<HumanGateDecision>(16);
    let (worker_reclaim_tx, _worker_reclaim_rx) = mpsc::channel::<()>(16);

    let current_step = Arc::new(Mutex::new(StepProgressEvent {
        run_id: run_id.clone(),
        step: WorkflowState::PlanDraft,
        iteration_info: None,
        message: "Starting plan convergence loop...".to_string(),
        review_result: None,
        validation_summary: None,
        plan_text: None,
        antigravity_dispatches: None,
        antigravity_dispatch_limit: None,
        budget_scope: None,
        waiting_reason: None,
        ..Default::default()
    }));

    let initial_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let initial_journal = super::recovery::RunJournal {
        schema_version: super::recovery::JOURNAL_SCHEMA_VERSION,
        run_id: run_id.clone(),
        workflow_type: "plan_convergence".to_string(),
        canonical_project_path: snapshot.project_path.clone(),
        task_prompt: Some(task_prompt.clone()),
        approved_plan: None,
        snapshot: snapshot.clone(),
        current_state: WorkflowState::PlanDraft,
        last_successful_state: None,
        stage_entry_info: None,
        iteration_counters: super::recovery::RunIterationCounters::default(),
        direct_mcp_invocations: Vec::new(),
        plan_convergence_audit: Vec::new(),
        plan_convergence_confirmation_intent: None,
        revision: 1,
        resume_generation: 0,
        checkpoint_manifest_ref: None,
        checkpoint_digest: None,
        last_shelve_backup_id: None,
        last_shelve_backup_digest: None,
        status: super::recovery::RunRecoveryStatus::Active,
        created_at_unix: initial_now,
        updated_at_unix: initial_now,
    };

    // Atomic reservation of active run
    {
        let _recovery_gate = state.recovery_gate.lock().map_err(|e| e.to_string())?;
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

    if let Some(ref jm) = state.journal_manager {
        if let Err(e) = jm.write_journal(&initial_journal) {
            if let Ok(mut active_lock) = state.active_run.lock() {
                if active_lock
                    .as_ref()
                    .is_some_and(|active| active.run_id == run_id)
                {
                    *active_lock = None;
                }
            }
            return Err(format!("Failed to create initial run journal: {e}"));
        }
    }

    let journal_state = Arc::new(Mutex::new(initial_journal));
    let jm_opt = state.journal_manager.clone();
    let runs_dir = manager.runs_dir().to_path_buf();

    let driver = super::convergence::ConvergenceDriver {
        runs_dir,
        run_id: run_id.clone(),
        project_path: PathBuf::from(&snapshot.project_path),
        task_prompt: task_prompt.clone(),
        planner_profile,
        reviewer_profile,
        plan_workspace_config: plan_cfg,
        frozen_plan_snapshot: frozen_snapshot,
        mcp_servers: snapshot.mcp_servers.clone(),
        max_plan_review_iterations: snapshot.iteration_limits.max_plan_review_iterations,
        convergence_config,
    };

    let spawned_run_id = run_id.clone();
    let state_clone = Arc::clone(&state);

    let on_event = make_run_event_callback(
        runtime.clone(),
        current_step.clone(),
        journal_state.clone(),
        jm_opt.clone(),
        // Planning convergence is not a workspace-execution checkpoint workflow.
        None,
        cancel_token.clone(),
    );

    let log_runtime = runtime.clone();
    let on_log: super::engine::LogCallback =
        Arc::new(move |log_event: super::types::RunLogEvent| {
            log_runtime.emit_log(log_event);
        });

    let j_state_audit = journal_state.clone();
    let jm_audit = jm_opt.clone();
    let audit_callback: super::convergence::AuditPersistenceFn = Arc::new(move |entry| {
        let mut guard = j_state_audit
            .lock()
            .map_err(|e| format!("Journal mutex poisoned: {e}"))?;
        let mut candidate = guard.clone();
        candidate.revision += 1;
        candidate.updated_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        candidate.plan_convergence_audit.push(entry);
        candidate.iteration_counters.plan_review_count =
            super::recovery::replay_convergence_review_count(&candidate)?;
        if let Some(ref jm) = jm_audit {
            jm.write_journal(&candidate)
                .map_err(|e| format!("Failed to persist convergence audit entry: {e}"))?;
        }
        *guard = candidate;
        Ok(())
    });

    let executor = super::convergence::EngineAdapterExecutor {
        engine: build_run_engine(&runtime, journal_state.clone(), jm_opt.clone()),
    };

    // Live convergence progress flows through the same durable run-event
    // callback as every other stage. The message is a localization key and the
    // iteration info carries the typed counters in a machine-readable form.
    let progress_on_event = on_event.clone();
    let progress_run_id = run_id.clone();
    let progress_callback: super::convergence::ConvergenceProgressCallback =
        Arc::new(move |event: super::convergence::ConvergenceProgressEvent| {
            use super::convergence::ConvergenceProgressEvent;
            let (phase, step, iteration_info) = match event {
                ConvergenceProgressEvent::PlannerDispatch { sequence } => (
                    "plannerDispatch",
                    WorkflowState::PlanDraft,
                    format!("phase:planner_dispatch,sequence:{sequence}"),
                ),
                ConvergenceProgressEvent::ReviewerReserve { sequence } => (
                    "reviewerReserve",
                    WorkflowState::PlanReview,
                    format!("phase:reviewer_reserve,sequence:{sequence}"),
                ),
                ConvergenceProgressEvent::ReviewerDispatch { sequence } => (
                    "reviewerDispatch",
                    WorkflowState::PlanReview,
                    format!("phase:reviewer_dispatch,sequence:{sequence}"),
                ),
                ConvergenceProgressEvent::Verdict {
                    sequence,
                    decision,
                    reviews_used,
                    reviews_limit,
                } => (
                    "verdict",
                    WorkflowState::PlanReview,
                    format!(
                        "phase:verdict,sequence:{sequence},decision:{decision},used:{reviews_used},limit:{reviews_limit}"
                    ),
                ),
            };
            progress_on_event(StepProgressEvent {
                run_id: progress_run_id.clone(),
                step,
                iteration_info: Some(iteration_info),
                message: format!("orchestrator.planConvergence.liveProgress.{phase}"),
                review_result: None,
                validation_summary: None,
                plan_text: None,
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                ..Default::default()
            })
        });

    let run_id_task = run_id.clone();
    let supervisor_run_id = run_id.clone();
    let supervisor_journal_state = Some(journal_state.clone());
    let convergence_cancel_token = cancel_token.clone();
    let workflow_on_event = on_event.clone();
    let workflow_journal = journal_state.clone();
    let workflow_runs_dir = manager.runs_dir().to_path_buf();

    runtime.spawn(Box::pin(async move {
        let workflow = async move {
            let outcome = driver
                .run(
                    &executor,
                    audit_callback,
                    Some(progress_callback),
                    Some(&convergence_cancel_token),
                )
                .await;

            match outcome {
                super::convergence::ConvergenceOutcome::Accepted {
                    candidate_id: _,
                    target_plan_id,
                    accepted_file_digest,
                    reviews_used,
                } => {
                    workflow_on_event(StepProgressEvent {
                        run_id: run_id_task.clone(),
                        step: WorkflowState::Complete,
                        iteration_info: Some(format!("Reviews used: {reviews_used}")),
                        message: format!(
                            "Plan convergence accepted. Plan {} updated (digest: {}).",
                            target_plan_id, accepted_file_digest
                        ),
                        review_result: None,
                        validation_summary: None,
                        plan_text: None,
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        ..Default::default()
                    })?;
                    Ok(WorkflowState::Complete)
                }
                super::convergence::ConvergenceOutcome::WaitingForUser {
                    reason,
                    diagnostic_code,
                    candidate_artifact_ref,
                    latest_verdict_artifact_ref: _,
                } => {
                    let preview = if let Some(reference) = candidate_artifact_ref {
                        let (digest, journal_revision) = {
                            let guard = workflow_journal.lock().map_err(|e| e.to_string())?;
                            let digest = guard
                                .plan_convergence_audit.iter()
                                .find(|entry| entry.artifact_ref.as_ref() == Some(&reference))
                                .and_then(|entry| entry.artifact_digest.clone())
                                .ok_or("PC_INVALID_REVIEW_HISTORY: missing candidate digest")?;
                            // The run-event callback increments the journal revision
                            // exactly once per event, so the waiting record below is
                            // durably persisted at `revision + 1`. That value is what a
                            // confirmation action must match to be current.
                            (digest, guard.revision + 1)
                        };
                        let candidate = super::convergence::load_and_verify_candidate(
                            &workflow_runs_dir, &run_id_task, &reference, &digest,
                        ).map_err(|error| error.to_string())?;
                        let (title, body, intent, proposed_revision) = match &candidate.operation {
                            super::convergence::PlannerOperationProposal::NewPrimaryPlan {
                                proposed_revision, title, initial_content,
                            } => (
                                title.clone(), initial_content.clone(),
                                "new_primary".to_string(), Some(*proposed_revision),
                            ),
                            super::convergence::PlannerOperationProposal::AppendSection {
                                section_title, section_content, ..
                            } => (
                                section_title.clone(), section_content.clone(),
                                "append_section".to_string(), None,
                            ),
                        };
                        let payload = super::convergence::PlanConvergenceWaitingCandidate {
                            run_id: run_id_task.clone(),
                            revision: journal_revision,
                            candidate_id: candidate.candidate_id.clone(),
                            sequence: candidate.sequence,
                            title,
                            plan_text: body,
                            intent,
                            proposed_revision,
                        };
                        Some(serde_json::to_string(&payload).map_err(|e| e.to_string())?)
                    } else { None };
                    workflow_on_event(StepProgressEvent {
                        run_id: run_id_task.clone(),
                        step: WorkflowState::WaitingForUser,
                        iteration_info: diagnostic_code.clone(),
                        message: reason.to_string(),
                        review_result: None,
                        validation_summary: None,
                        plan_text: preview,
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: Some(reason.to_string()),
                        ..Default::default()
                    })?;
                    Ok(WorkflowState::WaitingForUser)
                }
                super::convergence::ConvergenceOutcome::Cancelled => {
                    workflow_on_event(StepProgressEvent {
                        run_id: run_id_task.clone(),
                        step: WorkflowState::Cancelled,
                        iteration_info: None,
                        message: "Plan convergence run was cancelled.".to_string(),
                        review_result: None,
                        validation_summary: None,
                        plan_text: None,
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        ..Default::default()
                    })?;
                    Ok(WorkflowState::Cancelled)
                }
                super::convergence::ConvergenceOutcome::Failed {
                    stable_error_code,
                    safe_details,
                } => {
                    let msg = safe_details
                        .unwrap_or_else(|| format!("Plan convergence failed with code {stable_error_code}"));
                    Err(msg)
                }
            }
        };

        supervise_run(
            workflow,
            supervisor_run_id,
            state_clone,
            supervisor_journal_state,
            on_event,
            on_log,
        )
        .await;
    }));

    Ok(StartRunResponse {
        run_id: spawned_run_id,
    })
}

/// Reads a plan file's SHA-256 when it exists, distinguishing "absent" from
/// "unreadable" so confirmation never mistakes a read failure for a missing file.
fn file_digest_if_present(
    path: &Path,
) -> Result<Option<String>, super::convergence::PlanConvergenceError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(super::plan_workspace::sha256_bytes(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(super::convergence::PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to read plan '{}': {error}", path.display()),
        )),
    }
}

/// Rejects a confirmation target that is not a plain project-relative path or
/// that does not resolve to the canonical target of the reviewed candidate.
///
/// A recorded intent is untrusted recovery input, so traversal components,
/// absolute or prefixed paths, symlinked targets, non-regular targets, and any
/// path that aliases a different file are refused before the target is read.
fn resolve_confirmation_target(
    project_root: &Path,
    expected_rel: &str,
    target_rel: &str,
) -> Result<PathBuf, super::convergence::PlanConvergenceError> {
    use std::path::Component;

    let rejected = |detail: &str| {
        super::convergence::PlanConvergenceError::new(
            "PC_CONFIRMATION_TARGET_REJECTED",
            format!("Confirmation target '{target_rel}' {detail}."),
        )
    };
    let mismatch = |detail: &str| {
        super::convergence::PlanConvergenceError::new(
            "PC_CONFIRMATION_INTENT_MISMATCH",
            format!("Confirmation target '{target_rel}' {detail}."),
        )
    };

    let raw = Path::new(target_rel);
    if target_rel.trim().is_empty() || target_rel.trim() != target_rel {
        return Err(rejected("is empty or padded"));
    }
    if raw.is_absolute() {
        return Err(rejected("is an absolute path"));
    }
    if raw
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(rejected("contains a traversal, root, or prefix component"));
    }
    // Aliasing: the recorded target must be exactly the canonical path the
    // Plan Workspace derives for the reviewed revision, not an equivalent path.
    if target_rel != expected_rel {
        return Err(mismatch("is not the canonical target of the reviewed candidate"));
    }

    let canonical_root = super::plan_workspace::canonicalize_project_root(project_root)
        .map_err(|error| {
            super::convergence::PlanConvergenceError::new(&error.code, error.message)
        })?;
    // Containment: rejects an escape through a parent component or a reparse
    // point anywhere on the resolved path.
    super::plan_workspace::resolve_contained_path(&canonical_root, target_rel)
        .map_err(|error| {
            super::convergence::PlanConvergenceError::new(
                "PC_CONFIRMATION_TARGET_REJECTED",
                format!(
                    "Confirmation target '{target_rel}' escapes the project root: {}",
                    error.message
                ),
            )
        })?;

    let target_path = canonical_root.join(raw);
    if let Some(parent) = target_path.parent() {
        if let Ok(super::plan_workspace::PathProbe::Exists {
            is_dir,
            is_symlink,
            ..
        }) = super::plan_workspace::probe_path_symlink(parent)
        {
            if is_symlink || !is_dir {
                return Err(rejected("has a symlinked or non-directory parent"));
            }
        }
    }
    match super::plan_workspace::probe_path_symlink(&target_path) {
        Ok(super::plan_workspace::PathProbe::Missing) => {}
        Ok(super::plan_workspace::PathProbe::Exists { is_symlink: true, .. }) => {
            return Err(rejected("is a symlink"));
        }
        Ok(super::plan_workspace::PathProbe::Exists { is_file: false, .. }) => {
            return Err(rejected("is not a regular file"));
        }
        Ok(super::plan_workspace::PathProbe::Exists { .. }) => {}
        Err(error) => {
            return Err(super::convergence::PlanConvergenceError::new(
                &error.code,
                error.message,
            ))
        }
    }

    Ok(target_path)
}

/// The durable intent and the state of its publication target.
struct ReconciledIntent {
    intent: super::convergence::PlanConvergenceConfirmationIntent,
    /// Absolute path of the validated target.
    target_path: PathBuf,
    /// Digest of the bytes already published at the target, if any.
    published_digest: Option<String>,
}

/// Rebuilds every transaction binding from the journal and the verified
/// candidate artifact, then returns them for comparison with a durable intent.
struct ConfirmedTransaction {
    run_id: String,
    candidate_id: String,
    candidate_sequence: u32,
    candidate_artifact_ref: String,
    candidate_artifact_digest: String,
    operation_kind: String,
    operation_payload_digest: String,
    base_plan_digest: Option<String>,
    plan_context_digest: String,
    target_revision: u64,
    target_plan_id: String,
    target_path: String,
    content_digest: String,
    title: String,
    initial_content: String,
}

/// Explicit human decision on a `NewPrimaryPlan` candidate that reached the
/// `NEW_PRIMARY_PLAN_CONFIRMATION` human gate.
///
/// Bound to `(run_id, expected_revision, candidate_id)` so a stale or mismatched
/// UI action cannot act on a different candidate or run. The durable confirmation
/// intent is persisted before any filesystem publication and is rebound to the
/// verified candidate and a freshly resolved, project-contained target before
/// the target is read. Publication and its acceptance record are reconciled
/// forward across retries; rejection is refused once bytes are on disk because
/// convergence never deletes a published plan.
pub fn confirm_converged_new_plan_impl<R: RunStartRuntime>(
    runtime: R,
    state: Arc<OrchestratorState>,
    run_id: String,
    expected_revision: u64,
    candidate_id: String,
    action: String,
) -> Result<
    super::convergence::PlanConvergenceCommandResult,
    super::convergence::PlanConvergenceError,
> {
    use super::convergence::{
        ConvergenceWaitingReason, PlanConvergenceCommandResult, PlanConvergenceError,
        PlannerOperationProposal,
    };

    let pc_error = |code: &str, message: String| PlanConvergenceError::new(code, message);

    if action != "confirm" && action != "reject" {
        return Err(pc_error(
            "PC_INVALID_ACTION",
            format!("Unsupported convergence action '{action}'."),
        ));
    }

    let manager = state.journal_manager.clone().ok_or_else(|| {
        pc_error(
            "PC_PERSISTENCE_FAILED",
            "Journal storage is unavailable.".to_string(),
        )
    })?;

    // Serializes with run reservation/recovery so a concurrent run start cannot
    // interleave with this read-modify-write of the run journal. The lock is held
    // through the guarded plan mutation, so no run can become active while a
    // confirmation is creating a plan.
    let _gate = state.recovery_gate.lock().map_err(|error| {
        pc_error(
            "PC_PERSISTENCE_FAILED",
            format!("Recovery gate mutex poisoned: {error}"),
        )
    })?;

    {
        let active_lock = state.active_run.lock().map_err(|error| {
            pc_error(
                "PC_PERSISTENCE_FAILED",
                format!("Active run mutex poisoned: {error}"),
            )
        })?;
        if let Some(active) = active_lock.as_ref() {
            return Err(pc_error(
                "PC_CONFLICT_RUN_ACTIVE",
                format!(
                    "Run '{}' is active; run '{run_id}' cannot be confirmed while another run is in progress.",
                    active.run_id
                ),
            ));
        }
    }

    let mut journal = manager.read_journal(&run_id).map_err(|error| {
        pc_error(
            "PC_RUN_NOT_FOUND",
            format!("Run '{run_id}' has no durable journal: {error}"),
        )
    })?;

    if journal.workflow_type != "plan_convergence" {
        return Err(pc_error(
            "PC_NOT_CONVERGENCE_RUN",
            format!("Run '{run_id}' is not a plan convergence run."),
        ));
    }
    // A terminal run may only be replayed if the request is an exact replay
    // of an already durable terminal acceptance. All other requests against
    // a non-active run fail closed.
    if journal.status != super::recovery::RunRecoveryStatus::Active {
        if journal.status != super::recovery::RunRecoveryStatus::Complete || action != "confirm" {
            return Err(pc_error(
                "PC_RUN_ALREADY_RESOLVED",
                format!("Run '{run_id}' is already resolved and cannot be confirmed again."),
            ));
        }

        let intent = match &journal.plan_convergence_confirmation_intent {
            Some(intent) => intent,
            None => {
                return Err(pc_error(
                    "PC_RUN_ALREADY_RESOLVED",
                    format!("Run '{run_id}' has no durable confirmation intent."),
                ));
            }
        };

        if expected_revision != intent.request_revision {
            return Err(pc_error(
                "PC_RUN_ALREADY_RESOLVED",
                format!("Replay request revision {expected_revision} does not match accepted revision {} for run '{run_id}'.", intent.request_revision),
            ));
        }
        if intent.candidate_id != candidate_id {
            return Err(pc_error(
                "PC_RUN_ALREADY_RESOLVED",
                format!("Run '{run_id}' was resolved for candidate '{}', not '{candidate_id}'.", intent.candidate_id),
            ));
        }

        let accepted_entry_present = journal.plan_convergence_audit.iter().any(|entry| {
            entry.event_type == "new_primary_plan_confirmed"
                && entry.candidate_id.as_deref() == Some(candidate_id.as_str())
        });
        if !accepted_entry_present {
            return Err(pc_error(
                "PC_RUN_ALREADY_RESOLVED",
                format!("Run '{run_id}' has no durable terminal acceptance audit record."),
            ));
        }

        let candidate = super::convergence::load_and_verify_candidate(
            manager.runs_dir(),
            &run_id,
            &intent.candidate_artifact_ref,
            &intent.candidate_artifact_digest,
        )
        .map_err(|error| pc_error(&error.code, error.message))?;

        if candidate.candidate_id != candidate_id {
            return Err(pc_error(
                "PC_RUN_ALREADY_RESOLVED",
                format!("Reviewed candidate '{}' does not match '{candidate_id}'.", candidate.candidate_id),
            ));
        }

        let (proposed_revision, title, initial_content) = match candidate.operation.clone() {
            PlannerOperationProposal::NewPrimaryPlan {
                proposed_revision,
                title,
                initial_content,
            } => (proposed_revision, title, initial_content),
            PlannerOperationProposal::AppendSection { .. } => {
                return Err(pc_error(
                    "PC_NOT_NEW_PRIMARY_PLAN",
                    format!("Candidate '{candidate_id}' is not a new primary plan."),
                ));
            }
        };

        let project_path = journal.canonical_project_path.clone();
        let workspace_config = journal.snapshot.plan_workspace.clone();
        let project_root = Path::new(&project_path);
        let expected_rel = super::plan_workspace::resolve_plan_candidate_target(
            project_root,
            &workspace_config,
            proposed_revision,
        )
        .map_err(|error| pc_error(&error.code, error.message))?;

        let content_digest = super::plan_workspace::sha256_bytes(
            super::plan_workspace::render_new_primary_plan_content(&title, &initial_content).as_bytes(),
        );
        let operation_payload_digest =
            super::plan_workspace::sha256_bytes(&super::convergence::canonical_operation_bytes(
                &candidate.operation,
            ));

        let transaction = ConfirmedTransaction {
            run_id: run_id.clone(),
            candidate_id: candidate_id.clone(),
            candidate_sequence: candidate.sequence,
            candidate_artifact_ref: intent.candidate_artifact_ref.clone(),
            candidate_artifact_digest: intent.candidate_artifact_digest.clone(),
            operation_kind: "new_primary_plan".to_string(),
            operation_payload_digest: operation_payload_digest.clone(),
            base_plan_digest: candidate.base_plan_digest.clone(),
            plan_context_digest: candidate.plan_context_digest.clone(),
            target_revision: proposed_revision,
            target_plan_id: super::plan_workspace::plan_id_from_filename(
                expected_rel.rsplit('/').next().unwrap_or(expected_rel.as_str()),
            ),
            target_path: expected_rel.clone(),
            content_digest: content_digest.clone(),
            title,
            initial_content,
        };

        if !intent.matches_transaction(
            &transaction.run_id,
            &transaction.candidate_id,
            transaction.candidate_sequence,
            &transaction.candidate_artifact_ref,
            &transaction.candidate_artifact_digest,
            &transaction.operation_payload_digest,
            &transaction.content_digest,
        ) {
            return Err(pc_error(
                "PC_RUN_ALREADY_RESOLVED",
                format!("Confirmation intent for run '{run_id}' does not match the candidate transaction."),
            ));
        }

        if intent.operation_kind != transaction.operation_kind
            || intent.base_plan_digest != transaction.base_plan_digest
            || intent.plan_context_digest != transaction.plan_context_digest
            || intent.target_revision != transaction.target_revision
            || intent.target_plan_id != transaction.target_plan_id
            || intent.target_path != transaction.target_path
        {
            return Err(pc_error(
                "PC_RUN_ALREADY_RESOLVED",
                format!("Confirmation intent for candidate '{candidate_id}' does not bind the reviewed target."),
            ));
        }

        let target_path = resolve_confirmation_target(project_root, &expected_rel, &intent.target_path)?;
        let published_digest = file_digest_if_present(&target_path)?;
        if published_digest.as_deref() != Some(intent.content_digest.as_str()) {
            return Err(pc_error(
                "PC_CONFIRMATION_TARGET_CONFLICT",
                format!(
                    "Accepted plan '{}' does not match the confirmed candidate.",
                    intent.target_path
                ),
            ));
        }

        return Ok(PlanConvergenceCommandResult::Created {
            run_id,
            created_plan_id: intent.target_plan_id.clone(),
            created_path: intent.target_path.clone(),
            file_digest: intent.content_digest.clone(),
        });
    }

    if journal.current_state != WorkflowState::WaitingForUser {
        return Err(pc_error(
            "PC_RUN_NOT_WAITING",
            format!("Run '{run_id}' is not waiting for operator input."),
        ));
    }

    let waiting_entry = journal
        .plan_convergence_audit
        .last()
        .cloned()
        .ok_or_else(|| {
            pc_error(
                "PC_INVALID_REVIEW_HISTORY",
                format!("Run '{run_id}' has no convergence audit history."),
            )
        })?;

    // The pending state is either the driver's human-gate entry or the
    // confirmation intent a previous attempt recorded. Both must still name the
    // new-primary confirmation reason, so a superseded state cannot be acted on.
    let awaiting_new_primary = matches!(
        waiting_entry.event_type.as_str(),
        "human_gate_new_primary_plan" | "new_primary_plan_confirmation_intent"
    ) && waiting_entry.waiting_reason == Some(ConvergenceWaitingReason::NewPrimaryPlanConfirmation);

    if !awaiting_new_primary {
        return Err(pc_error(
            "PC_NOT_AWAITING_NEW_PRIMARY_CONFIRMATION",
            format!("Run '{run_id}' is not awaiting new-primary-plan confirmation."),
        ));
    }
    if waiting_entry.candidate_id.as_deref() != Some(candidate_id.as_str()) {
        return Err(pc_error(
            "PC_CANDIDATE_MISMATCH",
            format!("Run '{run_id}' is awaiting a different candidate than '{candidate_id}'."),
        ));
    }

    let candidate_ref = waiting_entry.artifact_ref.clone().ok_or_else(|| {
        pc_error(
            "PC_INVALID_REVIEW_HISTORY",
            format!("Run '{run_id}' waiting record has no candidate artifact reference."),
        )
    })?;
    let candidate_digest = journal
        .plan_convergence_audit
        .iter()
        .find(|entry| entry.artifact_ref.as_ref() == Some(&candidate_ref))
        .and_then(|entry| entry.artifact_digest.clone())
        .ok_or_else(|| {
            pc_error(
                "PC_INVALID_REVIEW_HISTORY",
                format!("Run '{run_id}' has no digest for candidate artifact '{candidate_ref}'."),
            )
        })?;

    let candidate = super::convergence::load_and_verify_candidate(
        manager.runs_dir(),
        &run_id,
        &candidate_ref,
        &candidate_digest,
    )
    .map_err(|error| pc_error(&error.code, error.message))?;

    if candidate.candidate_id != candidate_id {
        return Err(pc_error(
            "PC_CANDIDATE_MISMATCH",
            format!(
                "Reviewed candidate '{}' does not match '{candidate_id}'.",
                candidate.candidate_id
            ),
        ));
    }

    let (proposed_revision, title, initial_content) = match candidate.operation.clone() {
        PlannerOperationProposal::NewPrimaryPlan {
            proposed_revision,
            title,
            initial_content,
        } => (proposed_revision, title, initial_content),
        PlannerOperationProposal::AppendSection { .. } => {
            return Err(pc_error(
                "PC_NOT_NEW_PRIMARY_PLAN",
                format!("Candidate '{candidate_id}' is not a new primary plan."),
            ))
        }
    };

    // Every binding the durable intent commits to is derived here from the
    // verified candidate and a freshly resolved workspace, never from the
    // request alone.
    let project_path = journal.canonical_project_path.clone();
    let workspace_config = journal.snapshot.plan_workspace.clone();
    let project_root = Path::new(&project_path);
    let expected_rel = super::plan_workspace::resolve_plan_candidate_target(
        project_root,
        &workspace_config,
        proposed_revision,
    )
    .map_err(|error| pc_error(&error.code, error.message))?;

    let content_digest = super::plan_workspace::sha256_bytes(
        super::plan_workspace::render_new_primary_plan_content(&title, &initial_content).as_bytes(),
    );
    let operation_payload_digest =
        super::plan_workspace::sha256_bytes(&super::convergence::canonical_operation_bytes(
            &candidate.operation,
        ));

    let transaction = ConfirmedTransaction {
        run_id: run_id.clone(),
        candidate_id: candidate_id.clone(),
        candidate_sequence: candidate.sequence,
        candidate_artifact_ref: candidate_ref.clone(),
        candidate_artifact_digest: candidate_digest.clone(),
        operation_kind: "new_primary_plan".to_string(),
        operation_payload_digest: operation_payload_digest.clone(),
        base_plan_digest: candidate.base_plan_digest.clone(),
        plan_context_digest: candidate.plan_context_digest.clone(),
        target_revision: proposed_revision,
        target_plan_id: super::plan_workspace::plan_id_from_filename(
            expected_rel.rsplit('/').next().unwrap_or(expected_rel.as_str()),
        ),
        target_path: expected_rel.clone(),
        content_digest: content_digest.clone(),
        title,
        initial_content,
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // A durable intent left by an earlier attempt turns this action into a
    // reconciliation of that attempt instead of a second publication.
    let reconciled: Option<ReconciledIntent> = match journal
        .plan_convergence_confirmation_intent
        .clone()
    {
        Some(intent) => {
            if !intent.matches_transaction(
                &transaction.run_id,
                &transaction.candidate_id,
                transaction.candidate_sequence,
                &transaction.candidate_artifact_ref,
                &transaction.candidate_artifact_digest,
                &transaction.operation_payload_digest,
                &transaction.content_digest,
            ) {
                return Err(pc_error(
                    "PC_CONFIRMATION_INTENT_MISMATCH",
                    format!(
                        "Run '{run_id}' holds a confirmation intent that does not match candidate '{candidate_id}'."
                    ),
                ));
            }
            if intent.operation_kind != transaction.operation_kind
                || intent.base_plan_digest != transaction.base_plan_digest
                || intent.plan_context_digest != transaction.plan_context_digest
                || intent.target_revision != transaction.target_revision
                || intent.target_plan_id != transaction.target_plan_id
                || intent.target_path != transaction.target_path
            {
                return Err(pc_error(
                    "PC_CONFIRMATION_INTENT_MISMATCH",
                    format!(
                        "Confirmation intent for candidate '{candidate_id}' does not bind the reviewed target."
                    ),
                ));
            }
            let target_path =
                resolve_confirmation_target(project_root, &expected_rel, &intent.target_path)?;
            let published_digest = file_digest_if_present(&target_path)?;
            Some(ReconciledIntent {
                intent,
                target_path,
                published_digest,
            })
        }
        None => None,
    };

    // A matching terminal acceptance record makes this an exact replay: report
    // idempotent success without another publication or journal transition.
    if let Some(reconciled) = &reconciled {
        if journal.plan_convergence_audit.iter().any(|entry| {
            entry.event_type == "new_primary_plan_confirmed"
                && entry.candidate_id.as_deref() == Some(candidate_id.as_str())
        }) {
            if reconciled.published_digest.as_deref()
                != Some(reconciled.intent.content_digest.as_str())
            {
                return Err(pc_error(
                    "PC_CONFIRMATION_TARGET_CONFLICT",
                    format!(
                        "Accepted plan '{}' does not match the confirmed candidate.",
                        reconciled.intent.target_path
                    ),
                ));
            }
            return Ok(PlanConvergenceCommandResult::Created {
                run_id,
                created_plan_id: reconciled.intent.target_plan_id.clone(),
                created_path: reconciled.intent.target_path.clone(),
                file_digest: reconciled.intent.content_digest.clone(),
            });
        }
    }

    // Once the plan bytes are published this is an unfinished acceptance, not a
    // refusable wait: rejecting it would cancel a run whose plan file is already
    // on disk, and convergence never deletes a published plan.
    if action == "reject" {
        if let Some(reconciled) = &reconciled {
            if let Some(published_digest) = &reconciled.published_digest {
                if published_digest != &reconciled.intent.content_digest {
                    return Err(pc_error(
                        "PC_CONFIRMATION_TARGET_CONFLICT",
                        format!(
                            "Plan '{}' exists with different content than the confirmed candidate; refusing to overwrite or cancel it.",
                            reconciled.intent.target_path
                        ),
                    ));
                }

                // Reconcile forward to durable acceptance: never mark the run Cancelled after publication.
                journal.revision = journal.revision.saturating_add(1);
                journal.updated_at_unix = now;
                journal.status = super::recovery::RunRecoveryStatus::Complete;
                journal.current_state = WorkflowState::Complete;
                journal.plan_convergence_audit.push(
                    super::convergence::PlanConvergenceAuditEntry {
                        timestamp_unix: now,
                        event_type: "new_primary_plan_confirmed".to_string(),
                        sequence: candidate.sequence,
                        candidate_id: Some(candidate_id.clone()),
                        artifact_ref: Some(candidate_ref),
                        artifact_digest: Some(candidate_digest),
                        role: None,
                        profile_id: None,
                        model: None,
                        decision: None,
                        waiting_reason: None,
                        error_code: None,
                    },
                );
                manager.write_journal(&journal).map_err(|error| {
                    pc_error(
                        "PC_PERSISTENCE_FAILED",
                        format!(
                            "Plan '{}' was published but its acceptance record could not be persisted: {error}. Confirming this candidate again reconciles the published plan.",
                            reconciled.intent.target_plan_id
                        ),
                    )
                })?;

                runtime.emit_step(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::Complete,
                    message: "orchestrator.planConvergence.newPlanCreated".to_string(),
                    ..Default::default()
                });

                return Ok(PlanConvergenceCommandResult::Created {
                    run_id,
                    created_plan_id: reconciled.intent.target_plan_id.clone(),
                    created_path: reconciled.intent.target_path.clone(),
                    file_digest: reconciled.intent.content_digest.clone(),
                });
            }

            // Target is absent and an intent exists: validate expected_revision matches intent.request_revision.
            if expected_revision != reconciled.intent.request_revision {
                return Err(pc_error(
                    "PC_STALE_REVISION",
                    format!(
                        "The confirmation intent was recorded for request revision {} but the action targeted revision {expected_revision}.",
                        reconciled.intent.request_revision
                    ),
                ));
            }
            // Atomically consume/clear the intent so subsequent Confirm cannot reuse it.
            journal.plan_convergence_confirmation_intent = None;
        } else {
            // No intent recorded yet: validate expected_revision against current journal revision.
            if expected_revision != journal.revision {
                return Err(pc_error(
                    "PC_STALE_REVISION",
                    format!(
                        "Run '{run_id}' is at journal revision {} but the action targeted revision {expected_revision}.",
                        journal.revision
                    ),
                ));
            }
        }

        journal.revision = journal.revision.saturating_add(1);
        journal.updated_at_unix = now;
        journal.status = super::recovery::RunRecoveryStatus::Cancelled;
        journal.current_state = WorkflowState::Cancelled;
        journal.plan_convergence_audit.push(
            super::convergence::PlanConvergenceAuditEntry {
                timestamp_unix: now,
                event_type: "new_primary_plan_rejected".to_string(),
                sequence: candidate.sequence,
                candidate_id: Some(candidate_id.clone()),
                artifact_ref: Some(candidate_ref),
                artifact_digest: Some(candidate_digest),
                role: None,
                profile_id: None,
                model: None,
                decision: None,
                waiting_reason: None,
                error_code: Some("PC_HUMAN_REJECTED".to_string()),
            },
        );
        manager.write_journal(&journal).map_err(|error| {
            pc_error(
                "PC_PERSISTENCE_FAILED",
                format!("Failed to persist the rejection of candidate '{candidate_id}': {error}"),
            )
        })?;

        runtime.emit_step(StepProgressEvent {
            run_id: run_id.clone(),
            step: WorkflowState::Cancelled,
            message: "orchestrator.planConvergence.newPlanRejected".to_string(),
            ..Default::default()
        });

        return Ok(PlanConvergenceCommandResult::Rejected { run_id });
    }

    // Confirm: either record the intent for a first attempt, or adopt the one a
    // previous attempt already made durable.
    let (intent, published_digest) = match reconciled {
        Some(reconciled) => {
            // A retry may only reconcile the intent the operator actually
            // reviewed. A different request revision is stale and must not
            // mutate anything.
            if expected_revision != reconciled.intent.request_revision {
                return Err(pc_error(
                    "PC_STALE_REVISION",
                    format!(
                        "The confirmation intent was recorded for request revision {} but the action targeted revision {expected_revision}.",
                        reconciled.intent.request_revision
                    ),
                ));
            }
            // The journal must still sit at this intent's own revision. Anything
            // further along is a state this request cannot authorise.
            if journal.revision != reconciled.intent.intent_record_revision {
                return Err(pc_error(
                    "PC_STALE_REVISION",
                    format!(
                        "Run '{run_id}' is at journal revision {} but the confirmation intent was recorded at revision {}.",
                        journal.revision, reconciled.intent.intent_record_revision
                    ),
                ));
            }
            (reconciled.intent, reconciled.published_digest)
        }
        None => {
            // The first request must name the exact waiting revision the
            // operator reviewed. That value is recorded as `request_revision`.
            if expected_revision != journal.revision {
                return Err(pc_error(
                    "PC_STALE_REVISION",
                    format!(
                        "Run '{run_id}' is at journal revision {} but the action targeted revision {expected_revision}.",
                        journal.revision
                    ),
                ));
            }
            let request_revision = expected_revision;
            let intent = super::convergence::PlanConvergenceConfirmationIntent {
                run_id: run_id.clone(),
                request_revision,
                intent_record_revision: request_revision.saturating_add(1),
                candidate_id: candidate_id.clone(),
                candidate_sequence: candidate.sequence,
                candidate_artifact_ref: candidate_ref.clone(),
                candidate_artifact_digest: candidate_digest.clone(),
                operation_kind: transaction.operation_kind.clone(),
                operation_payload_digest: transaction.operation_payload_digest.clone(),
                base_plan_digest: transaction.base_plan_digest.clone(),
                plan_context_digest: transaction.plan_context_digest.clone(),
                target_revision: transaction.target_revision,
                target_plan_id: transaction.target_plan_id.clone(),
                target_path: transaction.target_path.clone(),
                content_digest: transaction.content_digest.clone(),
                idempotency_key: super::convergence::PlanConvergenceConfirmationIntent::idempotency_key_for(
                    &run_id, &candidate_id,
                ),
            };

            journal.revision = journal.revision.saturating_add(1);
            journal.updated_at_unix = now;
            journal.plan_convergence_confirmation_intent = Some(intent.clone());
            journal.plan_convergence_audit.push(
                super::convergence::PlanConvergenceAuditEntry {
                    timestamp_unix: now,
                    event_type: "new_primary_plan_confirmation_intent".to_string(),
                    sequence: candidate.sequence,
                    candidate_id: Some(candidate_id.clone()),
                    artifact_ref: Some(candidate_ref.clone()),
                    artifact_digest: Some(candidate_digest.clone()),
                    role: None,
                    profile_id: None,
                    model: None,
                    decision: None,
                    waiting_reason: Some(ConvergenceWaitingReason::NewPrimaryPlanConfirmation),
                    error_code: None,
                },
            );
            manager.write_journal(&journal).map_err(|error| {
                pc_error(
                    "PC_PERSISTENCE_FAILED",
                    format!(
                        "Failed to persist the confirmation intent for candidate '{candidate_id}': {error}"
                    ),
                )
            })?;

            // Publication must not begin until the intent is durable at exactly
            // the revision it claims.
            let saved = manager.read_journal(&run_id).map_err(|error| {
                pc_error(
                    "PC_PERSISTENCE_FAILED",
                    format!("Failed to re-read the confirmation intent: {error}"),
                )
            })?;
            let saved_intent = saved
                .plan_convergence_confirmation_intent
                .clone()
                .filter(|saved_intent| saved_intent == &intent)
                .ok_or_else(|| {
                    pc_error(
                        "PC_PERSISTENCE_FAILED",
                        "The recorded confirmation intent does not match the submitted one."
                            .to_string(),
                    )
                })?;
            if saved.revision != saved_intent.intent_record_revision {
                return Err(pc_error(
                    "PC_PERSISTENCE_FAILED",
                    format!(
                        "The confirmation intent was recorded at revision {} but the journal is at revision {}.",
                        saved_intent.intent_record_revision, saved.revision
                    ),
                ));
            }
            journal = saved;

            let target_path = resolve_confirmation_target(
                project_root,
                &expected_rel,
                &saved_intent.target_path,
            )?;
            let existing = file_digest_if_present(&target_path)?;
            (saved_intent, existing)
        }
    };

    // Reconcile publication forward under the shared Plan Workspace write lock.
    let (created_plan_id, created_path, file_digest) = match published_digest {
        Some(digest) => {
            // Bytes already published by an earlier attempt. Reconciliation never
            // overwrites, truncates, or duplicates that file.
            if digest != intent.content_digest {
                return Err(pc_error(
                    "PC_CONFIRMATION_TARGET_CONFLICT",
                    format!(
                        "Plan '{}' exists with different content than the confirmed candidate; refusing to overwrite it.",
                        intent.target_path
                    ),
                ));
            }
            (
                intent.target_plan_id.clone(),
                intent.target_path.clone(),
                digest,
            )
        }
        None => {
            let created = super::plan_workspace::plan_new_confirm_bound(
                project_root,
                &workspace_config,
                super::plan_workspace::PlanNewConfirmRequest {
                    token: {
                        let preview = super::plan_workspace::plan_new_preview(
                            project_root,
                            &workspace_config,
                        )
                        .map_err(|error| pc_error(&error.code, error.message))?;
                        if preview.candidate_path != intent.target_path
                            || preview.candidate_revision != intent.target_revision
                        {
                            return Err(pc_error(
                                "PC_STALE_CONFIRMATION",
                                format!(
                                    "The plan workspace no longer resolves candidate '{candidate_id}' to '{}'.",
                                    intent.target_path
                                ),
                            ));
                        }
                        preview.token
                    },
                    title: transaction.title.clone(),
                    initial_content: transaction.initial_content.clone(),
                },
                &super::plan_workspace::ExpectedNewPlanBinding {
                    target_path: intent.target_path.clone(),
                    content_digest: intent.content_digest.clone(),
                    expected_plan_context_digest: Some(intent.plan_context_digest.clone()),
                },
            )
            .map_err(|error| pc_error(&error.code, error.message))?;

            if created.created_path != intent.target_path
                || created.file_digest != intent.content_digest
            {
                return Err(pc_error(
                    "PC_CONFIRMATION_TARGET_MISMATCH",
                    format!(
                        "Published plan '{}' does not match the confirmed candidate.",
                        created.created_path
                    ),
                ));
            }
            (
                created.created_plan_id,
                created.created_path,
                created.file_digest,
            )
        }
    };

    // Durable success ordering: the created-plan identity and the terminal
    // state are persisted before the operator is told confirmation succeeded.
    journal.revision = journal.revision.saturating_add(1);
    journal.updated_at_unix = now;
    journal.status = super::recovery::RunRecoveryStatus::Complete;
    journal.current_state = WorkflowState::Complete;
    journal.plan_convergence_audit.push(
        super::convergence::PlanConvergenceAuditEntry {
            timestamp_unix: now,
            event_type: "new_primary_plan_confirmed".to_string(),
            sequence: candidate.sequence,
            candidate_id: Some(candidate_id.clone()),
            artifact_ref: Some(candidate_ref),
            artifact_digest: Some(candidate_digest),
            role: None,
            profile_id: None,
            model: None,
            decision: None,
            waiting_reason: None,
            error_code: None,
        },
    );
    manager.write_journal(&journal).map_err(|error| {
        pc_error(
            "PC_PERSISTENCE_FAILED",
            format!(
                "Plan '{created_plan_id}' was published but its acceptance record could not be persisted: {error}. Confirming this candidate again reconciles the published plan."
            ),
        )
    })?;

    runtime.emit_step(StepProgressEvent {
        run_id: run_id.clone(),
        step: WorkflowState::Complete,
        message: "orchestrator.planConvergence.newPlanCreated".to_string(),
        ..Default::default()
    });

    Ok(PlanConvergenceCommandResult::Created {
        run_id,
        created_plan_id,
        created_path,
        file_digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::plan_workspace::PlanWorkspaceConfig;
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
            lean_antigravity_mode: false,
            plan_workspace: PlanWorkspaceConfig::default(),
            mcp_servers: std::collections::HashMap::new(),
        }
    }

    fn recovery_antigravity_profile(id: &str, capabilities: Vec<ProfileCapability>) -> OrchestratorProfile {
        OrchestratorProfile {
            id: id.to_string(), display_name: "Antigravity Harness".to_string(),
            adapter: ExecutionAdapterType::Antigravity, capabilities,
            provider_id: None, provider_profile_id: None, model: None, thinking_mode: None,
            reasoning_effort: None, ollama_model: None, ollama_endpoint: None,
            executable: None, args: None, external_mcp_server: None, mcp_tool: None,
            context_window_tokens: None,
        }
    }

    fn prepare_human_gated_recovery_snapshot(project_path: &Path) -> RunConfigurationSnapshot {
        let mut snapshot = snapshot_for_overrides();
        snapshot.project_path = project_path.to_string_lossy().into_owned();
        if let Some(gate) = snapshot.validation_gates.first_mut() {
            gate.working_dir = Some(snapshot.project_path.clone());
        }
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());
        let harness = recovery_antigravity_profile("antigravity-harness", vec![ProfileCapability::WorkspaceWrite]);
        let cli_reviewer = snapshot.assignments.get(&AgentRole::Implementer).cloned().unwrap();
        snapshot.assignments.insert(AgentRole::PlanIntegrator, harness.clone());
        snapshot.assignments.insert(AgentRole::Implementer, harness);
        snapshot.assignments.insert(AgentRole::Fixer, recovery_antigravity_profile("antigravity-fixer", vec![ProfileCapability::WorkspaceWrite]));
        snapshot.assignments.insert(AgentRole::CodeReviewer, cli_reviewer);
        snapshot
    }

    fn init_recovery_test_repository(root: &Path) {
        std::fs::create_dir_all(root).unwrap();
        assert!(std::process::Command::new("git").args(["init", "-q"]).current_dir(root).status().unwrap().success());
        for (key, value) in [("user.name", "Test"), ("user.email", "test@example.invalid")] {
            assert!(std::process::Command::new("git").args(["config", key, value]).current_dir(root).status().unwrap().success());
        }
        std::fs::write(root.join("tracked.txt"), b"base").unwrap();
        std::fs::write(root.join("package.json"), br#"{"name":"test","version":"0.24.0"}"#).unwrap();
        assert!(std::process::Command::new("git").args(["add", "tracked.txt", "package.json"]).current_dir(root).status().unwrap().success());
        assert!(std::process::Command::new("git").args(["commit", "-qm", "initial"]).current_dir(root).status().unwrap().success());
    }

    #[test]
    fn recovery_resume_route_is_bounded_and_preserves_authoritative_counters() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        init_recovery_test_repository(&root);
        let runs_dir = temp.path().join("runs");
        let manager = Arc::new(super::super::recovery::JournalManager::new(runs_dir.clone()));
        let store = super::super::checkpoint::CheckpointStore::new(runs_dir);
        let (manifest, reference) = store.capture(
            "resume-route-test", WorkflowState::Implementation,
            super::super::recovery::CheckpointKind::EntryBaseline, &root,
            Some("task prompt".into()), Some(serde_json::json!({"planText":"approved plan"}).to_string()),
        ).unwrap();
        let snapshot = prepare_human_gated_recovery_snapshot(&root);
        let journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: "resume-route-test".into(), workflow_type: "human_gated_loop".into(),
            canonical_project_path: root.to_string_lossy().into_owned(), task_prompt: Some("task prompt".into()),
            approved_plan: Some("approved plan".into()),
            snapshot, current_state: WorkflowState::Implementation,
            last_successful_state: Some(WorkflowState::PlanReview), stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters {
                plan_review_count: 1, fix_count: 0, code_review_count: 0,
                antigravity_dispatches: Some(2),
                antigravity_task_dispatches: std::collections::HashMap::from([("task-plan-integration".into(), 2)]),
                mailbox_epoch: Some(4),
            },
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 9, resume_generation: 0, checkpoint_manifest_ref: Some(reference.clone()),
            checkpoint_digest: Some(manifest.manifest_digest.clone()), last_shelve_backup_id: None,
            last_shelve_backup_digest: None, status: super::super::recovery::RunRecoveryStatus::Interrupted,
            created_at_unix: 1, updated_at_unix: 1,
        };
        manager.write_journal(&journal).unwrap();
        let state = Arc::new(OrchestratorState::with_journal_manager(manager.clone()));
        let preflight = preflight_run_recovery_impl(&state, &journal.run_id).unwrap();
        assert!(preflight.can_resume);
        assert_eq!(preflight.resume_stage, Some(WorkflowState::Implementation));
        let loaded_manifest = store.load(&journal.run_id, &reference, &manifest.manifest_digest).unwrap();
        let context = build_human_gated_resume_context(&journal, &loaded_manifest).unwrap();
        assert_eq!(context.plan_text, "approved plan");
        assert_eq!(context.plan_review_count, 1);
        assert_eq!(context.antigravity_dispatches, 2);
        assert_eq!(context.antigravity_task_dispatches["task-plan-integration"], 2);
        assert_eq!(context.mailbox_epoch, 5);

        let mut unsupported = loaded_manifest.clone();
        unsupported.stage = WorkflowState::PlanDraft;
        assert!(build_human_gated_resume_context(&journal, &unsupported).is_err());
        let mut wrong_workflow = journal.clone();
        wrong_workflow.workflow_type = "full_loop".into();
        assert!(build_human_gated_resume_context(&wrong_workflow, &loaded_manifest).is_err());
    }

    #[tokio::test]
    async fn adopt_creates_validation_baseline_and_production_resume_revalidates_before_approval() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        init_recovery_test_repository(&root);
        let runs_dir = temp.path().join("runs");
        let manager = Arc::new(super::super::recovery::JournalManager::new(runs_dir.clone()));
        let store = super::super::checkpoint::CheckpointStore::new(runs_dir);
        let (manifest, reference) = store.capture(
            "adopt-validation-route", WorkflowState::Implementation,
            super::super::recovery::CheckpointKind::EntryBaseline, &root,
            Some("task prompt".into()), Some(serde_json::json!({"planText":"approved plan"}).to_string()),
        ).unwrap();
        let journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: "adopt-validation-route".into(), workflow_type: "human_gated_loop".into(),
            canonical_project_path: root.to_string_lossy().into_owned(), task_prompt: Some("task prompt".into()),
            approved_plan: Some("approved plan".into()),
            snapshot: prepare_human_gated_recovery_snapshot(&root), current_state: WorkflowState::Implementation,
            last_successful_state: Some(WorkflowState::PlanReview), stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters {
                plan_review_count: 1, fix_count: 0, code_review_count: 0,
                antigravity_dispatches: Some(2),
                antigravity_task_dispatches: std::collections::HashMap::from([("task-plan-integration".into(), 2)]),
                mailbox_epoch: Some(4),
            },
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 4, resume_generation: 0, checkpoint_manifest_ref: Some(reference),
            checkpoint_digest: Some(manifest.manifest_digest), last_shelve_backup_id: None,
            last_shelve_backup_digest: None, status: super::super::recovery::RunRecoveryStatus::Interrupted,
            created_at_unix: 1, updated_at_unix: 1,
        };
        manager.write_journal(&journal).unwrap();
        let state = Arc::new(OrchestratorState::with_journal_manager(manager.clone()));

        std::fs::write(root.join("tracked.txt"), b"adopted user content").unwrap();
        let drifted = preflight_run_recovery_impl(&state, &journal.run_id).unwrap();
        assert!(!drifted.workspace_matches);
        assert!(drifted.can_adopt);
        let resolution = resolve_run_recovery_impl(
            &state, &journal.run_id, "adopt", drifted.journal_revision,
            &drifted.current_fingerprint, &drifted.backup_id, true,
        ).unwrap();
        assert_eq!(std::fs::read(root.join("tracked.txt")).unwrap(), b"adopted user content");
        assert_eq!(resolution.resume_stage, Some(WorkflowState::Validation));

        let adopted = manager.read_journal(&journal.run_id).unwrap();
        assert_eq!(adopted.current_state, WorkflowState::Validation);
        assert_eq!(adopted.status, super::super::recovery::RunRecoveryStatus::Interrupted);
        assert_eq!(adopted.last_successful_state, journal.last_successful_state,
            "Adopt must not declare the interrupted implementation successful");
        assert_eq!(adopted.iteration_counters, journal.iteration_counters,
            "Adopt must preserve all task/run counters and the prior Worker epoch");
        let adopted_manifest = store.load(
            &journal.run_id,
            adopted.checkpoint_manifest_ref.as_deref().unwrap(),
            adopted.checkpoint_digest.as_deref().unwrap(),
        ).unwrap();
        assert_eq!(adopted_manifest.kind, super::super::recovery::CheckpointKind::AdoptBaseline);
        assert_eq!(adopted_manifest.stage, WorkflowState::Validation);
        let ready = preflight_run_recovery_impl(&state, &journal.run_id).unwrap();
        assert!(ready.workspace_matches);
        assert!(ready.can_resume);
        assert_eq!(ready.resume_stage, Some(WorkflowState::Validation));
        let context = build_human_gated_resume_context(&adopted, &adopted_manifest).unwrap();
        assert!(context.adopted_baseline);
        assert_eq!(context.stage, WorkflowState::Validation);
        assert_eq!(context.plan_text, "approved plan");
        assert_eq!(context.mailbox_epoch, 5);

        // Exercise the real resume command -> spawned workflow -> production
        // event callback -> durable journal path. The scripted reviewer and
        // validation runner keep this deterministic and provider-independent.
        let envelopes = Arc::new(Mutex::new(Vec::new()));
        let engine = OrchestratorEngine::with_scripted_adapters(
            vec![(
                AgentRole::CodeReviewer,
                scripted_output_helper(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )],
            vec![
                super::super::validation::ValidationRunSummary {
                    passed: false,
                    total_gates_run: 1,
                    failed_gate_names: vec!["scripted gate".into()],
                    results: Vec::new(),
                    formatted_diagnostics: "scripted first validation failure".into(),
                },
                super::super::validation::ValidationRunSummary {
                    passed: true,
                    total_gates_run: 1,
                    failed_gate_names: Vec::new(),
                    results: Vec::new(),
                    formatted_diagnostics: "all gates passed on retry".into(),
                },
            ],
        )
        .with_task_submission_hook(scripted_worker_submission_hook(envelopes.clone()));
        let runtime = PollingRunStartRuntime::new(engine, state.clone(), manager.clone(), false);
        let builds_before_resume = audited_engine_builds();
        let resumed = resume_interrupted_run_impl(
            runtime.clone(),
            state.clone(),
            &journal.run_id,
            ready.journal_revision,
            &ready.current_fingerprint,
            true,
        )
        .expect("verified adopted baseline should resume");
        assert_eq!(resumed.run_id, journal.run_id);
        assert!(
            audited_engine_builds() > builds_before_resume,
            "the resume path must build its engine through the audited helper"
        );
        runtime.join().await;

        let events = runtime.steps.lock().unwrap().clone();
        let states: Vec<_> = events.iter().map(|event| event.step).collect();
        assert_eq!(states.first(), Some(&WorkflowState::Validation));
        assert!(states.contains(&WorkflowState::CodeReview));
        assert!(states.contains(&WorkflowState::Fix));
        assert!(states.contains(&WorkflowState::HumanGate));
        assert!(runtime.human_gate_send_errors.lock().unwrap().is_empty(),
            "test runtime could not deliver HumanGate approval");
        assert_eq!(states.last(), Some(&WorkflowState::Complete),
            "resume workflow did not complete; states={states:?}; logs={:?}",
            runtime.logs.lock().unwrap().iter().map(|entry| entry.message.clone()).collect::<Vec<_>>());
        for skipped in [
            WorkflowState::PlanDraft,
            WorkflowState::PlanIntegration,
            WorkflowState::PlanReview,
            WorkflowState::Implementation,
        ] {
            assert!(!states.contains(&skipped), "resume replayed skipped stage {skipped:?}");
        }

        let durable_at_validation = runtime.journal_at_step.lock().unwrap().iter()
            .find(|(step, _, _, _, _)| *step == WorkflowState::Validation)
            .cloned().expect("validation event should be observed after journal persistence");
        assert_eq!(durable_at_validation.1, journal.last_successful_state,
            "an adopted Validation entry must not mark the interrupted stage successful");
        let durable_at_code_review = runtime.journal_at_step.lock().unwrap().iter()
            .find(|(step, _, _, _, _)| *step == WorkflowState::CodeReview)
            .cloned().expect("successful validation must persist before CodeReview");
        assert_eq!(durable_at_code_review.1, Some(WorkflowState::Validation));
        assert_eq!(durable_at_code_review.2, journal.iteration_counters.plan_review_count);
        assert_eq!(durable_at_code_review.3, journal.iteration_counters.fix_count + 1,
            "failed validation should produce exactly one persisted Fix iteration");
        assert_eq!(durable_at_code_review.4, journal.iteration_counters.code_review_count + 1,
            "the resumed code-review attempt must increment its authoritative counter once");
        let durable_at_fix = runtime.journal_at_step.lock().unwrap().iter()
            .find(|(step, _, _, _, _)| *step == WorkflowState::Fix)
            .cloned().expect("failed Validation should route into Fix");
        assert_eq!(durable_at_fix.1, journal.last_successful_state,
            "failed Validation must not become a completed stage at Fix entry");
        let fix_event = events.iter().find(|event| event.step == WorkflowState::Fix).unwrap();
        assert_eq!(fix_event.completed_stage, None,
            "Fix entry after failed Validation cannot claim a successful Validation boundary");
        let scripted_envelopes = envelopes.lock().unwrap();
        let fix_envelope = scripted_envelopes.iter().find(|envelope| envelope.stage == WorkflowState::Fix)
            .expect("the production wait path should create a Fix task envelope");
        assert_eq!(fix_envelope.role, AgentRole::Fixer);
        assert_eq!(fix_envelope.approved_plan.as_deref(), Some("approved plan"));
        assert_eq!(fix_envelope.epoch, 5);
        assert!(fix_envelope.validation_summary.as_deref().unwrap().contains("scripted first validation failure"));
        drop(scripted_envelopes);
        let completed = manager.read_journal(&journal.run_id).unwrap();
        assert_eq!(completed.status, super::super::recovery::RunRecoveryStatus::Complete);
        assert_eq!(completed.last_successful_state, Some(WorkflowState::HumanGate),
            "terminal completion must retain the final completed workflow stage");
        assert_eq!(completed.resume_generation, 1);
        assert_eq!(completed.iteration_counters.plan_review_count, journal.iteration_counters.plan_review_count);
        assert_eq!(completed.iteration_counters.fix_count, journal.iteration_counters.fix_count + 1);
        assert_eq!(completed.iteration_counters.code_review_count, journal.iteration_counters.code_review_count + 1);
        assert_eq!(completed.iteration_counters.antigravity_dispatches, journal.iteration_counters.antigravity_dispatches);
    }

    #[tokio::test]
    async fn explicit_resume_persists_generation_and_enters_implementation_without_replaying_planning() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        init_recovery_test_repository(&root);
        let runs_dir = temp.path().join("runs");
        let manager = Arc::new(super::super::recovery::JournalManager::new(runs_dir.clone()));
        let store = super::super::checkpoint::CheckpointStore::new(runs_dir);
        let (manifest, reference) = store.capture(
            "resume-start-test", WorkflowState::Implementation,
            super::super::recovery::CheckpointKind::EntryBaseline, &root,
            Some("task prompt".into()), Some(serde_json::json!({"planText":"approved plan"}).to_string()),
        ).unwrap();
        let journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: "resume-start-test".into(), workflow_type: "human_gated_loop".into(),
            canonical_project_path: root.to_string_lossy().into_owned(), task_prompt: Some("task prompt".into()),
            approved_plan: Some("approved plan".into()),
            snapshot: prepare_human_gated_recovery_snapshot(&root), current_state: WorkflowState::Implementation,
            last_successful_state: Some(WorkflowState::PlanReview), stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters {
                plan_review_count: 1, fix_count: 0, code_review_count: 0,
                antigravity_dispatches: Some(2),
                antigravity_task_dispatches: std::collections::HashMap::from([("task-plan-integration".into(), 2)]),
                mailbox_epoch: Some(4),
            },
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 9, resume_generation: 0, checkpoint_manifest_ref: Some(reference),
            checkpoint_digest: Some(manifest.manifest_digest), last_shelve_backup_id: None,
            last_shelve_backup_digest: None, status: super::super::recovery::RunRecoveryStatus::Interrupted,
            created_at_unix: 1, updated_at_unix: 1,
        };
        manager.write_journal(&journal).unwrap();
        let state = Arc::new(OrchestratorState::with_journal_manager(manager.clone()));
        let envelopes = Arc::new(Mutex::new(Vec::new()));
        let engine = OrchestratorEngine::with_scripted_adapters(
            vec![(
                AgentRole::CodeReviewer,
                scripted_output_helper(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )],
            vec![super::super::validation::ValidationRunSummary {
                passed: true,
                total_gates_run: 1,
                failed_gate_names: Vec::new(),
                results: Vec::new(),
                formatted_diagnostics: "all gates passed".into(),
            }],
        )
        .with_task_submission_hook(scripted_worker_submission_hook(envelopes.clone()));
        let runtime = PollingRunStartRuntime::new(engine, state.clone(), manager.clone(), false);
        let preflight = preflight_run_recovery_impl(&state, &journal.run_id).unwrap();
        assert!(resume_interrupted_run_impl(runtime.clone(), state.clone(), &journal.run_id,
            preflight.journal_revision, &preflight.current_fingerprint, false).is_err());
        assert_eq!(manager.read_journal(&journal.run_id).unwrap(), journal);
        assert!(runtime.task.lock().unwrap().is_none(), "unconfirmed resume must not spawn");

        let response = resume_interrupted_run_impl(runtime.clone(), state.clone(), &journal.run_id,
            preflight.journal_revision, &preflight.current_fingerprint, true).unwrap();
        assert_eq!(response.run_id, journal.run_id);
        runtime.join().await;
        let events = runtime.steps.lock().unwrap().clone();
        let states: Vec<_> = events.iter().map(|event| event.step).collect();
        assert_eq!(states.first(), Some(&WorkflowState::Implementation));
        assert_eq!(states.last(), Some(&WorkflowState::Complete));
        for skipped in [
            WorkflowState::PlanDraft,
            WorkflowState::PlanIntegration,
            WorkflowState::PlanReview,
        ] {
            assert!(!states.contains(&skipped), "resume replayed completed stage {skipped:?}");
        }
        let persisted = manager.read_journal(&journal.run_id).unwrap();
        assert_eq!(persisted.resume_generation, 1);
        assert!(persisted.revision > journal.revision,
            "resume-start and polled workflow progress must be durably revisioned");
        assert_eq!(persisted.iteration_counters.antigravity_dispatches, Some(2));
        assert_eq!(persisted.iteration_counters.mailbox_epoch, Some(5));
        assert!(events.iter().any(|event| event.step == WorkflowState::Implementation));
        let implementation_event = events.iter().find(|event| event.step == WorkflowState::Implementation).unwrap();
        assert_eq!(implementation_event.plan_text.as_deref(), Some("approved plan"));
        assert_eq!(implementation_event.plan_review_count, Some(1));
        let implementation_envelope = envelopes.lock().unwrap().iter()
            .find(|envelope| envelope.stage == WorkflowState::Implementation)
            .cloned().expect("production wait path should create the resumed Implementation task envelope");
        assert_eq!(implementation_envelope.run_id, journal.run_id);
        assert_eq!(implementation_envelope.role, AgentRole::Implementer);
        assert_eq!(implementation_envelope.approved_plan.as_deref(), Some("approved plan"));
        assert_eq!(implementation_envelope.epoch, 5);
        let completed = manager.read_journal(&journal.run_id).unwrap();
        assert_eq!(completed.status, super::super::recovery::RunRecoveryStatus::Complete);
        assert_eq!(completed.iteration_counters.antigravity_dispatches, Some(2),
            "the private envelope seam observes pre-claim dispatch; counters do not change until accepted claim");
        assert_eq!(completed.iteration_counters.antigravity_task_dispatches, journal.iteration_counters.antigravity_task_dispatches);
        assert_eq!(completed.iteration_counters.mailbox_epoch, Some(5));
        assert!(completed.revision > journal.revision + 1,
            "the polled workflow must persist progress beyond the resume-start write");
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
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
        (Arc::new(|_| Ok(())), Arc::new(|_| {}))
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
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
            None,
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
                None,
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

    #[tokio::test]
    async fn supervisor_preserves_waiting_for_user_without_emitting_failed() {
        let state = Arc::new(OrchestratorState::new());
        *state.active_run.lock().unwrap() = Some(active_run("waiting-run"));
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let on_event: super::super::engine::EventCallback = Arc::new(move |ev| {
            events_clone.lock().unwrap().push(ev);
            Ok(())
        });
        let on_log: super::super::engine::LogCallback = Arc::new(|_| {});

        supervise_run(
            async {
                // Workflow completes intentionally in WaitingForUser (e.g. budget exhaustion)
                Ok(WorkflowState::WaitingForUser)
            },
            "waiting-run".into(),
            Arc::clone(&state),
            None,
            on_event,
            on_log,
        )
        .await;

        // active_run is cleanly released so user is not locked
        assert!(state.active_run.lock().unwrap().is_none());
        // No Failed event was emitted by the supervisor!
        let recorded = events.lock().unwrap();
        assert!(recorded.iter().all(|e| e.step != WorkflowState::Failed));
    }

    #[tokio::test]
    async fn supervisor_emits_failed_event_on_genuine_error() {
        let state = Arc::new(OrchestratorState::new());
        *state.active_run.lock().unwrap() = Some(active_run("error-run"));
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let on_event: super::super::engine::EventCallback = Arc::new(move |ev| {
            events_clone.lock().unwrap().push(ev);
            Ok(())
        });
        let on_log: super::super::engine::LogCallback = Arc::new(|_| {});

        supervise_run(
            async {
                // Workflow fails with an error
                Err("Unrelated fatal process error".to_string())
            },
            "error-run".into(),
            Arc::clone(&state),
            None,
            on_event,
            on_log,
        )
        .await;

        assert!(state.active_run.lock().unwrap().is_none());
        let recorded = events.lock().unwrap();
        assert!(recorded.iter().any(|e| e.step == WorkflowState::Failed));
        let failed_event = recorded.iter().find(|e| e.step == WorkflowState::Failed).unwrap();
        assert!(failed_event.message.contains("Unrelated fatal process error"));
    }

    #[test]
    fn test_list_interrupted_runs_and_get_detail_integration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let runs_dir = temp_dir.path().join("runs");
        let jm = Arc::new(super::super::recovery::JournalManager::new(runs_dir));
        let state = OrchestratorState::with_journal_manager(jm.clone());

        // Empty at start
        let list_empty = list_interrupted_runs_impl(&state).unwrap();
        assert!(list_empty.is_empty());

        // Write a test active journal
        let snapshot = RunConfigurationSnapshot {
            project_path: temp_dir.path().to_string_lossy().to_string(),
            assignments: std::collections::HashMap::new(),
            iteration_limits: LoopIterationLimits::default(),
            validation_gates: Vec::new(),
            budget_limits: std::collections::HashMap::new(),
            created_at_unix: 1000,
            lean_antigravity_mode: true,
            plan_workspace: PlanWorkspaceConfig::default(),
            mcp_servers: std::collections::HashMap::new(),
        };
        let journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: "run-test-1".to_string(),
            workflow_type: "plan_only".to_string(),
            canonical_project_path: snapshot.project_path.clone(),
            task_prompt: Some("test prompt".to_string()),
            approved_plan: None,
            snapshot,
            current_state: WorkflowState::BuildingContext,
            last_successful_state: None,
            stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters::default(),
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 1,
            resume_generation: 0,
            checkpoint_manifest_ref: None,
            checkpoint_digest: None,
            last_shelve_backup_id: None,
            last_shelve_backup_digest: None,
            status: super::super::recovery::RunRecoveryStatus::Active,
            created_at_unix: 1000,
            updated_at_unix: 1000,
        };
        jm.write_journal(&journal).unwrap();

        // Enumerate: classified as Interrupted (Phase A reports is_resumable: false)
        let list = list_interrupted_runs_impl(&state).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].run_id, "run-test-1");
        assert_eq!(list[0].status, super::super::recovery::RunRecoveryStatus::Interrupted);
        assert!(!list[0].is_resumable, "Phase A reports is_resumable=false");

        // Get detail
        let loaded = get_run_recovery_detail_impl(&state, "run-test-1").unwrap();
        assert_eq!(loaded.run_id, "run-test-1");
        assert_eq!(loaded.task_prompt.as_deref(), Some("test prompt"));

        // Nonexistent run
        assert!(get_run_recovery_detail_impl(&state, "no-such-run").is_err());
    }

    #[test]
    fn test_rejected_start_creates_no_orphan_journal() {
        let temp = tempfile::tempdir().unwrap();
        let jm = Arc::new(super::super::recovery::JournalManager::new(temp.path().join("runs")));
        let state = Arc::new(OrchestratorState::with_journal_manager(jm.clone()));
        let runtime = CountingRunStartRuntime::default();

        let project = tempfile::tempdir().unwrap();
        let mut snapshot = snapshot_for_overrides();
        snapshot.project_path = project.path().to_string_lossy().into_owned();

        // 1. First run starts successfully
        let res1 = start_orchestrator_run_impl(
            runtime.clone(),
            Arc::clone(&state),
            snapshot.clone(),
            "first task".to_string(),
            "full_loop".to_string(),
            None,
            Some(PlanArchiveOptions {
                directory: ".plan".to_string(),
            }),
        );
        assert!(res1.is_ok(), "res1 failed: {:?}", res1.err());
        let run_id_1 = res1.unwrap().run_id;

        // 2. Second run start attempt while run 1 is active must be rejected
        let res2 = start_orchestrator_run_impl(
            runtime.clone(),
            Arc::clone(&state),
            snapshot.clone(),
            "second task".to_string(),
            "full_loop".to_string(),
            None,
            Some(PlanArchiveOptions {
                directory: ".plan".to_string(),
            }),
        );
        assert!(res2.is_err());
        assert!(res2.unwrap_err().contains("already active"));

        // 3. Verify on disk: ONLY run 1 exists; NO second journal was created
        let list = jm.list_journals().unwrap();
        assert_eq!(list.len(), 1, "Only the active run journal must exist");
        assert_eq!(list[0].run_id, run_id_1);
    }

    #[test]
    fn test_initial_persistence_failure_releases_active_run_reservation() {
        let temp = tempfile::tempdir().unwrap();
        let jm = Arc::new(super::super::recovery::JournalManager::new(temp.path().join("runs")));
        let state = Arc::new(OrchestratorState::with_journal_manager(jm.clone()));
        let runtime = CountingRunStartRuntime::default();

        let project = tempfile::tempdir().unwrap();
        let mut snapshot = snapshot_for_overrides();
        snapshot.project_path = project.path().to_string_lossy().into_owned();

        // Inject initial permission failure
        *jm.permissions_test_hook.lock().unwrap() = Some(Arc::new(|_| {
            Err("Injected initial write failure".to_string())
        }));

        let res = start_orchestrator_run_impl(
            runtime.clone(),
            Arc::clone(&state),
            snapshot.clone(),
            "task".to_string(),
            "full_loop".to_string(),
            None,
            Some(PlanArchiveOptions {
                directory: ".plan".to_string(),
            }),
        );
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Injected initial write failure"));

        // Active run reservation must be released!
        assert!(state.active_run.lock().unwrap().is_none());

        // Clear injection hook and retry: should succeed
        *jm.permissions_test_hook.lock().unwrap() = None;
        let res2 = start_orchestrator_run_impl(
            runtime,
            Arc::clone(&state),
            snapshot,
            "task".to_string(),
            "full_loop".to_string(),
            None,
            Some(PlanArchiveOptions {
                directory: ".plan".to_string(),
            }),
        );
        assert!(res2.is_ok(), "res2 failed: {:?}", res2.err());
    }

    #[derive(Clone, Default)]
    struct RecordingRunStartRuntime {
        pub steps: Arc<Mutex<Vec<StepProgressEvent>>>,
        pub logs: Arc<Mutex<Vec<super::super::types::RunLogEvent>>>,
    }

    impl RunStartRuntime for RecordingRunStartRuntime {
        fn emit_step(&self, event: StepProgressEvent) {
            self.steps.lock().unwrap().push(event);
        }

        fn emit_log(&self, event: super::super::types::RunLogEvent) {
            self.logs.lock().unwrap().push(event);
        }

        fn spawn(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
            tokio::spawn(task);
        }
    }

    #[derive(Clone)]
    struct PollingRunStartRuntime {
        engine: Arc<Mutex<Option<OrchestratorEngine>>>,
        state: Arc<OrchestratorState>,
        journals: Arc<super::super::recovery::JournalManager>,
        steps: Arc<Mutex<Vec<StepProgressEvent>>>,
        logs: Arc<Mutex<Vec<super::super::types::RunLogEvent>>>,
        journal_at_step: Arc<Mutex<Vec<(WorkflowState, Option<WorkflowState>, u32, u32, u32)>>>,
        human_gate_send_errors: Arc<Mutex<Vec<String>>>,
        task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
        cancel_on_implementation: bool,
    }

    impl PollingRunStartRuntime {
        fn new(
            engine: OrchestratorEngine,
            state: Arc<OrchestratorState>,
            journals: Arc<super::super::recovery::JournalManager>,
            cancel_on_implementation: bool,
        ) -> Self {
            Self {
                engine: Arc::new(Mutex::new(Some(engine))),
                state,
                journals,
                steps: Arc::new(Mutex::new(Vec::new())),
                logs: Arc::new(Mutex::new(Vec::new())),
                journal_at_step: Arc::new(Mutex::new(Vec::new())),
                human_gate_send_errors: Arc::new(Mutex::new(Vec::new())),
                task: Arc::new(Mutex::new(None)),
                cancel_on_implementation,
            }
        }

        async fn join(&self) {
            let task = self.task.lock().unwrap().take().expect("workflow task was spawned");
            task.await.expect("workflow supervisor task should not panic");
        }
    }

    impl RunStartRuntime for PollingRunStartRuntime {
        fn emit_step(&self, event: StepProgressEvent) {
            if let Ok(journal) = self.journals.read_journal(&event.run_id) {
                self.journal_at_step.lock().unwrap().push((
                    event.step,
                    journal.last_successful_state,
                    journal.iteration_counters.plan_review_count,
                    journal.iteration_counters.fix_count,
                    journal.iteration_counters.code_review_count,
                ));
            }
            self.steps.lock().unwrap().push(event.clone());

            if self.cancel_on_implementation && event.step == WorkflowState::Implementation {
                if let Some(active) = self.state.active_run.lock().unwrap().as_ref() {
                    active.cancel_token.cancel();
                }
            }
            if event.step == WorkflowState::HumanGate {
                if let Some(active) = self.state.active_run.lock().unwrap().as_ref() {
                    if let Err(error) = active.human_gate_tx.try_send(HumanGateDecision::Approve) {
                        self.human_gate_send_errors.lock().unwrap().push(error.to_string());
                    }
                } else {
                    self.human_gate_send_errors.lock().unwrap().push("active run missing".into());
                }
            }
        }

        fn emit_log(&self, event: super::super::types::RunLogEvent) {
            self.logs.lock().unwrap().push(event);
        }

        fn spawn(&self, task: Pin<Box<dyn Future<Output = ()> + Send + 'static>>) {
            *self.task.lock().unwrap() = Some(tokio::spawn(task));
        }

        fn create_engine(&self) -> OrchestratorEngine {
            self.engine.lock().unwrap().take().expect("engine is created once per runtime")
        }
    }

    fn scripted_worker_submission_hook(
        envelopes: Arc<Mutex<Vec<super::super::types::OrchestratorTaskEnvelope>>>,
    ) -> Arc<dyn Fn(super::super::types::OrchestratorTaskEnvelope) -> Result<super::super::types::SubmitTaskRequest, String> + Send + Sync> {
        Arc::new(move |envelope| {
            envelopes.lock().unwrap().push(envelope.clone());
            Ok(super::super::types::SubmitTaskRequest {
                run_id: envelope.run_id,
                task_id: envelope.task_id,
                epoch: envelope.epoch,
                idempotency_key: Some("resume-test-submission".into()),
                status: "success".into(),
                summary: "scripted resumed Worker completed task".into(),
                modified_files: Vec::new(),
            })
        })
    }

    #[tokio::test]
    async fn test_progress_persistence_failure_aborts_workflow_and_preserves_disk_revision() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let jm = Arc::new(super::super::recovery::JournalManager::new(runs_dir));
        let state = Arc::new(OrchestratorState::with_journal_manager(jm.clone()));
        let runtime = RecordingRunStartRuntime::default();

        let project = tempfile::tempdir().unwrap();
        let mut snapshot = snapshot_for_overrides();
        snapshot.project_path = project.path().to_string_lossy().into_owned();

        let start_res = start_orchestrator_run_impl(
            runtime.clone(),
            Arc::clone(&state),
            snapshot,
            "test task".to_string(),
            "plan_only".to_string(),
            None,
            Some(PlanArchiveOptions {
                directory: ".plan".to_string(),
            }),
        );
        assert!(start_res.is_ok());
        let run_id = start_res.unwrap().run_id;

        let initial_journal = jm.read_journal(&run_id).unwrap();
        assert_eq!(initial_journal.revision, 1);

        // Inject disk persistence failure for all subsequent writes
        *jm.permissions_test_hook.lock().unwrap() = Some(Arc::new(|_| {
            Err("Simulated progress write disk error".to_string())
        }));

        let cancel_token = {
            let active_lock = state.active_run.lock().unwrap();
            active_lock.as_ref().unwrap().cancel_token.clone()
        };

        // Wait for cancel token to be tripped by workflow progress persistence failure
        let wait_res = tokio::time::timeout(std::time::Duration::from_millis(2000), async {
            while !cancel_token.is_cancelled() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(wait_res.is_ok(), "Cancel token must be cancelled on persistence failure");
        assert!(cancel_token.is_cancelled());

        // Verify disk journal remains at revision 1 and is not corrupted
        let disk_journal_after = jm.read_journal(&run_id).unwrap();
        assert_eq!(disk_journal_after.revision, 1);

        // Verify failure log was emitted
        let logs = runtime.logs.lock().unwrap();
        assert!(logs.iter().any(|l| l.message.contains("[Recovery] Journal persistence failed: Simulated progress write disk error")));
    }

    #[tokio::test]
    async fn test_terminal_persistence_failure_is_logged() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let jm = Arc::new(super::super::recovery::JournalManager::new(runs_dir));
        let state = Arc::new(OrchestratorState::with_journal_manager(jm.clone()));

        let snapshot = snapshot_for_overrides();
        let journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: "term-fail-run".to_string(),
            workflow_type: "plan_only".to_string(),
            canonical_project_path: snapshot.project_path.clone(),
            task_prompt: Some("prompt".to_string()),
            approved_plan: None,
            snapshot,
            current_state: WorkflowState::BuildingContext,
            last_successful_state: None,
            stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters::default(),
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 1,
            resume_generation: 0,
            checkpoint_manifest_ref: None,
            checkpoint_digest: None,
            last_shelve_backup_id: None,
            last_shelve_backup_digest: None,
            status: super::super::recovery::RunRecoveryStatus::Active,
            created_at_unix: 1000,
            updated_at_unix: 1000,
        };
        jm.write_journal(&journal).unwrap();
        let journal_state = Arc::new(Mutex::new(journal));

        let active = active_run("term-fail-run");
        *state.active_run.lock().unwrap() = Some(active);

        let logs = Arc::new(Mutex::new(Vec::new()));
        let logs_clone = logs.clone();
        let on_log: super::super::engine::LogCallback = Arc::new(move |log_evt| {
            logs_clone.lock().unwrap().push(log_evt);
        });
        let on_event: super::super::engine::EventCallback = Arc::new(|_| Ok(()));

        // Inject write failure hook
        *jm.permissions_test_hook.lock().unwrap() = Some(Arc::new(|_| {
            Err("Terminal disk write error".to_string())
        }));

        supervise_run(
            async {
                Err("Workflow error".to_string())
            },
            "term-fail-run".into(),
            Arc::clone(&state),
            Some(journal_state),
            on_event,
            on_log,
        )
        .await;

        let recorded_logs = logs.lock().unwrap();
        assert!(recorded_logs.iter().any(|l| l.message.contains("[Recovery] Failed to persist terminal journal state: Terminal disk write error")));
    }

    #[test]
    fn test_direct_mcp_audit_persistence_failure_is_returned_without_advancing_journal() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let manager = Arc::new(super::super::recovery::JournalManager::new(runs_dir));
        let snapshot = snapshot_for_overrides();
        let journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: "audit-persist-fail".to_string(),
            workflow_type: "plan_only".to_string(),
            canonical_project_path: snapshot.project_path.clone(),
            task_prompt: Some("task".to_string()),
            approved_plan: None,
            snapshot,
            current_state: WorkflowState::PlanGeneration,
            last_successful_state: None,
            stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters::default(),
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 1,
            resume_generation: 0,
            checkpoint_manifest_ref: None,
            checkpoint_digest: None,
            last_shelve_backup_id: None,
            last_shelve_backup_digest: None,
            status: super::super::recovery::RunRecoveryStatus::Active,
            created_at_unix: 1000,
            updated_at_unix: 1000,
        };
        manager.write_journal(&journal).unwrap();
        let journal_state = Arc::new(Mutex::new(journal.clone()));
        let callback = make_direct_mcp_audit_callback(journal_state, Some(manager.clone()));
        let audit = super::super::adapters::direct_mcp::DirectMcpAuditRecord {
            invocation_id: "dmcp-audit-success".to_string(),
            role: "planner".to_string(),
            profile_id: "mcp-planner".to_string(),
            provider_id: Some("deepseek".to_string()),
            model_id: Some("model-x".to_string()),
            server_id: "server".to_string(),
            tool_name: "plan".to_string(),
            duration_ms: 10,
            pages_discovered: 1,
            tools_discovered: 1,
            result_code: "SUCCESS".to_string(),
            cleanup_outcome: "cleaned_up_success".to_string(),
            error_message: Some("provider controlled diagnostic secret".to_string()),
        };
        callback(audit.clone()).unwrap();
        let persisted = manager.read_journal("audit-persist-fail").unwrap();
        assert_eq!(persisted.revision, 2);
        assert_eq!(persisted.direct_mcp_invocations.len(), 1);
        assert_eq!(persisted.direct_mcp_invocations[0].result_code, "SUCCESS");
        let serialized_audit = serde_json::to_string(&persisted.direct_mcp_invocations[0]).unwrap();
        assert!(!serialized_audit.contains("provider controlled diagnostic secret"));

        *manager.permissions_test_hook.lock().unwrap() = Some(Arc::new(|_| {
            Err("injected audit persistence failure".to_string())
        }));
        let audit = super::super::adapters::direct_mcp::DirectMcpAuditRecord {
            invocation_id: "dmcp-audit-failed".to_string(),
            role: "planner".to_string(),
            profile_id: "mcp-planner".to_string(),
            provider_id: None,
            model_id: None,
            server_id: "server".to_string(),
            tool_name: "plan".to_string(),
            duration_ms: 10,
            pages_discovered: 1,
            tools_discovered: 1,
            result_code: "SUCCESS".to_string(),
            cleanup_outcome: "cleaned_up_success".to_string(),
            error_message: None,
        };
        let error = callback(audit).expect_err("journal persistence failure must stop progression");
        assert!(error.contains("injected audit persistence failure"));
        let disk = manager.read_journal("audit-persist-fail").unwrap();
        assert_eq!(disk.revision, 2);
        assert_eq!(disk.direct_mcp_invocations.len(), 1);
    }

    fn scripted_output_helper(content: &str) -> super::super::adapters::AdapterExecutionOutput {
        super::super::adapters::AdapterExecutionOutput {
            content: content.to_string(),
            raw_json: None,
            tokens_used: None,
            model_used: "scripted".to_string(),
            duration_ms: 0,
        }
    }

    /// Reads the Direct MCP wiring out of the engine's own `Debug` rendering, so
    /// the assertion tracks the production field rather than a test-only copy.
    fn engine_reports_direct_mcp_audit_wiring(engine: &OrchestratorEngine) -> bool {
        format!("{engine:?}").contains("direct_mcp_audit_callback: Some(\"configured\")")
    }

    #[test]
    fn test_build_run_engine_attaches_the_direct_mcp_audit_wiring() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(
            super::super::recovery::JournalManager::new(temp.path().join("runs")),
        );
        let runtime = CountingRunStartRuntime::default();

        let audited = build_run_engine(&runtime, Arc::new(Mutex::new(sample_journal_for_engine_wiring())), Some(manager.clone()));
        assert!(
            engine_reports_direct_mcp_audit_wiring(&audited),
            "the audited helper must attach the Direct MCP audit callback"
        );

        // The unwired engine is the defect this helper exists to prevent: a run
        // built this way cannot record an MCP invocation at all.
        let unwired = runtime.create_engine();
        assert!(!engine_reports_direct_mcp_audit_wiring(&unwired));
    }

    fn sample_journal_for_engine_wiring() -> super::super::recovery::RunJournal {
        let snapshot = snapshot_for_overrides();
        super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: "engine-wiring".to_string(),
            workflow_type: "plan_only".to_string(),
            canonical_project_path: snapshot.project_path.clone(),
            task_prompt: Some("task".to_string()),
            approved_plan: None,
            snapshot,
            current_state: WorkflowState::PlanGeneration,
            last_successful_state: None,
            stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters::default(),
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 1,
            resume_generation: 0,
            checkpoint_manifest_ref: None,
            checkpoint_digest: None,
            last_shelve_backup_id: None,
            last_shelve_backup_digest: None,
            status: super::super::recovery::RunRecoveryStatus::Active,
            created_at_unix: 1000,
            updated_at_unix: 1000,
        }
    }

    #[test]
    fn test_fresh_run_start_builds_its_engine_through_the_audited_helper() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(
            super::super::recovery::JournalManager::new(temp.path().join("runs")),
        );
        let state = Arc::new(OrchestratorState::with_journal_manager(manager));
        let runtime = CountingRunStartRuntime::default();
        let project = tempfile::tempdir().unwrap();
        let mut snapshot = snapshot_for_overrides();
        snapshot.project_path = project.path().to_string_lossy().into_owned();

        let builds_before = audited_engine_builds();
        let started = start_orchestrator_run_impl(
            runtime.clone(),
            Arc::clone(&state),
            snapshot,
            "test task".to_string(),
            "plan_only".to_string(),
            None,
            Some(PlanArchiveOptions {
                directory: ".plan".to_string(),
            }),
        )
        .expect("a valid start must reach engine construction");
        assert!(!started.run_id.is_empty());
        assert_eq!(
            audited_engine_builds(),
            builds_before + 1,
            "a fresh run start must build exactly one audited engine"
        );
    }

    #[tokio::test]
    async fn test_production_stage_and_counter_wiring_with_real_workflow_events() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let jm = Arc::new(super::super::recovery::JournalManager::new(runs_dir));
        let state = Arc::new(OrchestratorState::with_journal_manager(jm.clone()));

        let project = tempfile::tempdir().unwrap();
        assert!(std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(project.path())
            .status()
            .unwrap()
            .success());
        for (key, value) in [("user.name", "Test"), ("user.email", "test@example.invalid")] {
            assert!(std::process::Command::new("git")
                .args(["config", key, value])
                .current_dir(project.path())
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(project.path().join("tracked.txt"), "baseline").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(project.path())
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-qm", "initial"])
            .current_dir(project.path())
            .status()
            .unwrap()
            .success());
        std::fs::write(project.path().join("package.json"), br#"{"name":"test","version":"0.24.0"}"#).unwrap();
        let mut snapshot = snapshot_for_overrides();
        snapshot.project_path = project.path().to_string_lossy().into_owned();
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        let run_id = "test-prod-events-run".to_string();

        let initial_journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: run_id.clone(),
            workflow_type: "plan_only".to_string(),
            canonical_project_path: snapshot.project_path.clone(),
            task_prompt: Some("Task requiring revision".to_string()),
            approved_plan: None,
            snapshot: snapshot.clone(),
            current_state: WorkflowState::BuildingContext,
            last_successful_state: None,
            stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters::default(),
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 1,
            resume_generation: 0,
            checkpoint_manifest_ref: None,
            checkpoint_digest: None,
            last_shelve_backup_id: None,
            last_shelve_backup_digest: None,
            status: super::super::recovery::RunRecoveryStatus::Active,
            created_at_unix: 1000,
            updated_at_unix: 1000,
        };
        jm.write_journal(&initial_journal).unwrap();
        let journal_state = Arc::new(Mutex::new(initial_journal));

        let engine = OrchestratorEngine::with_scripted_adapters(
            vec![
                (AgentRole::Planner, scripted_output_helper("Plan Draft 1")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output_helper(r#"{"verdict":"changes_required","summary":"needs more tests","findings":[{"id":"F-1","severity":"medium","file":"src/lib.rs","line":1,"issue":"missing tests","recommendation":"add tests","is_blocking":true}]}"#),
                ),
                (AgentRole::Planner, scripted_output_helper("Plan Draft 2 with tests")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output_helper(r#"{"verdict":"approved","summary":"approved plan","findings":[]}"#),
                ),
            ],
            vec![],
        );

        let cancel_token = CancellationToken::new();
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(4);
        let (_blocking_tx, blocking_rx) = mpsc::channel(4);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(4);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(4);

        let on_event = make_run_event_callback(
            RecordingRunStartRuntime::default(),
            Arc::new(Mutex::new(StepProgressEvent::default())),
            journal_state.clone(),
            Some(jm.clone()),
            Some(Arc::new(super::super::checkpoint::CheckpointStore::new(
                jm.runs_dir().to_path_buf(),
            ))),
            cancel_token.clone(),
        );

        let on_log: super::super::engine::LogCallback = Arc::new(|_| {});

        let r_id = run_id.clone();
        let wf_cancel = cancel_token.clone();
        let wf_on_event = on_event.clone();
        let wf_on_log = on_log.clone();

        let workflow = async move {
            engine
                .run_workflow(
                    r_id,
                    snapshot,
                    "Task prompt".to_string(),
                    "plan_only".to_string(),
                    None,
                    vec![],
                    pause_rx,
                    wf_cancel,
                    clarification_rx,
                    blocking_rx,
                    human_gate_rx,
                    worker_reclaim_rx,
                    wf_on_event,
                    wf_on_log,
                    None,
                )
                .await
        };

        supervise_run(
            workflow,
            run_id.clone(),
            Arc::clone(&state),
            Some(journal_state.clone()),
            on_event,
            on_log,
        )
        .await;

        let final_journal = jm.read_journal(&run_id).unwrap();
        assert_eq!(final_journal.status, super::super::recovery::RunRecoveryStatus::Complete);
        assert_eq!(final_journal.current_state, WorkflowState::Complete);
        assert_eq!(final_journal.last_successful_state, Some(WorkflowState::PlanReview));
        assert_eq!(final_journal.approved_plan.as_deref(), Some("Plan Draft 2 with tests"),
            "the recovery plan is persisted from the production stage event");
        assert_eq!(final_journal.iteration_counters.plan_review_count, 2);
        assert_eq!(final_journal.iteration_counters.fix_count, 0);
        assert_eq!(final_journal.iteration_counters.code_review_count, 0);
        assert_eq!(final_journal.iteration_counters.antigravity_dispatches, None);
        assert!(final_journal.revision > 1);
        let reference = final_journal.checkpoint_manifest_ref.as_deref().unwrap();
        let digest = final_journal.checkpoint_digest.as_deref().unwrap();
        let manifest = super::super::checkpoint::CheckpointStore::new(
            jm.runs_dir().to_path_buf(),
        )
        .load(&run_id, reference, digest)
        .unwrap();
        assert_eq!(manifest.kind, super::super::recovery::CheckpointKind::StageCheckpoint);
        assert_eq!(manifest.stage, WorkflowState::PlanReview);
    }

    #[tokio::test]
    async fn test_mailbox_progress_relay_persistence_failure_propagates_and_aborts_workflow() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let jm = Arc::new(super::super::recovery::JournalManager::new(runs_dir));
        let runtime = RecordingRunStartRuntime::default();
        let run_id = "relay-persistence-failure".to_string();
        let snapshot = snapshot_for_overrides();
        let initial_journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: run_id.clone(),
            workflow_type: "human_gated_loop".to_string(),
            canonical_project_path: snapshot.project_path.clone(),
            task_prompt: Some("test".to_string()),
            approved_plan: None,
            snapshot,
            current_state: WorkflowState::PlanDraft,
            last_successful_state: None,
            stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters::default(),
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 1,
            resume_generation: 0,
            checkpoint_manifest_ref: None,
            checkpoint_digest: None,
            last_shelve_backup_id: None,
            last_shelve_backup_digest: None,
            status: super::super::recovery::RunRecoveryStatus::Active,
            created_at_unix: 1000,
            updated_at_unix: 1000,
        };
        jm.write_journal(&initial_journal).unwrap();
        let journal_state = Arc::new(Mutex::new(initial_journal));
        *jm.permissions_test_hook.lock().unwrap() = Some(Arc::new(|_| {
            Err("Simulated progress write disk error via mailbox relay".to_string())
        }));
        let cancel_token = CancellationToken::new();
        let relay_error = Arc::new(std::sync::Mutex::new(None));
        let current_step = Arc::new(Mutex::new(StepProgressEvent::default()));
        let on_event = make_run_event_callback(
            runtime.clone(),
            current_step,
            journal_state.clone(),
            Some(jm.clone()),
            None,
            cancel_token.clone(),
        );
        let (progress_tx, progress_rx) = mpsc::channel(1);
        let (submit_tx, _submit_rx) = mpsc::channel(1);
        let (unused_progress_tx, _unused_progress_rx) = mpsc::channel(1);
        let mailbox_state = super::super::mailbox::MailboxState {
            inner: Arc::new(tokio::sync::Mutex::new(super::super::mailbox::MailboxInner {
                run_id: run_id.clone(),
                project_path: "unused".to_string(),
                token: "unused".to_string(),
                epoch: 0,
                is_claimed: false,
                last_progress_at: None,
                lease_timeout_duration: std::time::Duration::from_secs(30),
                active_task: None,
                current_state: WorkflowState::PlanDraft,
                task_notify: Arc::new(tokio::sync::Notify::new()),
                submit_tx,
                progress_tx: unused_progress_tx,
                total_dispatches: 1,
                task_dispatches: Default::default(),
                max_dispatches_per_task: 2,
                max_dispatches_per_run: 6,
                budget_exhausted: false,
                budget_exhausted_details: None,
                dispatch_persistence: None,
            })),
        };
        let relay = super::super::engine::spawn_mailbox_progress_relay(
            progress_rx,
            run_id.clone(),
            mailbox_state,
            on_event,
            relay_error.clone(),
            cancel_token.clone(),
        );
        progress_tx
            .send(super::super::types::ReportProgressRequest {
                run_id: run_id.clone(),
                task_id: "task-plan".to_string(),
                epoch: 1,
                message: "Working on plan".to_string(),
                percent: Some(50),
            })
            .await
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !cancel_token.is_cancelled() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("relay must cancel the workflow after persistence failure");
        relay.await.unwrap();
        assert_eq!(
            relay_error.lock().unwrap().as_deref(),
            Some("[Recovery] Journal persistence failed: Simulated progress write disk error via mailbox relay")
        );
        let (_pause_tx, mut pause_rx) = watch::channel(RunControlState::Running);
        let control_error = super::super::engine::OrchestratorEngine::new()
            .check_run_control_with_relay(&mut pause_rx, &cancel_token, &relay_error)
            .await
            .unwrap_err();
        assert!(control_error.contains("Simulated progress write disk error via mailbox relay"));

        let disk_journal_after = jm.read_journal(&run_id).unwrap();
        assert_eq!(disk_journal_after.revision, 1);
        assert_eq!(disk_journal_after.status, super::super::recovery::RunRecoveryStatus::Active);
        let logs = runtime.logs.lock().unwrap();
        assert!(
            logs.iter().any(|l| l.message.contains("Simulated progress write disk error via mailbox relay")),
            "Expected error in logs, got: {:?}",
            logs.iter().map(|l| &l.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_concurrent_progress_events_serialize_journal_revisions() {
        let temp = tempfile::tempdir().unwrap();
        let jm = Arc::new(super::super::recovery::JournalManager::new(
            temp.path().join("runs"),
        ));
        let snapshot = snapshot_for_overrides();
        let journal = super::super::recovery::RunJournal {
            schema_version: super::super::recovery::JOURNAL_SCHEMA_VERSION,
            run_id: "concurrent-journal-events".to_string(),
            workflow_type: "plan_only".to_string(),
            canonical_project_path: snapshot.project_path.clone(),
            task_prompt: Some("test".to_string()),
            approved_plan: None,
            snapshot,
            current_state: WorkflowState::BuildingContext,
            last_successful_state: None,
            stage_entry_info: None,
            iteration_counters: super::super::recovery::RunIterationCounters::default(),
            direct_mcp_invocations: Vec::new(),
            plan_convergence_audit: Vec::new(),
            plan_convergence_confirmation_intent: None,
            revision: 1,
            resume_generation: 0,
            checkpoint_manifest_ref: None,
            checkpoint_digest: None,
            last_shelve_backup_id: None,
            last_shelve_backup_digest: None,
            status: super::super::recovery::RunRecoveryStatus::Active,
            created_at_unix: 1000,
            updated_at_unix: 1000,
        };
        jm.write_journal(&journal).unwrap();
        let journal_state = Arc::new(Mutex::new(journal));
        let callback = make_run_event_callback(
            RecordingRunStartRuntime::default(),
            Arc::new(Mutex::new(StepProgressEvent::default())),
            journal_state,
            Some(jm.clone()),
            None,
            CancellationToken::new(),
        );
        let barrier = Arc::new(std::sync::Barrier::new(9));
        let mut threads = Vec::new();
        for index in 0..8 {
            let callback = callback.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                callback(StepProgressEvent {
                    run_id: "concurrent-journal-events".to_string(),
                    step: WorkflowState::PlanDraft,
                    message: format!("progress {index}"),
                    plan_review_count: Some(index),
                    ..Default::default()
                })
            }));
        }
        barrier.wait();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }

        let persisted = jm.read_journal("concurrent-journal-events").unwrap();
        assert_eq!(persisted.revision, 9);
        assert_eq!(persisted.current_state, WorkflowState::PlanDraft);
    }

    // -----------------------------------------------------------------------
    // Plan convergence — reconstruction and rebuilt confirmation (§19.5)
    // -----------------------------------------------------------------------

    fn new_primary_plan_proposal_json() -> String {
        r##"{
  "schemaVersion": 1,
  "operation": {
    "kind": "new_primary_plan",
    "proposedRevision": 2,
    "title": "Converged Revision 2",
    "initialContent": "Created by human confirmation."
  }
}"##
        .to_string()
    }

    /// Builds an APPROVE verdict bound to the runtime candidate carried in the
    /// reviewer prompt, which is only known once the driver has stamped it.
    fn approve_verdict_for_reviewer_prompt(prompt: &str) -> String {
        let mut seq = String::new();
        let mut candidate_id = String::new();
        let mut op_digest = String::new();
        let mut ctx_digest = String::new();
        for line in prompt.lines() {
            if let Some(rest) = line.strip_prefix("Sequence: ") {
                seq = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("Candidate ID: ") {
                candidate_id = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("Operation Payload Digest: ") {
                op_digest = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("Plan Context Digest: ") {
                ctx_digest = rest.trim().to_string();
            }
        }
        format!(
            r#"{{
  "schemaVersion": 1,
  "decision": "APPROVE",
  "reviewedCandidate": {{
    "sequence": {seq},
    "candidateId": "{candidate_id}",
    "operationPayloadDigest": "{op_digest}",
    "planContextDigest": "{ctx_digest}"
  }},
  "summary": "Approved for human confirmation.",
  "findings": []
}}"#
        )
    }

    struct NewPrimaryWaitingHarness {
        _temp: tempfile::TempDir,
        project: PathBuf,
        manager: Arc<super::super::recovery::JournalManager>,
        state: Arc<OrchestratorState>,
        runtime: PollingRunStartRuntime,
        run_id: String,
        payload: super::super::convergence::PlanConvergenceWaitingCandidate,
    }

    impl NewPrimaryWaitingHarness {
        fn waiting_revision(&self) -> u64 {
            self.payload.revision
        }

        fn journal(&self) -> super::super::recovery::RunJournal {
            self.manager.read_journal(&self.run_id).unwrap()
        }

        fn plan_entries(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.project.join(".plan"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
                .collect();
            names.sort();
            names
        }

        fn target_path(&self) -> PathBuf {
            self.project.join(".plan/V0.23.0-r2.md")
        }

        /// Number of terminal completion events the runtime has emitted.
        fn completion_events(&self) -> usize {
            self.runtime
                .steps
                .lock()
                .unwrap()
                .iter()
                .filter(|step| step.step == WorkflowState::Complete)
                .count()
        }
    }

    /// Drives the production convergence starter to the new-primary human gate
    /// using a prompt-driven scripted reviewer, and returns the typed waiting
    /// payload the UI would receive.
    async fn drive_convergence_to_new_primary_waiting() -> NewPrimaryWaitingHarness {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir_all(project.join(".plan")).unwrap();
        std::fs::write(project.join("package.json"), br#"{"version":"0.23.0"}"#).unwrap();
        std::fs::write(project.join(".plan/V0.23.0-r1.md"), b"# Plan\n").unwrap();

        let manager =
            Arc::new(super::super::recovery::JournalManager::new(temp.path().join("runs")));
        let state = Arc::new(OrchestratorState::with_journal_manager(manager.clone()));

        let responder: Arc<dyn Fn(AgentRole, &str) -> Option<String> + Send + Sync> =
            Arc::new(|role, prompt| match role {
                AgentRole::Planner => Some(new_primary_plan_proposal_json()),
                AgentRole::PlanReviewer => Some(approve_verdict_for_reviewer_prompt(prompt)),
                _ => None,
            });
        let engine = OrchestratorEngine::with_scripted_adapters(vec![], vec![])
            .with_role_responder(responder);
        let runtime = PollingRunStartRuntime::new(engine, state.clone(), manager.clone(), false);

        let mut snapshot = snapshot_for_overrides();
        snapshot.project_path = project.to_string_lossy().into();
        let mut planner = snapshot.assignments.values().next().unwrap().clone();
        planner.id = "planner".into();
        let mut reviewer = planner.clone();
        reviewer.id = "reviewer".into();
        snapshot.assignments.insert(AgentRole::Planner, planner);
        snapshot.assignments.insert(AgentRole::PlanReviewer, reviewer);

        let started = start_plan_convergence_run_impl(
            runtime.clone(),
            state.clone(),
            snapshot,
            "Task".into(),
            super::super::convergence::PlanConvergenceConfig { opt_in: true, total_timeout_secs: 60 },
            Some(super::super::plan_workspace::PlanWorkspaceConfig {
                version_sources: vec!["package.json".into()],
                ..Default::default()
            }),
        )
        .unwrap();
        runtime.join().await;

        let (reason, plan_text) = {
            let steps = runtime.steps.lock().unwrap();
            let waiting = steps
                .iter()
                .find(|step| step.step == WorkflowState::WaitingForUser)
                .expect("convergence must stop at a human gate");
            (waiting.waiting_reason.clone(), waiting.plan_text.clone())
        };
        assert_eq!(reason.as_deref(), Some("NEW_PRIMARY_PLAN_CONFIRMATION"));

        let payload: super::super::convergence::PlanConvergenceWaitingCandidate =
            serde_json::from_str(plan_text.as_deref().expect("waiting event must carry a candidate"))
                .expect("waiting payload must be typed JSON");
        assert_eq!(payload.intent, "new_primary");
        assert_eq!(payload.proposed_revision, Some(2));

        NewPrimaryWaitingHarness {
            _temp: temp,
            project,
            manager,
            state,
            runtime,
            run_id: started.run_id,
            payload,
        }
    }

    fn confirm(
        harness: &NewPrimaryWaitingHarness,
        revision: u64,
        candidate_id: &str,
        action: &str,
    ) -> Result<
        super::super::convergence::PlanConvergenceCommandResult,
        super::super::convergence::PlanConvergenceError,
    > {
        confirm_converged_new_plan_impl(
            harness.runtime.clone(),
            harness.state.clone(),
            harness.run_id.clone(),
            revision,
            candidate_id.to_string(),
            action.to_string(),
        )
    }

    #[tokio::test]
    async fn test_convergence_start_reaches_the_driver_and_creates_durable_journal_state() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let journal = harness.journal();

        assert_eq!(journal.workflow_type, "plan_convergence");
        assert_eq!(journal.status, super::super::recovery::RunRecoveryStatus::Active);
        assert_eq!(journal.current_state, WorkflowState::WaitingForUser);
        assert!(journal
            .plan_convergence_audit
            .iter()
            .any(|entry| entry.event_type == "candidate_created"));
        assert!(journal
            .plan_convergence_audit
            .iter()
            .any(|entry| entry.event_type == "review_attempt_reserved"));
        assert!(journal
            .plan_convergence_audit
            .iter()
            .any(|entry| entry.event_type == "human_gate_new_primary_plan"));
        // The mandatory first review consumed exactly one budget slot.
        assert_eq!(journal.iteration_counters.plan_review_count, 1);
        assert_eq!(
            super::super::recovery::replay_convergence_review_count(&journal).unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn test_review_attempt_reservation_survives_restart_and_is_never_refunded() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let journal = harness.journal();
        let before = super::super::recovery::replay_convergence_review_count(&journal).unwrap();

        // Re-read from disk: the reservation is durable, and a verdict that never
        // arrived cannot give the slot back.
        let reloaded = harness.journal();
        assert_eq!(reloaded.iteration_counters.plan_review_count, before);
        assert_eq!(
            super::super::recovery::replay_convergence_review_count(&reloaded).unwrap(),
            before
        );

        // The recovered count is what a restart would use as its budget base:
        // the reserved slot is spent even though no verdict arrived for it.
        assert_eq!(before, 1, "exactly the mandatory first review is reserved");
        assert!(
            reloaded.snapshot.iteration_limits.max_plan_review_iterations > before,
            "the run still has a budget, but the reserved slot is not refundable"
        );
    }

    #[tokio::test]
    async fn test_confirmation_intent_is_durable_before_publication() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        // The durable intent must exist before any byte reaches the workspace.
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            // 1 = confirmation intent, 2 = acceptance record.
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let error = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        *harness.manager.write_test_hook.lock().unwrap() = None;

        assert_eq!(error.code, "PC_PERSISTENCE_FAILED");
        assert!(
            harness.target_path().exists(),
            "the intent write succeeded, so publication may have proceeded"
        );
        let journal = harness.journal();
        let intent = journal
            .plan_convergence_confirmation_intent
            .as_ref()
            .expect("the intent must be durable");
        assert_eq!(intent.request_revision, revision);
        assert_eq!(intent.intent_record_revision, revision + 1);
    }

    #[tokio::test]
    async fn test_intent_persistence_failure_leaves_the_target_absent() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        *harness.manager.write_test_hook.lock().unwrap() =
            Some(Arc::new(|_| Err("injected intent write failure".into())));
        let error = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        *harness.manager.write_test_hook.lock().unwrap() = None;

        assert_eq!(error.code, "PC_PERSISTENCE_FAILED");
        assert!(
            !harness.target_path().exists(),
            "publication must not begin before the intent is durable"
        );
        assert!(harness.journal().plan_convergence_confirmation_intent.is_none());
    }

    #[tokio::test]
    async fn test_intent_write_advances_the_journal_to_the_recorded_revision() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();
        let journal_before = harness.journal();
        assert_eq!(journal_before.revision, revision);

        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let _ = confirm(&harness, revision, &harness.payload.candidate_id, "confirm");
        *harness.manager.write_test_hook.lock().unwrap() = None;

        let journal = harness.journal();
        let intent = journal.plan_convergence_confirmation_intent.as_ref().unwrap();
        assert_eq!(intent.request_revision, revision);
        assert_eq!(intent.intent_record_revision, revision + 1);
        assert_eq!(
            journal.revision,
            intent.intent_record_revision,
            "the durable write must leave the journal at the intent's own revision"
        );
    }

    #[tokio::test]
    async fn test_retry_after_final_journal_failure_reconciles_the_exact_intent() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let error = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        *harness.manager.write_test_hook.lock().unwrap() = None;
        assert_eq!(error.code, "PC_PERSISTENCE_FAILED");

        let published = std::fs::read(harness.target_path()).unwrap();
        assert!(!published.is_empty());

        // A changed request revision is stale and mutates nothing.
        let stale = confirm(&harness, revision + 5, &harness.payload.candidate_id, "confirm")
            .unwrap_err();
        assert_eq!(stale.code, "PC_STALE_REVISION");

        // The original request revision reconciles the durable intent.
        let created = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap();
        match created {
            super::super::convergence::PlanConvergenceCommandResult::Created {
                created_plan_id, created_path, ..
            } => {
                assert_eq!(created_plan_id, "V0.23.0-r2");
                assert_eq!(created_path, ".plan/V0.23.0-r2.md");
            }
            other => panic!("expected Created, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(harness.target_path()).unwrap(),
            published,
            "reconciliation must not rewrite the published bytes"
        );
        assert_eq!(
            harness.plan_entries().len(),
            2,
            "reconciliation must not create a duplicate plan"
        );
    }

    #[tokio::test]
    async fn test_retry_after_terminal_acceptance_is_idempotent() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();
        let created = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap();
        assert!(matches!(
            created,
            super::super::convergence::PlanConvergenceCommandResult::Created { .. }
        ));
        let published = std::fs::read(harness.target_path()).unwrap();
        let entries_after_first = harness.plan_entries();
        let journal_after_first = harness.journal();
        assert_eq!(journal_after_first.status, super::super::recovery::RunRecoveryStatus::Complete);
        let completions_after_first = harness.completion_events();
        assert_eq!(
            completions_after_first, 1,
            "the first acceptance emits exactly one completion event"
        );

        // Exact replay of terminal acceptance succeeds idempotently without mutating
        // journal revision, file bytes, or acceptance records.
        let replay = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap();
        match replay {
            super::super::convergence::PlanConvergenceCommandResult::Created {
                created_plan_id,
                created_path,
                file_digest,
                ..
            } => {
                assert_eq!(created_plan_id, "V0.23.0-r2");
                assert_eq!(created_path, ".plan/V0.23.0-r2.md");
                assert_eq!(file_digest, super::super::plan_workspace::sha256_bytes(&published));
            }
            other => panic!("expected Created on exact replay, got {other:?}"),
        }

        assert_eq!(std::fs::read(harness.target_path()).unwrap(), published);
        assert_eq!(harness.plan_entries(), entries_after_first);
        let journal_after_replay = harness.journal();
        assert_eq!(
            journal_after_replay.revision, journal_after_first.revision,
            "an idempotent replay must not advance the journal"
        );
        assert_eq!(
            journal_after_replay
                .plan_convergence_audit
                .iter()
                .filter(|entry| entry.event_type == "new_primary_plan_confirmed")
                .count(),
            1,
            "idempotent replay must not append duplicate acceptance audit records"
        );
        assert_eq!(
            harness.completion_events(),
            completions_after_first,
            "an idempotent replay must not emit another completion event"
        );

        // A tampered replay is not the accepted transaction: it must fail closed
        // without a completion event, a journal change, or a plan write.
        let completions_before_tamper = harness.completion_events();
        for (label, revision, candidate_id) in [
            ("revision", revision + 5, harness.payload.candidate_id.clone()),
            ("candidate", revision, "00000000-0000-0000-0000-000000000000".to_string()),
        ] {
            let error = confirm(&harness, revision, &candidate_id, "confirm").unwrap_err();
            assert!(
                matches!(
                    error.code.as_str(),
                    "PC_STALE_REVISION"
                        | "PC_CANDIDATE_MISMATCH"
                        | "PC_RUN_ALREADY_RESOLVED"
                        | "PC_CONFIRMATION_INTENT_MISMATCH"
                ),
                "{label}: unexpected failure {error:?}"
            );
            let _ = revision;
        }
        assert_eq!(
            harness.completion_events(),
            completions_before_tamper,
            "a rejected replay must not emit a completion event"
        );
        assert_eq!(std::fs::read(harness.target_path()).unwrap(), published);
        assert_eq!(harness.plan_entries(), entries_after_first);
        assert_eq!(harness.journal().revision, journal_after_first.revision);

        // Tampered replay requests against the resolved run fail closed.
        let tampered_candidate = confirm(&harness, revision, "other-candidate", "confirm").unwrap_err();
        assert_eq!(tampered_candidate.code, "PC_RUN_ALREADY_RESOLVED");

        let tampered_revision = confirm(&harness, revision + 1, &harness.payload.candidate_id, "confirm").unwrap_err();
        assert_eq!(tampered_revision.code, "PC_RUN_ALREADY_RESOLVED");

        let tampered_action = confirm(&harness, revision, &harness.payload.candidate_id, "reject").unwrap_err();
        assert_eq!(tampered_action.code, "PC_RUN_ALREADY_RESOLVED");
    }

    #[tokio::test]
    async fn test_publication_failure_before_final_journal_leaves_intent_and_bytes() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        // Fail the acceptance write after the file is on disk.
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let error = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        *harness.manager.write_test_hook.lock().unwrap() = None;

        assert_eq!(error.code, "PC_PERSISTENCE_FAILED");
        let journal = harness.journal();
        assert_eq!(
            journal.status,
            super::super::recovery::RunRecoveryStatus::Active,
            "the run must not claim durable acceptance"
        );
        assert_eq!(journal.current_state, WorkflowState::WaitingForUser);
        assert!(journal.plan_convergence_confirmation_intent.is_some());
        assert!(!journal
            .plan_convergence_audit
            .iter()
            .any(|entry| entry.event_type == "new_primary_plan_confirmed"));
        assert!(
            harness.target_path().exists(),
            "the published bytes must remain for exact retry reconciliation"
        );
        assert!(
            !harness
                .runtime
                .steps
                .lock()
                .unwrap()
                .iter()
                .any(|step| step.step == WorkflowState::Complete),
            "Complete must never be reported before the acceptance record is durable"
        );
    }

    #[tokio::test]
    async fn test_reject_before_publication_records_cancellation_without_a_target() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        let rejected = confirm(&harness, revision, &harness.payload.candidate_id, "reject").unwrap();
        assert!(matches!(
            rejected,
            super::super::convergence::PlanConvergenceCommandResult::Rejected { .. }
        ));
        assert!(!harness.target_path().exists());
        assert_eq!(
            harness.plan_entries().len(),
            1,
            "rejection must not create a plan file"
        );
        let journal = harness.journal();
        assert_eq!(journal.status, super::super::recovery::RunRecoveryStatus::Cancelled);
        assert_eq!(journal.current_state, WorkflowState::Cancelled);
    }

    #[tokio::test]
    async fn test_reject_after_publication_is_refused_and_reconciles_to_acceptance() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let _ = confirm(&harness, revision, &harness.payload.candidate_id, "confirm");
        *harness.manager.write_test_hook.lock().unwrap() = None;
        let published = std::fs::read(harness.target_path()).unwrap();

        // Reject after publication reconciles forward to durable acceptance.
        // It never marks the run Cancelled and never rewrites published bytes.
        let reconciled = confirm(&harness, revision, &harness.payload.candidate_id, "reject").unwrap();
        match reconciled {
            super::super::convergence::PlanConvergenceCommandResult::Created {
                created_plan_id,
                created_path,
                ..
            } => {
                assert_eq!(created_plan_id, "V0.23.0-r2");
                assert_eq!(created_path, ".plan/V0.23.0-r2.md");
            }
            other => panic!("expected Created forward-reconciliation, got {other:?}"),
        }

        assert_eq!(std::fs::read(harness.target_path()).unwrap(), published);
        let journal = harness.journal();
        assert_eq!(journal.status, super::super::recovery::RunRecoveryStatus::Complete);
        assert_eq!(journal.current_state, WorkflowState::Complete);
        assert!(journal
            .plan_convergence_audit
            .iter()
            .any(|entry| entry.event_type == "new_primary_plan_confirmed"));
        assert!(!journal
            .plan_convergence_audit
            .iter()
            .any(|entry| entry.event_type == "new_primary_plan_rejected"));
    }

    #[tokio::test]
    async fn test_reject_with_unpublished_intent_binds_request_revision_and_consumes_intent() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        // Fail publication at the workspace preview/write step after intent is durable.
        // We simulate this by recording intent then deleting target before reject.
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let _ = confirm(&harness, revision, &harness.payload.candidate_id, "confirm");
        *harness.manager.write_test_hook.lock().unwrap() = None;

        // Target was published in that attempt; remove it to simulate an unpublished intent.
        std::fs::remove_file(harness.target_path()).unwrap();
        assert!(!harness.target_path().exists());
        let journal_before = harness.journal();
        assert!(journal_before.plan_convergence_confirmation_intent.is_some());

        // Reject with stale/wrong expected_revision fails closed.
        let stale = confirm(&harness, revision + 99, &harness.payload.candidate_id, "reject").unwrap_err();
        assert_eq!(stale.code, "PC_STALE_REVISION");
        assert!(harness.journal().plan_convergence_confirmation_intent.is_some());

        // Reject with correct request_revision succeeds, cancels run, and consumes intent.
        let rejected = confirm(&harness, revision, &harness.payload.candidate_id, "reject").unwrap();
        assert!(matches!(
            rejected,
            super::super::convergence::PlanConvergenceCommandResult::Rejected { .. }
        ));

        let journal_after = harness.journal();
        assert_eq!(journal_after.status, super::super::recovery::RunRecoveryStatus::Cancelled);
        assert_eq!(journal_after.current_state, WorkflowState::Cancelled);
        assert!(
            journal_after.plan_convergence_confirmation_intent.is_none(),
            "the unpublished intent must be consumed atomically on rejection"
        );
        assert!(!harness.target_path().exists());

        // Subsequent confirm cannot reuse the consumed intent and fails as already resolved.
        let reuse = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        assert_eq!(reuse.code, "PC_RUN_ALREADY_RESOLVED");
    }

    #[tokio::test]
    async fn test_guarded_write_rejects_stale_effective_context_digest() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        // Mutate a supplemental plan file before confirm so the effective PlanContext digest drifts
        // while the candidate's target path and proposed revision remain identical.
        let supplemental = harness.project.join(".plan/V0.23.0-r1.md");
        std::fs::write(&supplemental, b"# Mutated supplemental plan content\n").unwrap();

        let entries_before = harness.plan_entries();
        let error = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        assert_eq!(error.code, "stale_plan_context");

        // The target must not be published.
        assert!(!harness.target_path().exists());
        assert_eq!(harness.plan_entries(), entries_before);
    }

    #[tokio::test]
    async fn test_conflicting_target_bytes_are_unchanged_and_conflict() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let _ = confirm(&harness, revision, &harness.payload.candidate_id, "confirm");
        *harness.manager.write_test_hook.lock().unwrap() = None;

        std::fs::write(harness.target_path(), b"# Somebody else's plan\n").unwrap();

        let conflict = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        assert_eq!(conflict.code, "PC_CONFIRMATION_TARGET_CONFLICT");
        assert_eq!(
            std::fs::read(harness.target_path()).unwrap(),
            b"# Somebody else's plan\n",
            "conflicting content must not be overwritten"
        );
        assert_eq!(
            harness.plan_entries().len(),
            2,
            "no additional file may be created"
        );
        let journal = harness.journal();
        assert_eq!(journal.status, super::super::recovery::RunRecoveryStatus::Active);
    }

    /// Tampers one binding on the durable intent and asserts the confirmation
    /// fails closed without touching the plan or the journal.
    fn assert_binding_tamper_fails_closed(
        harness: &NewPrimaryWaitingHarness,
        expected_code: &str,
        mutate: impl FnOnce(
            &mut super::super::convergence::PlanConvergenceConfirmationIntent,
        ),
    ) {
        let revision = harness.waiting_revision();
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let _ = confirm(harness, revision, &harness.payload.candidate_id, "confirm");
        *harness.manager.write_test_hook.lock().unwrap() = None;

        let published = std::fs::read(harness.target_path()).unwrap();
        let entries_before = harness.plan_entries();
        let journal_before = harness.journal();

        let mut journal = harness.journal();
        mutate(
            journal
                .plan_convergence_confirmation_intent
                .as_mut()
                .expect("a failed acceptance write must leave a durable intent"),
        );
        // The journal refuses to hold a malformed intent at all, which is the
        // same fail-closed outcome as a confirmation that rejects the binding.
        if let Err(persist_error) = harness.manager.write_journal(&journal) {
            assert!(
                persist_error.contains("PC_INVALID_CONFIRMATION_INTENT"),
                "unexpected persistence rejection: {persist_error}"
            );
        } else {
            let error =
                confirm(harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
            assert_eq!(error.code, expected_code, "unexpected failure: {error:?}");
        }

        assert_eq!(std::fs::read(harness.target_path()).unwrap(), published);
        assert_eq!(harness.plan_entries(), entries_before);
        let journal_after = harness.journal();
        assert_eq!(journal_after.revision, journal_before.revision);
        assert_eq!(journal_after.status, journal_before.status);
    }

    #[tokio::test]
    async fn test_every_intent_binding_is_validated_before_mutation() {
        let harness = drive_convergence_to_new_primary_waiting().await;

        for (label, expected, mutate) in [
            (
                "run",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i: &mut super::super::convergence::PlanConvergenceConfirmationIntent| {
                    i.run_id = "00000000-0000-0000-0000-000000000000".into()
                })
                    as Box<dyn FnOnce(&mut super::super::convergence::PlanConvergenceConfirmationIntent)>,
            ),
            (
                "candidate id",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.candidate_id = "another-candidate".into()),
            ),
            (
                "sequence",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.candidate_sequence += 1),
            ),
            (
                "artifact ref",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| {
                    i.candidate_artifact_ref =
                        "artifacts/candidates/1_00000000-0000-0000-0000-000000000000.json".into()
                }),
            ),
            (
                "artifact digest",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.candidate_artifact_digest = "a1".repeat(32)),
            ),
            (
                "operation digest",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.operation_payload_digest = "b2".repeat(32)),
            ),
            (
                "plan context digest",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.plan_context_digest = "c3".repeat(32)),
            ),
            (
                "target revision",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.target_revision = 7),
            ),
            (
                "target plan id",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.target_plan_id = "V0.23.0-r9".into()),
            ),
            (
                "content digest",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.content_digest = "d4".repeat(32)),
            ),
            (
                "idempotency key",
                "PC_CONFIRMATION_INTENT_MISMATCH",
                Box::new(|i| i.idempotency_key = "plan-confirm:other:other".into()),
            ),
        ] {
            // Each case needs its own waiting run so tampering cannot leak.
            let harness = drive_convergence_to_new_primary_waiting().await;
            assert_binding_tamper_fails_closed(&harness, expected, move |intent| mutate(intent));
            let _ = label;
        }
        let _ = &harness;
    }

    #[tokio::test]
    async fn test_noncanonical_targets_are_rejected_before_write() {
        for (label, bad_path) in [
            ("traversal", "../.plan/V0.23.0-r2.md"),
            ("out of root", "elsewhere/V0.23.0-r2.md"),
            ("absolute", "/tmp/V0.23.0-r2.md"),
            ("alias", ".plan/./V0.23.0-r2.md"),
        ] {
            let harness = drive_convergence_to_new_primary_waiting().await;
            let revision = harness.waiting_revision();
            let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counter = writes.clone();
            *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                if n == 2 {
                    Err("injected acceptance write failure".into())
                } else {
                    Ok(())
                }
            }));
            let _ = confirm(&harness, revision, &harness.payload.candidate_id, "confirm");
            *harness.manager.write_test_hook.lock().unwrap() = None;

            let mut journal = harness.journal();
            journal
                .plan_convergence_confirmation_intent
                .as_mut()
                .unwrap()
                .target_path = bad_path.to_string();
            harness.manager.write_journal(&journal).unwrap();

            let error = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
            assert!(
                matches!(
                    error.code.as_str(),
                    "PC_CONFIRMATION_TARGET_REJECTED" | "PC_CONFIRMATION_INTENT_MISMATCH"
                ),
                "{label}: unexpected failure {error:?}"
            );
            // The tampered path must never appear in the workspace as a new
            // file. An alias resolves onto the real target, so it is checked by
            // the binding comparison rather than by file existence.
            if label != "alias" {
                assert!(
                    !harness.project.join(bad_path).exists(),
                    "{label}: the tampered target must not be created"
                );
            } else {
                assert_eq!(harness.plan_entries().len(), 2, "alias must not add a file");
            }
            let _ = label;
        }
    }

    #[tokio::test]
    async fn test_symlinked_target_is_rejected_before_write() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = writes.clone();
        *harness.manager.write_test_hook.lock().unwrap() = Some(Arc::new(move |_| {
            let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n == 2 {
                Err("injected acceptance write failure".into())
            } else {
                Ok(())
            }
        }));
        let _ = confirm(&harness, revision, &harness.payload.candidate_id, "confirm");
        *harness.manager.write_test_hook.lock().unwrap() = None;

        // Replace the published regular file with a symlink pointing outside.
        let outside = harness.project.parent().unwrap().join("outside-plan.md");
        std::fs::write(&outside, b"# outside\n").unwrap();
        std::fs::remove_file(harness.target_path()).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&outside, harness.target_path()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, harness.target_path()).unwrap();

        let error = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        assert!(
            matches!(
                error.code.as_str(),
                "PC_CONFIRMATION_TARGET_REJECTED"
                    | "PC_CONFIRMATION_INTENT_MISMATCH"
                    | "PC_CONFIRMATION_TARGET_CONFLICT"
                    | "unresolved_plan_context"
            ),
            "unexpected failure {error:?}"
        );
        assert_eq!(std::fs::read_to_string(&outside).unwrap(),
            "# outside\n",
            "the escape target must be untouched"
        );
    }

    #[tokio::test]
    async fn test_stale_preview_target_is_refused_without_writing() {
        let harness = drive_convergence_to_new_primary_waiting().await;
        let revision = harness.waiting_revision();

        // A concurrent writer advances the workspace past the reviewed revision
        // before the intent is recorded.
        std::fs::write(harness.project.join(".plan/V0.23.0-r5.md"), b"# Concurrent\n").unwrap();
        let entries_before = harness.plan_entries();

        let error = confirm(&harness, revision, &harness.payload.candidate_id, "confirm").unwrap_err();
        assert!(
            matches!(
                error.code.as_str(),
                "PC_STALE_CONFIRMATION" | "PC_CONFIRMATION_INTENT_MISMATCH"
            ),
            "unexpected failure {error:?}"
        );
        assert_eq!(harness.plan_entries(), entries_before);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_confirmation_races_a_new_run_without_interleaving_publication() {
        for _ in 0..2 {
            let harness = drive_convergence_to_new_primary_waiting().await;
            let revision = harness.waiting_revision();
            let entries_before = harness.plan_entries();

            let barrier = Arc::new(std::sync::Barrier::new(2));
            let confirm_done = Arc::new(std::sync::atomic::AtomicBool::new(false));

            let confirm_runtime = harness.runtime.clone();
            let confirm_state = harness.state.clone();
            let confirm_barrier = barrier.clone();
            let confirm_done_flag = confirm_done.clone();
            let run_id = harness.run_id.clone();
            let candidate_id = harness.payload.candidate_id.clone();

            let confirm_task = tokio::task::spawn_blocking(move || {
                confirm_barrier.wait();
                let result = confirm_converged_new_plan_impl(
                    confirm_runtime,
                    confirm_state,
                    run_id,
                    revision,
                    candidate_id,
                    "confirm".to_string(),
                );
                confirm_done_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                result.map(|_| ()).map_err(|error| error.code)
            });

            let mut snapshot = snapshot_for_overrides();
            snapshot.project_path = harness.project.to_string_lossy().into();
            let mut planner = snapshot.assignments.get(&AgentRole::Planner).unwrap().clone();
            planner.id = "race-planner".into();
            let mut reviewer = planner.clone();
            reviewer.id = "race-reviewer".into();
            snapshot.assignments.insert(AgentRole::Planner, planner);
            snapshot.assignments.insert(AgentRole::PlanReviewer, reviewer);

            let start_engine = OrchestratorEngine::with_scripted_adapters(
                vec![(AgentRole::Planner, scripted_output_helper("not a convergence proposal"))],
                vec![],
            );
            let start_runtime = PollingRunStartRuntime::new(
                start_engine,
                harness.state.clone(),
                harness.manager.clone(),
                false,
            );
            let start_runtime_thread = start_runtime.clone();
            let start_state = harness.state.clone();
            let start_barrier = barrier.clone();
            let start_done_flag = confirm_done.clone();

            let start_task = tokio::task::spawn_blocking(move || {
                start_barrier.wait();
                let started = start_plan_convergence_run_impl(
                    start_runtime_thread,
                    start_state,
                    snapshot,
                    "Race task".into(),
                    super::super::convergence::PlanConvergenceConfig {
                        opt_in: true,
                        total_timeout_secs: 60,
                    },
                    Some(super::super::plan_workspace::PlanWorkspaceConfig {
                        version_sources: vec!["package.json".into()],
                        ..Default::default()
                    }),
                );
                // Sampled immediately: a reservation observed before the
                // confirmation returned means the two ran concurrently.
                let interleaved = !start_done_flag.load(std::sync::atomic::Ordering::SeqCst);
                started.map(|_| interleaved)
            });

            let (confirm_outcome, start_outcome) = tokio::join!(confirm_task, start_task);
            let confirm_outcome = confirm_outcome.unwrap();
            let interleaved = start_outcome.unwrap().unwrap();

            if confirm_outcome.is_ok() {
                assert!(
                    !interleaved,
                    "plan publication interleaved with a run reservation"
                );
            } else {
                assert_eq!(confirm_outcome.unwrap_err(), "PC_CONFLICT_RUN_ACTIVE");
                assert_eq!(
                    harness.plan_entries(),
                    entries_before,
                    "a conflicting run must leave the workspace untouched"
                );
            }

            start_runtime.join().await;
            assert!(harness.state.active_run.lock().unwrap().is_none());
        }
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

pub fn list_interrupted_runs_impl(
    state: &OrchestratorState,
) -> Result<Vec<super::recovery::RunRecoverySummary>, String> {
    if let Some(ref jm) = state.journal_manager {
        jm.list_journals()
    } else {
        Ok(Vec::new())
    }
}

pub fn get_run_recovery_detail_impl(
    state: &OrchestratorState,
    run_id: &str,
) -> Result<super::recovery::RunJournal, String> {
    if let Some(ref jm) = state.journal_manager {
        jm.read_journal(run_id)
    } else {
        Err("Recovery journal manager is not initialized.".to_string())
    }
}

/// Explicitly resumes the one bounded Human-Gated route validated by recovery preflight.
/// The journal generation and active-run reservation are durable before the new Worker
/// capability is created or any workflow stage is dispatched.
pub fn resume_interrupted_run_impl<R: RunStartRuntime>(
    runtime: R,
    state: Arc<OrchestratorState>,
    run_id: &str,
    expected_revision: u64,
    expected_fingerprint: &str,
    confirmed_worker_stopped: bool,
) -> Result<StartRunResponse, String> {
    if !confirmed_worker_stopped {
        return Err("Resume requires explicit confirmation that the prior Worker is stopped.".into());
    }
    let _recovery_gate = state.recovery_gate.lock().map_err(|e| e.to_string())?;
    let mut active_lock = state.active_run.lock().map_err(|e| e.to_string())?;
    if active_lock.is_some() {
        return Err("Recovery is unavailable while another run is active.".into());
    }
    let manager = state.journal_manager.as_ref().ok_or("Recovery journal manager is not initialized")?;
    let mut journal = manager.read_journal(run_id)?;
    if journal.revision != expected_revision {
        return Err("Journal changed after recovery preflight".into());
    }
    if !matches!(journal.status, super::recovery::RunRecoveryStatus::Interrupted | super::recovery::RunRecoveryStatus::Active) {
        return Err("Only an interrupted run can be resumed.".into());
    }
    let reference = journal.checkpoint_manifest_ref.as_deref().ok_or("No checkpoint is available")?;
    let digest = journal.checkpoint_digest.as_deref().ok_or("Checkpoint digest is missing")?;
    let store = super::checkpoint::CheckpointStore::new(manager.runs_dir().to_path_buf());
    let preflight = store.preflight(run_id, journal.revision, reference, digest, Path::new(&journal.canonical_project_path))?;
    if !preflight.workspace_matches
        || preflight.current_fingerprint != expected_fingerprint
        || preflight.reason_code.as_deref() != Some("resume_route_unavailable")
    {
        return Err("Workspace changed after recovery preflight; inspect recovery again.".into());
    }
    let manifest = store.load(run_id, reference, digest)?;
    let resume_context = build_human_gated_resume_context(&journal, &manifest)?;
    let snapshot = prepare_run_snapshot("human_gated_loop", journal.snapshot.clone(), None)?;
    let task_prompt = journal.task_prompt.clone().ok_or("Original task prompt is unavailable")?;

    journal.revision = journal.revision.checked_add(1).ok_or("Journal revision overflow")?;
    journal.resume_generation = journal.resume_generation.checked_add(1).ok_or("Resume generation overflow")?;
    journal.current_state = resume_context.stage;
    journal.status = super::recovery::RunRecoveryStatus::Active;
    journal.iteration_counters.mailbox_epoch = Some(resume_context.mailbox_epoch);
    journal.updated_at_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_secs();

    let run_id = journal.run_id.clone();
    let (control_tx, control_rx) = watch::channel(RunControlState::Running);
    let cancel_token = CancellationToken::new();
    let (clarification_tx, clarification_rx) = mpsc::channel::<String>(16);
    let (blocking_tx, blocking_rx) = mpsc::channel::<BlockingResolution>(16);
    let (human_gate_tx, human_gate_rx) = mpsc::channel::<HumanGateDecision>(16);
    let (worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel::<()>(16);
    let current_step = Arc::new(Mutex::new(StepProgressEvent {
        run_id: run_id.clone(),
        step: resume_context.stage,
        message: format!("Resuming at the verified {:?} stage", resume_context.stage),
        ..Default::default()
    }));
    *active_lock = Some(ActiveRun {
        run_id: run_id.clone(), control_tx, cancel_token: cancel_token.clone(),
        clarification_tx, blocking_resolution_tx: blocking_tx, human_gate_tx,
        worker_reclaim_tx, current_step: current_step.clone(),
    });

    if let Err(error) = manager.write_journal(&journal) {
        *active_lock = None;
        return Err(format!("Could not durably reserve resumed run before dispatch: {error}"));
    }
    drop(active_lock);

    let journal_state = Arc::new(Mutex::new(journal));
    let manager = Some(manager.clone());
    let checkpoint_store = Some(Arc::new(store));
    let on_event = make_run_event_callback(
        runtime.clone(), current_step.clone(), journal_state.clone(), manager.clone(),
        checkpoint_store, cancel_token.clone(),
    );
    let dispatch_persistence = make_dispatch_persistence_callback(
        journal_state.clone(), manager.clone(), cancel_token.clone(),
    );
    let on_log: super::engine::LogCallback = Arc::new({
        let runtime = runtime.clone();
        move |event| runtime.emit_log(event)
    });
    let spawned_id = run_id.clone();
    let state_for_task = state.clone();
    let event_for_task = on_event.clone();
    let log_for_task = on_log.clone();
    let journal_for_task = Some(journal_state.clone());
    let worker_context = resume_context.clone();
    let workflow_run_id = run_id.clone();
    let workflow_event = event_for_task.clone();
    let workflow_log = log_for_task.clone();
    // A resumed run may still dispatch the Direct MCP adapter, so it needs the
    // same durable audit wiring as a fresh start.
    let engine = build_run_engine(&runtime, journal_state.clone(), manager.clone());
    runtime.spawn(Box::pin(async move {
        let workflow = async move {
            engine.run_human_gated_workflow_with_resume(
                workflow_run_id, snapshot, task_prompt, None, vec![], control_rx, cancel_token,
                clarification_rx, blocking_rx, human_gate_rx, worker_reclaim_rx,
                workflow_event, workflow_log, Some(worker_context), Some(dispatch_persistence),
            ).await
        };
        supervise_run(workflow, run_id, state_for_task, journal_for_task, event_for_task, log_for_task).await;
    }));
    Ok(StartRunResponse { run_id: spawned_id })
}

pub fn preflight_run_recovery_impl(
    state: &OrchestratorState,
    run_id: &str,
) -> Result<super::checkpoint::RecoveryPreflight, String> {
    let _recovery_gate = state.recovery_gate.lock().map_err(|e| e.to_string())?;
    if state.active_run.lock().map_err(|e| e.to_string())?.is_some() {
        return Err("Recovery is unavailable while another run is active.".into());
    }
    let manager = state.journal_manager.as_ref().ok_or("Recovery journal manager is not initialized")?;
    let journal = manager.read_journal(run_id)?;
    if matches!(journal.status, super::recovery::RunRecoveryStatus::Complete | super::recovery::RunRecoveryStatus::Failed | super::recovery::RunRecoveryStatus::Cancelled) {
        return Err("Terminal runs cannot be resumed or restored.".into());
    }
    let reference = journal.checkpoint_manifest_ref.as_deref().ok_or("No verified workspace checkpoint is available")?;
    let digest = journal.checkpoint_digest.as_deref().ok_or("Checkpoint digest is missing")?;
    let store = super::checkpoint::CheckpointStore::new(manager.runs_dir().to_path_buf());
    let mut preflight = store.preflight(run_id, journal.revision, reference, digest, Path::new(&journal.canonical_project_path))?;
    if preflight.reason_code.as_deref() == Some("resume_route_unavailable") {
        if let Ok(manifest) = store.load(run_id, reference, digest) {
            if let Ok(context) = build_human_gated_resume_context(&journal, &manifest) {
                preflight.resume_stage = Some(context.stage);
                preflight.can_resume = preflight.workspace_matches;
                preflight.can_restore = !preflight.workspace_matches;
                preflight.reason_code = None;
            }
            if !preflight.workspace_matches
                && build_human_gated_adopt_plan(&journal, Some(&manifest)).is_ok()
            {
                preflight.can_adopt = true;
            }
        }
    }
    Ok(preflight)
}

fn build_human_gated_resume_context(
    journal: &super::recovery::RunJournal,
    manifest: &super::recovery::CheckpointManifest,
) -> Result<super::recovery::HumanGatedResumeContext, String> {
    use super::recovery::CheckpointKind;
    if journal.workflow_type != "human_gated_loop" {
        return Err("Only Human-Gated runs have a verified resume route.".into());
    }
    let implementation_resume = matches!(
        (manifest.kind, manifest.stage),
        (CheckpointKind::EntryBaseline, WorkflowState::Implementation)
            | (CheckpointKind::StageCheckpoint, WorkflowState::PlanReview)
    ) && journal.last_successful_state == Some(WorkflowState::PlanReview)
        && journal.iteration_counters.plan_review_count > 0
        && journal.iteration_counters.fix_count == 0
        && journal.iteration_counters.code_review_count == 0;
    let adopted_validation_resume = matches!(
        (manifest.kind, manifest.stage),
        (CheckpointKind::AdoptBaseline, WorkflowState::Validation)
            | (CheckpointKind::EntryBaseline, WorkflowState::Validation)
    ) && journal.current_state == WorkflowState::Validation;
    if !implementation_resume && !adopted_validation_resume {
        return Err("This checkpoint stage has no implemented resume route.".into());
    }
    let plan_text = build_human_gated_adopt_plan(journal, Some(manifest))?;
    let profile = journal.snapshot.assignments.get(&AgentRole::Implementer)
        .ok_or("Implementer profile is missing from the run snapshot")?;
    if profile.adapter != ExecutionAdapterType::Antigravity {
        return Err("The saved Implementer is not the Antigravity harness.".into());
    }
    validate_workflow_role_capabilities("human_gated_loop", &AgentRole::Implementer, Some(profile))
        .map_err(|error| error.message)?;
    let _validated_snapshot = prepare_run_snapshot("human_gated_loop", journal.snapshot.clone(), None)?;
    validate_resume_credentials(&journal.snapshot)?;
    let task_dispatches = &journal.iteration_counters.antigravity_task_dispatches;
    let (total, epoch) = validate_resume_worker_budget(&journal.iteration_counters)?;
    if implementation_resume && task_dispatches.get("task-impl").copied().unwrap_or(0) != 0 {
        return Err("The Implementation task was already dispatched; replay is unsafe.".into());
    }
    let mailbox_epoch = epoch.checked_add(1).ok_or("Mailbox epoch overflow")?;
    Ok(super::recovery::HumanGatedResumeContext {
        stage: if adopted_validation_resume { WorkflowState::Validation } else { WorkflowState::Implementation },
        adopted_baseline: adopted_validation_resume,
        plan_text,
        plan_review_count: journal.iteration_counters.plan_review_count,
        fix_count: journal.iteration_counters.fix_count,
        code_review_count: journal.iteration_counters.code_review_count,
        antigravity_dispatches: total,
        antigravity_task_dispatches: task_dispatches.clone(),
        mailbox_epoch,
    })
}

fn build_human_gated_adopt_plan(
    journal: &super::recovery::RunJournal,
    manifest: Option<&super::recovery::CheckpointManifest>,
) -> Result<String, String> {
    if journal.workflow_type != "human_gated_loop" {
        return Err("Only Human-Gated runs have a verified recovery route.".into());
    }
    let supported_state = matches!(
        journal.current_state,
        WorkflowState::Implementation | WorkflowState::Fix | WorkflowState::Validation | WorkflowState::CodeReview
    );
    if !supported_state {
        return Err("The interrupted stage has no safe validation route.".into());
    }
    if journal.task_prompt.as_deref().is_none_or(|task| task.trim().is_empty()) {
        return Err("The original task prompt is unavailable.".into());
    }
    super::engine::validate_workflow_iteration_limits("human_gated_loop", &journal.snapshot.iteration_limits)?;
    prepare_run_snapshot("human_gated_loop", journal.snapshot.clone(), None)?;
    validate_resume_credentials(&journal.snapshot)?;
    validate_resume_worker_budget(&journal.iteration_counters)?;
    let plan_from_manifest = manifest
        .and_then(|manifest| manifest.stage_input.as_deref())
        .and_then(|input| serde_json::from_str::<serde_json::Value>(input).ok())
        .and_then(|input| input.get("planText").and_then(serde_json::Value::as_str).map(str::to_owned));
    journal.approved_plan.clone().filter(|plan| !plan.trim().is_empty())
        .or(plan_from_manifest.filter(|plan| !plan.trim().is_empty()))
        .ok_or_else(|| "The approved plan is unavailable; adoption cannot be safely revalidated.".into())
}

fn validate_resume_credentials(snapshot: &RunConfigurationSnapshot) -> Result<(), String> {
    for role in [AgentRole::Implementer, AgentRole::Fixer, AgentRole::CodeReviewer] {
        let profile = snapshot.assignments.get(&role)
            .ok_or_else(|| format!("Saved {:?} profile is missing", role))?;
        if profile.adapter == ExecutionAdapterType::Provider {
            let provider_id = profile.provider_id.as_deref()
                .ok_or_else(|| format!("Saved {:?} provider ID is missing", role))?;
            let (_, key_name) = super::adapters::provider::resolve_provider_endpoint_and_env(provider_id)?;
            let key = std::env::var(&key_name)
                .map_err(|_| format!("Required credential {key_name} is not available for {:?}", role))?;
            if key.trim().is_empty() {
                return Err(format!("Required credential {key_name} is empty for {:?}", role));
            }
        }
    }
    Ok(())
}

fn validate_resume_worker_budget(
    counters: &super::recovery::RunIterationCounters,
) -> Result<(u32, u64), String> {
    let total = counters.antigravity_dispatches
        .ok_or("Worker dispatch counters are not durably recorded; resume is unavailable")?;
    let epoch = counters.mailbox_epoch
        .ok_or("Previous Mailbox epoch is not durably recorded; resume is unavailable")?;
    let dispatched_sum = counters.antigravity_task_dispatches.values()
        .try_fold(0u32, |sum, value| sum.checked_add(*value))
        .ok_or("Worker dispatch counters overflow")?;
    if total == 0 || counters.antigravity_task_dispatches.is_empty() || dispatched_sum != total {
        return Err("Worker task budget state is incomplete.".into());
    }
    if total >= 6 {
        return Err("The run-wide Antigravity dispatch budget is exhausted.".into());
    }
    Ok((total, epoch))
}

pub fn resolve_run_recovery_impl(
    state: &OrchestratorState,
    run_id: &str,
    choice: &str,
    expected_revision: u64,
    expected_fingerprint: &str,
    expected_backup_id: &str,
    confirmed: bool,
) -> Result<super::checkpoint::RecoveryResolution, String> {
    let _recovery_gate = state.recovery_gate.lock().map_err(|e| e.to_string())?;
    if state.active_run.lock().map_err(|e| e.to_string())?.is_some() {
        return Err("Recovery is unavailable while another run is active.".into());
    }
    let manager = state.journal_manager.as_ref().ok_or("Recovery journal manager is not initialized")?;
    let mut journal = manager.read_journal(run_id)?;
    if journal.revision != expected_revision { return Err("Journal changed after recovery preflight".into()); }
    if matches!(journal.status, super::recovery::RunRecoveryStatus::Complete | super::recovery::RunRecoveryStatus::Failed | super::recovery::RunRecoveryStatus::Cancelled) {
        return Err("Terminal runs cannot be resumed, restored, or adopted.".into());
    }
    let reference = journal.checkpoint_manifest_ref.clone().ok_or("No workspace checkpoint is available")?;
    let digest = journal.checkpoint_digest.clone().ok_or("Checkpoint digest is missing")?;
    let store = super::checkpoint::CheckpointStore::new(manager.runs_dir().to_path_buf());
    match choice {
        "cancel" => Ok(super::checkpoint::RecoveryResolution { backup_id: None, backup_digest: None, checkpoint_id: None, resume_stage: None, journal_revision: journal.revision }),
        "resume" => Err("This checkpoint's workflow stage does not yet have a verified resume route.".into()),
        "restore" => {
            if !confirmed { return Err("Restore requires a separate explicit confirmation".into()); }
            let preflight = store.preflight(run_id, journal.revision, &reference, &digest, Path::new(&journal.canonical_project_path))?;
            let resume_route_ready = preflight.reason_code.as_deref() == Some("resume_route_unavailable")
                && store.load(run_id, &reference, &digest)
                    .and_then(|manifest| build_human_gated_resume_context(&journal, &manifest).map(|_| manifest))
                    .is_ok();
            if preflight.current_fingerprint != expected_fingerprint
                || preflight.workspace_matches
                || !resume_route_ready
            { return Err("Recovery preflight is stale or restore is unsupported".into()); }
            let mut resolution = store.resolve_restore(run_id, journal.revision, &reference, &digest, expected_fingerprint, expected_backup_id, Path::new(&journal.canonical_project_path))?;
            journal.revision = journal.revision.checked_add(1).ok_or("Journal revision overflow after restore")?;
            journal.last_shelve_backup_id = resolution.backup_id.clone();
            journal.last_shelve_backup_digest = resolution.backup_digest.clone();
            journal.status = super::recovery::RunRecoveryStatus::Interrupted;
            journal.updated_at_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
            manager.write_journal(&journal).map_err(|error| format!("Workspace restore completed and verified, but journaling the retained Shelve Backup failed; backup ID {:?} remains on disk: {error}", resolution.backup_id))?;
            resolution.journal_revision = journal.revision;
            Ok(resolution)
        }
        "adopt" => {
            let preflight = store.preflight(run_id, journal.revision, &reference, &digest, Path::new(&journal.canonical_project_path))?;
            let existing_manifest = store.load(run_id, &reference, &digest)?;
            if preflight.current_fingerprint != expected_fingerprint
                || preflight.workspace_matches
                || preflight.reason_code.as_deref() != Some("resume_route_unavailable")
                || build_human_gated_adopt_plan(&journal, Some(&existing_manifest)).is_err()
            { return Err("Recovery preflight is stale or adoption is unsupported".into()); }
            if !confirmed { return Err("Adopting the current workspace requires explicit confirmation".into()); }
            // Validate the exact continuation before publishing any new
            // baseline, so Adopt cannot strand the run in an unusable state.
            let plan_text = build_human_gated_adopt_plan(&journal, Some(&existing_manifest))?;
            let stage = WorkflowState::Validation;
            let stage_input = serde_json::json!({ "planText": plan_text }).to_string();
            let (manifest, new_ref) = store.capture(run_id, stage, super::recovery::CheckpointKind::AdoptBaseline, Path::new(&journal.canonical_project_path), journal.task_prompt.clone(), Some(stage_input))?;
            journal.revision = journal.revision.checked_add(1).ok_or("Journal revision overflow")?;
            journal.checkpoint_manifest_ref = Some(new_ref);
            journal.checkpoint_digest = Some(manifest.manifest_digest.clone());
            journal.current_state = stage;
            journal.status = super::recovery::RunRecoveryStatus::Interrupted;
            journal.updated_at_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
            manager.write_journal(&journal)?;
            Ok(super::checkpoint::RecoveryResolution { backup_id: None, backup_digest: None, checkpoint_id: Some(manifest.checkpoint_id), resume_stage: Some(stage), journal_revision: journal.revision })
        }
        other => Err(format!("Unsupported recovery choice '{other}'")),
    }
}

pub fn get_plan_context_impl(
    project_path: &str,
    config: Option<super::plan_workspace::PlanWorkspaceConfig>,
) -> Result<super::plan_workspace::PlanContext, super::plan_workspace::PlanWorkspaceError> {
    let cfg = config.unwrap_or_default();
    super::plan_workspace::resolve_plan_context(Path::new(project_path), &cfg)
}

pub fn get_plan_current_impl(
    project_path: &str,
    config: Option<super::plan_workspace::PlanWorkspaceConfig>,
) -> Result<super::plan_workspace::PlanCurrentResponse, super::plan_workspace::PlanWorkspaceError> {
    let cfg = config.unwrap_or_default();
    super::plan_workspace::plan_current(Path::new(project_path), &cfg)
}

pub fn append_plan_section_impl(
    project_path: &str,
    request: super::plan_workspace::PlanAppendRequest,
    config: Option<super::plan_workspace::PlanWorkspaceConfig>,
) -> Result<super::plan_workspace::PlanAppendResponse, super::plan_workspace::PlanWorkspaceError> {
    let cfg = config.unwrap_or_default();
    super::plan_workspace::plan_append(Path::new(project_path), &cfg, request)
}

pub fn preview_new_plan_impl(
    project_path: &str,
    config: Option<super::plan_workspace::PlanWorkspaceConfig>,
) -> Result<super::plan_workspace::PlanNewPreviewResponse, super::plan_workspace::PlanWorkspaceError> {
    let cfg = config.unwrap_or_default();
    super::plan_workspace::plan_new_preview(Path::new(project_path), &cfg)
}

pub fn confirm_new_plan_impl(
    project_path: &str,
    request: super::plan_workspace::PlanNewConfirmRequest,
    config: Option<super::plan_workspace::PlanWorkspaceConfig>,
) -> Result<super::plan_workspace::PlanNewConfirmResponse, super::plan_workspace::PlanWorkspaceError> {
    let cfg = config.unwrap_or_default();
    super::plan_workspace::plan_new_confirm(Path::new(project_path), &cfg, request)

}
