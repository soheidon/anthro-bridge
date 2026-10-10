use cap_std::ambient_authority;
use cap_std::fs::{Dir as CapabilityDir, OpenOptions as CapabilityOpenOptions};
use io_lifetimes::AsFilelike;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::VecDeque;
use std::fs::{self};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::adapters::codex_cli::CodexCliAdapter;
use super::adapters::direct_mcp::{DirectMcpAdapter, DirectMcpExecutionInput};
use super::adapters::ollama::OllamaAdapter;
use super::adapters::provider::ProviderAdapter;
use super::adapters::{AdapterExecutionInput, AdapterExecutionOutput};
use super::context_builder::{BuiltContext, ContextBuilder};
use super::finding_aggregator::FindingAggregator;
use super::mailbox::MailboxServer;
use super::plan_workspace::{FrozenPlanPayload, PlanContext};
use super::token_estimator::TokenCountQuality;
use super::types::{
    active_roles_for_workflow, validate_workflow_role_capabilities, AgentRole,
    AuthorizedCustomGate, ExecutionAdapterType, HumanGateDecision, LoopIterationLimits,
    McpServerConfig, OrchestratorProfile, OrchestratorTaskEnvelope, PlanArchiveOptions,
    PlanArchivePreview, SubmitTaskRequest,
    ReviewFinding, ReviewResult, ReviewVerdict, RunConfigurationSnapshot, RunControlState,
    WorkflowState,
};
use std::collections::HashMap;
use super::validation::{ValidationRunSummary, ValidationRunner};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "antigravity_dispatches")]
    pub antigravity_dispatches: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "antigravity_dispatch_limit")]
    pub antigravity_dispatch_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "budget_scope")]
    pub budget_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "waiting_reason")]
    pub waiting_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "completed_stage")]
    pub completed_stage: Option<WorkflowState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "plan_review_count")]
    pub plan_review_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "fix_count")]
    pub fix_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "code_review_count")]
    pub code_review_count: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum AntigravitySubmissionOutcome {
    Submitted(super::types::SubmitTaskRequest),
    BudgetExhausted(super::mailbox::BudgetExhaustionDetails),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockingResolution {
    Retry { guidance: Option<String> },
    Abort,
}

pub type EventCallback = Arc<dyn Fn(StepProgressEvent) -> Result<(), String> + Send + Sync>;
pub type DirectMcpAuditCallback = Arc<dyn Fn(super::adapters::direct_mcp::DirectMcpAuditRecord) -> Result<(), String> + Send + Sync>;
pub type LogCallback = Arc<dyn Fn(super::types::RunLogEvent) + Send + Sync>;

fn read_relay_error(relay_error: &std::sync::Mutex<Option<String>>) -> Option<String> {
    match relay_error.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

pub(super) fn spawn_mailbox_progress_relay(
    mut progress_rx: mpsc::Receiver<super::types::ReportProgressRequest>,
    run_id: String,
    mailbox_state: super::mailbox::MailboxState,
    on_event: EventCallback,
    relay_error: Arc<std::sync::Mutex<Option<String>>>,
    cancel_token: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let (current_state, dispatches, limit) = {
                let guard = mailbox_state.inner.lock().await;
                (
                    guard.current_state.clone(),
                    Some(guard.total_dispatches),
                    Some(guard.max_dispatches_per_run),
                )
            };
            let event = StepProgressEvent {
                run_id: run_id.clone(),
                step: current_state,
                iteration_info: progress.percent.map(|p| format!("{p}%")),
                message: progress.message,
                antigravity_dispatches: dispatches,
                antigravity_dispatch_limit: limit,
                ..Default::default()
            };
            if let Err(error) = on_event(event) {
                *relay_error
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(error);
                cancel_token.cancel();
                return;
            }
        }
    })
}

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

