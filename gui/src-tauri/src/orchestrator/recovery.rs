use super::types::{RunConfigurationSnapshot, WorkflowState};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Arc, Mutex};

pub const JOURNAL_SCHEMA_VERSION: u32 = 1;
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 3;

/// Validates that a run ID conforms to canonical format and contains no path separators or traversal components.
pub fn validate_run_id(run_id: &str) -> Result<(), String> {
    let trimmed = run_id.trim();
    if trimmed.is_empty() {
        return Err("Run ID cannot be empty".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(format!(
            "Run ID '{}' contains invalid path separators or traversal components",
            run_id
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!(
            "Run ID '{}' contains invalid characters; expected alphanumeric, dash, underscore, or dot",
            run_id
        ));
    }
    Ok(())
}

/// Recovery status of an orchestrator run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunRecoveryStatus {
    Active,
    Interrupted,
    Complete,
    Failed,
    Cancelled,
}

/// Durable iteration counters needed to reconstruct or verify loop limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunIterationCounters {
    #[serde(default)]
    #[serde(alias = "plan_review_count")]
    pub plan_review_count: u32,
    #[serde(default)]
    #[serde(alias = "fix_count")]
    pub fix_count: u32,
    #[serde(default)]
    #[serde(alias = "code_review_count")]
    pub code_review_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "antigravity_dispatches")]
    pub antigravity_dispatches: Option<u32>,
    #[serde(default)]
    pub antigravity_task_dispatches: std::collections::HashMap<String, u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mailbox_epoch: Option<u64>,
}

/// Runtime-only continuation data reconstructed from a verified checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanGatedResumeContext {
    pub stage: WorkflowState,
    pub adopted_baseline: bool,
    pub plan_text: String,
    pub plan_review_count: u32,
    pub fix_count: u32,
    pub code_review_count: u32,
    pub antigravity_dispatches: u32,
    pub antigravity_task_dispatches: std::collections::HashMap<String, u32>,
    pub mailbox_epoch: u64,
}

/// Durable run journal persisted to secure application storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunJournal {
    #[serde(alias = "schema_version")]
    pub schema_version: u32,
    #[serde(alias = "run_id")]
    pub run_id: String,
    #[serde(alias = "workflow_type")]
    pub workflow_type: String,
    #[serde(alias = "canonical_project_path")]
    pub canonical_project_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "task_prompt")]
    pub task_prompt: Option<String>,
    /// Last plan that crossed the PlanReview approval boundary. Older journals
    /// may omit it; a verified checkpoint stage input can provide the fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_plan: Option<String>,
    pub snapshot: RunConfigurationSnapshot,
    #[serde(alias = "current_state")]
    pub current_state: WorkflowState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "last_successful_state")]
    pub last_successful_state: Option<WorkflowState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "stage_entry_info")]
    pub stage_entry_info: Option<String>,
    #[serde(default)]
    #[serde(alias = "iteration_counters")]
    pub iteration_counters: RunIterationCounters,
    #[serde(alias = "revision")]
    pub revision: u64,
    #[serde(default)]
    pub resume_generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "checkpoint_manifest_ref")]
    pub checkpoint_manifest_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "checkpoint_digest")]
    pub checkpoint_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_shelve_backup_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_shelve_backup_digest: Option<String>,
    pub status: RunRecoveryStatus,
    #[serde(alias = "created_at_unix")]
    pub created_at_unix: u64,
    #[serde(alias = "updated_at_unix")]
    pub updated_at_unix: u64,
}

/// Typed summary response for listing interrupted, historical, and invalid runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecoverySummary {
    #[serde(alias = "run_id")]
    pub run_id: String,
    #[serde(alias = "workflow_type")]
    pub workflow_type: String,
    #[serde(alias = "project_path")]
    pub project_path: String,
    pub status: RunRecoveryStatus,
    #[serde(alias = "current_state")]
    pub current_state: WorkflowState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "last_successful_state")]
    pub last_successful_state: Option<WorkflowState>,
    pub revision: u64,
    #[serde(alias = "created_at_unix")]
    pub created_at_unix: u64,
    #[serde(alias = "updated_at_unix")]
    pub updated_at_unix: u64,
    #[serde(alias = "is_resumable")]
    pub is_resumable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "error")]
    pub error: Option<String>,
}

/// Type of tracked file change in a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackedChangeType {
    Modified,
    Added,
    Deleted,
    Renamed,
}

/// Checkpoint record for an individual tracked file change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedChangeRecord {
    pub path: String,
    #[serde(alias = "change_type")]
    pub change_type: TrackedChangeType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "old_path")]
    pub old_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "content_hash")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

/// Checkpoint record for an untracked non-ignored file or symlink.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UntrackedCheckpointEntry {
    #[serde(alias = "rel_path")]
    pub rel_path: String,
    #[serde(alias = "file_type")]
    pub file_type: String,
    #[serde(alias = "content_hash")]
    pub content_hash: String,
    pub mode: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "symlink_target")]
    pub symlink_target: Option<String>,
}

/// Checkpoint state for a single repository (root or submodule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryCheckpoint {
    #[serde(alias = "canonical_root")]
    pub canonical_root: String,
    #[serde(alias = "head_oid")]
    pub head_oid: String,
    #[serde(alias = "refs_snapshot")]
    pub refs_snapshot: String,
    #[serde(alias = "status_inventory")]
    pub status_inventory: Vec<u8>,
    #[serde(default)]
    #[serde(alias = "tracked_changes")]
    pub tracked_changes: Vec<TrackedChangeRecord>,
    #[serde(default)]
    #[serde(alias = "untracked_files")]
    pub untracked_files: Vec<UntrackedCheckpointEntry>,
    #[serde(alias = "integrity_hash")]
    pub integrity_hash: String,
}

/// Submodule child record in the checkpoint hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmoduleCheckpoint {
    #[serde(default)]
    pub node_key: String,
    #[serde(default)]
    pub immediate_parent_key: String,
    #[serde(alias = "immediate_parent_path")]
    pub immediate_parent_path: String,
    #[serde(alias = "rel_path")]
    pub rel_path: String,
    #[serde(alias = "gitlink_oid")]
    pub gitlink_oid: String,
    #[serde(alias = "head_oid")]
    pub head_oid: String,
    pub repository: RepositoryCheckpoint,
    #[serde(default)]
    pub payload_entries: Vec<CheckpointPayloadEntry>,
    #[serde(default)]
    pub index_blob_hash: String,
}

/// Durable manifest interface for workspace checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointManifest {
    #[serde(alias = "schema_version")]
    pub schema_version: u32,
    #[serde(alias = "checkpoint_id")]
    pub checkpoint_id: String,
    #[serde(alias = "run_id")]
    pub run_id: String,
    pub stage: WorkflowState,
    #[serde(default)]
    pub kind: CheckpointKind,
    #[serde(alias = "created_at_unix")]
    pub created_at_unix: u64,
    pub repository: RepositoryCheckpoint,
    #[serde(default)]
    pub payload_entries: Vec<CheckpointPayloadEntry>,
    #[serde(default)]
    pub index_blob_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_input: Option<String>,
    #[serde(default)]
    pub submodules: Vec<SubmoduleCheckpoint>,
    #[serde(alias = "manifest_digest")]
    pub manifest_digest: String,
}

/// Semantic checkpoint boundary. Adopted state is deliberately not a completed stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    EntryBaseline,
    #[default]
    StageCheckpoint,
    AdoptBaseline,
    ShelveBackup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointPayloadEntry {
    pub path: String,
    /// file, symlink, symlink_file, symlink_dir, submodule, or missing.
    pub kind: String,
    pub mode: u32,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
}

