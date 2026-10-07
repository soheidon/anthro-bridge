use cap_std::ambient_authority;
use cap_std::fs::{Dir as CapabilityDir, OpenOptions as CapabilityOpenOptions};
use io_lifetimes::AsFilelike;
use serde::{Deserialize, Serialize};
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
use super::adapters::ollama::OllamaAdapter;
use super::adapters::provider::ProviderAdapter;
use super::adapters::{AdapterExecutionInput, AdapterExecutionOutput};
use super::context_builder::{BuiltContext, ContextBuilder};
use super::finding_aggregator::FindingAggregator;
use super::mailbox::MailboxServer;
use super::token_estimator::TokenCountQuality;
use super::types::{
    active_roles_for_workflow, validate_workflow_role_capabilities, AgentRole,
    AuthorizedCustomGate, ExecutionAdapterType, HumanGateDecision, LoopIterationLimits,
    OrchestratorProfile, OrchestratorTaskEnvelope, PlanArchiveOptions, PlanArchivePreview,
    ReviewFinding, ReviewResult, ReviewVerdict, RunConfigurationSnapshot, RunControlState,
    WorkflowState,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
                });

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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: Some("clarification_required".to_string()),
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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
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
                            antigravity_dispatches: None,
                            antigravity_dispatch_limit: None,
                                budget_scope: None,
                                waiting_reason: None,
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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
                    });
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
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
                    antigravity_dispatches: None,
                    antigravity_dispatch_limit: None,
                        budget_scope: None,
                        waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: Some("clarification_required".to_string()),
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
                        antigravity_dispatches: None,
                        antigravity_dispatch_limit: None,
                            budget_scope: None,
                            waiting_reason: None,
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
                antigravity_dispatches: None,
                antigravity_dispatch_limit: None,
                    budget_scope: None,
                    waiting_reason: None,
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
                budget_scope: None,
                waiting_reason: None,
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
            antigravity_dispatches: None,
            antigravity_dispatch_limit: None,
            budget_scope: None,
            waiting_reason: None,
        });

        Ok(WorkflowState::Complete)
    }

    async fn execute_adapter(
        &self,
        role: AgentRole,
        profile: &OrchestratorProfile,
        system_prompt: &str,
        user_prompt: &str,
        project_path: &Path,
        cancel_token: Option<&CancellationToken>,
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
            role,
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
                Err("MCP adapter is reserved and currently disabled.".to_string())
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
    mailbox_state: &super::mailbox::MailboxState,
    submit_rx: &mut mpsc::Receiver<super::types::SubmitTaskRequest>,
    worker_reclaim_rx: &mut mpsc::Receiver<()>,
    cancel_token: &CancellationToken,
    on_event: &EventCallback,
    log: &(impl Fn(String) + Send + Sync),
) -> Result<AntigravitySubmissionOutcome, String> {
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
        });
        guard.task_notify.notify_waiters();
    }

    let mut lease_check_interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
    lease_check_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
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
                    });
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
                    });
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
                    });
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
        mut pause_rx: watch::Receiver<RunControlState>,
        cancel_token: CancellationToken,
        _clarification_rx: mpsc::Receiver<String>,
        mut blocking_resolution_rx: mpsc::Receiver<BlockingResolution>,
        mut human_gate_rx: mpsc::Receiver<HumanGateDecision>,
        mut worker_reclaim_rx: mpsc::Receiver<()>,
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

        // Start Localhost HTTP Mailbox Server
        let (mailbox_server, mailbox_state, mut submit_rx, mut progress_rx) =
            MailboxServer::start(run_id.clone(), snapshot.project_path.clone()).await?;

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
            on_event_raw(ev);
        });

        // Spawn background progress event relay
        let event_run_id = run_id.clone();
        let on_event_clone = on_event.clone();
        let mailbox_state_for_progress = mailbox_state.clone();
        let progress_relay = tokio::spawn(async move {
            while let Some(prog) = progress_rx.recv().await {
                let (current_state, dispatches, limit) = {
                    let guard = mailbox_state_for_progress.inner.lock().await;
                    (
                        guard.current_state.clone(),
                        Some(guard.total_dispatches),
                        Some(guard.max_dispatches_per_run),
                    )
                };
                on_event_clone(StepProgressEvent {
                    run_id: event_run_id.clone(),
                    step: current_state,
                    iteration_info: prog.percent.map(|p| format!("{}%", p)),
                    message: prog.message,
                    review_result: None,
                    validation_summary: None,
                    plan_text: None,
                    antigravity_dispatches: dispatches,
                    antigravity_dispatch_limit: limit,
                    budget_scope: None,
                    waiting_reason: None,
                });
            }
        });

        // Independent counters
        let mut plan_review_count = 0;
        let mut code_review_count = 0;
        let mut fix_count = 0;
        let mut current_plan = String::new();
        let mut latest_val_summary: Option<ValidationRunSummary> = None;
        let mut latest_cr_result: Option<ReviewResult> = None;

        log(format!("[Engine] Starting Human-Gated Loop for run {}", run_id));

        // ==========================================
        // Stage 1: PlanDraft (Planner)
        // ==========================================
        self.check_run_control(&mut pause_rx, &cancel_token).await?;

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
            "[Engine] Initial plan draft generated ({} chars)",
            current_plan.len()
        ));

        // ==========================================
        // Stage 2: PlanIntegration (Antigravity Harness)
        // ==========================================
        self.check_run_control(&mut pause_rx, &cancel_token).await?;

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
        });

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
            &mailbox_state,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
        )
        .await? {
            AntigravitySubmissionOutcome::Submitted(sub) => sub,
            AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                progress_relay.abort();
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
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

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
                });

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
            });

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
                    Some(&cancel_token),
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
            });

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
                &mailbox_state,
                &mut submit_rx,
                &mut worker_reclaim_rx,
                &cancel_token,
                &on_event,
                &log,
            )
            .await? {
                AntigravitySubmissionOutcome::Submitted(sub) => sub,
                AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                    log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                    progress_relay.abort();
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
                });
                return Ok(WorkflowState::Failed);
            }
        }

        // ==========================================
        // Stage 4: Implementation (Antigravity Harness)
        // ==========================================
        self.check_run_control(&mut pause_rx, &cancel_token).await?;

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
        });

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
            &mailbox_state,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
        )
        .await? {
            AntigravitySubmissionOutcome::Submitted(sub) => sub,
            AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                progress_relay.abort();
                mailbox_server.stop().await;
                return Ok(WorkflowState::WaitingForUser);
            }
        };

        log(format!(
            "[Engine] Implementation submitted (status={})",
            impl_submission.status
        ));

        // ==========================================
        // Stage 5 & 6: Validation, Fix, and Code Review Loop
        // ==========================================
        loop {
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

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
            });

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
                    });

                    let res = tokio::select! {
                        _ = cancel_token.cancelled() => return Err("Execution cancelled by user.".to_string()),
                        msg = blocking_resolution_rx.recv() => msg.ok_or_else(|| "Blocking resolution channel closed.".to_string())?,
                    };
                    if res == BlockingResolution::Abort {
                        progress_relay.abort();
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
                });

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
                    &mailbox_state,
                    &mut submit_rx,
                    &mut worker_reclaim_rx,
                    &cancel_token,
                    &on_event,
                    &log,
                )
                .await? {
                    AntigravitySubmissionOutcome::Submitted(sub) => sub,
                    AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                        log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                        progress_relay.abort();
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
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

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
                });

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
            });

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
            });

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
                });

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
            });

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
                &mailbox_state,
                &mut submit_rx,
                &mut worker_reclaim_rx,
                &cancel_token,
                &on_event,
                &log,
            )
            .await? {
                AntigravitySubmissionOutcome::Submitted(sub) => sub,
                AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                    log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                    progress_relay.abort();
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
            self.check_run_control(&mut pause_rx, &cancel_token).await?;

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
            });

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
                    });
                    progress_relay.abort();
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
                    });
                    progress_relay.abort();
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
                    });

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
                        &mailbox_state,
                        &mut submit_rx,
                        &mut worker_reclaim_rx,
                        &cancel_token,
                        &on_event,
                        &log,
                    )
                    .await? {
                        AntigravitySubmissionOutcome::Submitted(sub) => sub,
                        AntigravitySubmissionOutcome::BudgetExhausted(_) => {
                            log("[Engine] Workflow paused at WaitingForUser due to budget exhaustion.".to_string());
                            progress_relay.abort();
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
        let _ = stdin.write_all(patch).await;
        let _ = stdin.shutdown().await;
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

pub async fn verify_submodules_for_review(project_path: &Path) -> Result<Vec<String>, String> {
    let status_output = match tokio::process::Command::new("git")
        .args(["submodule", "status", "--recursive"])
        .current_dir(project_path)
        .output()
        .await
    {
        Ok(out) => {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(format!(
                    "Failed to check git submodule status (exit status {}): {}",
                    out.status, stderr
                ));
            }
            String::from_utf8_lossy(&out.stdout).to_string()
        }
        Err(e) => return Err(format!("Failed to execute 'git submodule status': {}", e)),
    };

    let mut sub_paths = Vec::new();

    for line in status_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let sub_path = parts[1].to_string();
        sub_paths.push(sub_path.clone());

        let sub_path_norm = sub_path.replace('\\', "/");
        let (parent_dir, rel_sub_path) = if let Some(last_slash) = sub_path_norm.rfind('/') {
            let parent_rel = &sub_path_norm[..last_slash];
            let sub_name = &sub_path_norm[last_slash + 1..];
            (project_path.join(parent_rel), sub_name.to_string())
        } else {
            (project_path.to_path_buf(), sub_path_norm.clone())
        };

        let gitlink_out = tokio::process::Command::new("git")
            .args(["rev-parse", &format!("HEAD:{}", rel_sub_path)])
            .current_dir(&parent_dir)
            .output()
            .await
            .map_err(|e| format!("Failed to read gitlink for submodule '{}': {}", sub_path, e))?;

        if !gitlink_out.status.success() {
            let stderr = String::from_utf8_lossy(&gitlink_out.stderr);
            return Err(format!(
                "Failed to resolve parent gitlink for submodule '{}': {}",
                sub_path, stderr
            ));
        }
        let parent_gitlink = String::from_utf8_lossy(&gitlink_out.stdout).trim().to_string();

        let sub_head_out = tokio::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(project_path.join(&sub_path))
            .output()
            .await
            .map_err(|e| format!("Failed to read local HEAD for submodule '{}': {}", sub_path, e))?;

        if !sub_head_out.status.success() {
            let stderr = String::from_utf8_lossy(&sub_head_out.stderr);
            return Err(format!(
                "Failed to resolve local HEAD for submodule '{}': {}",
                sub_path, stderr
            ));
        }
        let sub_head = String::from_utf8_lossy(&sub_head_out.stdout).trim().to_string();

        if parent_gitlink != sub_head {
            return Err(format!(
                "UnsupportedState: Submodule at '{}' has local commits ahead of/divergent from parent gitlink (parent gitlink: {}, submodule HEAD: {}). Local submodule commit synchronization is deferred to Plan 39C.",
                sub_path, parent_gitlink, sub_head
            ));
        }
    }

    Ok(sub_paths)
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
    if !reviewer_profile.capabilities.contains(&super::types::ProfileCapability::Review) {
        return Err("Preflight validation failure: Code Reviewer role must have Review capability.".to_string());
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

    // 1. Verify submodules first (fail closed if divergence found)
    let submodules = verify_submodules_for_review(project_path).await?;

    // 2. Base commit
    let base_commit = run_git_cmd(project_path, &["rev-parse", "HEAD"])
        .await?
        .trim()
        .to_string();

    // 3. Create temp directory
    let temp_dir =
        std::env::temp_dir().join(format!("anthro-bridge-review-{}", uuid::Uuid::new_v4()));
    let temp_dir_str = temp_dir.to_string_lossy().to_string();

    // 4. Add worktree
    run_git_cmd(
        project_path,
        &["worktree", "add", "--detach", &temp_dir_str, "HEAD"],
    )
    .await?;

    struct WorktreeGuard<'a> {
        project_path: &'a Path,
        temp_dir: PathBuf,
    }
    impl<'a> Drop for WorktreeGuard<'a> {
        fn drop(&mut self) {
            let _ = std::process::Command::new("git")
                .args(["worktree", "remove", "--force", &self.temp_dir.to_string_lossy()])
                .current_dir(self.project_path)
                .output();
            let _ = std::fs::remove_dir_all(&self.temp_dir);
        }
    }
    let guard = WorktreeGuard {
        project_path,
        temp_dir: temp_dir.clone(),
    };

    // 5. Initialize submodules in worktree
    if !submodules.is_empty() {
        let _ = run_git_cmd(&temp_dir, &["submodule", "update", "--init", "--recursive"]).await;
    }

    // 6. Capture root diff and apply to worktree
    let root_diff_bytes = tokio::process::Command::new("git")
        .args(["diff-index", "-p", "--binary", "HEAD"])
        .current_dir(project_path)
        .output()
        .await
        .map_err(|e| format!("Failed to read root diff: {}", e))?
        .stdout;

    if !root_diff_bytes.is_empty() {
        run_git_apply(&temp_dir, &root_diff_bytes).await?;
    }

    // 7. Copy root untracked files
    let untracked_out =
        run_git_cmd(project_path, &["ls-files", "--others", "--exclude-standard"]).await?;
    for file_rel in untracked_out.lines() {
        let file_rel = file_rel.trim();
        if file_rel.is_empty() {
            continue;
        }
        let src = project_path.join(file_rel);
        let dst = temp_dir.join(file_rel);
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if src.is_file() {
            let _ = std::fs::copy(&src, &dst);
        }
    }

    // 8. Capture and apply submodule diffs and untracked files
    for sub in &submodules {
        let sub_src = project_path.join(sub);
        let sub_dst = temp_dir.join(sub);

        if sub_src.exists() && sub_dst.exists() {
            let sub_diff_bytes = tokio::process::Command::new("git")
                .args(["diff-index", "-p", "--binary", "HEAD"])
                .current_dir(&sub_src)
                .output()
                .await
                .map_err(|e| format!("Failed to read diff for submodule '{}': {}", sub, e))?
                .stdout;

            if !sub_diff_bytes.is_empty() {
                run_git_apply(&sub_dst, &sub_diff_bytes).await?;
            }

            let sub_untracked =
                run_git_cmd(&sub_src, &["ls-files", "--others", "--exclude-standard"]).await?;
            for file_rel in sub_untracked.lines() {
                let file_rel = file_rel.trim();
                if file_rel.is_empty() {
                    continue;
                }
                let src = sub_src.join(file_rel);
                let dst = sub_dst.join(file_rel);
                if let Some(parent) = dst.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if src.is_file() {
                    let _ = std::fs::copy(&src, &dst);
                }
            }

            let _ = run_git_cmd(&sub_dst, &["add", "-A"]).await;
            let status = run_git_cmd(&sub_dst, &["status", "--porcelain=v1"])
                .await
                .unwrap_or_default();
            if !status.trim().is_empty() {
                let _ = run_git_cmd(
                    &sub_dst,
                    &[
                        "commit",
                        "-m",
                        "Snapshot review changes",
                        "--no-verify",
                        "--allow-empty",
                    ],
                )
                .await;
            }
        }
    }

    // 9. Commit snapshot in root worktree
    let _ = run_git_cmd(&temp_dir, &["add", "-A"]).await;
    let root_status = run_git_cmd(&temp_dir, &["status", "--porcelain=v1"])
        .await
        .unwrap_or_default();
    if !root_status.trim().is_empty() {
        let _ = run_git_cmd(
            &temp_dir,
            &[
                "commit",
                "-m",
                "Snapshot review changes",
                "--no-verify",
                "--allow-empty",
            ],
        )
        .await;
    }
    let snapshot_commit = run_git_cmd(&temp_dir, &["rev-parse", "HEAD"])
        .await?
        .trim()
        .to_string();

    // 10. Generate review diff
    let review_diff = run_git_cmd(
        &temp_dir,
        &[
            "diff",
            "--submodule=diff",
            &format!("{}..{}", base_commit, snapshot_commit),
        ],
    )
    .await
    .unwrap_or_default();
    let git_status = run_git_cmd(&temp_dir, &["status", "--short"])
        .await
        .unwrap_or_default();

    // 11. Run reviewer in temp_dir
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

    // 12. Post-Review Status Audit
    let post_status = run_git_cmd(&temp_dir, &["status", "--porcelain=v1"])
        .await
        .unwrap_or_default();
    if !post_status.trim().is_empty() {
        return Err("Security violation: Reviewer process modified files in the review worktree.".to_string());
    }

    for sub in &submodules {
        let sub_dst = temp_dir.join(sub);
        if sub_dst.exists() {
            let sub_post_status = run_git_cmd(&sub_dst, &["status", "--porcelain=v1"])
                .await
                .unwrap_or_default();
            if !sub_post_status.trim().is_empty() {
                return Err("Security violation: Reviewer process modified files in review worktree submodule.".to_string());
            }
        }
    }

    let cr_result = engine.finding_aggregator.parse_review_output(&cr_out.content);
    drop(guard);
    Ok(cr_result)
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
        RunConfigurationSnapshot {
            project_path,
            assignments,
            iteration_limits: Default::default(),
            validation_gates: vec![],
            budget_limits: Default::default(),
            created_at_unix: 0,
            lean_antigravity_mode: false,
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
                Arc::new(move |event| event_sink.lock().unwrap().push(event)),
                Arc::new(|_| {}),
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
        let on_event: EventCallback = Arc::new(|_| {});

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
                &state_clone,
                &mut submit_rx,
                &mut worker_reclaim_rx,
                &cancel_clone,
                &on_event_clone,
                &|_| {},
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
        let on_event: EventCallback = Arc::new(|_| {});

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
                &state_clone,
                &mut submit_rx,
                &mut worker_reclaim_rx,
                &cancel_clone,
                &on_event_clone,
                &|_| {},
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
            &state,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
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
            &state,
            &mut submit_rx,
            &mut worker_reclaim_rx,
            &cancel_token,
            &on_event,
            &log,
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
}