pub(crate) fn validate_workflow_iteration_limits(
    workflow_type: &str,
    limits: &LoopIterationLimits,
) -> Result<(), String> {
    let required_limits: &[(&str, u32)] = match workflow_type {
        "full_loop" | "human_gated_loop" => &[
            ("plan_review", limits.max_plan_review_iterations),
            ("fix", limits.max_fix_iterations),
            ("code_review", limits.max_code_review_iterations),
        ],
        "plan_only" => &[("plan_review", limits.max_plan_review_iterations)],
        "implement_only" => &[("fix", limits.max_fix_iterations)],
        // Review Only performs exactly one code review and has no configurable loop.
        "review_only" => &[],
        _ => {
            return Err(format!(
                "Unsupported Orchestrator workflow '{workflow_type}'."
            ))
        }
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

#[derive(Clone)]
pub struct ValidatedPlanArchive {
    requested_directory: PathBuf,
    project_root_at_start: PathBuf,
    directory_at_start: PathBuf,
    project_directory: Arc<CapabilityDir>,
    archive_relative_path: PathBuf,
    #[cfg(test)]
    persistence_test_hook: Option<ArchiveTestHook>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPlanArchiveDirectory {
    project_root: PathBuf,
    directory: PathBuf,
    pub exists: bool,
}

pub const LEAN_ANTIGRAVITY_POLICY: &str = "\n\n## Lean Antigravity Execution Policy\n- Inspect only files directly relevant to the approved task; expand exploration only when evidence shows it is necessary.\n- Use already-gathered context and do not reread files unless new evidence or a concrete uncertainty requires it.\n- For simple, bounded tasks, proceed after minimal inspection instead of creating an unnecessary plan; plan when the task is ambiguous or multi-stage.\n- Batch blocking clarification questions and combine already-known validation/review findings into one Fix task.\n- Avoid speculative refactors, unrelated cleanup, and work beyond the approved plan.\n- Make one bounded implementation pass and retry only for concrete validation failures or actionable review findings.\n- Avoid exhaustive self-review or redundant checks inside the Antigravity worker. Run only focused worker-side checks needed to catch immediate mistakes; the separately configured Orchestrator Validation Harness and Code Review remain authoritative and must still run.";

pub fn effective_task_prompt(task_prompt: Option<&str>, lean_mode: bool) -> Option<String> {
    task_prompt.map(|p| {
        if lean_mode {
            format!("{}{}", p, LEAN_ANTIGRAVITY_POLICY)
        } else {
            p.to_string()
        }
    })
}

const DEVELOPMENT_VERSION_SOURCE: &str = include_str!("../../resources/development-version.txt");
const MAX_ARCHIVE_INSTALL_ATTEMPTS: usize = 32;

pub fn parse_development_version(source: &str) -> Result<String, String> {
    let value = source
        .strip_suffix("\r\n")
        .or_else(|| source.strip_suffix('\n'))
        .unwrap_or(source);
    let components: Vec<&str> = value.split('.').collect();
    if components.len() != 3
        || components.iter().any(|component| {
            component.is_empty()
                || !component.bytes().all(|byte| byte.is_ascii_digit())
                || (component.len() > 1 && component.starts_with('0'))
                || component.parse::<u64>().is_err()
        })
    {
        return Err(
            "Invalid Orchestrator development version; expected decimal MAJOR.MINOR.PATCH."
                .to_string(),
        );
    }
    Ok(components.join("."))
}

pub fn development_target_version() -> Result<String, String> {
    parse_development_version(DEVELOPMENT_VERSION_SOURCE)
}

fn archive_folder_requested_path(project_path: &str, archive_directory: &str) -> PathBuf {
    let project_input = PathBuf::from(project_path);
    let path = PathBuf::from(archive_directory);
    if path.is_absolute() {
        path
    } else {
        project_input.join(path)
    }
}

pub fn resolve_plan_archive_directory(
    project_path: &str,
    archive_directory: &str,
) -> Result<ResolvedPlanArchiveDirectory, String> {
    let project_input = PathBuf::from(project_path);
    let project_root = fs::canonicalize(&project_input)
        .map_err(|e| format!("Could not resolve project directory: {e}"))?;
    if !project_root.is_dir() {
        return Err("Project path is not a directory.".to_string());
    }

    let requested = archive_folder_requested_path(project_path, archive_directory);
    let default_from_input = project_input.join(".plan");
    let default_from_root = project_root.join(".plan");
    let is_default_archive =
        paths_equal(&requested, &default_from_input) || paths_equal(&requested, &default_from_root);

    if let Ok(relative) = requested.strip_prefix(&project_input) {
        reject_parent_or_reparse_components(&project_input, relative)?;
    } else if !requested.is_absolute() {
        return Err("Plan archive folder must be inside the selected project.".to_string());
    }

    match fs::symlink_metadata(&requested) {
        Ok(metadata) => {
            if is_symlink_or_reparse(&metadata) {
                return Err("Plan archive folder cannot be a symlink or reparse point.".to_string());
            }
            if !metadata.is_dir() {
                return Err("Plan archive destination is not a directory.".to_string());
            }
            let directory = fs::canonicalize(&requested)
                .map_err(|e| format!("Could not resolve plan archive folder: {e}"))?;
            if !path_is_within(&project_root, &directory) {
                return Err(
                    "Plan archive folder must remain inside the selected project directory."
                        .to_string(),
                );
            }
            Ok(ResolvedPlanArchiveDirectory {
                project_root,
                directory,
                exists: true,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && is_default_archive => {
            let parent = requested
                .parent()
                .ok_or_else(|| "Plan archive folder has no parent.".to_string())?;
            let canonical_parent = fs::canonicalize(parent)
                .map_err(|e| format!("Could not resolve plan archive parent: {e}"))?;
            if !paths_equal(&canonical_parent, &project_root) {
                return Err(
                    "The default plan archive must be a direct child of the project.".to_string(),
                );
            }
            let directory = project_root.join(".plan");
            Ok(ResolvedPlanArchiveDirectory {
                project_root,
                directory,
                exists: false,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(
            "A custom plan archive folder must already exist inside the selected project."
                .to_string(),
        ),
        Err(error) => Err(format!("Could not inspect plan archive folder: {error}")),
    }
}

fn reject_parent_or_reparse_components(base: &Path, relative: &Path) -> Result<(), String> {
    let mut current = base.to_path_buf();
    for component in relative.components() {
        match component {
            std::path::Component::CurDir => continue,
            std::path::Component::Normal(part) => {
                current.push(part);
                match fs::symlink_metadata(&current) {
                    Ok(metadata) if is_symlink_or_reparse(&metadata) => {
                        return Err(
                            "Plan archive path cannot contain symlink or reparse-point components."
                                .to_string(),
                        );
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                    Err(error) => {
                        return Err(format!("Could not inspect plan archive path: {error}"))
                    }
                }
            }
            std::path::Component::ParentDir => {
                return Err(
                    "Plan archive path cannot traverse outside or above the project.".to_string(),
                );
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err("Plan archive folder must be inside the selected project.".to_string());
            }
        }
    }
    Ok(())
}

fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

pub fn next_plan_archive_filename(directory: &Path, version: &str) -> Result<String, String> {
    let version = parse_development_version(version)?;
    let prefix = format!("v{version}-r").to_ascii_lowercase();
    let mut maximum = 0u64;
    match fs::read_dir(directory) {
        Ok(entries) => {
            for entry in entries {
                let entry =
                    entry.map_err(|e| format!("Could not scan plan archive folder: {e}"))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let lower = name.to_ascii_lowercase();
                if !lower.starts_with(&prefix) || !lower.ends_with(".md") {
                    continue;
                }
                let suffix_end = lower.len() - 3;
                let suffix = &lower[prefix.len()..suffix_end];
                if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
                    continue;
                }
                let revision = suffix.parse::<u64>().map_err(|_| {
                    format!(
                        "Plan archive revision in '{name}' exceeds the supported integer range."
                    )
                })?;
                if revision > 0 && suffix == revision.to_string() {
                    maximum = maximum.max(revision);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not scan plan archive folder: {error}")),
    }
    let next = maximum
        .checked_add(1)
        .ok_or_else(|| "Plan archive revision number is exhausted.".to_string())?;
    Ok(format!("V{version}-r{next}.md"))
}

pub fn preview_plan_archive(
    project_path: &str,
    archive_directory: &str,
) -> Result<PlanArchivePreview, String> {
    let resolved = resolve_plan_archive_directory(project_path, archive_directory)?;
    let version = development_target_version()?;
    let next_file_name = next_plan_archive_filename(&resolved.directory, &version)?;
    Ok(PlanArchivePreview { next_file_name })
}

fn path_is_within(root: &Path, candidate: &Path) -> bool {
    #[cfg(windows)]
    {
        let root = root
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase();
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

pub fn validate_plan_archive_options(
    project_path: &str,
    options: PlanArchiveOptions,
) -> Result<ValidatedPlanArchive, String> {
    let resolved = resolve_plan_archive_directory(project_path, &options.directory)?;
    development_target_version()?;
    let project_directory =
        CapabilityDir::open_ambient_dir(&resolved.project_root, ambient_authority())
            .map_err(|error| format!("Could not open project directory capability: {error}"))?;
    let opened_project_root = fs::canonicalize(&resolved.project_root)
        .map_err(|error| format!("Could not revalidate opened project directory: {error}"))?;
    if !paths_equal(&opened_project_root, &resolved.project_root) {
        return Err(format!(
            "Project directory changed while its filesystem capability was being opened (expected '{}', now '{}').",
            resolved.project_root.display(),
            opened_project_root.display(),
        ));
    }
    let archive_relative_path = resolved
        .directory
        .strip_prefix(&resolved.project_root)
        .map_err(|_| "Plan archive folder must be inside the selected project.".to_string())?
        .to_path_buf();
    Ok(ValidatedPlanArchive {
        requested_directory: PathBuf::from(options.directory),
        project_root_at_start: resolved.project_root,
        directory_at_start: resolved.directory,
        project_directory: Arc::new(project_directory),
        archive_relative_path,
        #[cfg(test)]
        persistence_test_hook: None,
    })
}

fn persist_approved_plan(
    project_path: &str,
    output: &ValidatedPlanArchive,
    content: &str,
) -> Result<(), String> {
    persist_approved_plan_with_hook(project_path, output, content, |_, _| Ok(()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchivePersistenceStage {
    ComponentValidated,
    CapabilityOpened,
    TempWritten,
}

#[cfg(test)]
type ArchiveTestHook =
    Arc<Mutex<Box<dyn FnMut(ArchivePersistenceStage, &Path) -> std::io::Result<()> + Send>>>;

fn persist_approved_plan_with_hook<F>(
    project_path: &str,
    output: &ValidatedPlanArchive,
    content: &str,
    mut hook: F,
) -> Result<(), String>
where
    F: FnMut(ArchivePersistenceStage, &Path) -> std::io::Result<()>,
{
    // Revalidate the user-selected path at the save boundary. All I/O after opening
    // the archive capability is relative to that stable handle, never its pathname.
    let requested = output.requested_directory.to_string_lossy();
    let started = resolve_plan_archive_directory(project_path, &requested)?;
    if !paths_equal(&started.project_root, &output.project_root_at_start)
        || !paths_equal(&started.directory, &output.directory_at_start)
    {
        return Err(
            "Project or plan archive folder changed during the run; plan was not saved."
                .to_string(),
        );
    }
    let archive_directory = open_archive_capability_with_hook(output, &mut hook)?;
    invoke_archive_hook(
        output,
        &mut hook,
        ArchivePersistenceStage::CapabilityOpened,
        &output.directory_at_start,
    )
    .map_err(|error| format!("Could not validate archive capability: {error}"))?;
    let version = development_target_version()?;
    for _ in 0..MAX_ARCHIVE_INSTALL_ATTEMPTS {
        let filename = next_plan_archive_filename_in_capability(&archive_directory, &version)?;
        let temp_name = format!(".plan-archive-{}.tmp", uuid::Uuid::new_v4());
        let write_result = (|| {
            let mut options = CapabilityOpenOptions::new();
            options.write(true).create_new(true);
            let mut temp_file = archive_directory.open_with(&temp_name, &options)?;
            temp_file.write_all(content.as_bytes())?;
            temp_file.sync_all()?;
            drop(temp_file);
            Ok::<(), std::io::Error>(())
        })();
        if let Err(error) = write_result {
            let _ = archive_directory.remove_file(&temp_name);
            return Err(format!(
                "Could not write temporary plan archive file: {error}"
            ));
        }
        if let Err(error) = invoke_archive_hook(
            output,
            &mut hook,
            ArchivePersistenceStage::TempWritten,
            &output.directory_at_start,
        ) {
            let _ = archive_directory.remove_file(&temp_name);
            return Err(format!(
                "Could not validate archive before install: {error}"
            ));
        }
        match archive_directory.hard_link(&temp_name, &archive_directory, &filename) {
            Ok(()) => {
                archive_directory.remove_file(&temp_name).map_err(|error| {
                    format!("Plan archive installed but temporary link cleanup failed: {error}")
                })?;
                return Ok(());
            }
            Err(error) if is_destination_collision(&error) => {
                archive_directory
                    .remove_file(&temp_name)
                    .map_err(|cleanup| {
                        format!("Could not clean temporary plan archive after collision: {cleanup}")
                    })?;
            }
            Err(error) => {
                let _ = archive_directory.remove_file(&temp_name);
                return Err(format!(
                    "Could not safely install approved plan archive: {error}"
                ));
            }
        }
    }
    Err("Could not allocate a unique plan archive filename after repeated collisions.".to_string())
}

fn invoke_archive_hook<F>(
    _output: &ValidatedPlanArchive,
    hook: &mut F,
    stage: ArchivePersistenceStage,
    path: &Path,
) -> std::io::Result<()>
where
    F: FnMut(ArchivePersistenceStage, &Path) -> std::io::Result<()>,
{
    #[cfg(test)]
    if let Some(test_hook) = &_output.persistence_test_hook {
        (test_hook
            .lock()
            .map_err(|_| std::io::Error::other("archive test hook mutex poisoned"))?)(
            stage, path
        )?;
    }
    hook(stage, path)
}

fn open_archive_capability_with_hook<F>(
    output: &ValidatedPlanArchive,
    hook: &mut F,
) -> Result<CapabilityDir, String>
where
    F: FnMut(ArchivePersistenceStage, &Path) -> std::io::Result<()>,
{
    let mut current = output
        .project_directory
        .open_dir(".")
        .map_err(|error| format!("Could not clone project directory capability: {error}"))?;
    let mut components = output.archive_relative_path.components().peekable();
    while let Some(component) = components.next() {
        let name = match component {
            std::path::Component::Normal(name) => name,
            _ => return Err("Plan archive path is not a safe project-relative path.".to_string()),
        };
        let metadata = match current.symlink_metadata(name) {
            Ok(metadata) => metadata,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && output.archive_relative_path == Path::new(".plan")
                    && components.peek().is_none() =>
            {
                current.create_dir(name).map_err(|create_error| {
                    format!("Could not create default .plan folder: {create_error}")
                })?;
                current.symlink_metadata(name).map_err(|metadata_error| {
                    format!("Could not inspect created .plan folder: {metadata_error}")
                })?
            }
            Err(error) => return Err(format!("Could not inspect plan archive component: {error}")),
        };
        if is_capability_symlink_or_reparse(&metadata) {
            return Err(
                "Plan archive path cannot contain symlink or reparse-point components.".to_string(),
            );
        }
        if !metadata.is_dir() {
            return Err("Plan archive path component is not a directory.".to_string());
        }
        invoke_archive_hook(
            output,
            hook,
            ArchivePersistenceStage::ComponentValidated,
            &output.directory_at_start,
        )
        .map_err(|error| format!("Could not validate plan archive component: {error}"))?;
        let child = cap_primitives::fs::open_dir_nofollow(
            &current.as_filelike_view::<std::fs::File>(),
            Path::new(name),
        )
        .map_err(|error| format!("Could not open plan archive directory capability: {error}"))?;
        current = CapabilityDir::from_std_file(child);
    }
    Ok(current)
}

fn is_capability_symlink_or_reparse(metadata: &cap_std::fs::Metadata) -> bool {
    if metadata.is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

fn next_plan_archive_filename_in_capability(
    directory: &CapabilityDir,
    version: &str,
) -> Result<String, String> {
    let version = parse_development_version(version)?;
    let prefix = format!("v{version}-r").to_ascii_lowercase();
    let mut maximum = 0u64;
    let entries = directory
        .entries()
        .map_err(|error| format!("Could not scan plan archive folder: {error}"))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not scan plan archive folder: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if !lower.starts_with(&prefix) || !lower.ends_with(".md") {
            continue;
        }
        let suffix_end = lower.len() - 3;
        let suffix = &lower[prefix.len()..suffix_end];
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let revision = suffix.parse::<u64>().map_err(|_| {
            format!("Plan archive revision in '{name}' exceeds the supported integer range.")
        })?;
        if revision > 0 && suffix == revision.to_string() {
            maximum = maximum.max(revision);
        }
    }
    let next = maximum
        .checked_add(1)
        .ok_or_else(|| "Plan archive revision number is exhausted.".to_string())?;
    Ok(format!("V{version}-r{next}.md"))
}

fn is_destination_collision(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        return true;
    }
    #[cfg(windows)]
    {
        return matches!(error.raw_os_error(), Some(80 | 183));
    }
    #[cfg(not(windows))]
    false
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[derive(Clone)]
pub struct OrchestratorEngine {
    context_builder: ContextBuilder,
    validation_runner: ValidationRunner,
    finding_aggregator: FindingAggregator,
    provider_adapter: ProviderAdapter,
    ollama_adapter: OllamaAdapter,
    codex_cli_adapter: CodexCliAdapter,
    direct_mcp_adapter: DirectMcpAdapter,
    direct_mcp_audit_callback: Option<DirectMcpAuditCallback>,
    #[cfg(test)]
    scripted_adapter_executor: Option<Arc<ScriptedAdapterExecutor>>,
    #[cfg(test)]
    scripted_validation_executor: Option<Arc<ScriptedValidationExecutor>>,
    task_submission_hook: Option<TaskSubmissionHook>,
}

impl std::fmt::Debug for OrchestratorEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestratorEngine")
            .field("context_builder", &self.context_builder)
            .field("validation_runner", &self.validation_runner)
            .field("finding_aggregator", &self.finding_aggregator)
            .field("direct_mcp_adapter", &self.direct_mcp_adapter)
            .field("direct_mcp_audit_callback", &self.direct_mcp_audit_callback.as_ref().map(|_| "configured"))
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct TaskSubmissionHook(
    Arc<dyn Fn(OrchestratorTaskEnvelope) -> Result<SubmitTaskRequest, String> + Send + Sync>,
);

impl std::fmt::Debug for TaskSubmissionHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TaskSubmissionHook(..)")
    }
}

#[cfg(test)]
#[derive(Default)]
struct ScriptedAdapterExecutor {
    outputs: Mutex<VecDeque<(AgentRole, AdapterExecutionOutput)>>,
    calls: Mutex<Vec<AgentRole>>,
    captured_prompts: Mutex<Vec<(AgentRole, String)>>,
    before_role_symlink_swap: Mutex<Option<(AgentRole, PathBuf, PathBuf)>>,
    implementer_prompt: Mutex<Option<String>>,
    plan_file_to_observe: Option<PathBuf>,
    plan_exists_at_implementer: Mutex<Option<bool>>,
    on_reviewer_execute: Mutex<Option<Arc<dyn Fn(&Path) + Send + Sync>>>,
    symlink_op_hook: Mutex<Option<Arc<dyn Fn(SymlinkOperation, &Path, &Path) -> std::io::Result<()> + Send + Sync>>>,
    gitmodules_cmd_override: Mutex<Option<Arc<dyn Fn(&Path, &[&str]) -> std::io::Result<std::process::Output> + Send + Sync>>>,
    symlink_op_recorder: Mutex<Vec<(PathBuf, SymlinkOperation)>>,
    cleanup_git_worktree_remove_override: Mutex<Option<Arc<dyn Fn(&Path, &Path) -> Result<(), String> + Send + Sync>>>,
    cleanup_fs_remove_override: Mutex<Option<Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>>>,
}

#[cfg(test)]
impl std::fmt::Debug for ScriptedAdapterExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptedAdapterExecutor")
            .field("outputs", &self.outputs)
            .field("calls", &self.calls)
            .field("before_role_symlink_swap", &self.before_role_symlink_swap)
            .field("implementer_prompt", &self.implementer_prompt)
            .field("plan_file_to_observe", &self.plan_file_to_observe)
            .field("plan_exists_at_implementer", &self.plan_exists_at_implementer)
            .finish()
    }
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
            direct_mcp_adapter: DirectMcpAdapter::new(),
            direct_mcp_audit_callback: None,
            #[cfg(test)]
            scripted_adapter_executor: None,
            #[cfg(test)]
            scripted_validation_executor: None,
            task_submission_hook: None,
        }
    }

    pub fn with_direct_mcp_adapter(mut self, adapter: DirectMcpAdapter) -> Self {
        self.direct_mcp_adapter = adapter;
        self
    }

    pub fn with_direct_mcp_audit_callback(mut self, callback: DirectMcpAuditCallback) -> Self {
        self.direct_mcp_audit_callback = Some(callback);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_scripted_adapters(
        scripted_adapters: Vec<(AgentRole, AdapterExecutionOutput)>,
        scripted_validations: Vec<ValidationRunSummary>,
    ) -> Self {
        let mut engine = Self::new();
        engine.scripted_adapter_executor = Some(Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(scripted_adapters.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
            ..Default::default()
        }));
        engine.scripted_validation_executor = Some(Arc::new(ScriptedValidationExecutor {
            summaries: Mutex::new(scripted_validations.into_iter().collect()),
            runs: Mutex::new(0),
        }));
        engine
    }

    #[cfg(test)]
    pub(crate) fn with_task_submission_hook(
        mut self,
        hook: Arc<dyn Fn(OrchestratorTaskEnvelope) -> Result<SubmitTaskRequest, String> + Send + Sync>,
    ) -> Self {
        self.task_submission_hook = Some(TaskSubmissionHook(hook));
        self
    }

    /// Executes the development review workflow adhering to exact terminal conditions, independent counters, and fail-closed reviews.
    pub async fn run_workflow(
        &self,
        run_id: String,
        snapshot: RunConfigurationSnapshot,
        task_prompt: String,
        workflow_type: String, // "full_loop" | "plan_only" | "implement_only" | "review_only" | "human_gated_loop"
        plan_archive: Option<ValidatedPlanArchive>,
        authorized_custom_gates: Vec<AuthorizedCustomGate>,
        mut pause_rx: watch::Receiver<RunControlState>,
        cancel_token: CancellationToken,
        mut clarification_rx: mpsc::Receiver<String>,
        mut blocking_resolution_rx: mpsc::Receiver<BlockingResolution>,
        human_gate_rx: mpsc::Receiver<HumanGateDecision>,
        worker_reclaim_rx: mpsc::Receiver<()>,
        on_event: EventCallback,
        on_log: LogCallback,
        dispatch_persistence: Option<super::mailbox::DispatchPersistenceCallback>,
    ) -> Result<WorkflowState, String> {
        if workflow_type == "human_gated_loop" {
            return self
                .run_human_gated_workflow(
                    run_id,
                    snapshot,
                    task_prompt,
                    plan_archive,
                    authorized_custom_gates,
                    pause_rx,
                    cancel_token,
                    clarification_rx,
                    blocking_resolution_rx,
                    human_gate_rx,
                    worker_reclaim_rx,
                    on_event,
                    on_log,
                    dispatch_persistence,
                )
                .await;
        }

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
            // MCP adapter validation
            if profile.adapter == ExecutionAdapterType::Mcp {
                if *role != AgentRole::Planner && *role != AgentRole::PlanReviewer {
                    return Err(format!(
                        "MCP adapter is currently unavailable for role '{:?}'.",
                        role
                    ));
                }
                DirectMcpAdapter::validate_mcp_assignment(
                    role,
                    profile,
                    &snapshot.mcp_servers,
                    &project_path,
                )
                .map_err(|e| e.to_string())?;
            }
            validate_workflow_role_capabilities(&workflow_type, role, Some(profile))
                .map_err(|e| format!("Capability check failed: {}", e.message))?;
        }

        log(format!(
            "[Engine] Starting Orchestrator run {} for workflow '{}'",
            run_id, workflow_type
        ));

        let project_path_buf = PathBuf::from(&snapshot.project_path);
        let frozen_plan_snapshot = super::plan_workspace::capture_frozen_plan_snapshot(
            &project_path_buf,
            &snapshot.plan_workspace,
        )?;
        frozen_plan_snapshot.ensure_run_entry_allowed()?;
        let frozen_plan_payload = frozen_plan_snapshot.to_frozen_plan_payload();
        let plan_ctx_pair = Some((
            &frozen_plan_snapshot.plan_context,
            frozen_plan_snapshot.effective_plan_content.as_deref(),
        ));
        let plan_context_ref = Some(&frozen_plan_snapshot.plan_context);
        let frozen_plan_ref = frozen_plan_payload.as_ref();
        let mcp_servers_ref = Some(&snapshot.mcp_servers);

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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: Some(WorkflowState::BuildingContext),
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;

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
                    plan_ctx_pair,
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
                        mcp_servers_ref,
                        plan_context_ref,
                        frozen_plan_ref,
                        Some(&cancel_token),
                        &on_event,
                    )
                    .await?;

                let cr_result = self.finding_aggregator.parse_review_output(&cr_out.content);
                log(format!(
                    "[Engine] Review-only verdict: {:?} ({} findings)",
                    cr_result.verdict,
                    cr_result.findings.len()
                ));

                let cr_approved = classify_review_verdict(cr_result.verdict) == ReviewAction::Approved;
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::CodeReview,
                    iteration_info: Some("Single review pass".to_string()),
                    message: format!("Code review verdict: {:?}", cr_result.verdict),
                    review_result: Some(cr_result.clone()),
                    validation_summary: None,
                    plan_text: None,
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: if cr_approved {
                        Some(WorkflowState::CodeReview)
                    } else {
                        None
                    },
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;

                match classify_review_verdict(cr_result.verdict) {
                    ReviewAction::Approved => {
                        log("[Engine] Review-only APPROVED! Completing workflow with READY.".to_string());
                        on_event(StepProgressEvent {
                            run_id: run_id.clone(),
                            step: WorkflowState::Complete,
                            iteration_info: None,
                            message: "Review-only workflow completed with approved verdict (READY)".to_string(),
                            review_result: Some(cr_result),
                            validation_summary: None,
                            plan_text: None,
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
                            completed_stage: Some(WorkflowState::CodeReview),
                            plan_review_count: Some(plan_review_count),
                            fix_count: Some(fix_count),
                            code_review_count: Some(code_review_count),
                        })?;
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
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
                            completed_stage: None,
                            plan_review_count: Some(plan_review_count),
                            fix_count: Some(fix_count),
                            code_review_count: Some(code_review_count),
                        })?;
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
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
                            completed_stage: None,
                            plan_review_count: Some(plan_review_count),
                            fix_count: Some(fix_count),
                            code_review_count: Some(code_review_count),
                        })?;
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
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
                            completed_stage: None,
                            plan_review_count: Some(plan_review_count),
                            fix_count: Some(fix_count),
                            code_review_count: Some(code_review_count),
                        })?;
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: Some(WorkflowState::BuildingContext),
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
            })?;

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
                plan_ctx_pair,
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
                    mcp_servers_ref,
                    plan_context_ref,
                    frozen_plan_ref,
                    Some(&cancel_token),
                    &on_event,
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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        completed_stage: None,
                        plan_review_count: Some(plan_review_count),
                        fix_count: Some(fix_count),
                        code_review_count: Some(code_review_count),
                    })?;
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: if plan_review_count == 1 {
                        Some(WorkflowState::PlanGeneration)
                    } else {
                        Some(WorkflowState::PlanRevision)
                    },
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;

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
                    plan_ctx_pair,
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
                        mcp_servers_ref,
                        plan_context_ref,
                        frozen_plan_ref,
                        Some(&cancel_token),
                        &on_event,
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

                let pr_approved = classify_review_verdict(review_res.verdict) == ReviewAction::Approved;
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: if pr_approved {
                        Some(WorkflowState::PlanReview)
                    } else {
                        None
                    },
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;

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
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: Some("clarification_required".to_string()),
                            completed_stage: None,
                            plan_review_count: Some(plan_review_count),
                            fix_count: Some(fix_count),
                            code_review_count: Some(code_review_count),
                        })?;

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
                            plan_ctx_pair,
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
                                mcp_servers_ref,
                                plan_context_ref,
                                frozen_plan_ref,
                                Some(&cancel_token),
                                &on_event,
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
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
                            completed_stage: None,
                            plan_review_count: Some(plan_review_count),
                            fix_count: Some(fix_count),
                            code_review_count: Some(code_review_count),
                        })?;
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
                                antigravity_dispatches: None,
                                antigravity_dispatch_limit: None,
                                budget_scope: None,
                                waiting_reason: None,
                                completed_stage: None,
                                plan_review_count: Some(plan_review_count),
                                fix_count: Some(fix_count),
                                code_review_count: Some(code_review_count),
                            })?;
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
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
                            completed_stage: None,
                            plan_review_count: Some(plan_review_count),
                            fix_count: Some(fix_count),
                            code_review_count: Some(code_review_count),
                        })?;

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
                            plan_ctx_pair,
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
                                mcp_servers_ref,
                                plan_context_ref,
                                frozen_plan_ref,
                                Some(&cancel_token),
                                &on_event,
                            )
                            .await?;

                        current_plan = rev_output.content;
                    }
                }
            }

            if is_plan_only {
                if plan_approved {
                    if let Some(output) = &plan_archive {
                        let safe_plan =
                            super::secrets::SecretRedactor::new().redact_secrets(&current_plan);
                        if let Err(error) =
                            persist_approved_plan(&snapshot.project_path, output, &safe_plan)
                        {
                            let safe_error =
                                super::secrets::SecretRedactor::new().redact_secrets(&error);
                            log(format!(
                                "[Engine] Approved plan could not be saved: {safe_error}"
                            ));
                            on_event(StepProgressEvent {
                                run_id: run_id.clone(),
                                step: WorkflowState::Failed,
                                iteration_info: Some("plan_save_failed".to_string()),
                                message: format!("Approved plan could not be saved: {safe_error}"),
                                review_result: None,
                                validation_summary: None,
                                plan_text: Some(current_plan),
                                antigravity_dispatches: None,
                                antigravity_dispatch_limit: None,
                                budget_scope: None,
                                waiting_reason: None,
                                completed_stage: None,
                                plan_review_count: Some(plan_review_count),
                                fix_count: Some(fix_count),
                                code_review_count: Some(code_review_count),
                            })?;
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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        completed_stage: Some(WorkflowState::PlanReview),
                        plan_review_count: Some(plan_review_count),
                        fix_count: Some(fix_count),
                        code_review_count: Some(code_review_count),
                    })?;
                    return Ok(WorkflowState::Complete);
                } else {
                    return Ok(WorkflowState::Failed);
                }
            }
        }

        // In Full Loop, do not begin implementation until the final reviewed plan
        // is safely persisted. Other workflows never consume this archive option.
        if workflow_type == "full_loop" {
            if let Some(output) = &plan_archive {
                let safe_plan = super::secrets::SecretRedactor::new().redact_secrets(&current_plan);
                if let Err(error) =
                    persist_approved_plan(&snapshot.project_path, output, &safe_plan)
                {
                    let safe_error = super::secrets::SecretRedactor::new().redact_secrets(&error);
                    log(format!(
                        "[Engine] Approved plan could not be saved: {safe_error}"
                    ));
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::Failed,
                        iteration_info: Some("plan_save_failed".to_string()),
                        message: format!("Approved plan could not be saved: {safe_error}"),
                        review_result: None,
                        validation_summary: None,
                        plan_text: Some(current_plan),
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        completed_stage: None,
                        plan_review_count: Some(plan_review_count),
                        fix_count: Some(fix_count),
                        code_review_count: Some(code_review_count),
                    })?;
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: if is_implement_only {
                    Some(WorkflowState::BuildingContext)
                } else {
                    Some(WorkflowState::PlanReview)
                },
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
            })?;

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
                plan_ctx_pair,
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
                    mcp_servers_ref,
                    plan_context_ref,
                    frozen_plan_ref,
                    Some(&cancel_token),
                    &on_event,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: if fix_count == 0 {
                    Some(WorkflowState::Implementation)
                } else {
                    Some(WorkflowState::Fix)
                },
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
            })?;

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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: if val_summary.passed {
                    Some(WorkflowState::Validation)
                } else {
                    None
                },
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
            })?;

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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        completed_stage: None,
                        plan_review_count: Some(plan_review_count),
                        fix_count: Some(fix_count),
                        code_review_count: Some(code_review_count),
                    })?;
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;

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
                    plan_ctx_pair,
                    Some(&cancel_token),
                )
                .await?;

                self.execute_adapter(
                    AgentRole::Fixer,
                    fixer_profile,
                    fix_system,
                    &fix_ctx.prompt,
                    &project_path,
                    mcp_servers_ref,
                    plan_context_ref,
                    frozen_plan_ref,
                    Some(&cancel_token),
                    &on_event,
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: Some(WorkflowState::Validation),
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: Some(WorkflowState::Validation),
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
            })?;

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
                plan_ctx_pair,
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
                    mcp_servers_ref,
                    plan_context_ref,
                    frozen_plan_ref,
                    Some(&cancel_token),
                    &on_event,
                )
                .await?;

            let cr_result = self.finding_aggregator.parse_review_output(&cr_out.content);
            log(format!(
                "[Engine] Code review verdict: {:?} ({} findings)",
                cr_result.verdict,
                cr_result.findings.len()
            ));

            let cr_approved = classify_review_verdict(cr_result.verdict) == ReviewAction::Approved;
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: if cr_approved {
                    Some(WorkflowState::CodeReview)
                } else {
                    None
                },
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
            })?;

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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: Some("clarification_required".to_string()),
                    completed_stage: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;

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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;

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
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
                            completed_stage: None,
                            plan_review_count: Some(plan_review_count),
                            fix_count: Some(fix_count),
                            code_review_count: Some(code_review_count),
                        })?;
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                })?;
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: None,
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
            })?;

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
                plan_ctx_pair,
                Some(&cancel_token),
            )
            .await?;

            self.execute_adapter(
                AgentRole::Fixer,
                fixer_profile,
                fix_system,
                &fix_ctx.prompt,
                &project_path,
                mcp_servers_ref,
                plan_context_ref,
                frozen_plan_ref,
                Some(&cancel_token),
                &on_event,
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            completed_stage: Some(WorkflowState::CodeReview),
            plan_review_count: Some(plan_review_count),
            fix_count: Some(fix_count),
            code_review_count: Some(code_review_count),
        })?;

        Ok(WorkflowState::Complete)
    }

    pub(crate) async fn execute_adapter(
        &self,
        role: AgentRole,
        profile: &OrchestratorProfile,
        system_prompt: &str,
        user_prompt: &str,
        project_path: &Path,
        mcp_servers: Option<&HashMap<String, McpServerConfig>>,
        plan_context: Option<&PlanContext>,
        frozen_plan: Option<&FrozenPlanPayload>,
        cancel_token: Option<&CancellationToken>,
        _on_event: &EventCallback,
    ) -> Result<AdapterExecutionOutput, String> {
        #[cfg(test)]
        if let Some(scripted_executor) = &self.scripted_adapter_executor {
            let symlink_swap = {
                let mut pending = scripted_executor.before_role_symlink_swap.lock().unwrap();
                if pending
                    .as_ref()
                    .is_some_and(|(expected_role, _, _)| expected_role == &role)
                {
                    pending.take()
                } else {
                    None
                }
            };
            if let Some((_, link_path, new_target)) = symlink_swap {
                #[cfg(windows)]
                {
                    if link_path.is_dir() {
                        std::fs::remove_dir_all(&link_path)
                            .map_err(|e| format!("test archive removal failed: {e}"))?;
                    } else {
                        std::fs::remove_file(&link_path)
                            .map_err(|e| format!("test symlink removal failed: {e}"))?;
                    }
                    std::os::windows::fs::symlink_dir(&new_target, &link_path)
                        .map_err(|e| format!("test symlink replacement failed: {e}"))?;
                }
                #[cfg(unix)]
                {
                    if link_path.is_symlink() {
                        std::fs::remove_file(&link_path)
                            .map_err(|e| format!("test symlink removal failed: {e}"))?;
                    } else {
                        std::fs::remove_dir_all(&link_path)
                            .map_err(|e| format!("test archive removal failed: {e}"))?;
                    }
                    std::os::unix::fs::symlink(&new_target, &link_path)
                        .map_err(|e| format!("test symlink replacement failed: {e}"))?;
                }
            }
            let Some((expected_role, output)) =
                scripted_executor.outputs.lock().unwrap().pop_front()
            else {
                return Err(
                    "Scripted test adapter has no response for the requested role.".to_string(),
                );
            };
            scripted_executor.calls.lock().unwrap().push(role.clone());
            scripted_executor.captured_prompts.lock().unwrap().push((role.clone(), user_prompt.to_string()));
            if role == AgentRole::Implementer {
                *scripted_executor.implementer_prompt.lock().unwrap() =
                    Some(user_prompt.to_string());
                *scripted_executor.plan_exists_at_implementer.lock().unwrap() = scripted_executor
                    .plan_file_to_observe
                    .as_ref()
                    .map(|directory| {
                        fs::read_dir(directory)
                            .ok()
                            .into_iter()
                            .flatten()
                            .filter_map(Result::ok)
                            .any(|entry| {
                                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                                name.starts_with('v') && name.ends_with(".md")
                            })
                    });
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
            role: role.clone(),
            profile: profile.clone(),
            system_prompt: system_prompt.to_string(),
            user_prompt: user_prompt.to_string(),
            project_path: project_path.to_path_buf(),
            temperature: None,
        };

        match profile.adapter {
            ExecutionAdapterType::Provider => {
                self.provider_adapter.execute(&input, cancel_token).await
            }
            ExecutionAdapterType::Ollama => self.ollama_adapter.execute(&input, cancel_token).await,
            ExecutionAdapterType::Cli => self.codex_cli_adapter.execute(&input, cancel_token).await,
            ExecutionAdapterType::Mcp => {
                if role != AgentRole::Planner && role != AgentRole::PlanReviewer {
                    return Err(format!(
                        "MCP role authorization error: role '{role:?}' is not permitted to use MCP adapter."
                    ));
                }
                let empty_registry = HashMap::new();
                let server_registry = mcp_servers.unwrap_or(&empty_registry);
                let server_config = DirectMcpAdapter::validate_mcp_assignment(
                    &role,
                    profile,
                    server_registry,
                    project_path,
                )
                .map_err(|e| e.to_string())?;
                let direct_input = DirectMcpExecutionInput {
                    adapter_input: &input,
                    server_config: &server_config,
                    plan_context,
                    frozen_plan,
                };
                match self
                    .direct_mcp_adapter
                    .execute_with_audit(&direct_input, cancel_token)
                    .await
                {
                    Ok((output, audit)) => {
                        self.direct_mcp_audit_callback
                            .as_ref()
                            .ok_or_else(|| "[DirectMcp] Durable audit callback is unavailable.".to_string())?(audit)?;
                        Ok(output)
                    }
                    Err((error, audit)) => {
                        self.direct_mcp_audit_callback
                            .as_ref()
                            .ok_or_else(|| "[DirectMcp] Durable audit callback is unavailable.".to_string())?(audit)
                        .map_err(|persist_error| {
                            format!("{}; additionally, MCP audit persistence failed: {}", error, persist_error)
                        })?;
                        Err(error.to_string())
                    }
                }
            }
            ExecutionAdapterType::Antigravity => {
                Err("Antigravity adapter is driven asynchronously via the Localhost HTTP Mailbox and orchestrator state machine.".to_string())
            }
        }
    }

    /// Dedicated read-only review execution path for Code Reviewer.
    /// Strictly enforces read-only sandbox mode and operates inside the disposable worktree.
    pub async fn execute_sandboxed_review(
        &self,
        profile: &OrchestratorProfile,
        system_prompt: &str,
        user_prompt: &str,
        worktree_path: &Path,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<AdapterExecutionOutput, String> {
        if !profile.capabilities.contains(&super::types::ProfileCapability::Review) {
            return Err("Preflight validation failure: Code Reviewer role must have Review capability.".to_string());
        }

        #[cfg(test)]
        if let Some(scripted_executor) = &self.scripted_adapter_executor {
            if let Some(hook) = scripted_executor.on_reviewer_execute.lock().unwrap().as_ref() {
                hook(worktree_path);
            }
            let Some((expected_role, output)) =
                scripted_executor.outputs.lock().unwrap().pop_front()
            else {
                return Err(
                    "Scripted test adapter has no response for the requested role.".to_string(),
                );
            };
            scripted_executor.calls.lock().unwrap().push(AgentRole::CodeReviewer);
            if expected_role != AgentRole::CodeReviewer {
                return Err(format!(
                    "Scripted test adapter expected role {:?}, received {:?}.",
                    expected_role, AgentRole::CodeReviewer
                ));
            }
            return Ok(output);
        }

        let input = AdapterExecutionInput {
            role: AgentRole::CodeReviewer,
            profile: profile.clone(),
            system_prompt: system_prompt.to_string(),
            user_prompt: user_prompt.to_string(),
            project_path: worktree_path.to_path_buf(),
            temperature: None,
        };

        match profile.adapter {
            ExecutionAdapterType::Provider => {
                self.provider_adapter.execute(&input, cancel_token).await
            }
            ExecutionAdapterType::Ollama => {
                self.ollama_adapter.execute(&input, cancel_token).await
            }
            ExecutionAdapterType::Cli => {
                self.codex_cli_adapter
                    .execute_sandboxed_review(&input, cancel_token)
                    .await
            }
            ExecutionAdapterType::Antigravity | ExecutionAdapterType::Mcp => {
                Err("Unsupported adapter for Code Reviewer; failing closed.".to_string())
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
    pub(super) async fn check_run_control_with_relay(
        &self,
        rx: &mut watch::Receiver<RunControlState>,
        cancel_token: &CancellationToken,
        relay_error: &Arc<std::sync::Mutex<Option<String>>>,
    ) -> Result<(), String> {
        if let Some(err) = read_relay_error(relay_error) {
            return Err(err);
        }
        if cancel_token.is_cancelled() || *rx.borrow() == RunControlState::Cancelled {
            if let Some(err) = read_relay_error(relay_error) {
                return Err(err);
            }
            return Err("Execution cancelled by user.".to_string());
        }
        if *rx.borrow() == RunControlState::Paused {
            while *rx.borrow() == RunControlState::Paused {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        if let Some(err) = read_relay_error(relay_error) {
                            return Err(err);
                        }
                        return Err("Execution cancelled by user.".to_string());
                    }
                    res = rx.changed() => {
                        if res.is_err() {
                            break;
                        }
                    }
                }
            }
            if let Some(err) = read_relay_error(relay_error) {
                return Err(err);
            }
            if cancel_token.is_cancelled() || *rx.borrow() == RunControlState::Cancelled {
                return Err("Execution cancelled by user.".to_string());
            }
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn wait_for_antigravity_submission(
    stage: WorkflowState,
    role: AgentRole,
    task_id_prefix: &str,
    run_id: &str,
    snapshot: &RunConfigurationSnapshot,
    approved_plan: Option<String>,
    task_prompt: Option<String>,
    review_feedback: Option<String>,
    validation_summary: Option<String>,
    frozen_plan_context: Option<&super::plan_workspace::PlanContext>,
    frozen_plan_payload: Option<&super::plan_workspace::FrozenPlanPayload>,
    mailbox_state: &super::mailbox::MailboxState,
    submit_rx: &mut mpsc::Receiver<super::types::SubmitTaskRequest>,
    worker_reclaim_rx: &mut mpsc::Receiver<()>,
    cancel_token: &CancellationToken,
    on_event: &EventCallback,
    log: &(impl Fn(String) + Send + Sync),
    relay_error: Option<&Arc<std::sync::Mutex<Option<String>>>>,
    task_submission_hook: Option<&TaskSubmissionHook>,
) -> Result<AntigravitySubmissionOutcome, String> {
    if let Some(payload) = frozen_plan_payload {
        let ctx = frozen_plan_context.ok_or_else(|| {
            "Frozen plan payload requires its run-owned PlanContext".to_string()
        })?;
        if !ctx.is_plan_bound() {
            return Err("Frozen plan payload requires a resolved plan-bound PlanContext".to_string());
        }
        payload.validate_against_plan_context(ctx)?;
    } else if frozen_plan_context.is_some_and(|ctx| ctx.is_plan_bound()) {
        return Err("Frozen plan payload is required for a plan-bound PlanContext but was missing".to_string());
    }

    let effective_prompt = effective_task_prompt(task_prompt.as_deref(), snapshot.lean_antigravity_mode);
    {
        let mut guard = mailbox_state.inner.lock().await;
        guard.current_state = stage;
        guard.is_claimed = false;
        guard.last_progress_at = None;
        guard.active_task = Some(OrchestratorTaskEnvelope {
            run_id: run_id.to_string(),
            task_id: format!("{}-{}", task_id_prefix, guard.epoch),
            stage,
            role: role.clone(),
            epoch: guard.epoch,
            project_path: snapshot.project_path.clone(),
            approved_plan: approved_plan.clone(),
            task_prompt: effective_prompt.clone(),
            review_feedback: review_feedback.clone(),
            validation_summary: validation_summary.clone(),
            plan_context: frozen_plan_context.cloned(),
            frozen_plan: frozen_plan_payload.cloned(),
        });
        guard.task_notify.notify_waiters();
    }

    // Private deterministic seam for production-workflow recovery tests. It
    // observes the exact Worker envelope at the dispatch boundary without an
    // HTTP claim, external Worker, or provider call.
    if let Some(hook) = task_submission_hook {
        let envelope = mailbox_state.inner.lock().await.active_task.clone()
            .ok_or_else(|| "Scripted Worker task envelope was not created".to_string())?;
        let submission = (hook.0)(envelope.clone())?;
        if submission.run_id != envelope.run_id {
            return Err("Scripted Worker submission run ID did not match the active run".into());
        }
        if submission.task_id != envelope.task_id {
            return Err("Scripted Worker submission task ID did not match the active envelope".into());
        }
        if submission.epoch != envelope.epoch {
            return Err("Scripted Worker submission epoch did not match the active envelope".into());
        }
        return Ok(AntigravitySubmissionOutcome::Submitted(submission));
    }

    let mut lease_check_interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
    lease_check_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        if let Some(re) = relay_error {
            if let Some(err) = read_relay_error(re) {
                return Err(err);
            }
        }
        tokio::select! {
            _ = cancel_token.cancelled() => {
                if let Some(re) = relay_error {
                    if let Some(err) = read_relay_error(re) {
                        return Err(err);
                    }
                }
                return Err("Execution cancelled by user.".to_string());
            }
            _ = lease_check_interval.tick() => {
                let (timed_out, budget_exhausted_details) = {
                    let mut guard = mailbox_state.inner.lock().await;
                    if guard.budget_exhausted {
                        (false, guard.budget_exhausted_details.clone())
                    } else if guard.is_claimed {
                        if let Some(last_progress) = guard.last_progress_at {
                            if last_progress.elapsed() >= guard.lease_timeout_duration {
                                guard.is_claimed = false;
                                guard.last_progress_at = None;
                                guard.current_state = WorkflowState::WaitingForUser;
                                (true, None)
                            } else {
                                (false, None)
                            }
                        } else {
                            (false, None)
                        }
                    } else {
                        (false, None)
                    }
                };

                if let Some(details) = budget_exhausted_details {
                    let scope_str = match details.scope {
                        super::mailbox::BudgetExhaustionScope::Task => "task",
                        super::mailbox::BudgetExhaustionScope::Run => "run",
                    };
                    on_event(StepProgressEvent {
                        run_id: run_id.to_string(),
                        step: WorkflowState::WaitingForUser,
                        iteration_info: None,
                        // Budget exhaustion is rendered from the typed scope/count fields
                        // by the localized Dashboard UI, not as backend English prose.
                        message: String::new(),
                        review_result: None,
                        validation_summary: None,
                        plan_text: approved_plan.clone(),
                        antigravity_dispatches: Some(details.current),
                        antigravity_dispatch_limit: Some(details.limit),
                        budget_scope: Some(scope_str.to_string()),
                        waiting_reason: Some("budget_exhausted".to_string()),
                        ..Default::default()
                    })?;
                    return Ok(AntigravitySubmissionOutcome::BudgetExhausted(details));
                }

                if timed_out {
                    log(format!("[Engine] Worker lease timed out for stage {:?}. Transitioning to WaitingForUser.", stage));
                    on_event(StepProgressEvent {
                        run_id: run_id.to_string(),
                        step: WorkflowState::WaitingForUser,
                        iteration_info: None,
                        message: "Antigravity worker disconnected or lease timed out without progress. Waiting for human confirmation to reclaim.".to_string(),
                        review_result: None,
                        validation_summary: None,
                        plan_text: approved_plan.clone(),
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: Some("worker_disconnected".to_string()),
                        ..Default::default()
                    })?;
                }
            }
            sub = submit_rx.recv() => {
                let sub = sub.ok_or_else(|| "Mailbox submit channel closed unexpectedly.".to_string())?;
                return Ok(AntigravitySubmissionOutcome::Submitted(sub));
            }
            reclaim = worker_reclaim_rx.recv() => {
                if reclaim.is_some() {
                    let is_waiting = {
                        let guard = mailbox_state.inner.lock().await;
                        guard.current_state == WorkflowState::WaitingForUser
                    };
                    if !is_waiting {
                        log(format!("[Engine] Stale or early worker reclaim signal dropped because engine state is not WaitingForUser (stage {:?}).", stage));
                        continue;
                    }
                    let new_epoch = {
                        let mut guard = mailbox_state.inner.lock().await;
                        guard.epoch += 1;
                        guard.is_claimed = false;
                        guard.last_progress_at = None;
                        guard.active_task = Some(OrchestratorTaskEnvelope {
                            run_id: run_id.to_string(),
                            task_id: format!("{}-{}", task_id_prefix, guard.epoch),
                            stage,
                            role: role.clone(),
                            epoch: guard.epoch,
                            project_path: snapshot.project_path.clone(),
                            approved_plan: approved_plan.clone(),
                            task_prompt: effective_prompt.clone(),
                            review_feedback: review_feedback.clone(),
                            validation_summary: validation_summary.clone(),
                            plan_context: frozen_plan_context.cloned(),
                            frozen_plan: frozen_plan_payload.cloned(),
                        });
                        guard.current_state = WorkflowState::AwaitingAntigravityClaim;
                        guard.task_notify.notify_waiters();
                        guard.epoch
                    };
                    log(format!("[Engine] Worker stopped confirmed. Re-opened claim for stage {:?} on epoch {}.", stage, new_epoch));
                    on_event(StepProgressEvent {
                        run_id: run_id.to_string(),
                        step: WorkflowState::AwaitingAntigravityClaim,
                        iteration_info: Some(format!("Epoch {}", new_epoch)),
                        message: format!("Previous worker confirmed stopped. Ready for worker claim with epoch {}.", new_epoch),
                        review_result: None,
                        validation_summary: None,
                        plan_text: approved_plan.clone(),
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        ..Default::default()
                    })?;
                }
            }
        }
    }
}

impl OrchestratorEngine {
    /// Executes the human-gated development loop with Antigravity worker harness,
    /// Localhost HTTP Mailbox, sandboxed disposable worktree code review, and HumanGate approval.
    pub async fn run_human_gated_workflow(
        &self,
        run_id: String,
        snapshot: RunConfigurationSnapshot,
        task_prompt: String,
        plan_archive: Option<ValidatedPlanArchive>,
        authorized_custom_gates: Vec<AuthorizedCustomGate>,
        pause_rx: watch::Receiver<RunControlState>,
        cancel_token: CancellationToken,
        _clarification_rx: mpsc::Receiver<String>,
        blocking_resolution_rx: mpsc::Receiver<BlockingResolution>,
        human_gate_rx: mpsc::Receiver<HumanGateDecision>,
        worker_reclaim_rx: mpsc::Receiver<()>,
        on_event: EventCallback,
        on_log: LogCallback,
        dispatch_persistence: Option<super::mailbox::DispatchPersistenceCallback>,
    ) -> Result<WorkflowState, String> {
        self.run_human_gated_workflow_with_resume(
            run_id, snapshot, task_prompt, plan_archive, authorized_custom_gates,
            pause_rx, cancel_token, _clarification_rx, blocking_resolution_rx,
            human_gate_rx, worker_reclaim_rx, on_event, on_log, None, dispatch_persistence,
        ).await
    }

    pub async fn run_human_gated_workflow_with_resume(
        &self,
        run_id: String,
        snapshot: RunConfigurationSnapshot,
        task_prompt: String,
        plan_archive: Option<ValidatedPlanArchive>,
        authorized_custom_gates: Vec<AuthorizedCustomGate>,
        mut pause_rx: watch::Receiver<RunControlState>,
        cancel_token: CancellationToken,
        _clarification_rx: mpsc::Receiver<String>,
        mut blocking_resolution_rx: mpsc::Receiver<BlockingResolution>,
        mut human_gate_rx: mpsc::Receiver<HumanGateDecision>,
        mut worker_reclaim_rx: mpsc::Receiver<()>,
        on_event: EventCallback,
        on_log: LogCallback,
        resume: Option<super::recovery::HumanGatedResumeContext>,
        dispatch_persistence: Option<super::mailbox::DispatchPersistenceCallback>,
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

        let frozen_plan_snapshot = super::plan_workspace::capture_frozen_plan_snapshot(
            &project_path,
            &snapshot.plan_workspace,
        )?;
        frozen_plan_snapshot.ensure_run_entry_allowed()?;
        let frozen_plan_payload = frozen_plan_snapshot.to_frozen_plan_payload();
        let plan_ctx_pair = Some((
            &frozen_plan_snapshot.plan_context,
            frozen_plan_snapshot.effective_plan_content.as_deref(),
        ));
        let plan_context_ref = Some(&frozen_plan_snapshot.plan_context);
        let frozen_plan_ref = frozen_plan_payload.as_ref();
        let mcp_servers_ref = Some(&snapshot.mcp_servers);

        // Start Localhost HTTP Mailbox Server
        let (mailbox_server, mailbox_state, mut submit_rx, progress_rx) =
            MailboxServer::start(run_id.clone(), snapshot.project_path.clone()).await?;
        if let Some(callback) = dispatch_persistence {
            mailbox_state.inner.lock().await.dispatch_persistence = Some(callback);
        }
        if let Some(ref resume_context) = resume {
            let mut mailbox = mailbox_state.inner.lock().await;
            mailbox.epoch = resume_context.mailbox_epoch;
            mailbox.total_dispatches = resume_context.antigravity_dispatches;
            mailbox.task_dispatches = resume_context.antigravity_task_dispatches.clone();
            mailbox.current_state = resume_context.stage;
        }

        // Event callback wrapper that enriches events with live dispatch budget metrics
        let mailbox_state_for_events = mailbox_state.clone();
        let on_event_raw = on_event.clone();
        let on_event: EventCallback = Arc::new(move |mut ev: StepProgressEvent| {
            if ev.antigravity_dispatches.is_none() {
                if let Ok(guard) = mailbox_state_for_events.inner.try_lock() {
                    ev.antigravity_dispatches = Some(guard.total_dispatches);
                    ev.antigravity_dispatch_limit = Some(guard.max_dispatches_per_run);
                }
            }
            on_event_raw(ev)
        });

        // Spawn background progress event relay
        let relay_error = Arc::new(std::sync::Mutex::new(None));
        let progress_relay = spawn_mailbox_progress_relay(
            progress_rx,
            run_id.clone(),
            mailbox_state.clone(),
            on_event.clone(),
            relay_error.clone(),
            cancel_token.clone(),
        );

        // Independent counters
        let mut plan_review_count = resume.as_ref().map_or(0, |ctx| ctx.plan_review_count);
        let mut code_review_count = resume.as_ref().map_or(0, |ctx| ctx.code_review_count);
        let mut fix_count = resume.as_ref().map_or(0, |ctx| ctx.fix_count);
        let mut current_plan = resume.as_ref().map_or_else(String::new, |ctx| ctx.plan_text.clone());
        let mut latest_val_summary: Option<ValidationRunSummary> = None;
        let mut latest_cr_result: Option<ReviewResult> = None;

        log(format!("[Engine] Starting Human-Gated Loop for run {}", run_id));

        // ==========================================
        // Stage 1: PlanDraft (Planner)
        // ==========================================
        if resume.is_none() {
        self.check_run_control_with_relay(&mut pause_rx, &cancel_token, &relay_error).await?;

        on_event(StepProgressEvent {
            run_id: run_id.clone(),
            step: WorkflowState::PlanDraft,
            iteration_info: None,
            message: "Generating initial implementation plan draft...".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: None,
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            plan_review_count: Some(plan_review_count),
            fix_count: Some(fix_count),
            code_review_count: Some(code_review_count),
            ..Default::default()
        })?;

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
            plan_ctx_pair,
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
                mcp_servers_ref,
                plan_context_ref,
                frozen_plan_ref,
                Some(&cancel_token),
                &on_event,
            )
            .await?;

        current_plan = output.content;
        log(format!(
            "[Engine] Initial plan draft generated ({} chars)",
            current_plan.len()
        ));

        // ==========================================
        // Stage 2: PlanIntegration (Antigravity Harness)
        // ==========================================
        self.check_run_control_with_relay(&mut pause_rx, &cancel_token, &relay_error).await?;

        on_event(StepProgressEvent {
            run_id: run_id.clone(),
            step: WorkflowState::PlanIntegration,
            iteration_info: None,
            message: "Awaiting Antigravity plan integration...".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: Some(current_plan.clone()),
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            completed_stage: Some(WorkflowState::PlanDraft),
            plan_review_count: Some(plan_review_count),
            fix_count: Some(fix_count),
            code_review_count: Some(code_review_count),
            ..Default::default()
        })?;

        let plan_submission = match wait_for_antigravity_submission(
            WorkflowState::PlanIntegration,
            AgentRole::PlanIntegrator,
            "task-plan-integration",
            &run_id,
            &snapshot,
            Some(current_plan.clone()),
            Some(task_prompt.clone()),
            None,
            None,
            Some(&frozen_plan_snapshot.plan_context),
            frozen_plan_payload.as_ref(),
            &mailbox_state,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
            Some(&relay_error),
            self.task_submission_hook.as_ref(),
        )
        .await? {
            AntigravitySubmissionOutcome::Submitted(sub) => sub,
            AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                progress_relay.abort();
                if let Some(err) = read_relay_error(&relay_error) {
                    mailbox_server.stop().await;
                    return Err(err);
                }
                mailbox_server.stop().await;
                return Ok(WorkflowState::WaitingForUser);
            }
        };

        if !plan_submission.summary.is_empty() {
            current_plan = plan_submission.summary;
        }
        log(format!(
            "[Engine] Plan integration submitted ({} chars)",
            current_plan.len()
        ));

        // ==========================================
        // Stage 3: PlanReview & PlanRevision Loop
        // ==========================================
        let mut plan_approved = false;
        while !plan_approved {
            self.check_run_control_with_relay(&mut pause_rx, &cancel_token, &relay_error).await?;

            if plan_review_count >= snapshot.iteration_limits.max_plan_review_iterations {
                log(format!(
                    "[Engine] Plan review limit ({}) reached without approval. Transitioning to WaitingForUser.",
                    plan_review_count
                ));
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::WaitingForUser,
                    iteration_info: Some(format!(
                        "Plan review limit reached ({}/{})",
                        plan_review_count, snapshot.iteration_limits.max_plan_review_iterations
                    )),
                    message: "Plan review limit reached without approval.".to_string(),
                    review_result: None,
                    validation_summary: None,
                    plan_text: Some(current_plan.clone()),
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                    ..Default::default()
                })?;

                let res = tokio::select! {
                    _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                    msg = blocking_resolution_rx.recv() => msg.ok_or_else(|| "Blocking resolution channel closed.".to_string())?,
                };
                match res {
                    BlockingResolution::Retry { guidance: _ } => {
                        plan_approved = true;
                        break;
                    }
                    BlockingResolution::Abort => {
                        progress_relay.abort();
                        if let Some(err) = read_relay_error(&relay_error) {
                            mailbox_server.stop().await;
                            return Err(err);
                        }
                        mailbox_server.stop().await;
                        return Ok(WorkflowState::Cancelled);
                    }
                }
            }

            plan_review_count += 1;
            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::PlanReview,
                iteration_info: Some(format!(
                    "Call {}/{}",
                    plan_review_count, snapshot.iteration_limits.max_plan_review_iterations
                )),
                message: format!("Reviewing plan (Call {})...", plan_review_count),
                review_result: None,
                validation_summary: None,
                plan_text: Some(current_plan.clone()),
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: if plan_review_count == 1 {
                    Some(WorkflowState::PlanIntegration)
                } else {
                    Some(WorkflowState::PlanRevision)
                },
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
                ..Default::default()
            })?;

            let reviewer_profile = snapshot
                .assignments
                .get(&AgentRole::PlanReviewer)
                .ok_or_else(|| "Plan Reviewer role is not assigned.".to_string())?;

            let pr_system = "You are an elite software architect and reviewer. Review the proposed implementation plan against the requirements. Output your verdict as JSON with schema:\n{\n  \"verdict\": \"approved\" | \"changes_required\" | \"needs_clarification\",\n  \"summary\": \"...\",\n  \"findings\": [{\"id\": \"F-01\", \"severity\": \"critical\"|\"high\"|\"medium\"|\"low\", \"file\": \"src/...\", \"line\": 10, \"issue\": \"...\", \"recommendation\": \"...\", \"is_blocking\": true}]\n}";
            let pr_user = format!("## Task\n{}\n\n## Proposed Plan\n{}", task_prompt, current_plan);
            let pr_ctx = build_role_context(
                &self.context_builder,
                reviewer_profile,
                &project_path,
                pr_system,
                &pr_user,
                None,
                plan_ctx_pair,
                Some(&cancel_token),
            )
            .await?;

            let pr_out = self
                .execute_adapter(
                    AgentRole::PlanReviewer,
                    reviewer_profile,
                    pr_system,
                    &pr_ctx.prompt,
                    &project_path,
                    mcp_servers_ref,
                    plan_context_ref,
                    frozen_plan_ref,
                    Some(&cancel_token),
                    &on_event,
                )
                .await?;

            let pr_result = self.finding_aggregator.parse_review_output(&pr_out.content);
            log(format!("[Engine] Plan review verdict: {:?}", pr_result.verdict));

            if classify_review_verdict(pr_result.verdict) == ReviewAction::Approved {
                log("[Engine] Plan APPROVED!".to_string());
                plan_approved = true;
                break;
            }

            // PlanRevision by Antigravity Harness
            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::PlanRevision,
                iteration_info: Some(format!("Plan revision {}", plan_review_count)),
                message: "Awaiting Antigravity plan revision...".to_string(),
                review_result: Some(pr_result.clone()),
                validation_summary: None,
                plan_text: Some(current_plan.clone()),
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: Some(WorkflowState::PlanReview),
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
                ..Default::default()
            })?;

            let rev_submission = match wait_for_antigravity_submission(
                WorkflowState::PlanRevision,
                AgentRole::PlanIntegrator,
                "task-plan-revision",
                &run_id,
                &snapshot,
                Some(current_plan.clone()),
                Some(task_prompt.clone()),
                Some(pr_result.summary.clone()),
                None,
                Some(&frozen_plan_snapshot.plan_context),
                frozen_plan_payload.as_ref(),
                &mailbox_state,
                &mut submit_rx,
                &mut worker_reclaim_rx,
                &cancel_token,
                &on_event,
                &log,
                Some(&relay_error),
                self.task_submission_hook.as_ref(),
            )
            .await? {
                AntigravitySubmissionOutcome::Submitted(sub) => sub,
                AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                    log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                    progress_relay.abort();
                    if let Some(err) = read_relay_error(&relay_error) {
                        mailbox_server.stop().await;
                        return Err(err);
                    }
                    mailbox_server.stop().await;
                    return Ok(WorkflowState::WaitingForUser);
                }
            };

            if !rev_submission.summary.is_empty() {
                current_plan = rev_submission.summary;
            }
        }

        if let Some(output) = &plan_archive {
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                    ..Default::default()
                })?;
                return Ok(WorkflowState::Failed);
            }
        }

        }

        // ==========================================
        // Stage 4: Implementation (Antigravity Harness)
        // A recovery EntryBaseline at Validation must not replay this stage.
        // ==========================================
        if resume.as_ref().is_none_or(|context| context.stage == WorkflowState::Implementation) {
        self.check_run_control_with_relay(&mut pause_rx, &cancel_token, &relay_error).await?;

        on_event(StepProgressEvent {
            run_id: run_id.clone(),
            step: WorkflowState::Implementation,
            iteration_info: None,
            message: "Awaiting Antigravity implementation...".to_string(),
            review_result: None,
            validation_summary: None,
            plan_text: Some(current_plan.clone()),
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            completed_stage: if resume.is_some() { None } else { Some(WorkflowState::PlanReview) },
            plan_review_count: Some(plan_review_count),
            fix_count: Some(fix_count),
            code_review_count: Some(code_review_count),
            ..Default::default()
        })?;

        let impl_submission = match wait_for_antigravity_submission(
            WorkflowState::Implementation,
            AgentRole::Implementer,
            "task-impl",
            &run_id,
            &snapshot,
            Some(current_plan.clone()),
            Some(task_prompt.clone()),
            None,
            None,
            Some(&frozen_plan_snapshot.plan_context),
            frozen_plan_payload.as_ref(),
            &mailbox_state,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
            Some(&relay_error),
            self.task_submission_hook.as_ref(),
        )
        .await? {
            AntigravitySubmissionOutcome::Submitted(sub) => sub,
            AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                progress_relay.abort();
                if let Some(err) = read_relay_error(&relay_error) {
                    mailbox_server.stop().await;
                    return Err(err);
                }
                mailbox_server.stop().await;
                return Ok(WorkflowState::WaitingForUser);
            }
        };

        log(format!(
            "[Engine] Implementation submitted (status={})",
            impl_submission.status
        ));
        }

        // ==========================================
        // Stage 5 & 6: Validation, Fix, and Code Review Loop
        // ==========================================
        loop {
            self.check_run_control_with_relay(&mut pause_rx, &cancel_token, &relay_error).await?;

            // 1. Validation Gates
            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::Validation,
                iteration_info: Some(format!("Fix call {}", fix_count)),
                message: "Running automated validation gates...".to_string(),
                review_result: None,
                validation_summary: None,
                plan_text: None,
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: if resume.as_ref().is_some_and(|context| context.adopted_baseline) {
                    None
                } else if fix_count == 0 {
                    Some(WorkflowState::Implementation)
                } else {
                    Some(WorkflowState::Fix)
                },
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
                ..Default::default()
            })?;

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

            latest_val_summary = Some(val_summary.clone());
            log(format!(
                "[Engine] Validation completed: passed={}",
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
                ..Default::default()
            })?;

            if !val_summary.passed {
                if fix_count >= snapshot.iteration_limits.max_fix_iterations {
                    log(format!(
                        "[Engine] Max fix calls ({}) reached with validation failures. Transitioning to WaitingForUser.",
                        fix_count
                    ));
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::WaitingForUser,
                        iteration_info: Some(format!(
                            "Fix limit reached ({}/{})",
                            fix_count, snapshot.iteration_limits.max_fix_iterations
                        )),
                        message: "Validation failed at maximum fix limit.".to_string(),
                        review_result: None,
                        validation_summary: Some(val_summary.clone()),
                        plan_text: None,
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        plan_review_count: Some(plan_review_count),
                        fix_count: Some(fix_count),
                        code_review_count: Some(code_review_count),
                        ..Default::default()
                    })?;

                    let res = tokio::select! {
                        _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                        msg = blocking_resolution_rx.recv() => msg.ok_or_else(|| "Blocking resolution channel closed.".to_string())?,
                    };
                    if res == BlockingResolution::Abort {
                        progress_relay.abort();
                        if let Some(err) = read_relay_error(&relay_error) {
                            mailbox_server.stop().await;
                            return Err(err);
                        }
                        mailbox_server.stop().await;
                        return Ok(WorkflowState::Cancelled);
                    }
                }

                // Trigger Fixer (Antigravity Harness)
                fix_count += 1;
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::Fix,
                    iteration_info: Some(format!(
                        "Fix call {}/{}",
                        fix_count, snapshot.iteration_limits.max_fix_iterations
                    )),
                    message: format!(
                        "Awaiting Antigravity fix for validation errors (Call {})...",
                        fix_count
                    ),
                    review_result: None,
                    validation_summary: Some(val_summary.clone()),
                    plan_text: None,
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    completed_stage: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                    ..Default::default()
                })?;

                let fix_submission = match wait_for_antigravity_submission(
                    WorkflowState::Fix,
                    AgentRole::Fixer,
                    "task-fix",
                    &run_id,
                    &snapshot,
                    Some(current_plan.clone()),
                    Some(task_prompt.clone()),
                    None,
                    Some(val_summary.formatted_diagnostics.clone()),
                    Some(&frozen_plan_snapshot.plan_context),
                    frozen_plan_payload.as_ref(),
                    &mailbox_state,
                    &mut submit_rx,
                    &mut worker_reclaim_rx,
                    &cancel_token,
                    &on_event,
                    &log,
                    Some(&relay_error),
                    self.task_submission_hook.as_ref(),
                )
                .await? {
                    AntigravitySubmissionOutcome::Submitted(sub) => sub,
                    AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                        log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                        progress_relay.abort();
                        if let Some(err) = read_relay_error(&relay_error) {
                            mailbox_server.stop().await;
                            return Err(err);
                        }
                        mailbox_server.stop().await;
                        return Ok(WorkflowState::WaitingForUser);
                    }
                };

                log(format!(
                    "[Engine] Fix submitted (status={})",
                    fix_submission.status
                ));
                continue;
            }

            // 2. Code Review in Disposable Worktree
            self.check_run_control_with_relay(&mut pause_rx, &cancel_token, &relay_error).await?;

            if code_review_count >= snapshot.iteration_limits.max_code_review_iterations {
                log(format!(
                    "[Engine] Code review limit ({}) reached. Transitioning to WaitingForUser.",
                    code_review_count
                ));
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::WaitingForUser,
                    iteration_info: Some(format!(
                        "Code review limit reached ({}/{})",
                        code_review_count, snapshot.iteration_limits.max_code_review_iterations
                    )),
                    message: "Code review limit reached without approval.".to_string(),
                    review_result: latest_cr_result.clone(),
                    validation_summary: latest_val_summary.clone(),
                    plan_text: None,
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                    ..Default::default()
                })?;

                let res = tokio::select! {
                    _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                    msg = blocking_resolution_rx.recv() => msg.ok_or_else(|| "Blocking resolution channel closed.".to_string())?,
                };
                match res {
                    BlockingResolution::Retry { guidance: _ } => {
                        break;
                    }
                    BlockingResolution::Abort => {
                        progress_relay.abort();
                        if let Some(err) = read_relay_error(&relay_error) {
                            mailbox_server.stop().await;
                            return Err(err);
                        }
                        mailbox_server.stop().await;
                        return Ok(WorkflowState::Cancelled);
                    }
                }
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
                    "Running disposable worktree code review (Call {})...",
                    code_review_count
                ),
                review_result: None,
                validation_summary: latest_val_summary.clone(),
                plan_text: None,
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: Some(WorkflowState::Validation),
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
                ..Default::default()
            })?;

            let reviewer_profile = snapshot
                .assignments
                .get(&AgentRole::CodeReviewer)
                .ok_or_else(|| "Code Reviewer role is not assigned.".to_string())?;

            let cr_result = execute_disposable_worktree_review(
                self,
                reviewer_profile,
                &project_path,
                &task_prompt,
                &current_plan,
                &cancel_token,
            )
            .await?;

            latest_cr_result = Some(cr_result.clone());
            log(format!("[Engine] Code review verdict: {:?}", cr_result.verdict));

            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::CodeReview,
                iteration_info: Some(format!(
                    "Call {}/{}",
                    code_review_count, snapshot.iteration_limits.max_code_review_iterations
                )),
                message: format!("Code review verdict: {:?}", cr_result.verdict),
                review_result: Some(cr_result.clone()),
                validation_summary: latest_val_summary.clone(),
                plan_text: None,
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
                ..Default::default()
            })?;

            if classify_review_verdict(cr_result.verdict) == ReviewAction::Approved {
                log("[Engine] Code review APPROVED! Advancing to HumanGate.".to_string());
                break;
            }

            // Code review returned ChangesRequired -> trigger Fixer (Antigravity Harness)
            if fix_count >= snapshot.iteration_limits.max_fix_iterations {
                log(format!(
                    "[Engine] Max fix calls ({}) reached with review findings. Transitioning to WaitingForUser.",
                    fix_count
                ));
                on_event(StepProgressEvent {
                    run_id: run_id.clone(),
                    step: WorkflowState::WaitingForUser,
                    iteration_info: Some(format!(
                        "Fix limit reached ({}/{})",
                        fix_count, snapshot.iteration_limits.max_fix_iterations
                    )),
                    message: "Code review requested changes at maximum fix limit.".to_string(),
                    review_result: Some(cr_result.clone()),
                    validation_summary: latest_val_summary.clone(),
                    plan_text: None,
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                    plan_review_count: Some(plan_review_count),
                    fix_count: Some(fix_count),
                    code_review_count: Some(code_review_count),
                    ..Default::default()
                })?;

                let res = tokio::select! {
                    _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                    msg = blocking_resolution_rx.recv() => msg.ok_or_else(|| "Blocking resolution channel closed.".to_string())?,
                };
                match res {
                    BlockingResolution::Retry { guidance: _ } => {
                        break;
                    }
                    BlockingResolution::Abort => {
                        progress_relay.abort();
                        if let Some(err) = read_relay_error(&relay_error) {
                            mailbox_server.stop().await;
                            return Err(err);
                        }
                        mailbox_server.stop().await;
                        return Ok(WorkflowState::Cancelled);
                    }
                }
            }

            fix_count += 1;
            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::Fix,
                iteration_info: Some(format!(
                    "Fix call {}/{}",
                    fix_count, snapshot.iteration_limits.max_fix_iterations
                )),
                message: format!(
                    "Awaiting Antigravity fix for review findings (Call {})...",
                    fix_count
                ),
                review_result: Some(cr_result.clone()),
                validation_summary: latest_val_summary.clone(),
                plan_text: None,
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: Some(WorkflowState::CodeReview),
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
                ..Default::default()
            })?;

            let fix_submission = match wait_for_antigravity_submission(
                WorkflowState::Fix,
                AgentRole::Fixer,
                "task-fix",
                &run_id,
                &snapshot,
                Some(current_plan.clone()),
                Some(task_prompt.clone()),
                Some(cr_result.summary.clone()),
                None,
                Some(&frozen_plan_snapshot.plan_context),
                frozen_plan_payload.as_ref(),
                &mailbox_state,
                &mut submit_rx,
                &mut worker_reclaim_rx,
                &cancel_token,
                &on_event,
                &log,
                Some(&relay_error),
                self.task_submission_hook.as_ref(),
            )
            .await? {
                AntigravitySubmissionOutcome::Submitted(sub) => sub,
                AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                    log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                    progress_relay.abort();
                    if let Some(err) = read_relay_error(&relay_error) {
                        mailbox_server.stop().await;
                        return Err(err);
                    }
                    mailbox_server.stop().await;
                    return Ok(WorkflowState::WaitingForUser);
                }
            };

            log(format!(
                "[Engine] Fix submitted (status={})",
                fix_submission.status
            ));
        }

        // ==========================================
        // Stage 7: HumanGate
        // ==========================================
        loop {
            self.check_run_control_with_relay(&mut pause_rx, &cancel_token, &relay_error).await?;

            on_event(StepProgressEvent {
                run_id: run_id.clone(),
                step: WorkflowState::HumanGate,
                iteration_info: None,
                message: "Code review approved. Awaiting human operator approval...".to_string(),
                review_result: latest_cr_result.clone(),
                validation_summary: latest_val_summary.clone(),
                plan_text: Some(current_plan.clone()),
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
                completed_stage: Some(WorkflowState::CodeReview),
                plan_review_count: Some(plan_review_count),
                fix_count: Some(fix_count),
                code_review_count: Some(code_review_count),
                ..Default::default()
            })?;

            {
                let mut guard = mailbox_state.inner.lock().await;
                guard.current_state = WorkflowState::HumanGate;
                guard.active_task = None;
                guard.task_notify.notify_waiters();
            }

            let decision = tokio::select! {
                _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                dec = human_gate_rx.recv() => dec.ok_or_else(|| "Human gate channel closed unexpectedly.".to_string())?,
            };

            match decision {
                HumanGateDecision::Approve => {
                    log("[Engine] HumanGate APPROVED! Workflow complete.".to_string());
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::Complete,
                        iteration_info: None,
                        message: "Human operator approved changes. Run completed successfully.".to_string(),
                        review_result: latest_cr_result,
                        validation_summary: latest_val_summary,
                        plan_text: Some(current_plan),
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        completed_stage: Some(WorkflowState::HumanGate),
                        plan_review_count: Some(plan_review_count),
                        fix_count: Some(fix_count),
                        code_review_count: Some(code_review_count),
                        ..Default::default()
                    })?;
                    progress_relay.abort();
                    if let Some(err) = read_relay_error(&relay_error) {
                        mailbox_server.stop().await;
                        return Err(err);
                    }
                    mailbox_server.stop().await;
                    return Ok(WorkflowState::Complete);
                }
                HumanGateDecision::Abort => {
                    log("[Engine] HumanGate ABORTED by user.".to_string());
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::Cancelled,
                        iteration_info: None,
                        message: "Run aborted by human operator.".to_string(),
                        review_result: latest_cr_result,
                        validation_summary: latest_val_summary,
                        plan_text: Some(current_plan),
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        plan_review_count: Some(plan_review_count),
                        fix_count: Some(fix_count),
                        code_review_count: Some(code_review_count),
                        ..Default::default()
                    })?;
                    progress_relay.abort();
                    if let Some(err) = read_relay_error(&relay_error) {
                        mailbox_server.stop().await;
                        return Err(err);
                    }
                    mailbox_server.stop().await;
                    return Ok(WorkflowState::Cancelled);
                }
                HumanGateDecision::RequestChanges { feedback } => {
                    log(format!("[Engine] HumanGate REQUEST CHANGES: {}", feedback));
                    fix_count += 1;
                    on_event(StepProgressEvent {
                        run_id: run_id.clone(),
                        step: WorkflowState::Fix,
                        iteration_info: Some(format!("Fix call {} (human requested)", fix_count)),
                        message: "Awaiting Antigravity fix for human operator feedback...".to_string(),
                        review_result: latest_cr_result.clone(),
                        validation_summary: latest_val_summary.clone(),
                        plan_text: Some(current_plan.clone()),
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                        completed_stage: Some(WorkflowState::HumanGate),
                        plan_review_count: Some(plan_review_count),
                        fix_count: Some(fix_count),
                        code_review_count: Some(code_review_count),
                        ..Default::default()
                    })?;

                    let fix_submission = match wait_for_antigravity_submission(
                        WorkflowState::Fix,
                        AgentRole::Fixer,
                        "task-fix-human",
                        &run_id,
                        &snapshot,
                        Some(current_plan.clone()),
                        Some(task_prompt.clone()),
                        Some(feedback),
                        None,
                        Some(&frozen_plan_snapshot.plan_context),
                        frozen_plan_payload.as_ref(),
                        &mailbox_state,
                        &mut submit_rx,
                        &mut worker_reclaim_rx,
                        &cancel_token,
                        &on_event,
                        &log,
                        Some(&relay_error),
                        self.task_submission_hook.as_ref(),
                    )
                    .await? {
                        AntigravitySubmissionOutcome::Submitted(sub) => sub,
                        AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                            log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                            progress_relay.abort();
                            if let Some(err) = read_relay_error(&relay_error) {
                                mailbox_server.stop().await;
                                return Err(err);
                            }
                            mailbox_server.stop().await;
                            return Ok(WorkflowState::WaitingForUser);
                        }
                    };

                    log(format!(
                        "[Engine] Fix submitted (status={})",
                        fix_submission.status
                    ));

                    // Re-run validation and review loop before returning to HumanGate
                }
            }
        }
    }
}