pub struct JournalManager {
    runs_dir: PathBuf,
    #[cfg(test)]
    pub permissions_test_hook: Mutex<Option<Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>>>,
    #[cfg(test)]
    directory_permissions_test_hook: Mutex<Option<Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>>>,
    #[cfg(test)]
    existing_metadata_test_hook: Mutex<Option<Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>>>,
    #[cfg(test)]
    pub write_test_hook: Mutex<Option<Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>>>,
    #[cfg(test)]
    pub cleanup_test_hook: Mutex<Option<Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>>>,
}

fn validate_journal(journal: &RunJournal) -> Result<(), String> {
    validate_run_id(&journal.run_id)?;
    if journal.schema_version != JOURNAL_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported journal schema version {}; expected {}",
            journal.schema_version, JOURNAL_SCHEMA_VERSION
        ));
    }
    if journal.revision == 0 {
        return Err("Journal revision must be greater than zero".to_string());
    }
    if journal.updated_at_unix < journal.created_at_unix {
        return Err(format!(
            "Journal updated_at_unix {} precedes created_at_unix {}",
            journal.updated_at_unix, journal.created_at_unix
        ));
    }
    if journal.checkpoint_manifest_ref.is_some() != journal.checkpoint_digest.is_some() {
        return Err(
            "Checkpoint manifest reference and digest must either both be present or both be absent"
                .to_string(),
        );
    }

    let expected_terminal_state = match journal.status {
        RunRecoveryStatus::Complete => Some(WorkflowState::Complete),
        RunRecoveryStatus::Failed => Some(WorkflowState::Failed),
        RunRecoveryStatus::Cancelled => Some(WorkflowState::Cancelled),
        RunRecoveryStatus::Active | RunRecoveryStatus::Interrupted => None,
    };
    let is_terminal_state = matches!(
        journal.current_state,
        WorkflowState::Complete | WorkflowState::Failed | WorkflowState::Cancelled
    );
    match expected_terminal_state {
        Some(expected) if journal.current_state != expected => {
            return Err(format!(
                "Terminal journal status '{:?}' contradicts current state '{:?}'",
                journal.status, journal.current_state
            ));
        }
        None if is_terminal_state => {
            return Err(format!(
                "Non-terminal journal status '{:?}' contradicts terminal current state '{:?}'",
                journal.status, journal.current_state
            ));
        }
        _ => {}
    }
    Ok(())
}

impl JournalManager {
    pub fn new(runs_dir: PathBuf) -> Self {
        Self {
            runs_dir,
            #[cfg(test)]
            permissions_test_hook: Mutex::new(None),
            #[cfg(test)]
            directory_permissions_test_hook: Mutex::new(None),
            #[cfg(test)]
            existing_metadata_test_hook: Mutex::new(None),
            #[cfg(test)]
            write_test_hook: Mutex::new(None),
            #[cfg(test)]
            cleanup_test_hook: Mutex::new(None),
        }
    }

    pub fn runs_dir(&self) -> &Path {
        &self.runs_dir
    }

    pub fn ensure_directory(&self) -> Result<(), String> {
        if !self.runs_dir.exists() {
            fs::create_dir_all(&self.runs_dir).map_err(|e| {
                format!(
                    "Failed to create runs directory '{}': {}",
                    self.runs_dir.display(),
                    e
                )
            })?;
        }
        #[cfg(test)]
        if let Some(ref hook) = *self.directory_permissions_test_hook.lock().unwrap() {
            hook(&self.runs_dir)?;
        }
        // Unconditionally apply and verify permissions on runs directory on every use
        apply_and_verify_permissions(&self.runs_dir, true)?;
        Ok(())
    }

