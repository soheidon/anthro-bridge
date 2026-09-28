use super::engine::{BlockingResolution, OrchestratorEngine, StepProgressEvent};
use super::types::*;
use super::validation::compute_gate_command_hash;
use std::future::Future;
use std::path::{Path, PathBuf};
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
}

#[derive(Default)]
pub struct OrchestratorState {
    pub active_run: Mutex<Option<ActiveRun>>,
    pub authorized_custom_gates: Mutex<Vec<AuthorizedCustomGate>>,
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
                pyproject_toml: false,
                cargo_toml: false,
                git: false,
            },
        };
    }

    let is_directory = path.is_dir();
    let spec_md = path.join("SPEC.md").exists();
    let implementation_plan_md = path.join("IMPLEMENTATION_PLAN.md").exists();
    let agents_md = path.join("AGENTS.md").exists();
    let readme_md = path.join("README.md").exists();
    let package_json = path.join("package.json").exists();
    let pyproject_toml = path.join("pyproject.toml").exists();
    let cargo_toml = path.join("Cargo.toml").exists();
    let git = path.join(".git").exists();

    let mut types = Vec::new();
    if package_json || path.join("tsconfig.json").exists() {
        types.push("TypeScript / Node");
    }
    if cargo_toml {
        types.push("Rust");
    }
    if pyproject_toml || path.join("requirements.txt").exists() {
        types.push("Python");
    }

    let project_type = match types.len() {
        0 => "Unknown".to_string(),
        1 => types[0].to_string(),
        _ => "Mixed".to_string(),
    };

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
            pyproject_toml,
            cargo_toml,
            git,
        },
    }
}

/// Atomically starts an orchestrator run, installs ActiveRun, spawns the async workflow, and returns StartRunResponse immediately.
pub fn start_orchestrator_run_impl(
    app: AppHandle,
    state: Arc<OrchestratorState>,
    snapshot: RunConfigurationSnapshot,
    task_prompt: String,
    workflow_type: String,
) -> Result<StartRunResponse, String> {
    let run_id = Uuid::new_v4().to_string();

    let (control_tx, control_rx) = watch::channel(RunControlState::Running);
    let cancel_token = CancellationToken::new();
    let (clarification_tx, clarification_rx) = mpsc::channel::<String>(16);
    let (blocking_tx, blocking_rx) = mpsc::channel::<BlockingResolution>(16);

    // Snapshot authorized custom gates
    let authorized_gates = {
        let auth_lock = state
            .authorized_custom_gates
            .lock()
            .map_err(|e| e.to_string())?;
        auth_lock.clone()
    };

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
        });
    }

    let spawned_run_id = run_id.clone();
    let state_clone = Arc::clone(&state);

    let app_handle_event = app.clone();
    let on_event: super::engine::EventCallback = Arc::new(move |evt: StepProgressEvent| {
        let _ = app_handle_event.emit("orchestrator:step_update", evt);
    });

    let app_handle_log = app.clone();
    let on_log: super::engine::LogCallback =
        Arc::new(move |log_event: super::types::RunLogEvent| {
            let _ = app_handle_log.emit("orchestrator:log", log_event);
        });

    let run_id_for_task = run_id.clone();
    let supervisor_run_id = run_id_for_task.clone();
    let engine = OrchestratorEngine::new();
    let workflow_cancel_token = cancel_token.clone();
    let workflow_on_event = on_event.clone();
    let workflow_on_log = on_log.clone();

    tokio::spawn(async move {
        let workflow = async move {
            engine
                .run_workflow(
                    run_id_for_task.clone(),
                    snapshot,
                    task_prompt,
                    workflow_type,
                    authorized_gates,
                    control_rx,
                    workflow_cancel_token,
                    clarification_rx,
                    blocking_rx,
                    workflow_on_event,
                    workflow_on_log,
                )
                .await
        };
        supervise_run(workflow, supervisor_run_id, state_clone, on_event, on_log).await;
    });

    Ok(StartRunResponse {
        run_id: spawned_run_id,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn active_run(run_id: &str) -> ActiveRun {
        let (control_tx, _) = watch::channel(RunControlState::Running);
        let (clarification_tx, _) = mpsc::channel(1);
        let (blocking_resolution_tx, _) = mpsc::channel(1);
        ActiveRun {
            run_id: run_id.to_string(),
            control_tx,
            cancel_token: CancellationToken::new(),
            clarification_tx,
            blocking_resolution_tx,
        }
    }

    fn callbacks() -> (
        super::super::engine::EventCallback,
        super::super::engine::LogCallback,
    ) {
        (Arc::new(|_| {}), Arc::new(|_| {}))
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