async fn run_git_cmd(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .map_err(|e| format!("Failed to execute 'git {}': {}", args.join(" "), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "'git {}' failed with status {}: {}",
            args.join(" "),
            output.status,
            stderr
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn run_git_apply(dir: &Path, patch: &[u8]) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("git")
        .args(["apply", "--whitespace=nowarn", "--allow-empty", "-"])
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn git apply: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(patch)
            .await
            .map_err(|e| format!("Failed to write patch to git apply stdin: {}", e))?;
        stdin
            .shutdown()
            .await
            .map_err(|e| format!("Failed to flush/shutdown git apply stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Failed to wait for git apply: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git apply failed: {}", stderr));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmoduleInfo {
    /// Relative path from root project repository (e.g., "submod1" or "submod1/nested_sub")
    pub rel_path: String,
    /// Absolute path in primary workspace
    pub source_path: PathBuf,
    /// Relative path from immediate parent repository
    pub rel_to_parent: String,
    /// Absolute path of immediate parent repository
    pub parent_source_path: PathBuf,
    /// Gitlink OID in parent repository
    pub parent_gitlink: String,
    /// Submodule's local HEAD OID
    pub head_oid: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymlinkOperation {
    File,
    Dir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLinkSemantics {
    File,
    Dir,
}

impl SourceLinkSemantics {
    pub fn from_metadata(meta: &fs::Metadata) -> Self {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if (meta.file_attributes() & 0x10) != 0 {
                SourceLinkSemantics::Dir
            } else {
                SourceLinkSemantics::File
            }
        }
        #[cfg(not(windows))]
        {
            let _ = meta;
            SourceLinkSemantics::File
        }
    }
}

pub fn resolve_symlink_operation_from_semantics(semantics: SourceLinkSemantics) -> SymlinkOperation {
    match semantics {
        SourceLinkSemantics::Dir => SymlinkOperation::Dir,
        SourceLinkSemantics::File => SymlinkOperation::File,
    }
}

pub fn resolve_symlink_operation(meta: &fs::Metadata) -> Result<SymlinkOperation, String> {
    Ok(resolve_symlink_operation_from_semantics(SourceLinkSemantics::from_metadata(meta)))
}

fn perform_symlink_operation(
    op: SymlinkOperation,
    target: &Path,
    dst: &Path,
) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        match op {
            SymlinkOperation::Dir => std::os::windows::fs::symlink_dir(target, dst),
            SymlinkOperation::File => std::os::windows::fs::symlink_file(target, dst),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = op;
        std::os::unix::fs::symlink(target, dst)
    }
}

fn dispatch_symlink_recreation(
    engine: &OrchestratorEngine,
    semantics: SourceLinkSemantics,
    target: &Path,
    dst_file: &Path,
    context_file_rel: &str,
    context_repo_name: Option<&str>,
) -> Result<SymlinkOperation, String> {
    let op = resolve_symlink_operation_from_semantics(semantics);

    #[cfg(test)]
    let symlink_res = if let Some(ref executor) = engine.scripted_adapter_executor {
        executor
            .symlink_op_recorder
            .lock()
            .unwrap()
            .push((dst_file.to_path_buf(), op));

        let hook = executor.symlink_op_hook.lock().unwrap().clone();
        if let Some(h) = hook {
            h(op, target, dst_file)
        } else {
            perform_symlink_operation(op, target, dst_file)
        }
    } else {
        perform_symlink_operation(op, target, dst_file)
    };

    #[cfg(not(test))]
    let symlink_res = perform_symlink_operation(op, target, dst_file);

    symlink_res.map_err(|e| {
        if let Some(repo) = context_repo_name {
            format!(
                "Preflight failure: Failed to create {:?} symlink for '{}' in submodule '{}': {}",
                op, context_file_rel, repo, e
            )
        } else {
            format!(
                "Preflight failure: Failed to create {:?} root symlink for '{}': {}",
                op, context_file_rel, e
            )
        }
    })?;

    Ok(op)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UntrackedEntry {
    rel_path: String,
    kind: String,
    symlink_target: Option<String>,
    sha256_hex: String,
    mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoFingerprint {
    path: PathBuf,
    head_oid: String,
    refs_snapshot: String,
    porcelain_status: Vec<u8>,
    diff_index: Vec<u8>,
    untracked_files: Vec<UntrackedEntry>,
}

async fn capture_repo_fingerprint(repo_path: &Path) -> Result<RepoFingerprint, String> {
    let head_out = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to execute 'git rev-parse HEAD' in '{}': {}",
                repo_path.display(),
                e
            )
        })?;
    if !head_out.status.success() {
        let stderr = String::from_utf8_lossy(&head_out.stderr);
        return Err(format!(
            "Fingerprint error: 'git rev-parse HEAD' failed in '{}': {}",
            repo_path.display(),
            stderr.trim()
        ));
    }
    let head_oid = String::from_utf8_lossy(&head_out.stdout).trim().to_string();
    if head_oid.is_empty() {
        return Err(format!(
            "Fingerprint error: empty HEAD in '{}'",
            repo_path.display()
        ));
    }

    let refs_out = tokio::process::Command::new("git")
        .args(["for-each-ref", "--format=%(refname) %(objectname)"])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to execute 'git for-each-ref' in '{}': {}",
                repo_path.display(),
                e
            )
        })?;
    if !refs_out.status.success() {
        let stderr = String::from_utf8_lossy(&refs_out.stderr);
        return Err(format!(
            "Fingerprint error: 'git for-each-ref' failed in '{}': {}",
            repo_path.display(),
            stderr.trim()
        ));
    }
    let refs_snapshot = String::from_utf8_lossy(&refs_out.stdout).to_string();

    let status_out = tokio::process::Command::new("git")
        .args(["status", "--porcelain=v1", "-z", "--ignore-submodules=all"])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to execute 'git status' in '{}': {}",
                repo_path.display(),
                e
            )
        })?;
    if !status_out.status.success() {
        let stderr = String::from_utf8_lossy(&status_out.stderr);
        return Err(format!(
            "Fingerprint error: 'git status' failed in '{}': {}",
            repo_path.display(),
            stderr.trim()
        ));
    }
    let porcelain_status = status_out.stdout;

    let diff_out = tokio::process::Command::new("git")
        .args([
            "diff-index",
            "-p",
            "--binary",
            "--ignore-submodules=all",
            "HEAD",
        ])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to execute 'git diff-index' in '{}': {}",
                repo_path.display(),
                e
            )
        })?;
    if !diff_out.status.success() {
        let stderr = String::from_utf8_lossy(&diff_out.stderr);
        return Err(format!(
            "Fingerprint error: 'git diff-index' failed in '{}': {}",
            repo_path.display(),
            stderr.trim()
        ));
    }
    let diff_index = diff_out.stdout;

    let untracked_out = tokio::process::Command::new("git")
        .args(["ls-files", "-z", "--others", "--exclude-standard"])
        .current_dir(repo_path)
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to execute 'git ls-files' in '{}': {}",
                repo_path.display(),
                e
            )
        })?;
    if !untracked_out.status.success() {
        let stderr = String::from_utf8_lossy(&untracked_out.stderr);
        return Err(format!(
            "Fingerprint error: 'git ls-files' failed in '{}': {}",
            repo_path.display(),
            stderr.trim()
        ));
    }

    let mut untracked_files = Vec::new();
    for entry in untracked_out.stdout.split(|&b| b == 0) {
        if entry.is_empty() {
            continue;
        }
        let rel_path = String::from_utf8(entry.to_vec()).map_err(|e| {
            format!(
                "Fingerprint error: non-UTF8 untracked path in '{}': {}",
                repo_path.display(),
                e
            )
        })?;
        if rel_path.is_empty()
            || rel_path.starts_with('/')
            || rel_path.starts_with('\\')
            || rel_path.contains("..")
        {
            return Err(format!(
                "Fingerprint error: invalid or traversal untracked path '{}' in '{}'",
                rel_path,
                repo_path.display()
            ));
        }
        let file_path = repo_path.join(&rel_path);
        let symlink_meta = fs::symlink_metadata(&file_path).map_err(|e| {
            format!(
                "Fingerprint error: failed to read metadata for '{}': {}",
                file_path.display(),
                e
            )
        })?;
        let file_type = symlink_meta.file_type();

        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            symlink_meta.permissions().mode() & 0o777
        };
        #[cfg(not(unix))]
        let mode = {
            if symlink_meta.permissions().readonly() {
                0o444
            } else {
                0o644
            }
        };

        if file_type.is_symlink() {
            let target = fs::read_link(&file_path).map_err(|e| {
                format!(
                    "Fingerprint error: failed to read symlink target for '{}': {}",
                    file_path.display(),
                    e
                )
            })?;
            let target_str = target.to_string_lossy().to_string();
            let mut hasher = Sha256::new();
            hasher.update(target_str.as_bytes());
            let sha256_hex = format!("{:x}", hasher.finalize());
            untracked_files.push(UntrackedEntry {
                rel_path,
                kind: "symlink".to_string(),
                symlink_target: Some(target_str),
                sha256_hex,
                mode,
            });
        } else if file_type.is_file() {
            let content = fs::read(&file_path).map_err(|e| {
                format!(
                    "Fingerprint error: failed to read file '{}': {}",
                    file_path.display(),
                    e
                )
            })?;
            let mut hasher = Sha256::new();
            hasher.update(&content);
            let sha256_hex = format!("{:x}", hasher.finalize());
            untracked_files.push(UntrackedEntry {
                rel_path,
                kind: "file".to_string(),
                symlink_target: None,
                sha256_hex,
                mode,
            });
        } else {
            return Err(format!(
                "Fingerprint error: unsupported filesystem type for untracked path '{}' in '{}'",
                rel_path,
                repo_path.display()
            ));
        }
    }
    untracked_files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    Ok(RepoFingerprint {
        path: repo_path.to_path_buf(),
        head_oid,
        refs_snapshot,
        porcelain_status,
        diff_index,
        untracked_files,
    })
}