    pub fn write_journal(&self, journal: &RunJournal) -> Result<(), String> {
        validate_journal(journal)?;

        self.ensure_directory()?;

        let target_path = self.runs_dir.join(format!("{}.json", journal.run_id));

        // Only an explicit NotFound result permits creating a new journal. Do not
        // let Path::exists() collapse metadata errors into an apparent absence.
        #[cfg(test)]
        if let Some(ref hook) = *self.existing_metadata_test_hook.lock().unwrap() {
            hook(&target_path)?;
        }
        let existing_metadata = match fs::symlink_metadata(&target_path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!(
                    "Failed to inspect existing journal target '{}': {error}",
                    target_path.display()
                ));
            }
        };

        // Monotonic revision check & strict validation if an existing journal exists.
        if let Some(metadata) = existing_metadata {
            if !metadata.file_type().is_file() {
                return Err(format!(
                    "Existing journal target '{}' is not a regular file",
                    target_path.display()
                ));
            }
            let existing_bytes = fs::read(&target_path).map_err(|e| {
                format!(
                    "Failed to read existing journal at '{}': {}",
                    target_path.display(),
                    e
                )
            })?;
            let existing: RunJournal = serde_json::from_slice(&existing_bytes).map_err(|e| {
                format!(
                    "Failed to parse existing journal at '{}' (corrupt or invalid JSON): {}",
                    target_path.display(),
                    e
                )
            })?;
            if existing.schema_version != JOURNAL_SCHEMA_VERSION {
                return Err(format!(
                    "Existing journal at '{}' has unsupported schema version {}; expected {}",
                    target_path.display(),
                    existing.schema_version,
                    JOURNAL_SCHEMA_VERSION
                ));
            }
            if existing.run_id != journal.run_id {
                return Err(format!(
                    "Existing journal at '{}' has mismatched run ID '{}'; expected '{}'",
                    target_path.display(),
                    existing.run_id,
                    journal.run_id
                ));
            }
            validate_journal(&existing).map_err(|e| {
                format!(
                    "Existing journal at '{}' is semantically invalid: {e}",
                    target_path.display()
                )
            })?;
            if journal.revision < existing.revision {
                return Err(format!(
                    "Stale journal write rejected: incoming revision {} < existing revision {}",
                    journal.revision, existing.revision
                ));
            }
        }

        // 1. Create empty temporary file first
        let tmp_filename = format!("{}.tmp.{}", journal.run_id, uuid::Uuid::new_v4());
        let tmp_path = self.runs_dir.join(tmp_filename);

        fs::write(&tmp_path, b"").map_err(|e| {
            format!("Failed to create temporary journal file '{}': {}", tmp_path.display(), e)
        })?;

        // 2. Apply and verify permissions on temporary file BEFORE writing journal content
        #[cfg(test)]
        {
            if let Some(ref hook) = *self.permissions_test_hook.lock().unwrap() {
                if let Err(primary) = hook(&tmp_path) {
                    return Err(self.cleanup_error(&tmp_path, primary));
                }
            }
        }

        if let Err(e) = apply_and_verify_permissions(&tmp_path, false) {
            return Err(self.cleanup_error(
                &tmp_path,
                format!("Permission enforcement failure on temporary journal: {e}"),
            ));
        }

        // 3. Serialize journal content only after the temporary file is protected.
        let serialized = match serde_json::to_vec_pretty(journal) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Err(self.cleanup_error(&tmp_path, format!("Failed to serialize journal: {e}")));
            }
        };

        // Test seam at the content-write operation, after serialization and permission checks.
        #[cfg(test)]
        {
            if let Some(ref hook) = *self.write_test_hook.lock().unwrap() {
                if let Err(primary) = hook(&tmp_path) {
                    return Err(self.cleanup_error(&tmp_path, primary));
                }
            }
        }

        if let Err(e) = fs::write(&tmp_path, &serialized) {
            return Err(self.cleanup_error(
                &tmp_path,
                format!("Failed to write temporary journal file content: {e}"),
            ));
        }

        // 4. Atomic replace / rename
        if let Err(e) = replace_file_atomically(&tmp_path, &target_path) {
            return Err(self.cleanup_error(
                &tmp_path,
                format!("Atomic replace failed for journal: {e}"),
            ));
        }

        // 5. Verify permissions on target file
        apply_and_verify_permissions(&target_path, false)?;

        Ok(())
    }

    fn cleanup_temp_file(&self, path: &Path) -> Result<(), String> {
        #[cfg(test)]
        if let Some(ref hook) = *self.cleanup_test_hook.lock().unwrap() {
            return hook(path);
        }
        fs::remove_file(path).map_err(|e| {
            format!("Failed to remove temporary journal '{}': {e}", path.display())
        })
    }

    fn cleanup_error(&self, path: &Path, primary: String) -> String {
        match self.cleanup_temp_file(path) {
            Ok(()) => primary,
            Err(cleanup) => format!("{primary}; additionally, {cleanup}"),
        }
    }

    pub fn read_journal(&self, run_id: &str) -> Result<RunJournal, String> {
        validate_run_id(run_id)?;
        self.ensure_directory()?;
        let path = self.runs_dir.join(format!("{}.json", run_id));
        if !path.exists() {
            return Err(format!("Journal for run '{}' not found", run_id));
        }
        let data = fs::read(&path).map_err(|e| format!("Failed to read journal: {}", e))?;
        let journal: RunJournal = serde_json::from_slice(&data).map_err(|e| {
            format!("Failed to deserialize journal for run '{}': {}", run_id, e)
        })?;
        if journal.run_id != run_id {
            return Err(format!(
                "Run ID mismatch in journal: expected '{}', got '{}'",
                run_id, journal.run_id
            ));
        }
        validate_journal(&journal)?;

        Ok(journal)
    }

    pub fn list_journals(&self) -> Result<Vec<RunRecoverySummary>, String> {
        self.ensure_directory()?;

        let mut summaries = Vec::new();
        let entries = fs::read_dir(&self.runs_dir)
            .map_err(|e| format!("Failed to read runs directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Error reading directory entry: {}", e))?;
            let path = entry.path();
            if path.is_file() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.ends_with(".json") && !file_name.contains(".tmp.") {
                        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(file_name);
                        let mod_time = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        match fs::read(&path) {
                            Ok(bytes) => {
                                match serde_json::from_slice::<RunJournal>(&bytes) {
                                    Ok(journal) => {
                                        if validate_journal(&journal).is_ok() && journal.run_id == stem {
                                            let status = match journal.status {
                                                RunRecoveryStatus::Active => RunRecoveryStatus::Interrupted,
                                                other => other,
                                            };
                                            // In Phase A, resumability is false until payload restore is implemented
                                            summaries.push(RunRecoverySummary {
                                                run_id: journal.run_id,
                                                workflow_type: journal.workflow_type,
                                                project_path: journal.canonical_project_path,
                                                status,
                                                current_state: journal.current_state,
                                                last_successful_state: journal.last_successful_state,
                                                revision: journal.revision,
                                                created_at_unix: journal.created_at_unix,
                                                updated_at_unix: journal.updated_at_unix,
                                                is_resumable: false,
                                                error: None,
                                            });
                                        } else {
                                            // Invalid schema or run ID mismatch
                                            let err_msg = if journal.run_id != stem {
                                                format!("Run ID mismatch: filename '{}' != journal '{}'", stem, journal.run_id)
                                            } else if let Err(validation_error) = validate_journal(&journal) {
                                                validation_error
                                            } else if journal.schema_version != JOURNAL_SCHEMA_VERSION {
                                                format!("Unsupported schema version {}", journal.schema_version)
                                            } else {
                                                "Journal failed semantic validation".to_string()
                                            };
                                            summaries.push(RunRecoverySummary {
                                                run_id: stem.to_string(),
                                                workflow_type: "unknown".to_string(),
                                                project_path: String::new(),
                                                status: RunRecoveryStatus::Failed,
                                                current_state: WorkflowState::Failed,
                                                last_successful_state: None,
                                                revision: 0,
                                                created_at_unix: 0,
                                                updated_at_unix: mod_time,
                                                is_resumable: false,
                                                error: Some(err_msg),
                                            });
                                        }
                                    }
                                    Err(e) => {
                                        // Malformed JSON
                                        summaries.push(RunRecoverySummary {
                                            run_id: stem.to_string(),
                                            workflow_type: "unknown".to_string(),
                                            project_path: String::new(),
                                            status: RunRecoveryStatus::Failed,
                                            current_state: WorkflowState::Failed,
                                            last_successful_state: None,
                                            revision: 0,
                                            created_at_unix: 0,
                                            updated_at_unix: mod_time,
                                            is_resumable: false,
                                            error: Some(format!("Malformed journal JSON: {e}")),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                summaries.push(RunRecoverySummary {
                                    run_id: stem.to_string(),
                                    workflow_type: "unknown".to_string(),
                                    project_path: String::new(),
                                    status: RunRecoveryStatus::Failed,
                                    current_state: WorkflowState::Failed,
                                    last_successful_state: None,
                                    revision: 0,
                                    created_at_unix: 0,
                                    updated_at_unix: mod_time,
                                    is_resumable: false,
                                    error: Some(format!("Unreadable journal file: {e}")),
                                });
                            }
                        }
                    }
                }
            }
        }

        summaries.sort_by(|a, b| b.updated_at_unix.cmp(&a.updated_at_unix));
        Ok(summaries)
    }
}

#[cfg(windows)]
pub fn apply_permissions(path: &Path, _is_dir: bool) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{
        SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE,
        SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenUser, ACL, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    unsafe {
        // 1. Get current process token user SID
        let mut token_handle: HANDLE = INVALID_HANDLE_VALUE;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) == 0 {
            return Err(format!(
                "Failed to open current process token (err={})",
                GetLastError()
            ));
        }

        let mut token_info_len: u32 = 0;
        let _ = GetTokenInformation(
            token_handle,
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut token_info_len,
        );

        if token_info_len == 0 {
            CloseHandle(token_handle);
            return Err("Failed to query token user information length".to_string());
        }

        let mut token_buf = vec![0u8; token_info_len as usize];
        if GetTokenInformation(
            token_handle,
            TokenUser,
            token_buf.as_mut_ptr() as *mut _,
            token_info_len,
            &mut token_info_len,
        ) == 0
        {
            CloseHandle(token_handle);
            return Err(format!(
                "Failed to get token user information (err={})",
                GetLastError()
            ));
        }
        CloseHandle(token_handle);

        let token_user = &*(token_buf.as_ptr() as *const TOKEN_USER);
        let user_sid = token_user.User.Sid;

        const NO_INHERITANCE: u32 = 0;
        const SUB_CONTAINERS_AND_OBJECTS_INHERIT: u32 = 3; // CONTAINER_INHERIT_ACE (2) | OBJECT_INHERIT_ACE (1)

        // 2. Build explicit access structure granting access only to current user's SID
        let mut explicit_access: EXPLICIT_ACCESS_W = std::mem::zeroed();
        explicit_access.grfAccessPermissions = 0x1F01FF; // STANDARD_RIGHTS_ALL | SPECIFIC_RIGHTS_ALL (GENERIC_ALL equivalent)
        explicit_access.grfAccessMode = SET_ACCESS;
        explicit_access.grfInheritance = if _is_dir {
            SUB_CONTAINERS_AND_OBJECTS_INHERIT
        } else {
            NO_INHERITANCE
        };
        explicit_access.Trustee.pMultipleTrustee = std::ptr::null_mut();
        explicit_access.Trustee.MultipleTrusteeOperation = NO_MULTIPLE_TRUSTEE;
        explicit_access.Trustee.TrusteeForm = TRUSTEE_IS_SID;
        explicit_access.Trustee.TrusteeType = TRUSTEE_IS_USER;
        explicit_access.Trustee.ptstrName = user_sid as *mut u16;

        let mut new_acl: *mut ACL = std::ptr::null_mut();
        let set_entries_res = SetEntriesInAclW(
            1,
            &explicit_access,
            std::ptr::null_mut(),
            &mut new_acl,
        );
        if set_entries_res != 0 {
            return Err(format!(
                "Failed to build DACL with SetEntriesInAclW (code={})",
                set_entries_res
            ));
        }

        let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

        // 3. Set protected DACL with inheritance disabled
        let set_res = SetNamedSecurityInfoW(
            wide_path.as_ptr() as *mut u16,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            new_acl,
            std::ptr::null_mut(),
        );

        LocalFree(new_acl as *mut std::ffi::c_void);

        if set_res != 0 {
            return Err(format!(
                "Failed to set protected DACL on '{}' (code={})",
                path.display(),
                set_res
            ));
        }

        Ok(())
    }
}

#[cfg(windows)]
pub fn verify_permissions(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        EqualSid, GetAce, GetAclInformation, GetSecurityDescriptorControl, GetTokenInformation,
        TokenUser, ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED, TOKEN_QUERY,
        TOKEN_USER,
    };
    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }

    unsafe {
        // 1. Get current process token user SID
        let mut token_handle: HANDLE = INVALID_HANDLE_VALUE;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) == 0 {
            return Err(format!(
                "Failed to open current process token (err={})",
                GetLastError()
            ));
        }

        let mut token_info_len: u32 = 0;
        let _ = GetTokenInformation(
            token_handle,
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut token_info_len,
        );

        if token_info_len == 0 {
            CloseHandle(token_handle);
            return Err("Failed to query token user information length".to_string());
        }

        let mut token_buf = vec![0u8; token_info_len as usize];
        if GetTokenInformation(
            token_handle,
            TokenUser,
            token_buf.as_mut_ptr() as *mut _,
            token_info_len,
            &mut token_info_len,
        ) == 0
        {
            CloseHandle(token_handle);
            return Err(format!(
                "Failed to get token user information (err={})",
                GetLastError()
            ));
        }
        CloseHandle(token_handle);

        let token_user = &*(token_buf.as_ptr() as *const TOKEN_USER);
        let user_sid = token_user.User.Sid;

        let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

        // 2. Query security descriptor and DACL
        let mut sec_desc: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let get_res = GetNamedSecurityInfoW(
            wide_path.as_ptr() as *mut u16,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut sec_desc,
        );

        if get_res != 0 || sec_desc.is_null() || dacl.is_null() {
            if !sec_desc.is_null() {
                LocalFree(sec_desc as *mut std::ffi::c_void);
            }
            return Err(format!(
                "Failed to verify DACL on '{}' (code={})",
                path.display(),
                get_res
            ));
        }

        // Verify DACL is protected (inheritance disabled)
        let mut control: u16 = 0;
        let mut revision: u32 = 0;
        if GetSecurityDescriptorControl(sec_desc, &mut control, &mut revision) == 0 {
            LocalFree(sec_desc as *mut std::ffi::c_void);
            return Err("Failed to query security descriptor control bits".to_string());
        }
        if (control & SE_DACL_PROTECTED) == 0 {
            LocalFree(sec_desc as *mut std::ffi::c_void);
            return Err(format!(
                "DACL on '{}' is not protected (inheritance not disabled)",
                path.display()
            ));
        }

        // Verify ACL size and iterate ACEs to verify only user_sid has access
        let mut acl_size: ACL_SIZE_INFORMATION = std::mem::zeroed();
        if GetAclInformation(
            dacl,
            &mut acl_size as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        ) == 0
        {
            LocalFree(sec_desc as *mut std::ffi::c_void);
            return Err("Failed to get ACL size information".to_string());
        }

        if acl_size.AceCount == 0 {
            LocalFree(sec_desc as *mut std::ffi::c_void);
            return Err(format!("DACL on '{}' contains zero ACEs", path.display()));
        }

        for i in 0..acl_size.AceCount {
            let mut p_ace: *mut std::ffi::c_void = std::ptr::null_mut();
            if GetAce(dacl, i, &mut p_ace) == 0 || p_ace.is_null() {
                LocalFree(sec_desc as *mut std::ffi::c_void);
                return Err(format!("Failed to retrieve ACE {i} from DACL"));
            }

            let ace = &*(p_ace as *const ACCESS_ALLOWED_ACE);
            if ace.Header.AceType != ACCESS_ALLOWED_ACE_TYPE {
                LocalFree(sec_desc as *mut std::ffi::c_void);
                return Err(format!(
                    "Unexpected ACE type {} in DACL on '{}'",
                    ace.Header.AceType,
                    path.display()
                ));
            }

            let ace_sid = &ace.SidStart as *const u32 as *mut std::ffi::c_void;
            if EqualSid(user_sid, ace_sid) == 0 {
                LocalFree(sec_desc as *mut std::ffi::c_void);
                return Err(format!(
                    "Security violation: Foreign principal SID found in DACL on '{}'",
                    path.display()
                ));
            }
        }

        LocalFree(sec_desc as *mut std::ffi::c_void);
        Ok(())
    }
}

#[cfg(windows)]
pub fn apply_and_verify_permissions(path: &Path, is_dir: bool) -> Result<(), String> {
    apply_permissions(path, is_dir)?;
    verify_permissions(path)?;
    Ok(())
}

#[cfg(not(windows))]
pub fn apply_and_verify_permissions(path: &Path, is_dir: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let target_mode = if is_dir { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(target_mode))
        .map_err(|e| format!("Failed to set permissions on '{}': {}", path.display(), e))?;

    let meta = fs::metadata(path)
        .map_err(|e| format!("Failed to read metadata for '{}': {}", path.display(), e))?;
    let mode = meta.permissions().mode();
    if (mode & 0o077) != 0 {
        return Err(format!(
            "Permission verification failed on '{}': mode {:o} has group/other bits set",
            path.display(),
            mode
        ));
    }
    Ok(())
}