fn inventory_submodules_recursive<'a>(
    root_path: &'a Path,
    current_repo: &'a Path,
    rel_prefix: &'a str,
    visited: &'a [PathBuf],
    #[cfg(test)] scripted_executor: Option<Arc<ScriptedAdapterExecutor>>,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<SubmoduleInfo>, String>> + Send + 'a>,
> {
    Box::pin(async move {
        let git_dir_out = tokio::process::Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(current_repo)
            .output()
            .await
            .map_err(|e| {
                format!(
                    "Failed to check git repository at '{}': {}",
                    current_repo.display(),
                    e
                )
            })?;
        if !git_dir_out.status.success() {
            let stderr = String::from_utf8_lossy(&git_dir_out.stderr);
            return Err(format!(
                "Preflight failure: Repository at '{}' is not a valid git repository: {}",
                current_repo.display(),
                stderr.trim()
            ));
        }

        let mut sub_rel_paths: Vec<String> = Vec::new();

        let ls_out = tokio::process::Command::new("git")
            .args(["ls-files", "-s", "-z"])
            .current_dir(current_repo)
            .output()
            .await
            .map_err(|e| {
                format!(
                    "Failed to read git index at '{}': {}",
                    current_repo.display(),
                    e
                )
            })?;

        if !ls_out.status.success() {
            let stderr = String::from_utf8_lossy(&ls_out.stderr);
            return Err(format!(
                "Preflight failure: Failed to read index entries at '{}': {}",
                current_repo.display(),
                stderr.trim()
            ));
        }

        for entry in ls_out.stdout.split(|&b| b == 0) {
            if entry.is_empty() {
                continue;
            }
            if let Some(tab_idx) = entry.iter().position(|&b| b == b'\t') {
                let meta = String::from_utf8_lossy(&entry[..tab_idx]);
                let path_bytes = &entry[tab_idx + 1..];
                let path_str = String::from_utf8(path_bytes.to_vec()).map_err(|e| {
                    format!(
                        "Preflight failure: Non-UTF8 index path at '{}': {}",
                        current_repo.display(),
                        e
                    )
                })?;
                if meta.starts_with("160000 ") && !sub_rel_paths.contains(&path_str) {
                    sub_rel_paths.push(path_str);
                }
            }
        }

        let gitmodules_path = current_repo.join(".gitmodules");
        if gitmodules_path.exists() {
            let gm_args = [
                "config",
                "-z",
                "--file",
                ".gitmodules",
                "--get-regexp",
                r"^submodule\..*\.path$",
            ];

            #[cfg(test)]
            let gm_out_res = if let Some(ref executor) = scripted_executor {
                let override_fn = executor.gitmodules_cmd_override.lock().unwrap().clone();
                if let Some(cmd_fn) = override_fn {
                    cmd_fn(current_repo, &gm_args)
                } else {
                    tokio::process::Command::new("git")
                        .args(&gm_args)
                        .current_dir(current_repo)
                        .output()
                        .await
                }
            } else {
                tokio::process::Command::new("git")
                    .args(&gm_args)
                    .current_dir(current_repo)
                    .output()
                    .await
            };

            #[cfg(not(test))]
            let gm_out_res = tokio::process::Command::new("git")
                .args(&gm_args)
                .current_dir(current_repo)
                .output()
                .await;

            let gm_out = gm_out_res.map_err(|e| {
                format!(
                    "Preflight failure: Failed to execute 'git config' on .gitmodules in '{}': {}",
                    current_repo.display(),
                    e
                )
            })?;

            if gm_out.status.success() {
                for entry in gm_out.stdout.split(|&b| b == 0) {
                    if entry.is_empty() {
                        continue;
                    }
                    if let Some(newline_idx) = entry.iter().position(|&b| b == b'\n') {
                        let path_bytes = &entry[newline_idx + 1..];
                        let path_str = String::from_utf8(path_bytes.to_vec()).map_err(|e| {
                            format!(
                                "Preflight failure: Non-UTF8 submodule path in .gitmodules in '{}': {}",
                                current_repo.display(),
                                e
                            )
                        })?;
                        if !path_str.is_empty() && !sub_rel_paths.contains(&path_str) {
                            sub_rel_paths.push(path_str);
                        }
                    } else {
                        return Err(format!(
                            "Preflight failure: Malformed .gitmodules record in '{}'.",
                            current_repo.display()
                        ));
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&gm_out.stderr);
                if gm_out.status.code() != Some(1) || !stderr.trim().is_empty() {
                    return Err(format!(
                        "Preflight failure: Failed to read .gitmodules in '{}': {}",
                        current_repo.display(),
                        stderr.trim()
                    ));
                }
            }
        }

        let mut results = Vec::new();

        for rel_sub in sub_rel_paths {
            if rel_sub.is_empty()
                || rel_sub.starts_with('/')
                || rel_sub.starts_with('\\')
                || rel_sub.contains("..")
                || rel_sub.contains('\0')
            {
                return Err(format!(
                    "Preflight failure: Malformed or path traversal submodule path '{}' at '{}'.",
                    rel_sub,
                    current_repo.display()
                ));
            }

            let sub_src = current_repo.join(&rel_sub);
            let full_rel_path = if rel_prefix.is_empty() {
                rel_sub.clone()
            } else {
                format!("{}/{}", rel_prefix, rel_sub)
            };

            if !sub_src.exists() || !sub_src.is_dir() {
                return Err(format!(
                    "Preflight failure: Submodule at '{}' is uninitialized or missing directory.",
                    full_rel_path
                ));
            }

            if !sub_src.join(".git").exists() {
                return Err(format!(
                    "Preflight failure: Submodule at '{}' is uninitialized or not a valid git repository: missing .git",
                    full_rel_path
                ));
            }

            let canonical_sub = fs::canonicalize(&sub_src).map_err(|e| {
                format!(
                    "Preflight failure: Failed to canonicalize submodule path '{}': {}",
                    full_rel_path, e
                )
            })?;

            if visited.contains(&canonical_sub) {
                return Err(format!(
                    "Preflight failure: Submodule recursion cycle detected at '{}'.",
                    full_rel_path
                ));
            }

            let head_out = tokio::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&sub_src)
                .output()
                .await
                .map_err(|e| {
                    format!(
                        "Failed to read HEAD for submodule '{}': {}",
                        full_rel_path, e
                    )
                })?;

            if !head_out.status.success() {
                let stderr = String::from_utf8_lossy(&head_out.stderr);
                return Err(format!(
                    "Preflight failure: Submodule at '{}' has invalid or unresolvable HEAD: {}",
                    full_rel_path,
                    stderr.trim()
                ));
            }
            let head_oid = String::from_utf8_lossy(&head_out.stdout).trim().to_string();
            if head_oid.is_empty() {
                return Err(format!(
                    "Preflight failure: Submodule at '{}' has empty HEAD OID.",
                    full_rel_path
                ));
            }

            let cat_out = tokio::process::Command::new("git")
                .args(["cat-file", "-e", &head_oid])
                .current_dir(&sub_src)
                .output()
                .await
                .map_err(|e| {
                    format!(
                        "Failed to verify object '{}' in submodule '{}': {}",
                        head_oid, full_rel_path, e
                    )
                })?;

            if !cat_out.status.success() {
                return Err(format!(
                    "Preflight failure: Submodule at '{}' local HEAD object '{}' is missing.",
                    full_rel_path, head_oid
                ));
            }

            let gitlink_out = tokio::process::Command::new("git")
                .args(["rev-parse", &format!("HEAD:{}", rel_sub)])
                .current_dir(current_repo)
                .output()
                .await
                .map_err(|e| {
                    format!(
                        "Preflight failure: Failed to execute 'git rev-parse HEAD:{}' in '{}': {}",
                        rel_sub,
                        current_repo.display(),
                        e
                    )
                })?;

            let parent_gitlink = if gitlink_out.status.success() {
                let oid = String::from_utf8_lossy(&gitlink_out.stdout).trim().to_string();
                if oid.is_empty() {
                    return Err(format!(
                        "Preflight failure: Empty gitlink OID for submodule '{}' in '{}'.",
                        rel_sub,
                        current_repo.display()
                    ));
                }
                oid
            } else {
                let mut found_index_gitlink = None;
                for entry in ls_out.stdout.split(|&b| b == 0) {
                    if entry.is_empty() {
                        continue;
                    }
                    if let Some(tab_idx) = entry.iter().position(|&b| b == b'\t') {
                        let meta = String::from_utf8_lossy(&entry[..tab_idx]);
                        let path_str = String::from_utf8_lossy(&entry[tab_idx + 1..]).to_string();
                        if path_str == rel_sub && meta.starts_with("160000 ") {
                            let parts: Vec<&str> = meta.split_whitespace().collect();
                            if parts.len() >= 2 {
                                found_index_gitlink = Some(parts[1].to_string());
                                break;
                            }
                        }
                    }
                }
                found_index_gitlink.unwrap_or_else(|| head_oid.clone())
            };

            let mut next_visited = visited.to_vec();
            next_visited.push(canonical_sub);

            let mut children = inventory_submodules_recursive(
                root_path,
                &sub_src,
                &full_rel_path,
                &next_visited,
                #[cfg(test)]
                scripted_executor.clone(),
            )
            .await?;

            results.append(&mut children);
            results.push(SubmoduleInfo {
                rel_path: full_rel_path,
                source_path: sub_src,
                rel_to_parent: rel_sub,
                parent_source_path: current_repo.to_path_buf(),
                parent_gitlink,
                head_oid,
            });
        }

        Ok(results)
    })
}

pub async fn verify_submodules_for_review(
    project_path: &Path,
) -> Result<Vec<SubmoduleInfo>, String> {
    let canonical_root = fs::canonicalize(project_path)
        .map_err(|e| format!("Failed to canonicalize project path: {}", e))?;
    inventory_submodules_recursive(
        project_path,
        project_path,
        "",
        &[canonical_root],
        #[cfg(test)]
        None,
    )
    .await
}

#[cfg(test)]
async fn verify_submodules_for_review_with_executor(
    project_path: &Path,
    scripted_executor: Option<Arc<ScriptedAdapterExecutor>>,
) -> Result<Vec<SubmoduleInfo>, String> {
    let canonical_root = fs::canonicalize(project_path)
        .map_err(|e| format!("Failed to canonicalize project path: {}", e))?;
    inventory_submodules_recursive(
        project_path,
        project_path,
        "",
        &[canonical_root],
        scripted_executor,
    )
    .await
}

pub async fn execute_disposable_worktree_review(
    engine: &OrchestratorEngine,
    reviewer_profile: &OrchestratorProfile,
    project_path: &Path,
    task_prompt: &str,
    approved_plan: &str,
    cancel_token: &CancellationToken,
) -> Result<ReviewResult, String> {
    // Enforce read-only reviewer capabilities
    if !reviewer_profile
        .capabilities
        .contains(&super::types::ProfileCapability::Review)
    {
        return Err(
            "Preflight validation failure: Code Reviewer role must have Review capability."
                .to_string(),
        );
    }
    match reviewer_profile.adapter {
        ExecutionAdapterType::Provider | ExecutionAdapterType::Ollama => {}
        ExecutionAdapterType::Cli => {
            // Codex CLI review operates within the review worktree
        }
        ExecutionAdapterType::Antigravity | ExecutionAdapterType::Mcp => {
            return Err("Unsupported adapter for Code Reviewer; failing closed.".to_string());
        }
    }

    let has_git = project_path.join(".git").exists();
    if !has_git {
        // Fallback for non-git environments or mocked tests
        let cr_system = "You are an elite code reviewer. Audit the git diff against the requirements and specifications. Output your verdict as JSON with schema:\n{\n  \"verdict\": \"approved\" | \"changes_required\" | \"needs_clarification\",\n  \"summary\": \"...\",\n  \"findings\": [{\"id\": \"F-01\", \"severity\": \"critical\"|\"high\"|\"medium\"|\"low\", \"file\": \"src/...\", \"line\": 10, \"issue\": \"...\", \"recommendation\": \"...\", \"is_blocking\": true}]\n}";
        let review_task = format!(
            "## Review Request\n{}\n\n## Approved Plan\n{}",
            task_prompt, approved_plan
        );
        let cr_ctx = build_role_context(
            &engine.context_builder,
            reviewer_profile,
            project_path,
            cr_system,
            &review_task,
            None,
            None,
            Some(cancel_token),
        )
        .await?;
        let cr_out = engine
            .execute_sandboxed_review(
                reviewer_profile,
                cr_system,
                &cr_ctx.prompt,
                project_path,
                Some(cancel_token),
            )
            .await?;
        return Ok(engine.finding_aggregator.parse_review_output(&cr_out.content));
    }

    // 1. Inventory and verify submodules recursively (fail closed on any inconsistency)
    #[cfg(test)]
    let submodules = verify_submodules_for_review_with_executor(
        project_path,
        engine.scripted_adapter_executor.clone(),
    )
    .await?;
    #[cfg(not(test))]
    let submodules = verify_submodules_for_review(project_path).await?;

    // 2. Preflight fingerprint capture for primary root and all submodules
    let mut pre_fingerprints = Vec::new();
    pre_fingerprints.push(capture_repo_fingerprint(project_path).await?);
    for sub in &submodules {
        pre_fingerprints.push(capture_repo_fingerprint(&sub.source_path).await?);
    }

    // 3. Base commit
    let base_commit = run_git_cmd(project_path, &["rev-parse", "HEAD"])
        .await?
        .trim()
        .to_string();

    // 4. Create temp directory
    let temp_dir =
        std::env::temp_dir().join(format!("anthro-bridge-review-{}", uuid::Uuid::new_v4()));
    let temp_dir_str = temp_dir.to_string_lossy().to_string();

    // 5. Add worktree
    run_git_cmd(
        project_path,
        &["worktree", "add", "--detach", &temp_dir_str, "HEAD"],
    )
    .await?;

    let (review_run_result, cleanup_result) = async {
        struct WorktreeGuard<'a> {
            project_path: &'a Path,
            temp_dir: PathBuf,
            cleaned: bool,
            #[cfg(test)]
            scripted_executor: Option<Arc<ScriptedAdapterExecutor>>,
        }

        impl<'a> WorktreeGuard<'a> {
            fn new(
                project_path: &'a Path,
                temp_dir: PathBuf,
                #[cfg(test)] scripted_executor: Option<Arc<ScriptedAdapterExecutor>>,
            ) -> Self {
                Self {
                    project_path,
                    temp_dir,
                    cleaned: false,
                    #[cfg(test)]
                    scripted_executor,
                }
            }

            async fn clean_explicitly(&mut self) -> Result<(), String> {
                if self.cleaned {
                    return Ok(());
                }
                self.cleaned = true;

                let mut errors = Vec::new();

                // 1. Git worktree remove operation
                #[cfg(test)]
                let git_remove_hook = self
                    .scripted_executor
                    .as_ref()
                    .and_then(|exec| exec.cleanup_git_worktree_remove_override.lock().unwrap().clone());

                #[cfg(test)]
                let git_remove_res = if let Some(hook) = git_remove_hook {
                    hook(self.project_path, &self.temp_dir)
                } else {
                    Self::execute_git_worktree_remove(self.project_path, &self.temp_dir).await
                };

                #[cfg(not(test))]
                let git_remove_res =
                    Self::execute_git_worktree_remove(self.project_path, &self.temp_dir).await;

                if let Err(e) = git_remove_res {
                    errors.push(e);
                }

                // 2. Filesystem directory removal operation
                #[cfg(test)]
                let fs_remove_hook = self
                    .scripted_executor
                    .as_ref()
                    .and_then(|exec| exec.cleanup_fs_remove_override.lock().unwrap().clone());

                #[cfg(test)]
                let fs_remove_res = if let Some(hook) = fs_remove_hook {
                    hook(&self.temp_dir)
                } else {
                    Self::execute_fs_remove(&self.temp_dir).await
                };

                #[cfg(not(test))]
                let fs_remove_res = Self::execute_fs_remove(&self.temp_dir).await;

                if let Err(e) = fs_remove_res {
                    errors.push(e);
                }

                if !errors.is_empty() {
                    Err(format!(
                        "Disposable worktree cleanup failure: {}",
                        errors.join("; ")
                    ))
                } else {
                    Ok(())
                }
            }

            async fn execute_git_worktree_remove(
                project_path: &Path,
                temp_dir: &Path,
            ) -> Result<(), String> {
                let out_res = tokio::process::Command::new("git")
                    .args([
                        "worktree",
                        "remove",
                        "--force",
                        &temp_dir.to_string_lossy(),
                    ])
                    .current_dir(project_path)
                    .output()
                    .await;

                match out_res {
                    Ok(output) => {
                        if !output.status.success() {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            Err(format!(
                                "'git worktree remove --force' failed: {}",
                                stderr.trim()
                            ))
                        } else {
                            Ok(())
                        }
                    }
                    Err(e) => Err(format!(
                        "Failed to execute 'git worktree remove --force': {}",
                        e
                    )),
                }
            }

            async fn execute_fs_remove(temp_dir: &Path) -> Result<(), String> {
                if temp_dir.exists() {
                    tokio::fs::remove_dir_all(temp_dir).await.map_err(|e| {
                        format!(
                            "Failed to remove disposable directory '{}': {}",
                            temp_dir.display(),
                            e
                        )
                    })
                } else {
                    Ok(())
                }
            }
        }

        impl<'a> Drop for WorktreeGuard<'a> {
            fn drop(&mut self) {
                if !self.cleaned {
                    self.cleaned = true;
                    let _ = std::process::Command::new("git")
                        .args([
                            "worktree",
                            "remove",
                            "--force",
                            &self.temp_dir.to_string_lossy(),
                        ])
                        .current_dir(self.project_path)
                        .output();
                    let _ = std::fs::remove_dir_all(&self.temp_dir);
                }
            }
        }

        let mut guard = WorktreeGuard::new(
            project_path,
            temp_dir.clone(),
            #[cfg(test)]
            engine.scripted_adapter_executor.clone(),
        );

        // 6. Initialize submodules in worktree with checked result
        let review_output_res = async {
            if !submodules.is_empty() {
                run_git_cmd(
                    &temp_dir,
                    &[
                        "-c",
                        "protocol.file.allow=always",
                        "submodule",
                        "update",
                        "--init",
                        "--recursive",
                    ],
                )
                .await?;
            }

            // 7. Process submodules in deepest-first order
            for sub in &submodules {
                let sub_dst = temp_dir.join(&sub.rel_path);
                let sub_src = &sub.source_path;

                if !sub_dst.exists() {
                    return Err(format!(
                        "Preflight failure: Disposable worktree submodule directory '{}' does not exist.",
                        sub.rel_path
                    ));
                }

                // A. Transfer local commit if head differs from parent gitlink
                if sub.head_oid != sub.parent_gitlink {
                    run_git_cmd(
                        &sub_dst,
                        &[
                            "-c",
                            "protocol.file.allow=always",
                            "fetch",
                            "--no-tags",
                            &sub_src.to_string_lossy(),
                            &sub.head_oid,
                        ],
                    )
                    .await?;

                    run_git_cmd(&sub_dst, &["cat-file", "-e", &sub.head_oid]).await?;
                    run_git_cmd(&sub_dst, &["checkout", "--detach", &sub.head_oid]).await?;
                }

                // B. Apply working tree diff
                let sub_diff_out = tokio::process::Command::new("git")
                    .args([
                        "diff-index",
                        "-p",
                        "--binary",
                        "--ignore-submodules=all",
                        "HEAD",
                    ])
                    .current_dir(sub_src)
                    .output()
                    .await
                    .map_err(|e| {
                        format!(
                            "Failed to execute diff for submodule '{}': {}",
                            sub.rel_path, e
                        )
                    })?;

                if !sub_diff_out.status.success() {
                    let stderr = String::from_utf8_lossy(&sub_diff_out.stderr);
                    return Err(format!(
                        "Failed to read diff for submodule '{}': {}",
                        sub.rel_path,
                        stderr.trim()
                    ));
                }

                if !sub_diff_out.stdout.is_empty() {
                    run_git_apply(&sub_dst, &sub_diff_out.stdout).await?;
                }

                // C. Copy untracked non-ignored files
                let untracked_out = tokio::process::Command::new("git")
                    .args(["ls-files", "-z", "--others", "--exclude-standard"])
                    .current_dir(sub_src)
                    .output()
                    .await
                    .map_err(|e| {
                        format!(
                            "Failed to execute ls-files for submodule '{}': {}",
                            sub.rel_path, e
                        )
                    })?;

                if !untracked_out.status.success() {
                    let stderr = String::from_utf8_lossy(&untracked_out.stderr);
                    return Err(format!(
                        "Failed to list untracked files for submodule '{}': {}",
                        sub.rel_path,
                        stderr.trim()
                    ));
                }

                for file_rel_bytes in untracked_out.stdout.split(|&b| b == 0) {
                    if file_rel_bytes.is_empty() {
                        continue;
                    }
                    let file_rel = String::from_utf8(file_rel_bytes.to_vec()).map_err(|e| {
                        format!(
                            "Failed to decode untracked filename in submodule '{}': {}",
                            sub.rel_path, e
                        )
                    })?;
                    if file_rel.is_empty()
                        || file_rel.starts_with('/')
                        || file_rel.starts_with('\\')
                        || file_rel.contains("..")
                    {
                        return Err(format!(
                            "Preflight failure: Invalid or traversal untracked path '{}' in submodule '{}'.",
                            file_rel, sub.rel_path
                        ));
                    }
                    let src_file = sub_src.join(&file_rel);
                    let dst_file = sub_dst.join(&file_rel);
                    let meta = fs::symlink_metadata(&src_file).map_err(|e| {
                        format!(
                            "Failed to inspect untracked file metadata for '{}' in submodule '{}': {}",
                            file_rel, sub.rel_path, e
                        )
                    })?;

                    if let Some(parent) = dst_file.parent() {
                        fs::create_dir_all(parent).map_err(|e| {
                            format!(
                                "Failed to create parent directory for '{}' in submodule '{}': {}",
                                file_rel, sub.rel_path, e
                            )
                        })?;
                    }

                    if meta.file_type().is_symlink() {
                        let target = fs::read_link(&src_file).map_err(|e| {
                            format!(
                                "Failed to read symlink target for '{}' in submodule '{}': {}",
                                file_rel, sub.rel_path, e
                            )
                        })?;

                        let semantics = SourceLinkSemantics::from_metadata(&meta);
                        dispatch_symlink_recreation(
                            engine,
                            semantics,
                            &target,
                            &dst_file,
                            &file_rel,
                            Some(&sub.rel_path),
                        )?;
                    } else if meta.file_type().is_file() {
                        fs::copy(&src_file, &dst_file).map_err(|e| {
                            format!(
                                "Failed to copy untracked file '{}' in submodule '{}': {}",
                                file_rel, sub.rel_path, e
                            )
                        })?;
                    } else {
                        return Err(format!(
                            "Preflight failure: Unsupported untracked file type for '{}' in submodule '{}'.",
                            file_rel, sub.rel_path
                        ));
                    }
                }

                // D. Stage all and commit snapshot in sub_dst if modified
                run_git_cmd(&sub_dst, &["add", "-A"]).await?;
                let status_bytes = tokio::process::Command::new("git")
                    .args(["status", "--porcelain=v1", "-z", "--ignore-submodules=all"])
                    .current_dir(&sub_dst)
                    .output()
                    .await
                    .map_err(|e| {
                        format!(
                            "Failed to check status in disposable submodule '{}': {}",
                            sub.rel_path, e
                        )
                    })?;

                if !status_bytes.status.success() {
                    let stderr = String::from_utf8_lossy(&status_bytes.stderr);
                    return Err(format!(
                        "Failed to check status in disposable submodule '{}': {}",
                        sub.rel_path,
                        stderr.trim()
                    ));
                }

                if !status_bytes.stdout.is_empty() {
                    run_git_cmd(
                        &sub_dst,
                        &[
                            "-c",
                            "user.name=AnthroBridge Reviewer",
                            "-c",
                            "user.email=reviewer@anthro-bridge.local",
                            "commit",
                            "-m",
                            "Snapshot review changes",
                            "--no-verify",
                            "--allow-empty",
                        ],
                    )
                    .await?;
                }

                // E. Stage submodule in its parent repository inside temp_dir
                let parent_dst = if sub.rel_path == sub.rel_to_parent {
                    temp_dir.clone()
                } else {
                    let parent_rel = &sub.rel_path[..sub.rel_path.len() - sub.rel_to_parent.len()]
                        .trim_end_matches(['/', '\\']);
                    temp_dir.join(parent_rel)
                };

                run_git_cmd(&parent_dst, &["add", &sub.rel_to_parent]).await?;
            }

            // 8. Capture root diff and apply to worktree
            let root_diff_out = tokio::process::Command::new("git")
                .args([
                    "diff-index",
                    "-p",
                    "--binary",
                    "--ignore-submodules=all",
                    "HEAD",
                ])
                .current_dir(project_path)
                .output()
                .await
                .map_err(|e| format!("Failed to execute root diff: {}", e))?;

            if !root_diff_out.status.success() {
                let stderr = String::from_utf8_lossy(&root_diff_out.stderr);
                return Err(format!("Failed to read root diff: {}", stderr.trim()));
            }

            if !root_diff_out.stdout.is_empty() {
                run_git_apply(&temp_dir, &root_diff_out.stdout).await?;
            }

            // 9. Copy root untracked files
            let untracked_out = tokio::process::Command::new("git")
                .args(["ls-files", "-z", "--others", "--exclude-standard"])
                .current_dir(project_path)
                .output()
                .await
                .map_err(|e| format!("Failed to execute root ls-files: {}", e))?;

            if !untracked_out.status.success() {
                let stderr = String::from_utf8_lossy(&untracked_out.stderr);
                return Err(format!(
                    "Failed to list root untracked files: {}",
                    stderr.trim()
                ));
            }

            for file_rel_bytes in untracked_out.stdout.split(|&b| b == 0) {
                if file_rel_bytes.is_empty() {
                    continue;
                }
                let file_rel = String::from_utf8(file_rel_bytes.to_vec())
                    .map_err(|e| format!("Failed to decode root untracked path: {}", e))?;
                if file_rel.is_empty()
                    || file_rel.starts_with('/')
                    || file_rel.starts_with('\\')
                    || file_rel.contains("..")
                {
                    return Err(format!(
                        "Preflight failure: Invalid or traversal root untracked path '{}'.",
                        file_rel
                    ));
                }
                let src_file = project_path.join(&file_rel);
                let dst_file = temp_dir.join(&file_rel);
                let meta = fs::symlink_metadata(&src_file).map_err(|e| {
                    format!(
                        "Failed to inspect root untracked file metadata for '{}': {}",
                        file_rel, e
                    )
                })?;

                if let Some(parent) = dst_file.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!(
                            "Failed to create root parent directory for '{}': {}",
                            file_rel, e
                        )
                    })?;
                }

                if meta.file_type().is_symlink() {
                    let target = fs::read_link(&src_file).map_err(|e| {
                        format!(
                            "Failed to read root symlink target for '{}': {}",
                            file_rel, e
                        )
                    })?;

                    let semantics = SourceLinkSemantics::from_metadata(&meta);
                    dispatch_symlink_recreation(
                        engine,
                        semantics,
                        &target,
                        &dst_file,
                        &file_rel,
                        None,
                    )?;
                } else if meta.file_type().is_file() {
                    fs::copy(&src_file, &dst_file).map_err(|e| {
                        format!("Failed to copy root untracked file '{}': {}", file_rel, e)
                    })?;
                } else {
                    return Err(format!(
                        "Preflight failure: Unsupported root untracked file type for '{}'.",
                        file_rel
                    ));
                }
            }

            // 10. Stage all and commit snapshot in root worktree
            run_git_cmd(&temp_dir, &["add", "-A"]).await?;
            let root_status_bytes = tokio::process::Command::new("git")
                .args(["status", "--porcelain=v1", "-z"])
                .current_dir(&temp_dir)
                .output()
                .await
                .map_err(|e| format!("Failed to check status in root worktree: {}", e))?;

            if !root_status_bytes.status.success() {
                let stderr = String::from_utf8_lossy(&root_status_bytes.stderr);
                return Err(format!(
                    "Failed to check root worktree status: {}",
                    stderr.trim()
                ));
            }

            if !root_status_bytes.stdout.is_empty() {
                run_git_cmd(
                    &temp_dir,
                    &[
                        "-c",
                        "user.name=AnthroBridge Reviewer",
                        "-c",
                        "user.email=reviewer@anthro-bridge.local",
                        "commit",
                        "-m",
                        "Snapshot review changes",
                        "--no-verify",
                        "--allow-empty",
                    ],
                )
                .await?;
            }
            let snapshot_commit = run_git_cmd(&temp_dir, &["rev-parse", "HEAD"])
                .await?
                .trim()
                .to_string();

            // 11. Generate review diff
            let review_diff = run_git_cmd(
                &temp_dir,
                &[
                    "diff",
                    "--submodule=diff",
                    &format!("{}..{}", base_commit, snapshot_commit),
                ],
            )
            .await?;
            let git_status = run_git_cmd(&temp_dir, &["status", "--short"]).await?;

            // 12. Run reviewer in temp_dir
            let cr_system = "You are an elite code reviewer. Audit the git diff against the requirements and specifications. Output your verdict as JSON with schema:\n{\n  \"verdict\": \"approved\" | \"changes_required\" | \"needs_clarification\",\n  \"summary\": \"...\",\n  \"findings\": [{\"id\": \"F-01\", \"severity\": \"critical\"|\"high\"|\"medium\"|\"low\", \"file\": \"src/...\", \"line\": 10, \"issue\": \"...\", \"recommendation\": \"...\", \"is_blocking\": true}]\n}";
            let review_task = format!(
                "## Review Request\n{}\n\n## Approved Plan\n{}\n\n## Base Commit\n{}\n\n## Snapshot Commit\n{}\n\n## Diff (git diff --submodule=diff)\n{}\n\n## Status\n{}\n",
                task_prompt, approved_plan, base_commit, snapshot_commit, review_diff, git_status
            );

            let cr_ctx = build_role_context(
                &engine.context_builder,
                reviewer_profile,
                &temp_dir,
                cr_system,
                &review_task,
                None,
                None,
                Some(cancel_token),
            )
            .await?;

            let cr_out = engine
                .execute_sandboxed_review(
                    reviewer_profile,
                    cr_system,
                    &cr_ctx.prompt,
                    &temp_dir,
                    Some(cancel_token),
                )
                .await?;

            // 13. Post-Review Status Audit
            let post_status = run_git_cmd(
                &temp_dir,
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--ignore-submodules=all",
                ],
            )
            .await?;
            if !post_status.trim().is_empty() {
                return Err(
                    "Security violation: Reviewer process modified files in the review worktree."
                        .to_string(),
                );
            }

            for sub in &submodules {
                let sub_dst = temp_dir.join(&sub.rel_path);
                if !sub_dst.exists() {
                    return Err(format!(
                        "Security violation: Submodule worktree directory '{}' disappeared during review.",
                        sub.rel_path
                    ));
                }
                let sub_post_status = run_git_cmd(
                    &sub_dst,
                    &[
                        "status",
                        "--porcelain=v1",
                        "-z",
                        "--ignore-submodules=all",
                    ],
                )
                .await?;
                if !sub_post_status.trim().is_empty() {
                    return Err(
                        "Security violation: Reviewer process modified files in review worktree submodule."
                            .to_string(),
                    );
                }
            }

            let cr_result = engine.finding_aggregator.parse_review_output(&cr_out.content);
            Ok(cr_result)
        }
        .await;

        let cleanup_result = guard.clean_explicitly().await;
        (review_output_res, cleanup_result)
    }
    .await;

    // 14. Verify primary parent and submodule state fingerprints match before and after on ALL exits
    let mut post_fingerprints = Vec::new();
    post_fingerprints.push(capture_repo_fingerprint(project_path).await?);
    for sub in &submodules {
        post_fingerprints.push(capture_repo_fingerprint(&sub.source_path).await?);
    }

    if pre_fingerprints != post_fingerprints {
        return Err(
            "Security violation: Primary repository or submodule state was modified during review execution."
                .to_string(),
        );
    }

    match (review_run_result, cleanup_result) {
        (Ok(verdict), Ok(())) => Ok(verdict),
        (Ok(_), Err(clean_err)) => Err(clean_err),
        (Err(rev_err), Ok(())) => Err(rev_err),
        (Err(rev_err), Err(clean_err)) => {
            Err(format!("{}; also encountered cleanup failure: {}", rev_err, clean_err))
        }
    }
}