fn replace_file_atomically(temp: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
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
        }

        if !target.exists() {
            return fs::rename(temp, target);
        }

        let target_wide = target.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let temp_wide = temp.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let result = unsafe {
            ReplaceFileW(
                target_wide.as_ptr(),
                temp_wide.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            fs::rename(temp, target)
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(temp, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::types::{AgentRole, ExecutionAdapterType, LoopIterationLimits, OrchestratorProfile, ProfileCapability};
    use std::collections::HashMap;

    fn sample_snapshot() -> RunConfigurationSnapshot {
        let planner = OrchestratorProfile {
            id: "planner-1".to_string(),
            display_name: "Planner Profile".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![ProfileCapability::Reasoning],
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
        };
        let mut assignments = HashMap::new();
        assignments.insert(AgentRole::Planner, planner);

        RunConfigurationSnapshot {
            project_path: "C:\\dev\\project".to_string(),
            assignments,
            iteration_limits: LoopIterationLimits::default(),
            validation_gates: Vec::new(),
            budget_limits: HashMap::new(),
            created_at_unix: 1700000000,
            lean_antigravity_mode: true,
            plan_workspace: Default::default(),
        }
    }

    fn sample_journal(run_id: &str, revision: u64, status: RunRecoveryStatus) -> RunJournal {
        let current_state = match status {
            RunRecoveryStatus::Complete => WorkflowState::Complete,
            RunRecoveryStatus::Failed => WorkflowState::Failed,
            RunRecoveryStatus::Cancelled => WorkflowState::Cancelled,
            RunRecoveryStatus::Active | RunRecoveryStatus::Interrupted => WorkflowState::PlanGeneration,
        };
        RunJournal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            run_id: run_id.to_string(),
            workflow_type: "full_loop".to_string(),
            canonical_project_path: "C:\\dev\\project".to_string(),
            task_prompt: Some("Implement recovery".to_string()),
            approved_plan: Some("Approved recovery plan".to_string()),
            snapshot: sample_snapshot(),
            current_state,
            last_successful_state: Some(WorkflowState::BuildingContext),
            stage_entry_info: Some("Drafting initial plan".to_string()),
            iteration_counters: RunIterationCounters {
                plan_review_count: 1,
                fix_count: 0,
                code_review_count: 0,
                antigravity_dispatches: Some(1),
                antigravity_task_dispatches: std::collections::HashMap::new(),
                mailbox_epoch: Some(1),
            },
            revision,
            resume_generation: 0,
            checkpoint_manifest_ref: Some("chk-123".to_string()),
            checkpoint_digest: Some("sha256:abcd".to_string()),
            last_shelve_backup_id: None,
            last_shelve_backup_digest: None,
            status,
            created_at_unix: 1700000000,
            updated_at_unix: 1700000100,
        }
    }

    #[test]
    fn test_journal_round_trip_serialization() {
        let journal = sample_journal("run-test-1", 1, RunRecoveryStatus::Active);
        let serialized = serde_json::to_string_pretty(&journal).unwrap();
        let deserialized: RunJournal = serde_json::from_str(&serialized).unwrap();
        assert_eq!(journal, deserialized);
    }

    #[test]
    fn phase_a_journal_without_approved_plan_remains_readable() {
        let journal = sample_journal("phase-a-legacy", 1, RunRecoveryStatus::Interrupted);
        let mut value = serde_json::to_value(journal).unwrap();
        value.as_object_mut().unwrap().remove("approvedPlan");
        let deserialized: RunJournal = serde_json::from_value(value).unwrap();
        assert_eq!(deserialized.approved_plan, None);
    }

    #[test]
    fn test_checkpoint_manifest_round_trip() {
        let manifest = CheckpointManifest {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            checkpoint_id: "chk-001".to_string(),
            run_id: "run-test-1".to_string(),
            stage: WorkflowState::Implementation,
            kind: CheckpointKind::StageCheckpoint,
            created_at_unix: 1700000050,
            repository: RepositoryCheckpoint {
                canonical_root: "C:\\dev\\project".to_string(),
                head_oid: "a".repeat(40),
                refs_snapshot: "refs/heads/main aaaa".to_string(),
                status_inventory: b"M src/lib.rs\0".to_vec(),
                tracked_changes: vec![TrackedChangeRecord {
                    path: "src/lib.rs".to_string(),
                    change_type: TrackedChangeType::Modified,
                    old_path: None,
                    content_hash: Some("sha256:1111".to_string()),
                    mode: Some(0o644),
                }],
                untracked_files: vec![UntrackedCheckpointEntry {
                    rel_path: "untracked.txt".to_string(),
                    file_type: "file".to_string(),
                    content_hash: "sha256:2222".to_string(),
                    mode: 0o644,
                    symlink_target: None,
                }],
                integrity_hash: "sha256:repo_hash".to_string(),
            },
            payload_entries: vec![],
            index_blob_hash: "0".repeat(64),
            task_prompt: None,
            stage_input: None,
            submodules: vec![SubmoduleCheckpoint {
                node_key: "vendor/sub".to_string(),
                immediate_parent_key: ".".to_string(),
                immediate_parent_path: "C:\\dev\\project".to_string(),
                rel_path: "vendor/sub".to_string(),
                gitlink_oid: "b".repeat(40),
                head_oid: "b".repeat(40),
                repository: RepositoryCheckpoint {
                    canonical_root: "C:\\dev\\project\\vendor\\sub".to_string(),
                    head_oid: "b".repeat(40),
                    refs_snapshot: "refs/heads/main bbbb".to_string(),
                    status_inventory: vec![],
                    tracked_changes: vec![],
                    untracked_files: vec![],
                    integrity_hash: "sha256:sub_hash".to_string(),
                },
                payload_entries: vec![CheckpointPayloadEntry {
                    path: "sub_file.txt".to_string(),
                    kind: "file".to_string(),
                    mode: 0o644,
                    size: 10,
                    blob_hash: Some("sha256:subblob".to_string()),
                    symlink_target: None,
                }],
                index_blob_hash: "1".repeat(64),
            }],
            manifest_digest: "sha256:manifest_hash".to_string(),
        };

        let serialized = serde_json::to_string_pretty(&manifest).unwrap();
        let deserialized: CheckpointManifest = serde_json::from_str(&serialized).unwrap();
        assert_eq!(manifest, deserialized);
    }

    #[test]
    fn test_legacy_v2_checkpoint_manifest_deserialization() {
        let legacy_v2_json = r#"{
            "schemaVersion": 2,
            "checkpointId": "chk-v2-legacy",
            "runId": "run-legacy-2",
            "stage": "implementation",
            "kind": "stage_checkpoint",
            "createdAtUnix": 1700000000,
            "repository": {
                "canonicalRoot": "C:\\dev\\project",
                "headOid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "refsSnapshot": "refs/heads/main aaaa",
                "statusInventory": [],
                "trackedChanges": [],
                "untrackedFiles": [],
                "integrityHash": "sha256:root_hash"
            },
            "payloadEntries": [],
            "indexBlobHash": "0000000000000000000000000000000000000000000000000000000000000000",
            "submodules": [
                {
                    "immediateParentPath": "C:\\dev\\project",
                    "relPath": "vendor/sub",
                    "gitlinkOid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "headOid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "repository": {
                        "canonicalRoot": "C:\\dev\\project\\vendor\\sub",
                        "headOid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "refsSnapshot": "refs/heads/main bbbb",
                        "statusInventory": [],
                        "trackedChanges": [],
                        "untrackedFiles": [],
                        "integrityHash": "sha256:sub_hash"
                    }
                }
            ],
            "manifestDigest": "sha256:manifest_hash"
        }"#;

        let deserialized: CheckpointManifest = serde_json::from_str(legacy_v2_json).unwrap();
        assert_eq!(deserialized.schema_version, 2);
        assert_eq!(deserialized.submodules.len(), 1);
        assert_eq!(deserialized.submodules[0].node_key, "");
        assert_eq!(deserialized.submodules[0].payload_entries, vec![]);
        assert_eq!(deserialized.submodules[0].index_blob_hash, "");
    }

    #[test]
    fn test_journal_manager_atomic_write_and_read() {
        let temp = tempfile::tempdir().unwrap();
        let manager = JournalManager::new(temp.path().join("runs"));

        let journal = sample_journal("run-write-1", 1, RunRecoveryStatus::Active);
        manager.write_journal(&journal).unwrap();

        let loaded = manager.read_journal("run-write-1").unwrap();
        assert_eq!(journal, loaded);
    }

    #[test]
    fn test_journal_manager_rejects_stale_revision() {
        let temp = tempfile::tempdir().unwrap();
        let manager = JournalManager::new(temp.path().join("runs"));

        let j1 = sample_journal("run-rev-1", 5, RunRecoveryStatus::Active);
        manager.write_journal(&j1).unwrap();

        let mut j_stale = sample_journal("run-rev-1", 4, RunRecoveryStatus::Active);
        j_stale.current_state = WorkflowState::Validation;
        let res = manager.write_journal(&j_stale);

        assert!(res.is_err(), "Must reject stale revision");
        assert!(res.unwrap_err().contains("Stale journal write rejected"));

        let current = manager.read_journal("run-rev-1").unwrap();
        assert_eq!(current.revision, 5);
        assert_eq!(current.current_state, WorkflowState::PlanGeneration);
    }

    #[test]
    fn test_startup_classification_active_becomes_interrupted() {
        let temp = tempfile::tempdir().unwrap();
        let manager = JournalManager::new(temp.path().join("runs"));

        let active_journal = sample_journal("run-active", 1, RunRecoveryStatus::Active);
        manager.write_journal(&active_journal).unwrap();

        let complete_journal = sample_journal("run-complete", 1, RunRecoveryStatus::Complete);
        manager.write_journal(&complete_journal).unwrap();

        let failed_journal = sample_journal("run-failed", 1, RunRecoveryStatus::Failed);
        manager.write_journal(&failed_journal).unwrap();

        let list = manager.list_journals().unwrap();
        assert_eq!(list.len(), 3);

        let active_summary = list.iter().find(|s| s.run_id == "run-active").unwrap();
        assert_eq!(active_summary.status, RunRecoveryStatus::Interrupted);
        assert!(!active_summary.is_resumable, "Phase A reports is_resumable=false");
        assert!(active_summary.error.is_none());

        let complete_summary = list.iter().find(|s| s.run_id == "run-complete").unwrap();
        assert_eq!(complete_summary.status, RunRecoveryStatus::Complete);
        assert!(!complete_summary.is_resumable);

        let failed_summary = list.iter().find(|s| s.run_id == "run-failed").unwrap();
        assert_eq!(failed_summary.status, RunRecoveryStatus::Failed);
        assert!(!failed_summary.is_resumable);
    }

    #[test]
    fn test_zero_content_bytes_written_on_permission_failure() {
        let temp = tempfile::tempdir().unwrap();
        let manager = JournalManager::new(temp.path().join("runs"));
        *manager.permissions_test_hook.lock().unwrap() = Some(Arc::new(|_path| {
            Err("Injected permission setup failure".to_string())
        }));

        let journal = sample_journal("run-zero-bytes", 1, RunRecoveryStatus::Active);
        let res = manager.write_journal(&journal);

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Injected permission setup failure"));

        let target_path = temp.path().join("runs").join("run-zero-bytes.json");
        assert!(!target_path.exists(), "Target journal must not exist on permission failure");

        // Verify no orphaned temporary files exist in runs directory
        let files: Vec<_> = fs::read_dir(temp.path().join("runs")).unwrap().collect();
        assert!(files.is_empty(), "No temporary file should remain on permission failure");
    }

    #[test]
    fn test_existing_runs_directory_permissions_rechecked() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        fs::create_dir_all(&runs_dir).unwrap();

        let manager = JournalManager::new(runs_dir);
        // Ensure directory rechecks permissions on existing folder without error
        assert!(manager.ensure_directory().is_ok());
    }

    #[test]
    fn test_read_and_list_fail_before_exposing_journals_when_directory_protection_fails() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let manager = JournalManager::new(runs_dir.clone());
        let journal = sample_journal("protected-access", 1, RunRecoveryStatus::Active);
        manager.write_journal(&journal).unwrap();
        assert!(runs_dir.exists(), "test exercises an already-existing runs directory");

        let verification_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = Arc::clone(&verification_count);
        *manager.directory_permissions_test_hook.lock().unwrap() = Some(Arc::new(move |path| {
            assert!(path.exists());
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err("injected runs-directory protection verification failure".to_string())
        }));

        let read_error = manager.read_journal("protected-access").unwrap_err();
        assert!(read_error.contains("injected runs-directory protection verification failure"));
        let list_error = manager.list_journals().unwrap_err();
        assert!(list_error.contains("injected runs-directory protection verification failure"));
        assert_eq!(verification_count.load(std::sync::atomic::Ordering::SeqCst), 2);
        // The existing journal is preserved; neither API returns its contents after failed verification.
        assert!(runs_dir.join("protected-access.json").exists());
    }

    #[test]
    fn test_read_and_list_succeed_after_directory_protection_verification() {
        let temp = tempfile::tempdir().unwrap();
        let manager = JournalManager::new(temp.path().join("runs"));
        let journal = sample_journal("protected-access-ok", 1, RunRecoveryStatus::Active);
        manager.write_journal(&journal).unwrap();

        let read_back = manager.read_journal("protected-access-ok").unwrap();
        assert_eq!(read_back, journal);
        let summaries = manager.list_journals().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].run_id, "protected-access-ok");
    }

    #[test]
    fn test_validate_run_id_rejects_path_traversal() {
        assert!(validate_run_id("").is_err());
        assert!(validate_run_id("   ").is_err());
        assert!(validate_run_id("../escape").is_err());
        assert!(validate_run_id("dir/subdir").is_err());
        assert!(validate_run_id("dir\\subdir").is_err());
        assert!(validate_run_id("run;bad").is_err());
        assert!(validate_run_id("valid-run_id.123").is_ok());
        assert!(validate_run_id("4a7844eb-fc56-427a-8f12-c2053fe9a80e").is_ok());
    }

    #[test]
    fn test_corrupt_and_invalid_journals_enumerated_as_errors() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        fs::create_dir_all(&runs_dir).unwrap();

        // 1. Corrupt JSON file
        fs::write(runs_dir.join("run-corrupt.json"), b"{ invalid json ").unwrap();

        // 2. Schema mismatch file
        let mut bad_schema = sample_journal("run-bad-schema", 1, RunRecoveryStatus::Active);
        bad_schema.schema_version = 999;
        fs::write(runs_dir.join("run-bad-schema.json"), serde_json::to_vec_pretty(&bad_schema).unwrap()).unwrap();

        let manager = JournalManager::new(runs_dir);
        let list = manager.list_journals().unwrap();
        assert_eq!(list.len(), 2);

        let corrupt = list.iter().find(|s| s.run_id == "run-corrupt").unwrap();
        assert_eq!(corrupt.status, RunRecoveryStatus::Failed);
        assert!(!corrupt.is_resumable);
        assert!(corrupt.error.as_ref().unwrap().contains("Malformed journal JSON"));

        let bad_sch = list.iter().find(|s| s.run_id == "run-bad-schema").unwrap();
        assert_eq!(bad_sch.status, RunRecoveryStatus::Failed);
        assert!(!bad_sch.is_resumable);
        assert!(bad_sch
            .error
            .as_ref()
            .unwrap()
            .contains("Unsupported journal schema version 999"));
    }

    #[test]
    fn test_write_journal_fails_closed_and_preserves_corrupt_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let manager = JournalManager::new(runs_dir.clone());
        manager.ensure_directory().unwrap();

        let target_file = runs_dir.join("run-corrupt-target.json");
        let corrupt_content = b"{\"corrupt\": true, \"unclosed_json\": ";
        fs::write(&target_file, corrupt_content).unwrap();

        let journal = sample_journal("run-corrupt-target", 1, RunRecoveryStatus::Active);

        let res = manager.write_journal(&journal);
        assert!(res.is_err(), "write_journal must fail on corrupt existing file");
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("corrupt or invalid JSON"), "Error was: {err_msg}");

        // Assert file bytes remain strictly unchanged
        let current_bytes = fs::read(&target_file).unwrap();
        assert_eq!(current_bytes, corrupt_content, "Corrupt file must remain byte-for-byte untouched");
    }

    #[test]
    fn test_write_journal_fails_closed_on_existing_target_metadata_error() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let manager = JournalManager::new(runs_dir.clone());
        manager.ensure_directory().unwrap();

        let target = runs_dir.join("metadata-error.json");
        let original = b"existing journal bytes must remain untouched";
        fs::write(&target, original).unwrap();
        *manager.existing_metadata_test_hook.lock().unwrap() = Some(Arc::new(|path| {
            assert!(path.exists(), "injected metadata failure targets an existing file");
            Err("injected metadata inspection failure".to_string())
        }));

        let replacement = sample_journal("metadata-error", 2, RunRecoveryStatus::Active);
        let error = manager.write_journal(&replacement).unwrap_err();
        assert!(error.contains("injected metadata inspection failure"), "{error}");
        assert_eq!(fs::read(&target).unwrap(), original);
    }

    #[test]
    fn test_write_journal_fails_closed_on_unknown_schema_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let manager = JournalManager::new(runs_dir.clone());
        manager.ensure_directory().unwrap();

        let target_file = runs_dir.join("run-unknown-schema.json");
        let mut bad_journal = sample_journal("run-unknown-schema", 1, RunRecoveryStatus::Active);
        bad_journal.schema_version = 42;
        let serialized = serde_json::to_vec_pretty(&bad_journal).unwrap();
        fs::write(&target_file, &serialized).unwrap();

        let valid_journal = sample_journal("run-unknown-schema", 2, RunRecoveryStatus::Active);

        let res = manager.write_journal(&valid_journal);
        assert!(res.is_err(), "write_journal must fail on unsupported schema existing file");
        assert!(res.unwrap_err().contains("unsupported schema version 42"));

        // Assert file bytes remain strictly untouched
        let current_bytes = fs::read(&target_file).unwrap();
        assert_eq!(current_bytes, serialized);
    }

    #[test]
    fn test_write_journal_fails_closed_on_run_id_mismatch_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let manager = JournalManager::new(runs_dir.clone());
        manager.ensure_directory().unwrap();

        let target_file = runs_dir.join("run-target-id.json");
        let mismatch_journal = sample_journal("other-run-id", 1, RunRecoveryStatus::Active);
        let serialized = serde_json::to_vec_pretty(&mismatch_journal).unwrap();
        fs::write(&target_file, &serialized).unwrap();

        let valid_journal = sample_journal("run-target-id", 2, RunRecoveryStatus::Active);

        let res = manager.write_journal(&valid_journal);
        assert!(res.is_err(), "write_journal must fail on run ID mismatch");
        assert!(res.unwrap_err().contains("mismatched run ID"));

        // Assert file bytes remain untouched
        let current_bytes = fs::read(&target_file).unwrap();
        assert_eq!(current_bytes, serialized);
    }

    #[test]
    fn test_semantically_invalid_existing_journal_is_rejected_without_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let runs_dir = temp.path().join("runs");
        let manager = JournalManager::new(runs_dir.clone());
        manager.ensure_directory().unwrap();

        let target = runs_dir.join("invalid-semantic.json");
        let mut invalid = sample_journal("invalid-semantic", 1, RunRecoveryStatus::Active);
        invalid.checkpoint_digest = None;
        let original = serde_json::to_vec(&invalid).unwrap();
        fs::write(&target, &original).unwrap();

        let replacement = sample_journal("invalid-semantic", 2, RunRecoveryStatus::Active);
        let error = manager.write_journal(&replacement).unwrap_err();
        assert!(error.contains("Checkpoint manifest reference and digest"));
        assert_eq!(fs::read(&target).unwrap(), original);
        assert!(manager
            .read_journal("invalid-semantic")
            .unwrap_err()
            .contains("Checkpoint manifest reference and digest"));
        let summary = manager.list_journals().unwrap();
        assert_eq!(summary.len(), 1);
        assert!(summary[0]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("Checkpoint manifest reference and digest"));
    }

    #[test]
    fn test_journal_semantic_validator_rejects_invalid_boundaries() {
        let mut journal = sample_journal("semantic-validation", 1, RunRecoveryStatus::Active);
        journal.revision = 0;
        assert!(validate_journal(&journal).unwrap_err().contains("revision"));

        journal = sample_journal("semantic-validation", 1, RunRecoveryStatus::Active);
        journal.updated_at_unix = journal.created_at_unix - 1;
        assert!(validate_journal(&journal).unwrap_err().contains("precedes"));

        journal = sample_journal("semantic-validation", 1, RunRecoveryStatus::Complete);
        journal.current_state = WorkflowState::PlanGeneration;
        assert!(validate_journal(&journal).unwrap_err().contains("contradicts"));

        journal = sample_journal("semantic-validation", 1, RunRecoveryStatus::Active);
        journal.current_state = WorkflowState::Failed;
        assert!(validate_journal(&journal).unwrap_err().contains("contradicts"));
    }

    #[test]
    fn test_temp_cleanup_failure_is_reported_with_primary_write_error() {
        for primary_failure in ["permission", "content"] {
            let temp = tempfile::tempdir().unwrap();
            let runs_dir = temp.path().join("runs");
            let manager = JournalManager::new(runs_dir.clone());
            if primary_failure == "permission" {
                *manager.permissions_test_hook.lock().unwrap() = Some(Arc::new(|_| {
                    Err("injected permission failure".to_string())
                }));
            } else {
                *manager.write_test_hook.lock().unwrap() = Some(Arc::new(|_| {
                    Err("injected content write failure".to_string())
                }));
            }
            *manager.cleanup_test_hook.lock().unwrap() = Some(Arc::new(|path| {
                Err(format!("injected temp cleanup failure for {}", path.display()))
            }));

            let journal = sample_journal("cleanup-failure", 1, RunRecoveryStatus::Active);
            let error = manager.write_journal(&journal).unwrap_err();
            assert!(error.contains(&format!("injected {primary_failure}")), "{error}");
            assert!(error.contains("injected temp cleanup failure"), "{error}");
            assert!(!runs_dir.join("cleanup-failure.json").exists());
            let leftovers: Vec<_> = fs::read_dir(&runs_dir).unwrap().collect();
            assert_eq!(leftovers.len(), 1, "injected cleanup failure should leave only the reported temp file");
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_dacl_protected_current_user_accepted() {
        let temp = tempfile::tempdir().unwrap();
        let test_file = temp.path().join("valid_dacl.json");
        fs::write(&test_file, b"test").unwrap();

        assert!(apply_permissions(&test_file, false).is_ok());
        assert!(verify_permissions(&test_file).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_dacl_foreign_ace_rejected() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Security::Authorization::{
            SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE,
            SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
        };
        use windows_sys::Win32::Security::{
            CreateWellKnownSid, GetTokenInformation, TokenUser, WinWorldSid, ACL,
            DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
            SECURITY_MAX_SID_SIZE, TOKEN_QUERY, TOKEN_USER,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        #[link(name = "kernel32")]
        extern "system" {
            fn LocalFree(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        }

        let temp = tempfile::tempdir().unwrap();
        let test_file = temp.path().join("foreign_ace.json");
        fs::write(&test_file, b"test").unwrap();

        unsafe {
            // Get user SID
            let mut token_handle: HANDLE = INVALID_HANDLE_VALUE;
            assert_ne!(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle), 0);
            let mut token_info_len: u32 = 0;
            let _ = GetTokenInformation(token_handle, TokenUser, std::ptr::null_mut(), 0, &mut token_info_len);
            let mut token_buf = vec![0u8; token_info_len as usize];
            assert_ne!(GetTokenInformation(token_handle, TokenUser, token_buf.as_mut_ptr() as *mut _, token_info_len, &mut token_info_len), 0);
            CloseHandle(token_handle);
            let token_user = &*(token_buf.as_ptr() as *const TOKEN_USER);
            let user_sid = token_user.User.Sid;

            // Create well-known Everyone SID (WinWorldSid)
            let mut world_sid = vec![0u8; SECURITY_MAX_SID_SIZE as usize];
            let mut world_sid_size: u32 = SECURITY_MAX_SID_SIZE;
            assert_ne!(
                CreateWellKnownSid(
                    WinWorldSid,
                    std::ptr::null_mut(),
                    world_sid.as_mut_ptr() as *mut _,
                    &mut world_sid_size
                ),
                0
            );

            // Build explicit access entries for user SID and Everyone
            let mut entries: [EXPLICIT_ACCESS_W; 2] = std::mem::zeroed();
            entries[0].grfAccessPermissions = 0x1F01FF;
            entries[0].grfAccessMode = SET_ACCESS;
            entries[0].grfInheritance = 0;
            entries[0].Trustee.pMultipleTrustee = std::ptr::null_mut();
            entries[0].Trustee.MultipleTrusteeOperation = NO_MULTIPLE_TRUSTEE;
            entries[0].Trustee.TrusteeForm = TRUSTEE_IS_SID;
            entries[0].Trustee.TrusteeType = TRUSTEE_IS_USER;
            entries[0].Trustee.ptstrName = user_sid as *mut u16;

            entries[1].grfAccessPermissions = 0x1F01FF;
            entries[1].grfAccessMode = SET_ACCESS;
            entries[1].grfInheritance = 0;
            entries[1].Trustee.pMultipleTrustee = std::ptr::null_mut();
            entries[1].Trustee.MultipleTrusteeOperation = NO_MULTIPLE_TRUSTEE;
            entries[1].Trustee.TrusteeForm = TRUSTEE_IS_SID;
            entries[1].Trustee.TrusteeType = TRUSTEE_IS_USER;
            entries[1].Trustee.ptstrName = world_sid.as_mut_ptr() as *mut u16;

            let mut new_acl: *mut ACL = std::ptr::null_mut();
            assert_eq!(SetEntriesInAclW(2, entries.as_ptr(), std::ptr::null_mut(), &mut new_acl), 0);

            let wide_path: Vec<u16> = test_file.as_os_str().encode_wide().chain(Some(0)).collect();
            assert_eq!(
                SetNamedSecurityInfoW(
                    wide_path.as_ptr() as *mut u16,
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    new_acl,
                    std::ptr::null_mut(),
                ),
                0
            );
            LocalFree(new_acl as *mut std::ffi::c_void);
        }

        let verify_res = verify_permissions(&test_file);
        assert!(verify_res.is_err(), "Must reject foreign principal ACE");
        let err = verify_res.unwrap_err();
        assert!(err.contains("Foreign principal SID found in DACL"), "Error was: {err}");
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_dacl_unprotected_inheritance_rejected() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Security::Authorization::{
            SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE,
            SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
        };
        use windows_sys::Win32::Security::{
            GetTokenInformation, TokenUser, ACL, DACL_SECURITY_INFORMATION,
            UNPROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        #[link(name = "kernel32")]
        extern "system" {
            fn LocalFree(hmem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        }

        let temp = tempfile::tempdir().unwrap();
        let test_file = temp.path().join("unprotected_dacl.json");
        fs::write(&test_file, b"test").unwrap();

        unsafe {
            let mut token_handle: HANDLE = INVALID_HANDLE_VALUE;
            assert_ne!(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle), 0);
            let mut token_info_len: u32 = 0;
            let _ = GetTokenInformation(token_handle, TokenUser, std::ptr::null_mut(), 0, &mut token_info_len);
            let mut token_buf = vec![0u8; token_info_len as usize];
            assert_ne!(GetTokenInformation(token_handle, TokenUser, token_buf.as_mut_ptr() as *mut _, token_info_len, &mut token_info_len), 0);
            CloseHandle(token_handle);
            let token_user = &*(token_buf.as_ptr() as *const TOKEN_USER);
            let user_sid = token_user.User.Sid;

            let mut explicit_access: EXPLICIT_ACCESS_W = std::mem::zeroed();
            explicit_access.grfAccessPermissions = 0x1F01FF;
            explicit_access.grfAccessMode = SET_ACCESS;
            explicit_access.grfInheritance = 0;
            explicit_access.Trustee.pMultipleTrustee = std::ptr::null_mut();
            explicit_access.Trustee.MultipleTrusteeOperation = NO_MULTIPLE_TRUSTEE;
            explicit_access.Trustee.TrusteeForm = TRUSTEE_IS_SID;
            explicit_access.Trustee.TrusteeType = TRUSTEE_IS_USER;
            explicit_access.Trustee.ptstrName = user_sid as *mut u16;

            let mut new_acl: *mut ACL = std::ptr::null_mut();
            assert_eq!(SetEntriesInAclW(1, &explicit_access, std::ptr::null_mut(), &mut new_acl), 0);

            let wide_path: Vec<u16> = test_file.as_os_str().encode_wide().chain(Some(0)).collect();
            assert_eq!(
                SetNamedSecurityInfoW(
                    wide_path.as_ptr() as *mut u16,
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | UNPROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    new_acl,
                    std::ptr::null_mut(),
                ),
                0
            );
            LocalFree(new_acl as *mut std::ffi::c_void);
        }

        let verify_res = verify_permissions(&test_file);
        assert!(verify_res.is_err(), "Must reject unprotected/inheritable DACL");
        let err = verify_res.unwrap_err();
        assert!(err.contains("is not protected (inheritance not disabled)"), "Error was: {err}");
    }

    #[test]
    fn test_stage_completion_boundary_and_counters_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let manager = JournalManager::new(temp.path().join("runs"));

        let mut journal = sample_journal("run-stage-1", 1, RunRecoveryStatus::Active);
        journal.last_successful_state = None;
        journal.current_state = WorkflowState::BuildingContext;
        journal.iteration_counters = RunIterationCounters {
            plan_review_count: 2,
            fix_count: 3,
            code_review_count: 1,
            antigravity_dispatches: Some(4),
            antigravity_task_dispatches: std::collections::HashMap::new(),
            mailbox_epoch: Some(3),
        };

        manager.write_journal(&journal).unwrap();
        let loaded = manager.read_journal("run-stage-1").unwrap();
        assert_eq!(loaded.last_successful_state, None);
        assert_eq!(loaded.iteration_counters.plan_review_count, 2);
        assert_eq!(loaded.iteration_counters.fix_count, 3);
        assert_eq!(loaded.iteration_counters.code_review_count, 1);
        assert_eq!(loaded.iteration_counters.antigravity_dispatches, Some(4));

        // Advance successful boundary explicitly
        journal.revision = 2;
        journal.last_successful_state = Some(WorkflowState::PlanGeneration);
        journal.current_state = WorkflowState::Implementation;
        manager.write_journal(&journal).unwrap();

        let loaded2 = manager.read_journal("run-stage-1").unwrap();
        assert_eq!(loaded2.last_successful_state, Some(WorkflowState::PlanGeneration));
        assert_eq!(loaded2.current_state, WorkflowState::Implementation);

        // A failed stage does not clear or regress the last successful boundary
        journal.revision = 3;
        journal.current_state = WorkflowState::Failed;
        journal.status = RunRecoveryStatus::Failed;
        manager.write_journal(&journal).unwrap();

        let loaded3 = manager.read_journal("run-stage-1").unwrap();
        assert_eq!(loaded3.last_successful_state, Some(WorkflowState::PlanGeneration));
        assert_eq!(loaded3.status, RunRecoveryStatus::Failed);
    }
}