async fn build_role_context(
    builder: &ContextBuilder,
    profile: &OrchestratorProfile,
    project_path: &std::path::Path,
    system_prompt: &str,
    user_prompt: &str,
    findings: Option<&[ReviewFinding]>,
    plan_context: Option<(&super::plan_workspace::PlanContext, Option<&str>)>,
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
        .build_with_plan_context(
            project_path,
            user_prompt,
            None,
            findings,
            plan_context,
            Some(project_budget),
            cancel_token,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
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
        antigravity_dispatches: None,
        antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
            ..Default::default()
            };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["runId"], "run-123");
        assert_eq!(value["step"], "validation");
        assert_eq!(value["validationSummary"]["totalGatesRun"], 1);
        assert_eq!(
            value["validationSummary"]["failedGateNames"][0],
            "cargo test"
        );
        assert_eq!(
            value["validationSummary"]["formattedDiagnostics"],
            "diagnostics"
        );
        assert_eq!(value["validationSummary"]["results"][0]["gateId"], "gate-1");
        assert_eq!(
            value["validationSummary"]["results"][0]["gateName"],
            "Rust tests"
        );
        assert_eq!(value["validationSummary"]["results"][0]["exitCode"], 1);
        assert_eq!(
            value["validationSummary"]["results"][0]["isTruncated"],
            false
        );
        assert_eq!(value["validationSummary"]["results"][0]["durationMs"], 12);
        assert_eq!(
            value["validationSummary"]["results"][0]["failOnError"],
            true
        );
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
            capabilities: vec![ProfileCapability::Review, ProfileCapability::WorkspaceRead],
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

        let project_dir = Path::new(&project_path);
        let version_sources = if project_dir.join("gui/package.json").exists() {
            vec![
                "gui/package.json".to_string(),
                "gui/src-tauri/tauri.conf.json".to_string(),
                "gui/src-tauri/Cargo.toml".to_string(),
            ]
        } else if project_dir.join("package.json").exists() {
            vec!["package.json".to_string()]
        } else {
            let _ = fs::write(
                project_dir.join("package.json"),
                r#"{"name":"test-project","version":"0.24.0"}"#,
            );
            vec!["package.json".to_string()]
        };

        RunConfigurationSnapshot {
            project_path,
            assignments,
            iteration_limits: Default::default(),
            validation_gates: vec![],
            budget_limits: Default::default(),
            created_at_unix: 0,
            lean_antigravity_mode: false,
            plan_workspace: crate::orchestrator::plan_workspace::PlanWorkspaceConfig {
                plan_dir: crate::orchestrator::plan_workspace::DEFAULT_PLAN_DIR.to_string(),
                filename_template: crate::orchestrator::plan_workspace::DEFAULT_FILENAME_TEMPLATE.to_string(),
                version_sources,
                plan_series_version_override: Some("0.24.0".to_string()),
            },
            mcp_servers: Default::default(),
        }
    }

    #[tokio::test]
    async fn successful_mcp_call_is_not_reported_as_workflow_success_when_audit_persist_fails() {
        let fixture_temp = tempfile::tempdir().unwrap();
        let peer = crate::orchestrator::adapters::direct_mcp::tests::build_stdio_peer(&fixture_temp);
        let project = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(project.path().to_string_lossy().into_owned());
        let mut planner = snapshot.assignments[&AgentRole::Planner].clone();
        planner.id = "mcp-planner".to_string();
        planner.adapter = ExecutionAdapterType::Mcp;
        planner.capabilities = vec![ProfileCapability::Reasoning];
        planner.external_mcp_server = Some("fake".to_string());
        planner.mcp_tool = Some("plan".to_string());
        snapshot.assignments.insert(AgentRole::Planner, planner);
        snapshot.mcp_servers.insert(
            "fake".to_string(),
            McpServerConfig {
                transport: super::super::types::McpServerTransport::Stdio,
                executable: peer.to_string_lossy().into_owned(),
                args: vec!["success".to_string()],
                working_directory: Some(project.path().to_string_lossy().into_owned()),
                allowed_environment: vec![],
                tool_contract: super::super::types::McpToolContract::PromptEnvelopeV1,
            },
        );

        let engine = OrchestratorEngine::new().with_direct_mcp_audit_callback(Arc::new(|_| {
            Err("injected durable audit write failure".to_string())
        }));
        let (pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let cancel = CancellationToken::new();
        let (_clarification_tx, clarification_rx) = mpsc::channel(4);
        let (_blocking_tx, blocking_rx) = mpsc::channel(4);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(4);
        let (_reclaim_tx, reclaim_rx) = mpsc::channel(4);
        let events = Arc::new(std::sync::Mutex::new(Vec::<StepProgressEvent>::new()));
        let events_clone = events.clone();
        let on_event: EventCallback = Arc::new(move |event| {
            events_clone.lock().unwrap().push(event);
            Ok(())
        });
        let on_log: LogCallback = Arc::new(|_| {});

        let result = engine
            .run_workflow(
                "audit-persistence-failure-run".to_string(),
                snapshot,
                "write a plan".to_string(),
                "plan_only".to_string(),
                None,
                vec![],
                pause_rx,
                cancel.clone(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                reclaim_rx,
                on_event,
                on_log,
                None,
            )
            .await;
        drop(pause_tx);
        assert!(result.as_ref().is_err_and(|error| error.contains("injected durable audit write failure")));
        assert!(!events.lock().unwrap().iter().any(|event| event.step == WorkflowState::Complete));
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
        .map(|(state, events, calls, validation_runs, _, _)| {
            (state, events, calls, validation_runs)
        })
    }

    async fn run_scripted_workflow_with_plan_output(
        workflow_type: &str,
        snapshot: RunConfigurationSnapshot,
        scripted_adapters: Vec<(AgentRole, AdapterExecutionOutput)>,
        scripted_validations: Vec<ValidationRunSummary>,
        plan_archive: Option<ValidatedPlanArchive>,
        before_role_symlink_swap: Option<(AgentRole, PathBuf, PathBuf)>,
    ) -> Result<
        (
            WorkflowState,
            Vec<StepProgressEvent>,
            Vec<AgentRole>,
            usize,
            Option<String>,
            Option<bool>,
        ),
        String,
    > {
        let plan_archive_to_observe = plan_archive
            .as_ref()
            .map(|output| output.directory_at_start.clone());
        let mut engine = OrchestratorEngine::new();
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(scripted_adapters.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
            before_role_symlink_swap: Mutex::new(before_role_symlink_swap),
            implementer_prompt: Mutex::new(None),
            plan_file_to_observe: plan_archive_to_observe,
            plan_exists_at_implementer: Mutex::new(None),
            ..Default::default()
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
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

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
                plan_archive,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                None,
            )
            .await?;
        drop(pause_tx);
        let events = events.lock().unwrap().clone();
        let calls = scripted_executor.calls.lock().unwrap().clone();
        let val_runs = *scripted_val_executor.runs.lock().unwrap();
        let implementer_prompt = scripted_executor.implementer_prompt.lock().unwrap().clone();
        let plan_exists_at_implementer =
            *scripted_executor.plan_exists_at_implementer.lock().unwrap();
        Ok((
            state,
            events,
            calls,
            val_runs,
            implementer_prompt,
            plan_exists_at_implementer,
        ))
    }

    fn validated_archive(project: &Path, directory: &Path) -> ValidatedPlanArchive {
        validate_plan_archive_options(
            &project.to_string_lossy(),
            PlanArchiveOptions {
                directory: directory.to_string_lossy().into_owned(),
            },
        )
        .unwrap()
    }

    fn archive_files(directory: &Path) -> Vec<PathBuf> {
        if !directory.exists() {
            return Vec::new();
        }
        fs::read_dir(directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.to_ascii_lowercase().starts_with("v")
                            && name.to_ascii_lowercase().ends_with(".md")
                    })
            })
            .collect()
    }

    #[test]
    fn development_version_parser_is_strict_and_accepts_optional_final_newline() {
        assert_eq!(parse_development_version("0.24.0\n").unwrap(), "0.24.0");
        assert_eq!(parse_development_version("0.24.0\r\n").unwrap(), "0.24.0");
        for invalid in [
            "",
            "0.24",
            "v0.24.0",
            "V0.24.0",
            "0.24.0-r1",
            "0.24.0-beta",
            "../0.24.0",
            "0.24.0\n\n",
            "01.24.0",
            "18446744073709551616.0.0",
        ] {
            assert!(
                parse_development_version(invalid).is_err(),
                "accepted invalid version {invalid:?}"
            );
        }
        assert_eq!(development_target_version().unwrap(), "0.24.0");
    }

    #[test]
    fn archive_allocator_uses_max_revision_and_keeps_version_series_independent() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path();
        assert_eq!(
            next_plan_archive_filename(archive, "0.24.0").unwrap(),
            "V0.24.0-r1.md"
        );
        for name in [
            "V0.24.0-r1.md",
            "V0.24.0-r2.md",
            "V0.24.0-r7.md",
            "V0.25.0-r90.md",
            "notes.md",
        ] {
            fs::write(archive.join(name), "existing").unwrap();
        }
        assert_eq!(
            next_plan_archive_filename(archive, "0.24.0").unwrap(),
            "V0.24.0-r8.md"
        );
        assert_eq!(
            next_plan_archive_filename(archive, "0.25.0").unwrap(),
            "V0.25.0-r91.md"
        );
        fs::remove_file(archive.join("V0.25.0-r90.md")).unwrap();
        assert_eq!(
            next_plan_archive_filename(archive, "0.25.0").unwrap(),
            "V0.25.0-r1.md"
        );
    }

    #[test]
    fn archive_allocator_fails_closed_on_revision_overflow() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("V0.24.0-r18446744073709551616.md"),
            "occupied",
        )
        .unwrap();
        assert!(next_plan_archive_filename(temp.path(), "0.24.0")
            .unwrap_err()
            .contains("integer range"));
    }

    #[test]
    fn archive_folder_validation_allows_only_project_contained_existing_or_default_folder() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&outside).unwrap();
        assert!(
            !resolve_plan_archive_directory(
                &project.to_string_lossy(),
                &project.join(".plan").to_string_lossy()
            )
            .unwrap()
            .exists
        );
        assert!(resolve_plan_archive_directory(
            &project.to_string_lossy(),
            &outside.to_string_lossy()
        )
        .unwrap_err()
        .contains("inside"));
        assert!(resolve_plan_archive_directory(
            &project.to_string_lossy(),
            &project.join("custom-missing").to_string_lossy()
        )
        .is_err());
        assert!(resolve_plan_archive_directory(
            &project.to_string_lossy(),
            &project.join("..\\outside").to_string_lossy()
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn archive_folder_rejects_symlink_escape() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, project.join("archive-link")).unwrap();
        assert!(resolve_plan_archive_directory(
            &project.to_string_lossy(),
            &project.join("archive-link").to_string_lossy()
        )
        .is_err());
    }

    #[test]
    fn approved_plan_archive_creates_default_folder_and_never_touches_implementation_plan() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let old_plan = project.join("IMPLEMENTATION_PLAN.md");
        fs::write(&old_plan, b"leave this file alone").unwrap();
        let archive = project.join(".plan");
        let output = validated_archive(&project, &archive);

        persist_approved_plan(&project.to_string_lossy(), &output, "approved plan ✓").unwrap();
        assert_eq!(
            fs::read_to_string(archive.join("V0.24.0-r1.md")).unwrap(),
            "approved plan ✓"
        );
        assert_eq!(fs::read(&old_plan).unwrap(), b"leave this file alone");
        persist_approved_plan(&project.to_string_lossy(), &output, "second approved plan").unwrap();
        assert_eq!(
            fs::read_to_string(archive.join("V0.24.0-r2.md")).unwrap(),
            "second approved plan"
        );
        assert_eq!(archive_files(&archive).len(), 2);
        assert!(!fs::read_dir(&archive)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp")));
    }

    #[test]
    fn archive_candidate_race_reallocates_without_overwriting() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let archive = project.join(".plan");
        let output = validated_archive(&project, &archive);
        let mut collision_injected = false;
        persist_approved_plan_with_hook(
            &project.to_string_lossy(),
            &output,
            "approved plan",
            |stage, directory| {
                if stage == ArchivePersistenceStage::TempWritten && !collision_injected {
                    collision_injected = true;
                    let destination = directory.join("V0.24.0-r1.md");
                    fs::write(destination, b"other run owns r1")?;
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            fs::read(archive.join("V0.24.0-r1.md")).unwrap(),
            b"other run owns r1"
        );
        assert_eq!(
            fs::read_to_string(archive.join("V0.24.0-r2.md")).unwrap(),
            "approved plan"
        );
    }

    #[test]
    fn failed_archive_install_cleans_temp_and_preserves_existing_archive_entries() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let archive = project.join(".plan");
        fs::create_dir_all(&archive).unwrap();
        fs::write(archive.join("V0.24.0-r1.md"), b"prior bytes").unwrap();
        let output = validated_archive(&project, &archive);
        let result = persist_approved_plan_with_hook(
            &project.to_string_lossy(),
            &output,
            "replacement",
            |stage, _| {
                if stage == ArchivePersistenceStage::TempWritten {
                    Err(std::io::Error::other("injected pre-install failure"))
                } else {
                    Ok(())
                }
            },
        );
        assert!(result.is_err());
        assert_eq!(
            fs::read(archive.join("V0.24.0-r1.md")).unwrap(),
            b"prior bytes"
        );
        assert_eq!(fs::read_dir(&archive).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn archive_directory_replacement_after_capability_open_never_redirects_temp_or_install() {
        for replacement_stage in [
            ArchivePersistenceStage::CapabilityOpened,
            ArchivePersistenceStage::TempWritten,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let project = temp.path().join("project");
            let archive = project.join(".plan");
            let moved_archive = project.join(".plan-opened-capability");
            let outside = temp.path().join("outside");
            fs::create_dir_all(&archive).unwrap();
            fs::create_dir(&outside).unwrap();
            fs::write(archive.join("V0.24.0-r1.md"), b"prior bytes").unwrap();
            let output = validated_archive(&project, &archive);
            let mut replaced = false;
            persist_approved_plan_with_hook(
                &project.to_string_lossy(),
                &output,
                "approved through opened capability",
                |stage, _| {
                    if stage == replacement_stage && !replaced {
                        replaced = true;
                        fs::rename(&archive, &moved_archive)?;
                        std::os::unix::fs::symlink(&outside, &archive)?;
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert!(replaced);
            assert!(
                fs::read_dir(&outside).unwrap().next().is_none(),
                "I/O escaped into substituted directory"
            );
            assert_eq!(
                fs::read(moved_archive.join("V0.24.0-r1.md")).unwrap(),
                b"prior bytes"
            );
            assert_eq!(
                fs::read_to_string(moved_archive.join("V0.24.0-r2.md")).unwrap(),
                "approved through opened capability"
            );
            assert!(!fs::read_dir(&moved_archive)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp")));
        }
    }

    #[cfg(windows)]
    #[test]
    fn archive_directory_handle_prevents_path_replacement_until_capability_io_finishes() {
        for replacement_stage in [
            ArchivePersistenceStage::CapabilityOpened,
            ArchivePersistenceStage::TempWritten,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let project = temp.path().join("project");
            let archive = project.join(".plan");
            let moved_archive = project.join(".plan-opened-capability");
            fs::create_dir_all(&archive).unwrap();
            let output = validated_archive(&project, &archive);
            let mut replacement_blocked = false;
            persist_approved_plan_with_hook(
                &project.to_string_lossy(),
                &output,
                "approved through opened capability",
                |stage, _| {
                    if stage == replacement_stage && !replacement_blocked {
                        replacement_blocked = true;
                        assert!(fs::rename(&archive, &moved_archive).is_err());
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert!(replacement_blocked);
            assert_eq!(
                fs::read_to_string(archive.join("V0.24.0-r1.md")).unwrap(),
                "approved through opened capability"
            );
        }
    }

    #[tokio::test]
    async fn plan_only_archives_only_the_final_reviewer_approved_revision() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let output = validated_archive(&project, &project.join(".plan"));
        let (state, _, calls, _, _, _) = run_scripted_workflow_with_plan_output(
            "plan_only",
            workflow_snapshot(project.to_string_lossy().into_owned()),
            vec![
                (AgentRole::Planner, scripted_output("draft plan")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(
                        r#"{"verdict":"changes_required","summary":"add tests","findings":[]}"#,
                    ),
                ),
                (AgentRole::Planner, scripted_output("final approved plan")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"ready","findings":[]}"#),
                ),
            ],
            vec![],
            Some(output),
            None,
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
                AgentRole::PlanReviewer
            ]
        );
        let files = archive_files(&project.join(".plan"));
        assert_eq!(files.len(), 1);
        assert_eq!(
            fs::read_to_string(&files[0]).unwrap(),
            "final approved plan"
        );
    }

    #[tokio::test]
    async fn plan_only_failed_review_creates_no_archive_entry() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let output = validated_archive(&project, &project.join(".plan"));
        let (state, events, calls, _, _, _) = run_scripted_workflow_with_plan_output(
            "plan_only",
            workflow_snapshot(project.to_string_lossy().into_owned()),
            vec![
                (AgentRole::Planner, scripted_output("unapproved draft")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(
                        r#"{"verdict":"failed","summary":"cannot review","findings":[]}"#,
                    ),
                ),
            ],
            vec![],
            Some(output),
            None,
        )
        .await
        .unwrap();
        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert!(archive_files(&project.join(".plan")).is_empty());
        assert!(!events
            .iter()
            .any(|event| event.step == WorkflowState::Complete));
    }

    #[tokio::test]
    async fn full_loop_archives_before_implementer_and_preserves_implementer_plan() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let output = validated_archive(&project, &project.join(".plan"));
        let (state, _, calls, _, implementer_prompt, archive_exists_at_implementer) =
            run_scripted_workflow_with_plan_output(
                "full_loop",
                workflow_snapshot(project.to_string_lossy().into_owned()),
                vec![
                    (AgentRole::Planner, scripted_output("approved plan body")),
                    (
                        AgentRole::PlanReviewer,
                        scripted_output(
                            r#"{"verdict":"approved","summary":"ready","findings":[]}"#,
                        ),
                    ),
                    (AgentRole::Implementer, scripted_output("implementation")),
                    (
                        AgentRole::CodeReviewer,
                        scripted_output(r#"{"verdict":"approved","summary":"done","findings":[]}"#),
                    ),
                ],
                vec![],
                Some(output),
                None,
            )
            .await
            .unwrap();
        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls[2], AgentRole::Implementer);
        let files = archive_files(&project.join(".plan"));
        assert_eq!(files.len(), 1);
        assert_eq!(fs::read_to_string(&files[0]).unwrap(), "approved plan body");
        assert_eq!(archive_exists_at_implementer, Some(true));
        assert!(implementer_prompt.unwrap().contains("approved plan body"));
    }

    #[tokio::test]
    async fn save_time_archive_symlink_escape_fails_before_implementer() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&outside).unwrap();
        let archive = project.join(".plan");
        fs::create_dir(&archive).unwrap();
        let output = validated_archive(&project, &archive);
        let (state, _, calls, _, _, _) = run_scripted_workflow_with_plan_output(
            "full_loop",
            workflow_snapshot(project.to_string_lossy().into_owned()),
            vec![
                (AgentRole::Planner, scripted_output("approved plan")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"ready","findings":[]}"#),
                ),
            ],
            vec![],
            Some(output),
            Some((AgentRole::PlanReviewer, archive.clone(), outside.clone())),
        )
        .await
        .unwrap();
        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert!(archive_files(&outside).is_empty());
    }

    #[tokio::test]
    async fn archive_no_follow_open_race_fails_and_blocks_downstream_roles() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let archive = project.join(".plan");
        let moved_archive = project.join(".plan-before-symlink-race");
        let outside = temp.path().join("outside");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&archive).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(archive.join("V0.24.0-r1.md"), b"pre-existing bytes").unwrap();

        let mut output = validated_archive(&project, &archive);
        let archive_for_hook = archive.clone();
        let moved_for_hook = moved_archive.clone();
        let outside_for_hook = outside.clone();
        let mut replaced = false;
        output.persistence_test_hook = Some(Arc::new(Mutex::new(Box::new(move |stage, _| {
            if stage == ArchivePersistenceStage::ComponentValidated && !replaced {
                replaced = true;
                fs::rename(&archive_for_hook, &moved_for_hook)?;
                #[cfg(windows)]
                std::os::windows::fs::symlink_dir(&outside_for_hook, &archive_for_hook)?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(&outside_for_hook, &archive_for_hook)?;
            }
            Ok(())
        }))));

        let (state, _, calls, _, _, _) = run_scripted_workflow_with_plan_output(
            "full_loop",
            workflow_snapshot(project.to_string_lossy().into_owned()),
            vec![
                (AgentRole::Planner, scripted_output("approved plan")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"ready","findings":[]}"#),
                ),
            ],
            vec![],
            Some(output),
            None,
        )
        .await
        .unwrap();

        assert!(moved_archive.exists());
        assert_eq!(state, WorkflowState::Failed);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert_eq!(
            fs::read(moved_archive.join("V0.24.0-r1.md")).unwrap(),
            b"pre-existing bytes"
        );
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&moved_archive).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn plan_only_success_completes_with_approved_plan_and_no_downstream_roles() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "plan_only",
            snapshot,
            vec![
                (
                    AgentRole::Planner,
                    scripted_output("Generated architecture plan"),
                ),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(
                        r#"{"verdict":"approved","summary":"plan approved","findings":[]}"#,
                    ),
                ),
            ],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::Planner, AgentRole::PlanReviewer]);
        assert_eq!(val_runs, 0);
        assert!(events
            .iter()
            .any(|e| e.step == WorkflowState::Complete && e.plan_text.is_some()));
        assert!(!events.iter().any(|e| matches!(
            e.step,
            WorkflowState::Implementation
                | WorkflowState::Validation
                | WorkflowState::CodeReview
                | WorkflowState::Fix
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
                    scripted_output(
                        r#"{"verdict":"failed","summary":"spec violation","findings":[]}"#,
                    ),
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
            WorkflowState::Implementation
                | WorkflowState::Validation
                | WorkflowState::CodeReview
                | WorkflowState::Complete
        )));
    }

    #[tokio::test]
    async fn implement_only_success_completes_without_planning_or_code_review() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let (state, events, calls, val_runs) = run_scripted_workflow_full(
            "implement_only",
            snapshot,
            vec![(
                AgentRole::Implementer,
                scripted_output("Code changes applied"),
            )],
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
        assert!(events
            .iter()
            .any(|e| e.step == WorkflowState::Implementation));
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
                (
                    AgentRole::Implementer,
                    scripted_output("Initial implementation"),
                ),
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
                (
                    AgentRole::Implementer,
                    scripted_output("Initial implementation"),
                ),
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
                scripted_output(
                    r#"{"verdict":"approved","summary":"diff looks clean","findings":[]}"#,
                ),
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
            WorkflowState::PlanGeneration
                | WorkflowState::Implementation
                | WorkflowState::Validation
                | WorkflowState::Fix
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
        assert_eq!(complete_event.waiting_reason, None, "Complete event on changes_required must have no waiting_reason");
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
        assert!(!events
            .iter()
            .any(|e| matches!(e.step, WorkflowState::Fix | WorkflowState::Validation)));
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
        assert!(!events
            .iter()
            .any(|e| e.step == WorkflowState::WaitingForUser));
    }

    #[tokio::test]
    async fn review_only_rejects_cli_adapter_without_mutating_capabilities() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let mut cli_profile = mutating_cli_profile();
        cli_profile.capabilities =
            vec![ProfileCapability::Review, ProfileCapability::WorkspaceRead];
        snapshot
            .assignments
            .insert(AgentRole::CodeReviewer, cli_profile);

        let result = run_scripted_workflow_full("review_only", snapshot, vec![], vec![]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires a read-only adapter"));
    }

    #[tokio::test]
    async fn review_only_accepts_provider_with_workspace_write_and_executes_review_only() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let mut profile = readonly_reviewer_profile();
        profile.capabilities.push(ProfileCapability::WorkspaceWrite);
        snapshot
            .assignments
            .insert(AgentRole::CodeReviewer, profile);

        let (state, _events, calls, val_runs) = run_scripted_workflow_full(
            "review_only",
            snapshot,
            vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"review passed","findings":[]}"#),
            )],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::CodeReviewer]);
        assert_eq!(val_runs, 0);
    }

    #[tokio::test]
    async fn review_only_accepts_provider_with_command_execution_and_executes_review_only() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let mut profile = readonly_reviewer_profile();
        profile
            .capabilities
            .push(ProfileCapability::CommandExecution);
        snapshot
            .assignments
            .insert(AgentRole::CodeReviewer, profile);

        let (state, _events, calls, val_runs) = run_scripted_workflow_full(
            "review_only",
            snapshot,
            vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"review passed","findings":[]}"#),
            )],
            vec![],
        )
        .await
        .unwrap();

        assert_eq!(state, WorkflowState::Complete);
        assert_eq!(calls, vec![AgentRole::CodeReviewer]);
        assert_eq!(val_runs, 0);
    }

    #[tokio::test]
    async fn review_only_rejects_provider_missing_review_capability() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = workflow_snapshot(directory.path().to_string_lossy().into_owned());
        let mut profile = readonly_reviewer_profile();
        profile.capabilities.retain(|c| *c != ProfileCapability::Review);
        snapshot
            .assignments
            .insert(AgentRole::CodeReviewer, profile);

        let result = run_scripted_workflow_full("review_only", snapshot, vec![], vec![]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing capability [Review]"));
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
            snapshot
                .assignments
                .insert(AgentRole::CodeReviewer, profile);

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
                (
                    AgentRole::PlanReviewer,
                    scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
                ),
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

        let mut plan_required_zero =
            workflow_snapshot(directory.path().to_string_lossy().into_owned());
        plan_required_zero
            .iteration_limits
            .max_plan_review_iterations = 0;
        let err = run_scripted_workflow_full("plan_only", plan_required_zero, vec![], vec![])
            .await
            .unwrap_err();
        assert!(err.contains("plan_review iteration limit"));

        let mut implement_required_zero =
            workflow_snapshot(directory.path().to_string_lossy().into_owned());
        implement_required_zero.iteration_limits.max_fix_iterations = 0;
        let err =
            run_scripted_workflow_full("implement_only", implement_required_zero, vec![], vec![])
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
                (
                    AgentRole::Implementer,
                    scripted_output("Full implementation"),
                ),
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
        assert!(events
            .iter()
            .any(|e| e.step == WorkflowState::PlanGeneration));
        assert!(events.iter().any(|e| e.step == WorkflowState::PlanReview));
        assert!(events
            .iter()
            .any(|e| e.step == WorkflowState::Implementation));
        assert!(events.iter().any(|e| e.step == WorkflowState::Validation));
        assert!(events.iter().any(|e| e.step == WorkflowState::CodeReview));
        assert!(events.iter().any(|e| e.step == WorkflowState::Complete));
    }

    #[tokio::test]
    async fn test_verify_submodules_for_review_fail_closed() {
        // Non-git directory must fail closed with Err
        let temp_dir = tempfile::tempdir().unwrap();
        let res = verify_submodules_for_review(temp_dir.path()).await;
        assert!(res.is_err(), "verify_submodules_for_review must fail closed on invalid/non-git repo: {:?}", res);
    }

    #[tokio::test]
    async fn test_execute_disposable_worktree_review_capability_enforcement() {
        let engine = OrchestratorEngine::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();

        // Profile missing Review capability
        let mut invalid_profile = readonly_reviewer_profile();
        invalid_profile.capabilities = vec![ProfileCapability::Reasoning]; // Missing Review

        let res = execute_disposable_worktree_review(
            &engine,
            &invalid_profile,
            temp_dir.path(),
            "Task",
            "Plan",
            &cancel,
        )
        .await;

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("must have Review capability"));
    }

    #[tokio::test]
    async fn test_execute_sandboxed_review_unsupported_adapter_fails_closed() {
        let engine = OrchestratorEngine::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();

        let mut antigravity_profile = readonly_reviewer_profile();
        antigravity_profile.adapter = ExecutionAdapterType::Antigravity;

        let res = engine
            .execute_sandboxed_review(
                &antigravity_profile,
                "System",
                "User",
                temp_dir.path(),
                Some(&cancel),
            )
            .await;

        assert!(res.is_err(), "Antigravity adapter must fail closed for Code Reviewer");
        assert!(res.unwrap_err().contains("Unsupported adapter"));

        let mut mcp_profile = readonly_reviewer_profile();
        mcp_profile.adapter = ExecutionAdapterType::Mcp;

        let res_mcp = engine
            .execute_sandboxed_review(
                &mcp_profile,
                "System",
                "User",
                temp_dir.path(),
                Some(&cancel),
            )
            .await;

        assert!(res_mcp.is_err(), "MCP adapter must fail closed for Code Reviewer");
        assert!(res_mcp.unwrap_err().contains("Unsupported adapter"));
    }

    #[tokio::test]
    async fn test_wait_for_antigravity_submission_reclaim_epoch_increment() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_path = temp_dir.path().join("orchestrator_session.json");
        let run_id = "test-reclaim-run".to_string();
        let (server, mailbox_state, mut submit_rx, _progress_rx) =
            MailboxServer::start_with_path_and_perm_fn(
                run_id.clone(),
                "C:/dummy".to_string(),
                session_path,
                super::super::mailbox::restrict_session_file_permissions,
            )
            .await
            .unwrap();

        let (worker_reclaim_tx, mut worker_reclaim_rx) = mpsc::channel::<()>(4);
        let cancel_token = CancellationToken::new();
        let on_event: EventCallback = Arc::new(|_| Ok(()));

        let snapshot = workflow_snapshot("C:/dummy".to_string());

        // Spawn wait_for_antigravity_submission in background
        let state_clone = mailbox_state.clone();
        let r_id = run_id.clone();
        let snap_clone = snapshot.clone();
        let cancel_clone = cancel_token.clone();
        let on_event_clone = on_event.clone();

        let wait_handle = tokio::spawn(async move {
            wait_for_antigravity_submission(
                WorkflowState::Implementation,
                AgentRole::Implementer,
                "task-impl",
                &r_id,
                &snap_clone,
                Some("Plan".to_string()),
                Some("Task".to_string()),
                None,
                None,
                None,
                None,
                &state_clone,
                &mut submit_rx,
                &mut worker_reclaim_rx,
                &cancel_clone,
                &on_event_clone,
                &|_| {},
                None,
                None,
            )
            .await
        });

        // Give it a moment to arm
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify initial epoch is 1
        {
            let guard = mailbox_state.inner.lock().await;
            assert_eq!(guard.epoch, 1);
            assert_eq!(guard.current_state, WorkflowState::Implementation);
        }

        // 1. Send early worker reclaim signal while in Implementation -> MUST BE DROPPED
        worker_reclaim_tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify epoch is STILL 1 and state is STILL Implementation
        {
            let guard = mailbox_state.inner.lock().await;
            assert_eq!(guard.epoch, 1, "Early reclaim must NOT increment epoch");
            assert_eq!(guard.current_state, WorkflowState::Implementation);
        }

        // 2. Transition state to WaitingForUser (simulating worker disconnect/lease timeout)
        {
            let mut guard = mailbox_state.inner.lock().await;
            guard.current_state = WorkflowState::WaitingForUser;
        }

        // 3. Send valid reclaim signal while in WaitingForUser
        worker_reclaim_tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify epoch incremented to 2 and state transitioned to AwaitingAntigravityClaim
        {
            let guard = mailbox_state.inner.lock().await;
            assert_eq!(guard.epoch, 2, "Valid reclaim in WaitingForUser must increment epoch");
            assert_eq!(guard.current_state, WorkflowState::AwaitingAntigravityClaim);
            assert_eq!(guard.active_task.as_ref().unwrap().epoch, 2);
        }

        // Submit task with new epoch 2
        let client = reqwest::Client::new();
        // First claim
        let claim_resp = client
            .post(format!("http://127.0.0.1:{}/mailbox/claim", server.port))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&super::super::types::ClaimTaskRequest {
                run_id: run_id.clone(),
                wait_seconds: Some(1),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(claim_resp.status(), reqwest::StatusCode::OK);

        // Submit
        let sub_resp = client
            .post(format!("http://127.0.0.1:{}/mailbox/submit", server.port))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&super::super::types::SubmitTaskRequest {
                run_id: run_id.clone(),
                task_id: "task-impl-2".to_string(),
                epoch: 2,
                idempotency_key: None,
                status: "success".to_string(),
                summary: "Reclaim task finished".to_string(),
                modified_files: vec![],
            })
            .send()
            .await
            .unwrap();
        assert_eq!(sub_resp.status(), reqwest::StatusCode::OK);

        let result = wait_handle.await.unwrap().unwrap();
        match result {
            AntigravitySubmissionOutcome::Submitted(sub) => {
                assert_eq!(sub.summary, "Reclaim task finished");
            }
            AntigravitySubmissionOutcome::BudgetExhausted(_) => panic!("Expected Submitted"),
        }

        server.stop().await;
    }

    #[tokio::test]
    async fn test_worker_lease_timeout_transitions_to_waiting_for_user_and_reclaim() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_path = temp_dir.path().join("orchestrator_session.json");
        let run_id = "test-timeout-run".to_string();
        let (server, mailbox_state, mut submit_rx, _progress_rx) =
            MailboxServer::start_with_path_and_perm_fn(
                run_id.clone(),
                "C:/dummy".to_string(),
                session_path,
                super::super::mailbox::restrict_session_file_permissions,
            )
            .await
            .unwrap();

        // Configure a short lease timeout for fast testing
        {
            let mut guard = mailbox_state.inner.lock().await;
            guard.lease_timeout_duration = Duration::from_millis(200);
        }

        let (worker_reclaim_tx, mut worker_reclaim_rx) = mpsc::channel::<()>(4);
        let cancel_token = CancellationToken::new();
        let on_event: EventCallback = Arc::new(|_| Ok(()));

        let snapshot = workflow_snapshot("C:/dummy".to_string());

        let state_clone = mailbox_state.clone();
        let r_id = run_id.clone();
        let snap_clone = snapshot.clone();
        let cancel_clone = cancel_token.clone();
        let on_event_clone = on_event.clone();

        let wait_handle = tokio::spawn(async move {
            wait_for_antigravity_submission(
                WorkflowState::Implementation,
                AgentRole::Implementer,
                "task-impl",
                &r_id,
                &snap_clone,
                Some("Plan".to_string()),
                Some("Task".to_string()),
                None,
                None,
                None,
                None,
                &state_clone,
                &mut submit_rx,
                &mut worker_reclaim_rx,
                &cancel_clone,
                &on_event_clone,
                &|_| {},
                None,
                None,
            )
            .await
        });

        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", server.port);

        // 1. Worker 1 claims task
        let claim_resp = client
            .post(format!("{}/mailbox/claim", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&super::super::types::ClaimTaskRequest {
                run_id: run_id.clone(),
                wait_seconds: Some(1),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(claim_resp.status(), reqwest::StatusCode::OK);

        // 2. Wait for lease to expire without progress reports
        tokio::time::sleep(Duration::from_millis(700)).await;

        // Verify state is now WaitingForUser and lease is released
        {
            let guard = mailbox_state.inner.lock().await;
            assert_eq!(guard.current_state, WorkflowState::WaitingForUser);
            assert!(!guard.is_claimed, "Lease must be revoked after timeout");
        }

        // 3. New claim attempt while in WaitingForUser must be rejected (204 No Content)
        let claim_during_waiting = client
            .post(format!("{}/mailbox/claim", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&super::super::types::ClaimTaskRequest {
                run_id: run_id.clone(),
                wait_seconds: Some(1),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(claim_during_waiting.status(), reqwest::StatusCode::NO_CONTENT);

        // 4. Stale submit from Worker 1 (with old epoch 1) must be rejected with 409 CONFLICT
        let stale_submit_resp = client
            .post(format!("{}/mailbox/submit", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&super::super::types::SubmitTaskRequest {
                run_id: run_id.clone(),
                task_id: "task-impl-1".to_string(),
                epoch: 1,
                idempotency_key: None,
                status: "success".to_string(),
                summary: "Stale worker submission".to_string(),
                modified_files: vec![],
            })
            .send()
            .await
            .unwrap();
        assert_eq!(stale_submit_resp.status(), reqwest::StatusCode::CONFLICT);

        // 5. Human operator confirms previous worker is stopped -> trigger reclaim
        worker_reclaim_tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify state is now AwaitingAntigravityClaim and epoch is 2
        {
            let guard = mailbox_state.inner.lock().await;
            assert_eq!(guard.epoch, 2);
            assert_eq!(guard.current_state, WorkflowState::AwaitingAntigravityClaim);
        }

        // 6. Worker 2 claims task with new epoch
        let claim2_resp = client
            .post(format!("{}/mailbox/claim", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&super::super::types::ClaimTaskRequest {
                run_id: run_id.clone(),
                wait_seconds: Some(1),
            })
            .send()
            .await
            .unwrap();
        assert_eq!(claim2_resp.status(), reqwest::StatusCode::OK);
        let task2: super::super::types::OrchestratorTaskEnvelope = claim2_resp.json().await.unwrap();
        assert_eq!(task2.epoch, 2);
        assert_eq!(task2.task_id, "task-impl-2");

        // 7. Worker 2 submits successfully with epoch 2
        let submit2_resp = client
            .post(format!("{}/mailbox/submit", base_url))
            .header("Authorization", format!("Bearer {}", server.token))
            .json(&super::super::types::SubmitTaskRequest {
                run_id: run_id.clone(),
                task_id: "task-impl-2".to_string(),
                epoch: 2,
                idempotency_key: None,
                status: "success".to_string(),
                summary: "Worker 2 recovery successful".to_string(),
                modified_files: vec![],
            })
            .send()
            .await
            .unwrap();
        assert_eq!(submit2_resp.status(), reqwest::StatusCode::OK);

        let final_result = wait_handle.await.unwrap().unwrap();
        match final_result {
            AntigravitySubmissionOutcome::Submitted(sub) => {
                assert_eq!(sub.summary, "Worker 2 recovery successful");
            }
            AntigravitySubmissionOutcome::BudgetExhausted(_) => panic!("Expected Submitted"),
        }

        server.stop().await;
    }

    #[test]
    fn test_lean_antigravity_policy_prompt_injection() {
        let base_prompt = "Implement the widget feature as specified in SPEC.md";

        // Mode OFF: Byte-for-byte preserved
        let off_prompt = effective_task_prompt(Some(base_prompt), false);
        assert_eq!(off_prompt, Some(base_prompt.to_string()));

        // Mode ON: Carries LEAN_ANTIGRAVITY_POLICY guidance
        let on_prompt = effective_task_prompt(Some(base_prompt), true);
        assert!(on_prompt.is_some());
        let on_str = on_prompt.unwrap();
        assert!(on_str.starts_with(base_prompt));
        assert!(on_str.contains("## Lean Antigravity Execution Policy"));
        assert!(on_str.contains("Inspect only files directly relevant"));
        assert!(on_str.contains("Validation Harness and Code Review remain authoritative"));

        // None prompt handling
        assert_eq!(effective_task_prompt(None, false), None);
        assert_eq!(effective_task_prompt(None, true), None);
    }

    #[tokio::test]
    async fn test_wait_for_antigravity_submission_budget_exhaustion_task_and_run_scopes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let project_path = temp_dir.path().to_string_lossy().to_string();
        let run_id = "test-budget-engine-run".to_string();

        let (server, state, mut submit_rx, mut _progress_rx) =
            MailboxServer::start(run_id.clone(), project_path.clone())
                .await
                .unwrap();

        // 1. Test Task Scope Exhaustion (2 / 2)
        {
            let mut guard = state.inner.lock().await;
            guard.budget_exhausted = true;
            guard.budget_exhausted_details = Some(super::super::mailbox::BudgetExhaustionDetails {
                scope: super::super::mailbox::BudgetExhaustionScope::Task,
                current: 2,
                limit: 2,
                task_id: "task-impl".to_string(),
                reason: "Antigravity per-task dispatch limit (2/2) reached for 'task-impl'.".to_string(),
            });
            guard.total_dispatches = 2;
            guard.max_dispatches_per_run = 6;
        }

        let mut snapshot = workflow_snapshot(project_path.clone());
        snapshot.lean_antigravity_mode = true;

        let cancel_token = CancellationToken::new();
        let (_worker_reclaim_tx, mut worker_reclaim_rx) = mpsc::channel::<()>(4);
        let events = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let on_event: EventCallback = Arc::new(move |ev| {
            events_clone.lock().unwrap().push(ev);
            Ok(())
        });
        let log = |_msg: String| {};

        let wait_result = wait_for_antigravity_submission(
            WorkflowState::Implementation,
            AgentRole::Implementer,
            "task-impl",
            &run_id,
            &snapshot,
            None,
            Some("Do work".to_string()),
            None,
            None,
            None,
            None,
            &state,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
            None,
            None,
        )
        .await;

        assert!(wait_result.is_ok(), "Must return Ok(BudgetExhausted), not Err");
        match wait_result.unwrap() {
            AntigravitySubmissionOutcome::BudgetExhausted(details) => {
                assert_eq!(details.scope, super::super::mailbox::BudgetExhaustionScope::Task);
                assert_eq!(details.current, 2);
                assert_eq!(details.limit, 2);
            }
            AntigravitySubmissionOutcome::Submitted(_) => panic!("Expected BudgetExhausted"),
        }

        let recorded_events = events.lock().unwrap();
        assert!(!recorded_events.is_empty());
        let last_event = recorded_events.last().unwrap();
        assert_eq!(last_event.step, WorkflowState::WaitingForUser);
        assert_eq!(last_event.budget_scope, Some("task".to_string()));
        assert_eq!(last_event.waiting_reason, Some("budget_exhausted".to_string()));
        assert_eq!(last_event.antigravity_dispatches, Some(2));
        assert_eq!(last_event.antigravity_dispatch_limit, Some(2));
        assert!(last_event.message.is_empty());
        drop(recorded_events);

        // 2. Test Run Scope Exhaustion (6 / 6)
        {
            let mut guard = state.inner.lock().await;
            guard.budget_exhausted = true;
            guard.budget_exhausted_details = Some(super::super::mailbox::BudgetExhaustionDetails {
                scope: super::super::mailbox::BudgetExhaustionScope::Run,
                current: 6,
                limit: 6,
                task_id: "task-fix".to_string(),
                reason: "Antigravity run dispatch limit (6/6) reached.".to_string(),
            });
            guard.total_dispatches = 6;
            guard.max_dispatches_per_run = 6;
        }

        let wait_result_run = wait_for_antigravity_submission(
            WorkflowState::Fix,
            AgentRole::Fixer,
            "task-fix",
            &run_id,
            &snapshot,
            None,
            Some("Fix bugs".to_string()),
            None,
            None,
            None,
            None,
            &state,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
            None,
            None,
        )
        .await;

        assert!(wait_result_run.is_ok());
        match wait_result_run.unwrap() {
            AntigravitySubmissionOutcome::BudgetExhausted(details) => {
                assert_eq!(details.scope, super::super::mailbox::BudgetExhaustionScope::Run);
                assert_eq!(details.current, 6);
                assert_eq!(details.limit, 6);
            }
            AntigravitySubmissionOutcome::Submitted(_) => panic!("Expected BudgetExhausted"),
        }

        let recorded_events2 = events.lock().unwrap();
        let last_event2 = recorded_events2.last().unwrap();
        assert_eq!(last_event2.step, WorkflowState::WaitingForUser);
        assert_eq!(last_event2.budget_scope, Some("run".to_string()));
        assert_eq!(last_event2.waiting_reason, Some("budget_exhausted".to_string()));
        assert_eq!(last_event2.antigravity_dispatches, Some(6));
        assert_eq!(last_event2.antigravity_dispatch_limit, Some(6));
        assert!(last_event2.message.is_empty());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_plan_review_needs_clarification_emits_clarification_required_waiting_reason() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let output = validated_archive(&project, &project.join(".plan"));
        let (state, events, calls, _, _, _) = run_scripted_workflow_with_plan_output(
            "plan_only",
            workflow_snapshot(project.to_string_lossy().into_owned()),
            vec![
                (AgentRole::Planner, scripted_output("initial plan draft")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(
                        r#"{"verdict":"needs_clarification","summary":"need clarifications on DB schema","findings":[]}"#,
                    ),
                ),
                (AgentRole::Planner, scripted_output("revised plan draft")),
                (
                    AgentRole::PlanReviewer,
                    scripted_output(
                        r#"{"verdict":"approved","summary":"plan looks great","findings":[]}"#,
                    ),
                ),
            ],
            vec![],
            Some(output),
            None,
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
                AgentRole::PlanReviewer
            ]
        );
        let waiting_event = events
            .iter()
            .find(|e| e.step == WorkflowState::WaitingForUser)
            .expect("Expected WaitingForUser event for plan reviewer clarification request");
        assert_eq!(
            waiting_event.waiting_reason,
            Some("clarification_required".to_string())
        );
    }

    fn test_engine_with_reviewer_response(content: &str) -> OrchestratorEngine {
        let mut engine = OrchestratorEngine::new();
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(content),
            )])),
            ..Default::default()
        });
        engine.scripted_adapter_executor = Some(scripted_executor);
        engine
    }

    fn run_git_test_cmd(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|e| panic!("Failed to run git {:?} in {}: {}", args, dir.display(), e));
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            panic!(
                "git {:?} in {} failed (code {:?}):\nstdout: {}\nstderr: {}",
                args,
                dir.display(),
                out.status.code(),
                stdout,
                stderr
            );
        }
    }

    fn create_test_git_repo(dir: &Path) {
        let _ = fs::create_dir_all(dir);
        run_git_test_cmd(dir, &["init", "-b", "main"]);
        run_git_test_cmd(dir, &["config", "user.name", "Tester"]);
        run_git_test_cmd(dir, &["config", "user.email", "tester@example.com"]);
    }

    #[tokio::test]
    async fn test_submodule_clean_initialized_head_equals_gitlink() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub v1\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub v1"]);

        create_test_git_repo(&parent_path);
        fs::write(parent_path.join("root.txt"), "root v1\n").unwrap();
        run_git_test_cmd(&parent_path, &["add", "root.txt"]);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        let submodules = verify_submodules_for_review(&parent_path).await.unwrap();
        assert_eq!(submodules.len(), 1);
        assert_eq!(submodules[0].rel_path, "submod");
        assert_eq!(submodules[0].head_oid, submodules[0].parent_gitlink);

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub_fp = capture_repo_fingerprint(&parent_path.join("submod"))
            .await
            .unwrap();

        let engine = test_engine_with_reviewer_response(
            r#"{"verdict":"approved","summary":"clean submodule","findings":[]}"#,
        );
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(res.verdict, ReviewVerdict::Approved);

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp = capture_repo_fingerprint(&parent_path.join("submod"))
            .await
            .unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub_fp, post_sub_fp);
    }

    #[tokio::test]
    async fn test_submodule_local_commit_ahead_of_gitlink() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub v1\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub v1"]);

        create_test_git_repo(&parent_path);
        fs::write(parent_path.join("root.txt"), "root v1\n").unwrap();
        run_git_test_cmd(&parent_path, &["add", "root.txt"]);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        // Advance submodule locally inside parent repository
        let parent_submod = parent_path.join("submod");
        fs::write(parent_submod.join("sub.txt"), "sub v2 local commit\n").unwrap();
        run_git_test_cmd(&parent_submod, &["add", "sub.txt"]);
        run_git_test_cmd(&parent_submod, &["commit", "-m", "sub v2 local"]);

        let submodules = verify_submodules_for_review(&parent_path).await.unwrap();
        assert_eq!(submodules.len(), 1);
        assert_ne!(submodules[0].head_oid, submodules[0].parent_gitlink);

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();

        let engine = test_engine_with_reviewer_response(
            r#"{"verdict":"approved","summary":"submodule commit ahead reviewed","findings":[]}"#,
        );
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(res.verdict, ReviewVerdict::Approved);

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub_fp, post_sub_fp);
    }

    #[tokio::test]
    async fn test_submodule_divergent_local_history() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub v1\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub v1"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        let parent_submod = parent_path.join("submod");
        run_git_test_cmd(&parent_submod, &["checkout", "-b", "feature-branch"]);
        fs::write(parent_submod.join("feature.txt"), "feature data\n").unwrap();
        run_git_test_cmd(&parent_submod, &["add", "feature.txt"]);
        run_git_test_cmd(&parent_submod, &["commit", "-m", "feature commit"]);

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();

        let engine = test_engine_with_reviewer_response(
            r#"{"verdict":"approved","summary":"divergent branch reviewed","findings":[]}"#,
        );
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(res.verdict, ReviewVerdict::Approved);

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub_fp, post_sub_fp);
    }

    #[tokio::test]
    async fn test_submodule_staged_unstaged_edits_and_renames() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "v1\n").unwrap();
        fs::write(sub_path.join("to_rename.txt"), "rename me\n").unwrap();
        fs::write(sub_path.join("to_delete.txt"), "delete me\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt", "to_rename.txt", "to_delete.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub init"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent init"]);

        let parent_submod = parent_path.join("submod");
        // Staged edit
        fs::write(parent_submod.join("sub.txt"), "v2 staged\n").unwrap();
        run_git_test_cmd(&parent_submod, &["add", "sub.txt"]);
        // Unstaged edit on top of staged
        fs::write(parent_submod.join("sub.txt"), "v2 staged + unstaged\n").unwrap();
        // Rename
        run_git_test_cmd(&parent_submod, &["mv", "to_rename.txt", "renamed.txt"]);
        // Delete unstaged
        fs::remove_file(parent_submod.join("to_delete.txt")).unwrap();

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();

        let engine = test_engine_with_reviewer_response(
            r#"{"verdict":"approved","summary":"staged unstaged rename delete reviewed","findings":[]}"#,
        );
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(res.verdict, ReviewVerdict::Approved);

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub_fp, post_sub_fp);
    }

    #[tokio::test]
    async fn test_submodule_nested_with_local_commits_and_dirty_changes() {
        let temp = tempfile::tempdir().unwrap();
        let sub2_path = temp.path().join("sub2");
        let sub1_path = temp.path().join("sub1");
        let parent_path = temp.path().join("parent");

        // sub2
        create_test_git_repo(&sub2_path);
        fs::write(sub2_path.join("sub2.txt"), "sub2 v1\n").unwrap();
        run_git_test_cmd(&sub2_path, &["add", "sub2.txt"]);
        run_git_test_cmd(&sub2_path, &["commit", "-m", "sub2 v1"]);

        // sub1 contains sub2
        create_test_git_repo(&sub1_path);
        fs::write(sub1_path.join("sub1.txt"), "sub1 v1\n").unwrap();
        run_git_test_cmd(&sub1_path, &["add", "sub1.txt"]);
        run_git_test_cmd(
            &sub1_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub2_path.to_string_lossy(),
                "nested_sub2",
            ],
        );
        run_git_test_cmd(&sub1_path, &["commit", "-m", "sub1 v1"]);

        // parent contains sub1
        create_test_git_repo(&parent_path);
        fs::write(parent_path.join("root.txt"), "root v1\n").unwrap();
        run_git_test_cmd(&parent_path, &["add", "root.txt"]);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub1_path.to_string_lossy(),
                "sub1",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
                "--recursive",
            ],
        );

        // Advance nested_sub2
        let nested_sub2_path = parent_path.join("sub1").join("nested_sub2");
        fs::write(nested_sub2_path.join("sub2.txt"), "sub2 v2 local\n").unwrap();
        run_git_test_cmd(&nested_sub2_path, &["add", "sub2.txt"]);
        run_git_test_cmd(&nested_sub2_path, &["commit", "-m", "sub2 v2 local"]);
        fs::write(nested_sub2_path.join("sub2_dirty.txt"), "dirty in nested\n").unwrap();

        // Dirty change in sub1
        let sub1_in_parent = parent_path.join("sub1");
        fs::write(sub1_in_parent.join("sub1_dirty.txt"), "dirty in sub1\n").unwrap();

        // Dirty change in root
        fs::write(parent_path.join("root_dirty.txt"), "dirty in root\n").unwrap();

        let submodules = verify_submodules_for_review(&parent_path).await.unwrap();
        assert_eq!(submodules.len(), 2);
        // Deepest first
        assert_eq!(submodules[0].rel_path, "sub1/nested_sub2");
        assert_eq!(submodules[1].rel_path, "sub1");

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub1_fp = capture_repo_fingerprint(&sub1_in_parent).await.unwrap();
        let pre_sub2_fp = capture_repo_fingerprint(&nested_sub2_path).await.unwrap();

        let engine = test_engine_with_reviewer_response(
            r#"{"verdict":"approved","summary":"nested submodules reviewed","findings":[]}"#,
        );
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(res.verdict, ReviewVerdict::Approved);

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub1_fp = capture_repo_fingerprint(&sub1_in_parent).await.unwrap();
        let post_sub2_fp = capture_repo_fingerprint(&nested_sub2_path).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub1_fp, post_sub1_fp);
        assert_eq!(pre_sub2_fp, post_sub2_fp);
    }

    #[tokio::test]
    async fn test_submodule_untracked_copied_and_ignored_excluded() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join(".gitignore"), "*.log\n").unwrap();
        fs::write(sub_path.join("sub.txt"), "sub v1\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", ".gitignore", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub v1"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        let parent_submod = parent_path.join("submod");
        fs::write(parent_submod.join("ignored.log"), "do not copy\n").unwrap();
        fs::write(parent_submod.join("eligible.txt"), "please copy\n").unwrap();

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();

        let engine = test_engine_with_reviewer_response(
            r#"{"verdict":"approved","summary":"untracked eligible copied","findings":[]}"#,
        );
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(res.verdict, ReviewVerdict::Approved);

        // Verify primary ignored.log still exists intact
        assert!(parent_submod.join("ignored.log").exists());
        assert!(parent_submod.join("eligible.txt").exists());

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub_fp, post_sub_fp);
    }

    #[tokio::test]
    async fn test_submodule_path_with_spaces_and_unicode() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub content\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub init"]);

        create_test_git_repo(&parent_path);
        let sub_rel = "sub dir with spaces/nested_sub_äöü";
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                sub_rel,
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent init"]);

        let submodules = verify_submodules_for_review(&parent_path).await.unwrap();
        assert_eq!(submodules.len(), 1);
        assert_eq!(submodules[0].rel_path, sub_rel);

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub_fp = capture_repo_fingerprint(&parent_path.join(sub_rel))
            .await
            .unwrap();

        let engine = test_engine_with_reviewer_response(
            r#"{"verdict":"approved","summary":"spaces and unicode path reviewed","findings":[]}"#,
        );
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await
        .unwrap();

        assert_eq!(res.verdict, ReviewVerdict::Approved);

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp = capture_repo_fingerprint(&parent_path.join(sub_rel))
            .await
            .unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub_fp, post_sub_fp);
    }

    #[tokio::test]
    async fn test_submodule_preflight_fails_closed_on_uninitialized_and_corrupt() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent"]);

        // Case A: Uninitialized submodule directory deleted
        let _ = fs::remove_dir_all(parent_path.join("submod"));
        let res = verify_submodules_for_review(&parent_path).await;
        assert!(res.is_err());
        assert!(
            res.unwrap_err().contains("uninitialized"),
            "Must fail closed on uninitialized submodule"
        );

        // Case B: Directory exists but not a git repo (.git deleted)
        let _ = fs::create_dir_all(parent_path.join("submod"));
        let res2 = verify_submodules_for_review(&parent_path).await;
        assert!(res2.is_err());
        assert!(
            res2.unwrap_err().contains("not a valid git repository"),
            "Must fail closed on missing .git in submodule"
        );
    }

    #[tokio::test]
    async fn test_submodule_cancellation_and_error_cleans_only_disposable_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent"]);

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();

        let engine = OrchestratorEngine::new();
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();
        cancel.cancel(); // Cancel immediately

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err());

        // Primary parent remains untouched
        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
    }

    #[tokio::test]
    async fn test_submodule_reviewer_write_in_nested_submodule_detected_security_violation() {
        let temp = tempfile::tempdir().unwrap();
        let sub2_path = temp.path().join("sub2");
        let sub1_path = temp.path().join("sub1");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub2_path);
        fs::write(sub2_path.join("sub2.txt"), "sub2 v1\n").unwrap();
        run_git_test_cmd(&sub2_path, &["add", "sub2.txt"]);
        run_git_test_cmd(&sub2_path, &["commit", "-m", "sub2 v1"]);

        create_test_git_repo(&sub1_path);
        fs::write(sub1_path.join("sub1.txt"), "sub1 v1\n").unwrap();
        run_git_test_cmd(&sub1_path, &["add", "sub1.txt"]);
        run_git_test_cmd(
            &sub1_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub2_path.to_string_lossy(),
                "nested_sub2",
            ],
        );
        run_git_test_cmd(&sub1_path, &["commit", "-m", "sub1 v1"]);

        create_test_git_repo(&parent_path);
        fs::write(parent_path.join("root.txt"), "root v1\n").unwrap();
        run_git_test_cmd(&parent_path, &["add", "root.txt"]);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub1_path.to_string_lossy(),
                "sub1",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
                "--recursive",
            ],
        );

        let sub1_in_parent = parent_path.join("sub1");
        let nested_sub2_path = sub1_in_parent.join("nested_sub2");

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub1_fp = capture_repo_fingerprint(&sub1_in_parent).await.unwrap();
        let pre_sub2_fp = capture_repo_fingerprint(&nested_sub2_path).await.unwrap();

        // Simulate a reviewer process that rogue-writes a file into the nested submodule inside disposable worktree
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(
                    r#"{"verdict":"approved","summary":"rogue write attempt","findings":[]}"#,
                ),
            )])),
            on_reviewer_execute: Mutex::new(Some(Arc::new(|worktree_path: &Path| {
                let rogue_file = worktree_path
                    .join("sub1")
                    .join("nested_sub2")
                    .join("rogue.txt");
                let _ = fs::write(rogue_file, "rogue reviewer write\n");
            }))),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err());
        let err_msg = res.unwrap_err();
        assert!(
            err_msg.contains("Security violation: Reviewer process modified files in review worktree submodule."),
            "Expected submodule security violation error, got: {}",
            err_msg
        );

        // Verify reviewer was called exactly once
        assert_eq!(scripted_executor.calls.lock().unwrap().len(), 1);

        // Verify primary repos remain completely untouched
        assert!(!parent_path.join("sub1").join("nested_sub2").join("rogue.txt").exists());
        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub1_fp = capture_repo_fingerprint(&sub1_in_parent).await.unwrap();
        let post_sub2_fp = capture_repo_fingerprint(&nested_sub2_path).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub1_fp, post_sub1_fp);
        assert_eq!(pre_sub2_fp, post_sub2_fp);
    }

    #[tokio::test]
    async fn test_submodule_untracked_content_change_same_size_detected() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        create_test_git_repo(&repo_path);
        fs::write(repo_path.join("tracked.txt"), "tracked content\n").unwrap();
        run_git_test_cmd(&repo_path, &["add", "tracked.txt"]);
        run_git_test_cmd(&repo_path, &["commit", "-m", "init"]);

        // Create untracked file with 4 bytes
        fs::write(repo_path.join("untracked.txt"), "AAAA").unwrap();
        let fp1 = capture_repo_fingerprint(&repo_path).await.unwrap();

        // Mutate untracked file with same 4 bytes length but different hash
        fs::write(repo_path.join("untracked.txt"), "BBBB").unwrap();
        let fp2 = capture_repo_fingerprint(&repo_path).await.unwrap();

        assert_ne!(
            fp1, fp2,
            "Cryptographic hash must distinguish same-size content modifications in untracked files"
        );
    }

    #[tokio::test]
    async fn test_capture_repo_fingerprint_fails_closed_on_corrupt_or_invalid_git() {
        let temp = tempfile::tempdir().unwrap();
        let non_git = temp.path().join("non_git");
        fs::create_dir_all(&non_git).unwrap();

        let res = capture_repo_fingerprint(&non_git).await;
        assert!(res.is_err(), "capture_repo_fingerprint must fail closed on non-git directory");
    }

    #[tokio::test]
    async fn test_submodule_snapshot_error_prevents_reviewer_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent"]);

        // Corrupt submodule before review by removing its .git handle
        let git_path = parent_path.join("submod").join(".git");
        if git_path.is_dir() {
            let _ = fs::remove_dir_all(&git_path);
        } else {
            let _ = fs::remove_file(&git_path);
        }

        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )])),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err());
        assert!(
            scripted_executor.calls.lock().unwrap().is_empty(),
            "Reviewer must never be dispatched if submodule preflight/snapshot fails"
        );
    }

    #[tokio::test]
    async fn test_submodule_malformed_gitmodules_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub v1\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub v1"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        // Corrupt .gitmodules with invalid syntax
        fs::write(
            parent_path.join(".gitmodules"),
            "[submodule \"submod\"\n  path = \n  bad syntax !!\n",
        )
        .unwrap();

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();

        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )])),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err(), "Must fail closed on malformed .gitmodules");
        assert!(
            scripted_executor.calls.lock().unwrap().is_empty(),
            "Reviewer must never be dispatched on malformed .gitmodules"
        );

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
    }

    #[tokio::test]
    async fn test_submodule_path_traversal_in_gitmodules_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let parent_path = temp.path().join("parent");
        create_test_git_repo(&parent_path);
        fs::write(parent_path.join("root.txt"), "root\n").unwrap();
        run_git_test_cmd(&parent_path, &["add", "root.txt"]);
        run_git_test_cmd(&parent_path, &["commit", "-m", "init"]);

        // Craft .gitmodules with path traversal
        fs::write(
            parent_path.join(".gitmodules"),
            "[submodule \"traversal\"]\n  path = ../escape\n  url = https://example.com/repo\n",
        )
        .unwrap();

        let res = verify_submodules_for_review(&parent_path).await;
        assert!(res.is_err());
        assert!(
            res.unwrap_err().contains("path traversal"),
            "Must detect path traversal in submodule path"
        );
    }

    #[tokio::test]
    async fn test_submodule_untracked_mode_change_detected() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        create_test_git_repo(&repo_path);
        fs::write(repo_path.join("tracked.txt"), "tracked content\n").unwrap();
        run_git_test_cmd(&repo_path, &["add", "tracked.txt"]);
        run_git_test_cmd(&repo_path, &["commit", "-m", "init"]);

        // Create untracked file
        let file_path = repo_path.join("untracked_script.sh");
        fs::write(&file_path, "echo hello\n").unwrap();
        let fp1 = capture_repo_fingerprint(&repo_path).await.unwrap();

        // Mutate mode (permissions) on the untracked file without altering content
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file_path, perms).unwrap();
        }
        #[cfg(not(unix))]
        {
            let mut perms = fs::metadata(&file_path).unwrap().permissions();
            perms.set_readonly(true);
            fs::set_permissions(&file_path, perms).unwrap();
        }

        let fp2 = capture_repo_fingerprint(&repo_path).await.unwrap();
        assert_ne!(
            fp1, fp2,
            "Mode change on untracked file must produce different fingerprint"
        );

        // Revert mode
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o644);
            fs::set_permissions(&file_path, perms).unwrap();
        }
        #[cfg(not(unix))]
        {
            let mut perms = fs::metadata(&file_path).unwrap().permissions();
            perms.set_readonly(false);
            fs::set_permissions(&file_path, perms).unwrap();
        }

        let fp3 = capture_repo_fingerprint(&repo_path).await.unwrap();
        assert_eq!(
            fp1, fp3,
            "Reverting mode must restore original fingerprint"
        );
    }

    #[tokio::test]
    async fn test_submodule_root_symlink_creation_failure_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        create_test_git_repo(&repo_path);
        fs::write(repo_path.join("root.txt"), "root\n").unwrap();
        run_git_test_cmd(&repo_path, &["add", "root.txt"]);
        run_git_test_cmd(&repo_path, &["commit", "-m", "init"]);

        // Create an untracked file and an untracked symlink
        fs::write(repo_path.join("target.txt"), "target content\n").unwrap();
        let link_path = repo_path.join("link.txt");
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink("target.txt", &link_path);
        }
        #[cfg(windows)]
        {
            let _ = std::os::windows::fs::symlink_file("target.txt", &link_path);
        }

        if !link_path.is_symlink() {
            eprintln!("SKIPPED test_submodule_root_symlink_creation_failure_fails_closed: Host OS does not permit unprivileged symlink creation; deterministic dispatch is tested in test_symlink_operation_dispatch_and_no_fallback_deterministic.");
            return;
        }

        let pre_fp = capture_repo_fingerprint(&repo_path).await.unwrap();

        // Inject symlink failure via test seam
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )])),
            symlink_op_hook: Mutex::new(Some(Arc::new(|_op: SymlinkOperation, _src: &Path, _dst: &Path| {
                Err(std::io::Error::other("injected root symlink recreation error"))
            }))),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &repo_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err(), "Must fail closed on symlink recreation failure");
        assert!(
            scripted_executor.calls.lock().unwrap().is_empty(),
            "Reviewer must never be dispatched on symlink creation failure"
        );

        let post_fp = capture_repo_fingerprint(&repo_path).await.unwrap();
        assert_eq!(pre_fp, post_fp, "Primary repo must remain unchanged");
    }

    #[tokio::test]
    async fn test_submodule_nested_symlink_creation_failure_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub v1\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub v1"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        let parent_submod = parent_path.join("submod");
        fs::write(parent_submod.join("target.txt"), "sub target\n").unwrap();
        let sub_link_path = parent_submod.join("sub_link.txt");
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink("target.txt", &sub_link_path);
        }
        #[cfg(windows)]
        {
            let _ = std::os::windows::fs::symlink_file("target.txt", &sub_link_path);
        }

        if !sub_link_path.is_symlink() {
            eprintln!("SKIPPED test_submodule_nested_symlink_creation_failure_fails_closed: Host OS does not permit unprivileged symlink creation; deterministic dispatch is tested in test_symlink_operation_dispatch_and_no_fallback_deterministic.");
            return;
        }

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();

        // Inject symlink failure via test seam
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )])),
            symlink_op_hook: Mutex::new(Some(Arc::new(|_op: SymlinkOperation, _src: &Path, _dst: &Path| {
                Err(std::io::Error::other("injected submodule symlink recreation error"))
            }))),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err(), "Must fail closed on nested submodule symlink failure");
        assert!(
            scripted_executor.calls.lock().unwrap().is_empty(),
            "Reviewer must never be dispatched on nested symlink creation failure"
        );

        let post_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp = capture_repo_fingerprint(&parent_submod).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp);
        assert_eq!(pre_sub_fp, post_sub_fp);
    }

    fn make_test_output(exit_code: i32, stderr_msg: &str) -> std::process::Output {
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt;
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        #[cfg(windows)]
        let status = std::process::ExitStatus::from_raw(exit_code as u32);
        #[cfg(unix)]
        let status = std::process::ExitStatus::from_raw(exit_code << 8);

        std::process::Output {
            status,
            stdout: Vec::new(),
            stderr: stderr_msg.as_bytes().to_vec(),
        }
    }

    #[tokio::test]
    async fn test_submodule_gitmodules_command_spawn_and_exit_failure_seam() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub v1\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub v1"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        let pre_parent_fp = capture_repo_fingerprint(&parent_path).await.unwrap();
        let pre_sub_fp = capture_repo_fingerprint(&parent_path.join("submod")).await.unwrap();

        // 1. Test spawn failure via gitmodules_cmd_override seam
        let scripted_executor_spawn_err = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )])),
            gitmodules_cmd_override: Mutex::new(Some(Arc::new(|_repo: &Path, _args: &[&str]| {
                Err(std::io::Error::other("simulated git config spawn failure"))
            }))),
            ..Default::default()
        });

        let mut engine1 = OrchestratorEngine::new();
        engine1.scripted_adapter_executor = Some(scripted_executor_spawn_err.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res1 = execute_disposable_worktree_review(
            &engine1,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res1.is_err(), "Must fail closed on .gitmodules command spawn failure");
        assert!(
            res1.unwrap_err().contains("Failed to execute 'git config' on .gitmodules"),
            "Error must identify git config execution failure"
        );
        assert!(
            scripted_executor_spawn_err.calls.lock().unwrap().is_empty(),
            "Reviewer must never be dispatched on spawn failure"
        );

        let post_parent_fp1 = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp1 = capture_repo_fingerprint(&parent_path.join("submod")).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp1);
        assert_eq!(pre_sub_fp, post_sub_fp1);

        // 2. Test non-zero exit code (exit 128) via gitmodules_cmd_override seam
        let scripted_executor_exit_128 = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )])),
            gitmodules_cmd_override: Mutex::new(Some(Arc::new(|_repo: &Path, _args: &[&str]| {
                Ok(make_test_output(128, "fatal: unable to read config file .gitmodules"))
            }))),
            ..Default::default()
        });

        let mut engine2 = OrchestratorEngine::new();
        engine2.scripted_adapter_executor = Some(scripted_executor_exit_128.clone());

        let res2 = execute_disposable_worktree_review(
            &engine2,
            &profile,
            &parent_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res2.is_err(), "Must fail closed on .gitmodules command non-zero exit");
        assert!(
            res2.unwrap_err().contains("Failed to read .gitmodules"),
            "Error must identify git config non-zero exit failure"
        );
        assert!(
            scripted_executor_exit_128.calls.lock().unwrap().is_empty(),
            "Reviewer must never be dispatched on non-zero exit failure"
        );

        let post_parent_fp2 = capture_repo_fingerprint(&parent_path).await.unwrap();
        let post_sub_fp2 = capture_repo_fingerprint(&parent_path.join("submod")).await.unwrap();
        assert_eq!(pre_parent_fp, post_parent_fp2);
        assert_eq!(pre_sub_fp, post_sub_fp2);
    }

    #[tokio::test]
    async fn test_submodule_symlink_operation_selector_deterministic() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("regular_file.txt");
        let dir_path = temp.path().join("regular_dir");
        fs::write(&file_path, "file\n").unwrap();
        fs::create_dir_all(&dir_path).unwrap();

        let file_meta = fs::symlink_metadata(&file_path).unwrap();
        let dir_meta = fs::symlink_metadata(&dir_path).unwrap();

        let file_op = resolve_symlink_operation(&file_meta).unwrap();
        assert_eq!(file_op, SymlinkOperation::File);

        #[cfg(windows)]
        {
            let dir_op = resolve_symlink_operation(&dir_meta).unwrap();
            assert_eq!(dir_op, SymlinkOperation::Dir);
        }

        assert_eq!(
            resolve_symlink_operation_from_semantics(SourceLinkSemantics::File),
            SymlinkOperation::File
        );
        assert_eq!(
            resolve_symlink_operation_from_semantics(SourceLinkSemantics::Dir),
            SymlinkOperation::Dir
        );
    }

    #[tokio::test]
    async fn test_submodule_symlink_operation_dispatch_and_no_fallback_deterministic() {
        // 1. File-link semantics select SymlinkOperation::File exactly once, Dir is never called
        {
            let mut engine = OrchestratorEngine::new();
            let executor = Arc::new(ScriptedAdapterExecutor {
                symlink_op_hook: Mutex::new(Some(Arc::new(|_op, _src, _dst| Ok(())))),
                ..Default::default()
            });
            engine.scripted_adapter_executor = Some(executor.clone());

            let res = dispatch_symlink_recreation(
                &engine,
                SourceLinkSemantics::File,
                Path::new("target_file.txt"),
                Path::new("dst_link_file.txt"),
                "dst_link_file.txt",
                None,
            );

            assert_eq!(res.unwrap(), SymlinkOperation::File);
            let recorded = executor.symlink_op_recorder.lock().unwrap().clone();
            assert_eq!(recorded.len(), 1, "Exactly one symlink operation should be recorded");
            assert_eq!(recorded[0].1, SymlinkOperation::File, "Selected operation must be File");
            let dir_calls = recorded.iter().filter(|(_, op)| *op == SymlinkOperation::Dir).count();
            assert_eq!(dir_calls, 0, "Opposite operation (Dir) must never be called for file semantics");
        }

        // 2. Directory-link semantics select SymlinkOperation::Dir exactly once, File is never called
        {
            let mut engine = OrchestratorEngine::new();
            let executor = Arc::new(ScriptedAdapterExecutor {
                symlink_op_hook: Mutex::new(Some(Arc::new(|_op, _src, _dst| Ok(())))),
                ..Default::default()
            });
            engine.scripted_adapter_executor = Some(executor.clone());

            let res = dispatch_symlink_recreation(
                &engine,
                SourceLinkSemantics::Dir,
                Path::new("target_dir"),
                Path::new("dst_link_dir"),
                "dst_link_dir",
                Some("my_submodule"),
            );

            assert_eq!(res.unwrap(), SymlinkOperation::Dir);
            let recorded = executor.symlink_op_recorder.lock().unwrap().clone();
            assert_eq!(recorded.len(), 1, "Exactly one symlink operation should be recorded");
            assert_eq!(recorded[0].1, SymlinkOperation::Dir, "Selected operation must be Dir");
            let file_calls = recorded.iter().filter(|(_, op)| *op == SymlinkOperation::File).count();
            assert_eq!(file_calls, 0, "Opposite operation (File) must never be called for directory semantics");
        }

        // 3. Injected matching File failure propagates immediately and Dir is uncalled
        {
            let mut engine = OrchestratorEngine::new();
            let executor = Arc::new(ScriptedAdapterExecutor {
                symlink_op_hook: Mutex::new(Some(Arc::new(|op, _src, _dst| {
                    if op == SymlinkOperation::File {
                        Err(std::io::Error::other("injected matching file symlink failure"))
                    } else {
                        Ok(())
                    }
                }))),
                ..Default::default()
            });
            engine.scripted_adapter_executor = Some(executor.clone());

            let res = dispatch_symlink_recreation(
                &engine,
                SourceLinkSemantics::File,
                Path::new("target_file.txt"),
                Path::new("dst_link_file.txt"),
                "dst_link_file.txt",
                None,
            );

            assert!(res.is_err(), "Must fail closed on matching file symlink failure");
            let err_msg = res.unwrap_err();
            assert!(
                err_msg.contains("injected matching file symlink failure"),
                "Error message must contain injected failure: {}",
                err_msg
            );
            let recorded = executor.symlink_op_recorder.lock().unwrap().clone();
            assert_eq!(recorded.len(), 1, "Only matching operation should be attempted");
            assert_eq!(recorded[0].1, SymlinkOperation::File);
            let dir_calls = recorded.iter().filter(|(_, op)| *op == SymlinkOperation::Dir).count();
            assert_eq!(dir_calls, 0, "Opposite operation (Dir) must never be attempted on file failure");
        }

        // 4. Injected matching Dir failure propagates immediately and File is uncalled
        {
            let mut engine = OrchestratorEngine::new();
            let executor = Arc::new(ScriptedAdapterExecutor {
                symlink_op_hook: Mutex::new(Some(Arc::new(|op, _src, _dst| {
                    if op == SymlinkOperation::Dir {
                        Err(std::io::Error::other("injected matching dir symlink failure"))
                    } else {
                        Ok(())
                    }
                }))),
                ..Default::default()
            });
            engine.scripted_adapter_executor = Some(executor.clone());

            let res = dispatch_symlink_recreation(
                &engine,
                SourceLinkSemantics::Dir,
                Path::new("target_dir"),
                Path::new("dst_link_dir"),
                "dst_link_dir",
                Some("my_submodule"),
            );

            assert!(res.is_err(), "Must fail closed on matching dir symlink failure");
            let err_msg = res.unwrap_err();
            assert!(
                err_msg.contains("injected matching dir symlink failure"),
                "Error message must contain injected failure: {}",
                err_msg
            );
            let recorded = executor.symlink_op_recorder.lock().unwrap().clone();
            assert_eq!(recorded.len(), 1, "Only matching operation should be attempted");
            assert_eq!(recorded[0].1, SymlinkOperation::Dir);
            let file_calls = recorded.iter().filter(|(_, op)| *op == SymlinkOperation::File).count();
            assert_eq!(file_calls, 0, "Opposite operation (File) must never be attempted on dir failure");
        }
    }

    #[tokio::test]
    async fn test_submodule_symlink_kind_selection_file_and_dir() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        create_test_git_repo(&repo_path);
        fs::write(repo_path.join("root.txt"), "root\n").unwrap();
        run_git_test_cmd(&repo_path, &["add", "root.txt"]);
        run_git_test_cmd(&repo_path, &["commit", "-m", "init"]);

        // Create untracked targets
        fs::write(repo_path.join("file_target.txt"), "file content\n").unwrap();
        fs::create_dir_all(repo_path.join("dir_target")).unwrap();
        fs::write(repo_path.join("dir_target").join("inner.txt"), "inner\n").unwrap();

        // Create untracked symlinks
        let file_symlink = repo_path.join("link_file.txt");
        let dir_symlink = repo_path.join("link_dir");
        #[cfg(windows)]
        {
            let _ = std::os::windows::fs::symlink_file("file_target.txt", &file_symlink);
            let _ = std::os::windows::fs::symlink_dir("dir_target", &dir_symlink);
        }
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink("file_target.txt", &file_symlink);
            let _ = std::os::unix::fs::symlink("dir_target", &dir_symlink);
        }

        // Test if symlink creation succeeded on this platform
        if !file_symlink.is_symlink() || !dir_symlink.is_symlink() {
            eprintln!("SKIPPED test_submodule_symlink_kind_selection_file_and_dir: Host OS does not permit unprivileged symlink creation; deterministic dispatch is tested in test_symlink_operation_dispatch_and_no_fallback_deterministic.");
            return;
        }

        let pre_fp = capture_repo_fingerprint(&repo_path).await.unwrap();

        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"ok","findings":[]}"#),
            )])),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &repo_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_ok(), "Review execution should succeed: {:?}", res.err());

        let recorded = scripted_executor.symlink_op_recorder.lock().unwrap().clone();
        #[cfg(windows)]
        {
            let file_link_rec = recorded
                .iter()
                .find(|(p, _)| p.file_name().unwrap() == "link_file.txt");
            assert!(file_link_rec.is_some());
            assert_eq!(file_link_rec.unwrap().1, SymlinkOperation::File);

            let dir_link_rec = recorded
                .iter()
                .find(|(p, _)| p.file_name().unwrap() == "link_dir");
            assert!(dir_link_rec.is_some());
            assert_eq!(dir_link_rec.unwrap().1, SymlinkOperation::Dir);
        }

        let post_fp = capture_repo_fingerprint(&repo_path).await.unwrap();
        assert_eq!(pre_fp, post_fp);
    }

    #[tokio::test]
    async fn test_submodule_cleanup_git_worktree_remove_failure_returns_error() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        create_test_git_repo(&repo_path);
        fs::write(repo_path.join("root.txt"), "root\n").unwrap();
        run_git_test_cmd(&repo_path, &["add", "root.txt"]);
        run_git_test_cmd(&repo_path, &["commit", "-m", "init"]);

        let pre_fp = capture_repo_fingerprint(&repo_path).await.unwrap();

        // Inject Git worktree removal failure
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"clean code","findings":[]}"#),
            )])),
            cleanup_git_worktree_remove_override: Mutex::new(Some(Arc::new(|_proj: &Path, _wt: &Path| {
                Err("'git worktree remove --force' failed: simulated locked worktree".to_string())
            }))),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &repo_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err(), "Must return error on Git worktree remove failure even if review was approved");
        let err_msg = res.unwrap_err();
        assert!(
            err_msg.contains("simulated locked worktree"),
            "Error must contain git worktree removal failure message: {}",
            err_msg
        );
        assert_eq!(
            scripted_executor.calls.lock().unwrap().len(),
            1,
            "Reviewer was executed before cleanup failure"
        );

        let post_fp = capture_repo_fingerprint(&repo_path).await.unwrap();
        assert_eq!(pre_fp, post_fp, "Primary repository must remain untouched");
    }

    #[tokio::test]
    async fn test_submodule_cleanup_fs_remove_failure_returns_error() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        create_test_git_repo(&repo_path);
        fs::write(repo_path.join("root.txt"), "root\n").unwrap();
        run_git_test_cmd(&repo_path, &["add", "root.txt"]);
        run_git_test_cmd(&repo_path, &["commit", "-m", "init"]);

        let pre_fp = capture_repo_fingerprint(&repo_path).await.unwrap();

        // Inject filesystem removal failure
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"clean code","findings":[]}"#),
            )])),
            cleanup_fs_remove_override: Mutex::new(Some(Arc::new(|wt: &Path| {
                Err(format!("Failed to remove disposable directory '{}': simulated permission denied", wt.display()))
            }))),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &repo_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err(), "Must return error on filesystem remove failure even if review was approved");
        let err_msg = res.unwrap_err();
        assert!(
            err_msg.contains("simulated permission denied"),
            "Error must contain filesystem removal failure message: {}",
            err_msg
        );
        assert_eq!(
            scripted_executor.calls.lock().unwrap().len(),
            1,
            "Reviewer was executed before cleanup failure"
        );

        let post_fp = capture_repo_fingerprint(&repo_path).await.unwrap();
        assert_eq!(pre_fp, post_fp, "Primary repository must remain untouched");
    }

    #[tokio::test]
    async fn test_submodule_cleanup_review_error_and_cleanup_failure_preserves_both() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        create_test_git_repo(&repo_path);
        fs::write(repo_path.join("root.txt"), "root\n").unwrap();
        run_git_test_cmd(&repo_path, &["add", "root.txt"]);
        run_git_test_cmd(&repo_path, &["commit", "-m", "init"]);

        let pre_fp = capture_repo_fingerprint(&repo_path).await.unwrap();

        // Inject reviewer writing to worktree (causing review security violation) AND git worktree remove failure
        let scripted_executor = Arc::new(ScriptedAdapterExecutor {
            outputs: Mutex::new(VecDeque::from(vec![(
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"looks good","findings":[]}"#),
            )])),
            on_reviewer_execute: Mutex::new(Some(Arc::new(|wt: &Path| {
                let _ = fs::write(wt.join("unauthorized_mutation.txt"), "bad reviewer write\n");
            }))),
            cleanup_git_worktree_remove_override: Mutex::new(Some(Arc::new(|_proj: &Path, _wt: &Path| {
                Err("'git worktree remove --force' failed: simulated locked worktree".to_string())
            }))),
            ..Default::default()
        });

        let mut engine = OrchestratorEngine::new();
        engine.scripted_adapter_executor = Some(scripted_executor.clone());
        let profile = readonly_reviewer_profile();
        let cancel = CancellationToken::new();

        let res = execute_disposable_worktree_review(
            &engine,
            &profile,
            &repo_path,
            "Review task",
            "Approved plan",
            &cancel,
        )
        .await;

        assert!(res.is_err(), "Must return error when review fails and cleanup fails");
        let err_msg = res.unwrap_err();
        assert!(
            err_msg.contains("Security violation: Reviewer process modified files in the review worktree"),
            "Primary review error must be preserved: {}",
            err_msg
        );
        assert!(
            err_msg.contains("also encountered cleanup failure: Disposable worktree cleanup failure: 'git worktree remove --force' failed: simulated locked worktree"),
            "Cleanup failure must also be reported: {}",
            err_msg
        );

        let post_fp = capture_repo_fingerprint(&repo_path).await.unwrap();
        assert_eq!(pre_fp, post_fp, "Primary repository must remain untouched");
    }

    #[tokio::test]
    async fn test_submodule_before_after_fingerprints_match() {
        let temp = tempfile::tempdir().unwrap();
        let sub_path = temp.path().join("sub_src");
        let parent_path = temp.path().join("parent");

        create_test_git_repo(&sub_path);
        fs::write(sub_path.join("sub.txt"), "sub v1\n").unwrap();
        run_git_test_cmd(&sub_path, &["add", "sub.txt"]);
        run_git_test_cmd(&sub_path, &["commit", "-m", "sub v1"]);

        create_test_git_repo(&parent_path);
        run_git_test_cmd(
            &parent_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path.to_string_lossy(),
                "submod",
            ],
        );
        run_git_test_cmd(&parent_path, &["commit", "-m", "parent v1"]);

        let fp1 = capture_repo_fingerprint(&parent_path).await.unwrap();
        let fp2 = capture_repo_fingerprint(&parent_path).await.unwrap();
        assert_eq!(fp1, fp2);

        // Modifying a file causes fingerprint difference
        fs::write(parent_path.join("root_modified.txt"), "mutation\n").unwrap();
        let fp3 = capture_repo_fingerprint(&parent_path).await.unwrap();
        assert_ne!(fp1, fp3);
    }

    #[tokio::test]
    async fn test_production_workflow_run_entry_captures_frozen_snapshot_and_delivers_identical_body_and_digest() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(
            root.join("package.json"),
            r#"{"name":"test-proj","version":"0.24.0"}"#,
        )
        .unwrap();

        let plan_dir = root.join(".plan");
        fs::create_dir_all(&plan_dir).unwrap();
        let primary_content = "# Primary Plan Rev 1\nPrimary details.\n";
        let supp_content = "# Supplemental Plan Rev 1a\nEXACT_FROZEN_LEAF_BODY\n";
        fs::write(plan_dir.join("V0.24.0-r1.md"), primary_content).unwrap();
        fs::write(plan_dir.join("V0.24.0-r1a.md"), supp_content).unwrap();

        let mut snapshot = workflow_snapshot(root.to_string_lossy().to_string());
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        let captured_envelope: Arc<Mutex<Option<OrchestratorTaskEnvelope>>> = Arc::new(Mutex::new(None));
        let env_capture = Arc::clone(&captured_envelope);

        let scripted_adapters = vec![
            (
                AgentRole::Planner,
                scripted_output("# Generated Initial Plan Draft\n"),
            ),
            (
                AgentRole::PlanReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"plan ok","findings":[]}"#),
            ),
            (
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"diff ok","findings":[]}"#),
            ),
        ];

        let mut engine = OrchestratorEngine::with_scripted_adapters(
            scripted_adapters,
            vec![ValidationRunSummary {
                passed: true,
                total_gates_run: 1,
                failed_gate_names: vec![],
                results: vec![],
                formatted_diagnostics: String::new(),
            }],
        );
        let scripted_executor = engine.scripted_adapter_executor.clone().unwrap();

        engine = engine.with_task_submission_hook(Arc::new(move |envelope| {
            *env_capture.lock().unwrap() = Some(envelope.clone());
            Ok(SubmitTaskRequest {
                run_id: envelope.run_id,
                task_id: envelope.task_id,
                epoch: envelope.epoch,
                idempotency_key: None,
                status: "success".to_string(),
                summary: "Task finished".to_string(),
                modified_files: vec![],
            })
        }));

        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        // Auto-approve human gate in background
        let events_clone = Arc::clone(&events);
        tokio::spawn(async move {
            for _ in 0..100 {
                tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;
                let is_human_gate = events_clone
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|e| e.step == WorkflowState::HumanGate);
                if is_human_gate {
                    let _ = human_gate_tx.send(HumanGateDecision::Approve).await;
                    break;
                }
            }
        });

        // Run human_gated_loop production workflow
        let state = engine
            .run_workflow(
                "run-prod-entry-test".into(),
                snapshot,
                "Implement the plan".into(),
                "human_gated_loop".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                None,
            )
            .await
            .unwrap();

        assert_eq!(state, WorkflowState::Complete);

        // 1. Verify provider prompt received exact frozen plan content and digest
        let prompts = scripted_executor.captured_prompts.lock().unwrap().clone();
        let planner_prompt = prompts
            .iter()
            .find(|(role, _)| role == &AgentRole::Planner)
            .map(|(_, p)| p.clone())
            .expect("Planner prompt was captured");
        assert!(planner_prompt.contains("EXACT_FROZEN_LEAF_BODY"));
        assert!(planner_prompt.contains("Current Leaf Plan**: V0.24.0-r1a"));

        // 2. Verify worker task envelope received identical frozen PlanContext and digest, plus distinct frozen_plan payload
        let envelope = captured_envelope.lock().unwrap().clone().expect("Worker envelope was captured");
        let plan_ctx = envelope.plan_context.expect("plan_context in envelope");
        assert_eq!(plan_ctx.current_leaf_plan_id.as_deref(), Some("V0.24.0-r1a"));
        assert_eq!(plan_ctx.current_primary_revision, Some(1));
        assert_eq!(plan_ctx.next_primary_revision, Some(2));
        assert_eq!(plan_ctx.active_supplemental_plans.len(), 1);
        let eff_digest = plan_ctx.effective_plan_digest.expect("effective digest");
        assert!(planner_prompt.contains(&eff_digest));

        let frozen = envelope.frozen_plan.expect("frozen_plan in envelope");
        assert!(frozen.effective_plan_content.contains("EXACT_FROZEN_LEAF_BODY"));
        assert!(frozen.effective_plan_content.contains("Primary details"));
        assert_eq!(frozen.ordered_sources.len(), 2);
        assert_eq!(frozen.ordered_sources[0].content, primary_content);
        assert_eq!(frozen.ordered_sources[1].content, supp_content);
        assert_eq!(frozen.effective_plan_digest, eff_digest);
        assert_eq!(frozen.primary_plan_identity_and_digest.as_ref().map(|p| p.id.as_str()), Some("V0.24.0-r1"));
        assert_eq!(frozen.ordered_supplemental_plan_identities_and_digests.len(), 1);
        assert_eq!(frozen.ordered_supplemental_plan_identities_and_digests[0].id, "V0.24.0-r1a");
        assert_eq!(envelope.task_prompt.as_deref(), Some("Implement the plan"));
    }

    #[tokio::test]
    async fn test_production_human_gated_workflow_active_run_immutability_and_worker_reclaim_consistency() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(
            root.join("package.json"),
            r#"{"name":"test-proj","version":"0.24.0"}"#,
        )
        .unwrap();

        let plan_dir = root.join(".plan");
        fs::create_dir_all(&plan_dir).unwrap();
        let primary_content = "# Primary Plan Rev 1\nFROZEN_PRIMARY_ORIGINAL\n";
        let supp_content = "# Supplemental Plan Rev 1a\nFROZEN_LEAF_ORIGINAL_CONTENT\n";
        fs::write(plan_dir.join("V0.24.0-r1.md"), primary_content).unwrap();
        fs::write(plan_dir.join("V0.24.0-r1a.md"), supp_content).unwrap();

        let mut snapshot = workflow_snapshot(root.to_string_lossy().to_string());
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir_for_mutation = plan_dir.clone();
        let envelopes_seen: Arc<Mutex<Vec<OrchestratorTaskEnvelope>>> = Arc::new(Mutex::new(Vec::new()));
        let env_sink = Arc::clone(&envelopes_seen);

        let scripted_adapters = vec![
            (
                AgentRole::Planner,
                scripted_output("# Generated Plan\n"),
            ),
            (
                AgentRole::PlanReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"plan ok","findings":[]}"#),
            ),
            (
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"diff ok","findings":[]}"#),
            ),
        ];

        let mut engine = OrchestratorEngine::with_scripted_adapters(
            scripted_adapters,
            vec![ValidationRunSummary {
                passed: true,
                total_gates_run: 1,
                failed_gate_names: vec![],
                results: vec![],
                formatted_diagnostics: String::new(),
            }],
        );
        let scripted_executor = engine.scripted_adapter_executor.clone().unwrap();

        engine = engine.with_task_submission_hook(Arc::new(move |envelope| {
            env_sink.lock().unwrap().push(envelope.clone());

            // During the active run, mutate plan files on disk
            fs::write(plan_dir_for_mutation.join("V0.24.0-r1.md"), "# Corrupted Primary Content\n").unwrap();
            fs::write(plan_dir_for_mutation.join("V0.24.0-r1a.md"), "# Corrupted Leaf Content\n").unwrap();
            fs::write(plan_dir_for_mutation.join("V0.24.0-r2.md"), "# Injected Rev 2\n").unwrap();

            Ok(SubmitTaskRequest {
                run_id: envelope.run_id,
                task_id: envelope.task_id,
                epoch: envelope.epoch,
                idempotency_key: None,
                status: "success".to_string(),
                summary: "Task finished".to_string(),
                modified_files: vec![],
            })
        }));

        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        // Auto-approve human gate in background
        let events_clone = Arc::clone(&events);
        tokio::spawn(async move {
            for _ in 0..100 {
                tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;
                let is_human_gate = events_clone
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|e| e.step == WorkflowState::HumanGate);
                if is_human_gate {
                    let _ = human_gate_tx.send(HumanGateDecision::Approve).await;
                    break;
                }
            }
        });

        // Run human_gated_loop production workflow
        let state = engine
            .run_workflow(
                "run-hg-immutability-test".into(),
                snapshot,
                "Human gated test".into(),
                "human_gated_loop".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                None,
            )
            .await
            .unwrap();

        assert_eq!(state, WorkflowState::Complete);

        // 1. Verify PlanReviewer provider prompt received original frozen plan and digest
        let prompts = scripted_executor.captured_prompts.lock().unwrap().clone();
        let reviewer_prompt = prompts
            .iter()
            .find(|(role, _)| role == &AgentRole::PlanReviewer)
            .map(|(_, p)| p.clone())
            .expect("PlanReviewer prompt captured");
        assert!(reviewer_prompt.contains("FROZEN_LEAF_ORIGINAL_CONTENT"));
        assert!(!reviewer_prompt.contains("Corrupted"));

        // 2. Verify all Worker envelopes in this run carry the exact original frozen PlanContext, digest, and FrozenPlanPayload
        let envelopes = envelopes_seen.lock().unwrap().clone();
        assert!(!envelopes.is_empty());
        for env in &envelopes {
            let ctx = env.plan_context.as_ref().expect("plan_context present in envelope");
            assert_eq!(ctx.current_leaf_plan_id.as_deref(), Some("V0.24.0-r1a"));
            assert_eq!(ctx.current_primary_revision, Some(1));
            assert_eq!(ctx.next_primary_revision, Some(2));
            assert!(reviewer_prompt.contains(ctx.effective_plan_digest.as_ref().unwrap()));

            let frozen = env.frozen_plan.as_ref().expect("frozen_plan present in envelope");
            assert!(frozen.effective_plan_content.contains("FROZEN_LEAF_ORIGINAL_CONTENT"));
            assert!(frozen.effective_plan_content.contains("FROZEN_PRIMARY_ORIGINAL"));
            assert_eq!(frozen.ordered_sources.len(), 2);
            assert_eq!(frozen.ordered_sources[0].content, primary_content);
            assert_eq!(frozen.ordered_sources[1].content, supp_content);
            assert_eq!(frozen.effective_plan_digest, *ctx.effective_plan_digest.as_ref().unwrap());
            assert_eq!(frozen.primary_plan_identity_and_digest.as_ref().map(|p| p.id.as_str()), Some("V0.24.0-r1"));
            assert_eq!(frozen.ordered_supplemental_plan_identities_and_digests.len(), 1);
            assert_eq!(frozen.ordered_supplemental_plan_identities_and_digests[0].id, "V0.24.0-r1a");
        }
    }

    #[tokio::test]
    async fn test_production_workflow_run_entry_fails_closed_on_invalid_utf8_plan_file_before_side_effects() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(
            root.join("package.json"),
            r#"{"name":"test-proj","version":"0.24.0"}"#,
        )
        .unwrap();

        let plan_dir = root.join(".plan");
        fs::create_dir_all(&plan_dir).unwrap();
        // Write invalid UTF-8 bytes to plan file
        fs::write(plan_dir.join("V0.24.0-r1.md"), vec![0xFF, 0xFE, 0xFD]).unwrap();

        let mut snapshot = workflow_snapshot(root.to_string_lossy().to_string());
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        let engine = OrchestratorEngine::with_scripted_adapters(vec![], vec![]);
        let scripted_executor = engine.scripted_adapter_executor.clone().unwrap();

        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        let err = engine
            .run_workflow(
                "run-fail-utf8".into(),
                snapshot,
                "Implement".into(),
                "full_loop".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                None,
            )
            .await
            .unwrap_err();

        // 1. Assert typed error
        assert!(err.contains("plan_file_not_utf8"));

        // 2. Assert no provider/model call occurred
        assert!(scripted_executor.calls.lock().unwrap().is_empty());

        // 3. Assert no stage event emitted
        assert!(events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_production_workflow_run_entry_fails_closed_on_persistent_digest_drift_seam_before_side_effects() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(
            root.join("package.json"),
            r#"{"name":"test-proj","version":"0.24.0"}"#,
        )
        .unwrap();

        let plan_dir = root.join(".plan");
        fs::create_dir_all(&plan_dir).unwrap();
        let plan_file = plan_dir.join("V0.24.0-r1.md");
        fs::write(&plan_file, "# Initial Plan\n").unwrap();

        let mut snapshot = workflow_snapshot(root.to_string_lossy().to_string());
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        let plan_file_clone = plan_file.clone();
        super::super::plan_workspace::set_capture_drift_seam(Some(Arc::new(move |_, attempt| {
            fs::write(&plan_file_clone, format!("# Mutated on attempt {attempt}\n")).unwrap();
        })));

        let engine = OrchestratorEngine::with_scripted_adapters(vec![], vec![]);
        let scripted_executor = engine.scripted_adapter_executor.clone().unwrap();

        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        let err = engine
            .run_workflow(
                "run-fail-drift".into(),
                snapshot,
                "Implement".into(),
                "full_loop".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                None,
            )
            .await
            .unwrap_err();

        // Clear seam
        super::super::plan_workspace::set_capture_drift_seam(None);

        // 1. Assert typed error
        assert!(err.contains("stale_plan_file_digest"));

        // 2. Assert no provider/model call occurred
        assert!(scripted_executor.calls.lock().unwrap().is_empty());

        // 3. Assert no stage event emitted
        assert!(events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_production_workflow_run_entry_recovers_on_transient_drift_seam() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(
            root.join("package.json"),
            r#"{"name":"test-proj","version":"0.24.0"}"#,
        )
        .unwrap();

        let plan_dir = root.join(".plan");
        fs::create_dir_all(&plan_dir).unwrap();
        let plan_file = plan_dir.join("V0.24.0-r1.md");
        fs::write(&plan_file, "# Initial Plan\n").unwrap();

        let mut snapshot = workflow_snapshot(root.to_string_lossy().to_string());
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        let plan_file_clone = plan_file.clone();
        super::super::plan_workspace::set_capture_drift_seam(Some(Arc::new(move |_, attempt| {
            // Mutate ONLY on attempt 0 (transient drift)
            if attempt == 0 {
                fs::write(&plan_file_clone, "# Transient Mutated Plan Content\n").unwrap();
            }
        })));

        let scripted_adapters = vec![
            (
                AgentRole::Planner,
                scripted_output("# Generated Plan\n"),
            ),
            (
                AgentRole::PlanReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"plan ok","findings":[]}"#),
            ),
            (
                AgentRole::Implementer,
                scripted_output("Implementation done"),
            ),
            (
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"diff ok","findings":[]}"#),
            ),
        ];

        let engine = OrchestratorEngine::with_scripted_adapters(
            scripted_adapters,
            vec![ValidationRunSummary {
                passed: true,
                total_gates_run: 1,
                failed_gate_names: vec![],
                results: vec![],
                formatted_diagnostics: String::new(),
            }],
        );
        let scripted_executor = engine.scripted_adapter_executor.clone().unwrap();

        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        let state = engine
            .run_workflow(
                "run-transient-drift".into(),
                snapshot,
                "Implement".into(),
                "full_loop".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                None,
            )
            .await
            .unwrap();

        // Clear seam
        super::super::plan_workspace::set_capture_drift_seam(None);

        assert_eq!(state, WorkflowState::Complete);

        // Verify prompt captured the recovered plan content from retry attempt 1
        let prompts = scripted_executor.captured_prompts.lock().unwrap().clone();
        let planner_prompt = prompts
            .iter()
            .find(|(role, _)| role == &AgentRole::Planner)
            .map(|(_, p)| p.clone())
            .expect("Planner prompt captured");
        assert!(planner_prompt.contains("Transient Mutated Plan Content"));
    }

    #[tokio::test]
    async fn test_wait_for_antigravity_submission_fails_closed_when_frozen_payload_missing_or_mismatched() {
        let snapshot = workflow_snapshot("test-proj".into());
        let mailbox = super::super::mailbox::MailboxState::new_mock(0);
        let (_submit_tx, mut submit_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, mut worker_reclaim_rx) = mpsc::channel(1);
        let cancel_token = CancellationToken::new();
        let on_event: EventCallback = Arc::new(|_| Ok(()));
        let log = |_: String| {};

        let source_content = "content".to_string();
        let source_digest = super::super::plan_workspace::sha256_bytes(source_content.as_bytes());
        let primary_source = super::super::plan_workspace::FrozenPlanSource {
            kind: super::super::plan_workspace::FrozenPlanSourceKind::Primary,
            id: "V0.24.0-r1".into(),
            suffix: None,
            path: ".plan/V0.24.0-r1.md".into(),
            source_digest: source_digest.clone(),
            content: source_content.clone(),
        };
        let assembled = super::super::plan_workspace::canonical_assemble_sources(&[primary_source.clone()]).unwrap();
        let content_digest = super::super::plan_workspace::sha256_bytes(assembled.as_bytes());

        let plan_context = super::super::plan_workspace::PlanContext {
            project_root_identity: "canon".into(),
            plan_directory: ".plan".into(),
            application_version: "0.24.0".into(),
            plan_series_version: "0.24.0".into(),
            current_primary_plan: Some(super::super::plan_workspace::PlanFileRecord {
                id: "V0.24.0-r1".into(),
                path: ".plan/V0.24.0-r1.md".into(),
                digest: source_digest.clone(),
                revision: 1,
            }),
            active_supplemental_plans: vec![],
            current_leaf_plan_id: Some("V0.24.0-r1".into()),
            effective_plan_digest: Some("eff-digest-1".into()),
            current_primary_revision: Some(1),
            next_primary_revision: Some(2),
            resolver_status: super::super::plan_workspace::PlanResolverStatus::Resolved,
            unresolved_reason_code: None,
        };

        // 1. Missing frozen_plan_payload when PlanContext is Resolved fails closed
        let err1 = wait_for_antigravity_submission(
            WorkflowState::Implementation,
            AgentRole::Implementer,
            "worker",
            "run-1",
            &snapshot,
            None,
            Some("Task prompt".into()),
            None,
            None,
            Some(&plan_context),
            None,
            &mailbox,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert!(err1.contains("Frozen plan payload is required"));

        // 2. Mismatched digest in frozen_plan_payload fails closed
        let bad_payload = super::super::plan_workspace::FrozenPlanPayload {
            schema_version: 1,
            effective_plan_content: assembled,
            content_digest,
            effective_plan_digest: "corrupted-digest".into(),
            primary_plan_identity_and_digest: Some(super::super::plan_workspace::PlanFileIdentityAndDigest {
                id: "V0.24.0-r1".into(),
                path: ".plan/V0.24.0-r1.md".into(),
                digest: source_digest,
                revision: 1,
            }),
            ordered_supplemental_plan_identities_and_digests: vec![],
            ordered_sources: vec![primary_source],
        };

        let err2 = wait_for_antigravity_submission(
            WorkflowState::Implementation,
            AgentRole::Implementer,
            "worker",
            "run-1",
            &snapshot,
            None,
            Some("Task prompt".into()),
            None,
            None,
            Some(&plan_context),
            Some(&bad_payload),
            &mailbox,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert!(err2.contains("effective digest mismatch"));
    }

    #[tokio::test]
    async fn test_unconfigured_plan_workspace_resolves_cleanly_and_allows_run_entry() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(
            root.join("package.json"),
            r#"{"name":"test-proj","version":"0.24.0"}"#,
        )
        .unwrap();

        // Project root has no .plan directory and unconfigured plan path
        let mut snapshot = workflow_snapshot(root.to_string_lossy().to_string());
        snapshot.plan_workspace.plan_dir = ".nonexistent_plan".to_string();
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        // Verify direct resolution is Resolved with no plan
        let ctx = super::super::plan_workspace::resolve_plan_context(&root, &snapshot.plan_workspace).unwrap();
        assert_eq!(ctx.resolver_status, super::super::plan_workspace::PlanResolverStatus::Resolved);
        assert!(ctx.current_primary_plan.is_none());
        assert!(ctx.active_supplemental_plans.is_empty());
        assert!(!ctx.is_plan_bound());

        let frozen = super::super::plan_workspace::capture_frozen_plan_snapshot(&root, &snapshot.plan_workspace).unwrap();
        assert_eq!(frozen.plan_context.resolver_status, super::super::plan_workspace::PlanResolverStatus::Resolved);
        assert!(frozen.ensure_run_entry_allowed().is_ok());

        let scripted_adapters = vec![
            (
                AgentRole::Planner,
                scripted_output("# Initial Draft Plan\n"),
            ),
            (
                AgentRole::PlanReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"plan ok","findings":[]}"#),
            ),
            (
                AgentRole::Implementer,
                scripted_output("Implementation done"),
            ),
            (
                AgentRole::CodeReviewer,
                scripted_output(r#"{"verdict":"approved","summary":"diff ok","findings":[]}"#),
            ),
        ];

        let engine = OrchestratorEngine::with_scripted_adapters(
            scripted_adapters,
            vec![ValidationRunSummary {
                passed: true,
                total_gates_run: 1,
                failed_gate_names: vec![],
                results: vec![],
                formatted_diagnostics: String::new(),
            }],
        );

        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        let state = engine
            .run_workflow(
                "run-unconfigured-workspace".into(),
                snapshot,
                "Draft Initial Plan".into(),
                "full_loop".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                None,
            )
            .await
            .unwrap();

        assert_eq!(state, WorkflowState::Complete);
    }

    #[tokio::test]
    async fn test_production_full_loop_fails_closed_on_unresolved_plan_workspace_with_zero_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(
            root.join("package.json"),
            r#"{"name":"test-proj","version":"0.24.0"}"#,
        )
        .unwrap();

        let plan_dir = root.join(".plan");
        fs::create_dir_all(&plan_dir).unwrap();
        // Create orphaned supplemental plan to force Unresolved status
        fs::write(plan_dir.join("V0.24.0-r2a.md"), "# Orphaned Supplemental\n").unwrap();

        let mut snapshot = workflow_snapshot(root.to_string_lossy().to_string());
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        let engine = OrchestratorEngine::with_scripted_adapters(vec![], vec![]);
        let scripted_executor = engine.scripted_adapter_executor.clone().unwrap();

        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        let persistence_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let p_calls_clone = Arc::clone(&persistence_calls);
        let persistence_cb: super::super::mailbox::DispatchPersistenceCallback = Arc::new(move |_| {
            p_calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });

        let err = engine
            .run_workflow(
                "run-fail-full-loop-unresolved".into(),
                snapshot,
                "Implement".into(),
                "full_loop".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                Some(persistence_cb),
            )
            .await
            .unwrap_err();

        // 1. Assert typed error contains reason code
        assert!(err.contains("orphaned_supplemental_plan"));

        // 2. Assert exhaustive 6-counter zero-dispatch:
        // 1) provider/model calls == 0
        assert_eq!(scripted_executor.calls.lock().unwrap().len(), 0);
        // 2) Worker envelopes == 0
        assert_eq!(scripted_executor.captured_prompts.lock().unwrap().len(), 0);
        // 3) MCP/Antigravity dispatch == 0
        // 4) Mailbox claim/reclaim == 0
        // 5) successful stage events == 0
        assert_eq!(events.lock().unwrap().len(), 0);
        // 6) successful checkpoint/stage persistence == 0
        assert_eq!(persistence_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_production_human_gated_loop_fails_closed_on_unresolved_plan_workspace_with_zero_dispatch() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::write(
            root.join("package.json"),
            r#"{"name":"test-proj","version":"0.24.0"}"#,
        )
        .unwrap();

        let plan_dir = root.join(".plan");
        fs::create_dir_all(&plan_dir).unwrap();
        // Create orphaned supplemental plan to force Unresolved status
        fs::write(plan_dir.join("V0.24.0-r2a.md"), "# Orphaned Supplemental\n").unwrap();

        let mut snapshot = workflow_snapshot(root.to_string_lossy().to_string());
        snapshot.plan_workspace.version_sources = vec!["package.json".to_string()];
        snapshot.plan_workspace.plan_series_version_override = Some("0.24.0".to_string());

        let mut engine = OrchestratorEngine::with_scripted_adapters(vec![], vec![]);
        let scripted_executor = engine.scripted_adapter_executor.clone().unwrap();

        let envelopes_seen = Arc::new(Mutex::new(Vec::new()));
        let env_sink = Arc::clone(&envelopes_seen);
        engine.task_submission_hook = Some(TaskSubmissionHook(Arc::new(move |envelope| {
            env_sink.lock().unwrap().push(envelope.clone());
            Ok(SubmitTaskRequest {
                run_id: envelope.run_id,
                task_id: envelope.task_id,
                epoch: envelope.epoch,
                idempotency_key: None,
                status: "success".to_string(),
                summary: "Task finished".to_string(),
                modified_files: vec![],
            })
        })));

        let events: Arc<Mutex<Vec<StepProgressEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        let persistence_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let p_calls_clone = Arc::clone(&persistence_calls);
        let persistence_cb: super::super::mailbox::DispatchPersistenceCallback = Arc::new(move |_| {
            p_calls_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });

        let err = engine
            .run_workflow(
                "run-fail-hg-loop-unresolved".into(),
                snapshot,
                "Implement".into(),
                "human_gated_loop".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(move |event| {
                    event_sink.lock().unwrap().push(event);
                    Ok(())
                }),
                Arc::new(|_| {}),
                Some(persistence_cb),
            )
            .await
            .unwrap_err();

        // 1. Assert typed error contains reason code
        assert!(err.contains("orphaned_supplemental_plan"));

        // 2. Assert exhaustive 6-counter zero-dispatch:
        // 1) provider/model calls == 0
        assert_eq!(scripted_executor.calls.lock().unwrap().len(), 0);
        // 2) Worker envelopes == 0
        assert_eq!(envelopes_seen.lock().unwrap().len(), 0);
        // 3) MCP/Antigravity dispatch == 0
        // 4) Mailbox claim/reclaim == 0
        // 5) successful stage events == 0
        assert_eq!(events.lock().unwrap().len(), 0);
        // 6) successful checkpoint/stage persistence == 0
        assert_eq!(persistence_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_mcp_preflight_rejects_unauthorized_roles() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().to_str().unwrap().to_string();
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        let mut snapshot = workflow_snapshot(project_path);
        let mut mcp_profile = readonly_reviewer_profile();
        mcp_profile.adapter = ExecutionAdapterType::Mcp;
        mcp_profile.external_mcp_server = Some("srv".to_string());
        mcp_profile.mcp_tool = Some("tool".to_string());

        // Assign CodeReviewer to MCP
        snapshot.assignments.insert(AgentRole::CodeReviewer, mcp_profile);

        let engine = OrchestratorEngine::new();
        let err = engine
            .run_workflow(
                "run-mcp-unauth".into(),
                snapshot,
                "Review task".into(),
                "review_only".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(|_| Ok(())),
                Arc::new(|_| {}),
                None,
            )
            .await
            .unwrap_err();

        assert!(err.contains("MCP adapter is currently unavailable for role 'CodeReviewer'") || err.contains("MCP role authorization error"));
    }

    #[tokio::test]
    async fn test_mcp_preflight_rejects_missing_server_in_registry() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().to_str().unwrap().to_string();
        let (_pause_tx, pause_rx) = watch::channel(RunControlState::Running);
        let (_clarification_tx, clarification_rx) = mpsc::channel(1);
        let (_blocking_tx, blocking_rx) = mpsc::channel(1);
        let (_human_gate_tx, human_gate_rx) = mpsc::channel(1);
        let (_worker_reclaim_tx, worker_reclaim_rx) = mpsc::channel(1);

        let mut snapshot = workflow_snapshot(project_path);
        let mut mcp_planner = mutating_cli_profile();
        mcp_planner.adapter = ExecutionAdapterType::Mcp;
        mcp_planner.external_mcp_server = Some("nonexistent-server".to_string());
        mcp_planner.mcp_tool = Some("plan".to_string());

        snapshot.assignments.insert(AgentRole::Planner, mcp_planner);

        let engine = OrchestratorEngine::new();
        let err = engine
            .run_workflow(
                "run-mcp-missing-srv".into(),
                snapshot,
                "Plan task".into(),
                "plan_only".into(),
                None,
                vec![],
                pause_rx,
                CancellationToken::new(),
                clarification_rx,
                blocking_rx,
                human_gate_rx,
                worker_reclaim_rx,
                Arc::new(|_| Ok(())),
                Arc::new(|_| {}),
                None,
            )
            .await
            .unwrap_err();

        assert!(err.contains("unknown_server") || err.contains("not found in registered mcp_servers"));
    }

    #[tokio::test]
    async fn test_engine_mcp_dispatch_zero_mailbox_activity() {
        use crate::orchestrator::adapters::direct_mcp::tests::build_stdio_peer;
        use crate::orchestrator::mailbox::{get_session_descriptor_path, MailboxState};
        use crate::orchestrator::types::{McpServerTransport, McpToolContract};

        let temp = tempfile::tempdir().unwrap();
        let peer = build_stdio_peer(&temp);
        let project_dir = tempfile::tempdir().unwrap();
        let project_path = project_dir.path();

        let mut mcp_servers = HashMap::new();
        mcp_servers.insert(
            "fake-mcp".to_string(),
            McpServerConfig {
                transport: McpServerTransport::Stdio,
                executable: peer.to_string_lossy().to_string(),
                args: vec!["paged".to_string()],
                working_directory: Some(project_path.to_string_lossy().to_string()),
                allowed_environment: vec![],
                tool_contract: McpToolContract::PromptEnvelopeV1,
            },
        );

        let profile = OrchestratorProfile {
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
            external_mcp_server: Some("fake-mcp".to_string()),
            mcp_tool: Some("plan".to_string()),
            context_window_tokens: Some(128_000),
        };

        // Mailbox baseline check
        let session_descriptor = get_session_descriptor_path();
        let descriptor_existed_before = session_descriptor.exists();
        let mailbox_state = MailboxState::new_mock(100);
        let initial_epoch = mailbox_state.inner.lock().await.epoch;
        let initial_dispatches = mailbox_state.inner.lock().await.total_dispatches;
        let initial_claimed = mailbox_state.inner.lock().await.is_claimed;
        let initial_active_task = mailbox_state.inner.lock().await.active_task.clone();

        let engine = OrchestratorEngine::new()
            .with_direct_mcp_audit_callback(Arc::new(|_record| Ok(())));
        let on_event: EventCallback = Arc::new(|_| Ok(()));

        let output = engine
            .execute_adapter(
                AgentRole::Planner,
                &profile,
                "system prompt",
                "user prompt",
                project_path,
                Some(&mcp_servers),
                None,
                None,
                None,
                &on_event,
            )
            .await
            .expect("engine MCP dispatch must succeed");

        assert_eq!(output.content, "fake peer success");

        // Verify zero Mailbox activity at engine boundary
        assert_eq!(
            session_descriptor.exists(),
            descriptor_existed_before,
            "Engine MCP dispatch must not create or alter Mailbox session descriptor"
        );
        let guard = mailbox_state.inner.lock().await;
        assert_eq!(guard.epoch, initial_epoch, "Engine MCP dispatch must not change mailbox epoch");
        assert_eq!(
            guard.total_dispatches, initial_dispatches,
            "Engine MCP dispatch must produce zero mailbox dispatches"
        );
        assert_eq!(
            guard.is_claimed, initial_claimed,
            "Engine MCP dispatch must not claim mailbox tasks"
        );
        assert_eq!(
            guard.active_task, initial_active_task,
            "Engine MCP dispatch must not set active mailbox tasks"
        );
        assert!(guard.task_dispatches.is_empty(), "Engine MCP dispatch must not record task dispatches");
    }
}
