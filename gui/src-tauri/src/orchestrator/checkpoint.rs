//! Durable, content-addressed workspace checkpoints for explicit run recovery.
//!
//! This module supports deterministic recursive repository graphs (root repository
//! plus all initialized nested submodules). It never changes HEAD or refs. Restore is
//! path-scoped and refuses ambiguous repository states before creating a backup or
//! changing the workspace.

use super::recovery::{
    apply_and_verify_permissions, CheckpointKind, CheckpointManifest, CheckpointPayloadEntry,
    RepositoryCheckpoint, SubmoduleCheckpoint, TrackedChangeRecord, UntrackedCheckpointEntry,
    CHECKPOINT_SCHEMA_VERSION,
};
use super::types::WorkflowState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use uuid::Uuid;

const MAX_FILES: usize = 100_000;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryPreflight {
    pub run_id: String,
    pub journal_revision: u64,
    pub checkpoint_id: String,
    pub checkpoint_kind: CheckpointKind,
    pub checkpoint_stage: WorkflowState,
    pub resume_stage: Option<WorkflowState>,
    pub checkpoint_digest: String,
    pub backup_id: String,
    pub backup_destination: String,
    pub current_fingerprint: String,
    pub workspace_matches: bool,
    pub can_resume: bool,
    pub can_restore: bool,
    pub can_adopt: bool,
    pub affected_paths: Vec<String>,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryChoice {
    Resume,
    ShelveAndRestore,
    Adopt,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryResolution {
    pub backup_id: Option<String>,
    pub backup_digest: Option<String>,
    pub checkpoint_id: Option<String>,
    pub resume_stage: Option<WorkflowState>,
    pub journal_revision: u64,
}

#[derive(Debug, Clone)]
struct SubmoduleInventory {
    checkpoint: SubmoduleCheckpoint,
    index_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct Inventory {
    repository: RepositoryCheckpoint,
    entries: Vec<CheckpointPayloadEntry>,
    submodules: Vec<SubmoduleInventory>,
    index_bytes: Vec<u8>,
}

struct NodeInventory {
    repository: RepositoryCheckpoint,
    entries: Vec<CheckpointPayloadEntry>,
    gitlinks: BTreeMap<String, String>,
    index_bytes: Vec<u8>,
}

pub struct CheckpointStore {
    runs_dir: PathBuf,
}

impl CheckpointStore {
    pub fn new(runs_dir: PathBuf) -> Self {
        Self { runs_dir }
    }

    pub fn sidecar_dir(&self, run_id: &str) -> Result<PathBuf, String> {
        super::recovery::validate_run_id(run_id)?;
        Ok(self.runs_dir.join(run_id))
    }

    pub fn capture(
        &self,
        run_id: &str,
        stage: WorkflowState,
        kind: CheckpointKind,
        project_path: &Path,
        task_prompt: Option<String>,
        stage_input: Option<String>,
    ) -> Result<(CheckpointManifest, String), String> {
        self.capture_with_id(
            run_id,
            stage,
            kind,
            project_path,
            task_prompt,
            stage_input,
            Uuid::new_v4().to_string(),
        )
    }

    fn capture_with_id(
        &self,
        run_id: &str,
        stage: WorkflowState,
        kind: CheckpointKind,
        project_path: &Path,
        task_prompt: Option<String>,
        stage_input: Option<String>,
        checkpoint_id: String,
    ) -> Result<(CheckpointManifest, String), String> {
        super::recovery::validate_run_id(run_id)?;
        Uuid::parse_str(&checkpoint_id).map_err(|_| "Invalid checkpoint ID".to_string())?;
        let root = canonical_repo_root(project_path)?;
        let inventory = inventory_repository(&root)?;
        let starting_fingerprint = recursive_fingerprint(&inventory)?;
        let checkpoint_dir = self
            .sidecar_dir(run_id)?
            .join(match kind {
                CheckpointKind::ShelveBackup => "backups",
                _ => "checkpoints",
            })
            .join(&checkpoint_id);
        ensure_new_checkpoint_target(&checkpoint_dir)?;
        secure_create_dir_all(&self.runs_dir, &checkpoint_dir)?;

        let mut total_bytes = 0u64;

        // 1. Capture root payload entries
        let mut root_entries = inventory.entries;
        for entry in &mut root_entries {
            if entry.kind == "file" {
                let path = safe_join(&root, &entry.path)?;
                let bytes = fs::read(&path).map_err(|e| {
                    format!("Cannot read inventoried file '{}': {e}", path.display())
                })?;
                if bytes.len() as u64 != entry.size
                    || entry.blob_hash.as_deref() != Some(sha256(&bytes).as_str())
                {
                    return Err(format!(
                        "File changed during checkpoint capture: {}",
                        entry.path
                    ));
                }
                total_bytes = total_bytes
                    .checked_add(bytes.len() as u64)
                    .ok_or("Checkpoint size overflow")?;
                if total_bytes > MAX_TOTAL_BYTES {
                    return Err("Checkpoint exceeds total payload limit".into());
                }
                entry.blob_hash = Some(write_blob(&self.runs_dir, &checkpoint_dir, &bytes)?);
            } else if entry.kind.starts_with("symlink") || entry.kind == "symlink" {
                let target = entry
                    .symlink_target
                    .as_ref()
                    .ok_or("Symlink target missing from inventory")?;
                entry.blob_hash = Some(write_blob(
                    &self.runs_dir,
                    &checkpoint_dir,
                    target.as_bytes(),
                )?);
            }
        }
        let index_blob_hash = write_blob(&self.runs_dir, &checkpoint_dir, &inventory.index_bytes)?;

        // 2. Capture recursive submodule payload entries and index blobs
        let mut submodules_checkpoints = Vec::with_capacity(inventory.submodules.len());
        for sub_inv in inventory.submodules {
            let mut sub_cp = sub_inv.checkpoint;
            let sub_path = PathBuf::from(&sub_cp.repository.canonical_root);
            for entry in &mut sub_cp.payload_entries {
                if entry.kind == "file" {
                    let path = safe_join(&sub_path, &entry.path)?;
                    let bytes = fs::read(&path).map_err(|e| {
                        format!(
                            "Cannot read submodule inventoried file '{}': {e}",
                            path.display()
                        )
                    })?;
                    if bytes.len() as u64 != entry.size
                        || entry.blob_hash.as_deref() != Some(sha256(&bytes).as_str())
                    {
                        return Err(format!(
                            "Submodule file changed during checkpoint capture: {}/{}",
                            sub_cp.node_key, entry.path
                        ));
                    }
                    total_bytes = total_bytes
                        .checked_add(bytes.len() as u64)
                        .ok_or("Checkpoint size overflow")?;
                    if total_bytes > MAX_TOTAL_BYTES {
                        return Err("Checkpoint exceeds total payload limit".into());
                    }
                    entry.blob_hash = Some(write_blob(&self.runs_dir, &checkpoint_dir, &bytes)?);
                } else if entry.kind.starts_with("symlink") || entry.kind == "symlink" {
                    let target = entry
                        .symlink_target
                        .as_ref()
                        .ok_or("Submodule symlink target missing from inventory")?;
                    entry.blob_hash = Some(write_blob(
                        &self.runs_dir,
                        &checkpoint_dir,
                        target.as_bytes(),
                    )?);
                }
            }
            sub_cp.index_blob_hash =
                write_blob(&self.runs_dir, &checkpoint_dir, &sub_inv.index_bytes)?;
            submodules_checkpoints.push(sub_cp);
        }

        // The workspace may be edited while payloads are copied. Re-inventory
        // after the last read and refuse to publish a manifest for a mixed-time
        // snapshot. Orphaned content-addressed blobs are harmless and retained.
        let after_capture = inventory_repository(&root)?;
        let ending_fingerprint = recursive_fingerprint(&after_capture)?;
        if ending_fingerprint != starting_fingerprint {
            return Err(
                "Workspace changed during checkpoint capture; manifest was not published".into(),
            );
        }

        let mut manifest = CheckpointManifest {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            checkpoint_id: checkpoint_id.clone(),
            run_id: run_id.to_string(),
            stage,
            kind,
            created_at_unix: now_unix()?,
            repository: inventory.repository,
            payload_entries: root_entries,
            index_blob_hash,
            task_prompt,
            stage_input,
            submodules: submodules_checkpoints,
            manifest_digest: String::new(),
        };
        manifest.manifest_digest = manifest_digest(&manifest)?;
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("Serialize checkpoint manifest: {e}"))?;
        write_protected_atomic(
            &self.runs_dir,
            &checkpoint_dir.join("manifest.json"),
            &manifest_bytes,
        )?;
        let reference = format!(
            "{}/{}/manifest.json",
            if kind == CheckpointKind::ShelveBackup {
                "backups"
            } else {
                "checkpoints"
            },
            checkpoint_id
        );
        let verified = self.load(run_id, &reference, &manifest.manifest_digest)?;
        if verified != manifest {
            return Err("Checkpoint manifest changed during post-write verification".into());
        }
        Ok((manifest, reference))
    }

    pub fn load(
        &self,
        run_id: &str,
        reference: &str,
        expected_digest: &str,
    ) -> Result<CheckpointManifest, String> {
        super::recovery::validate_run_id(run_id)?;
        let relative = validate_manifest_ref(reference)?;
        let sidecar = self.sidecar_dir(run_id)?;
        verify_existing_dir(&self.runs_dir)?;
        verify_existing_dir(&sidecar)?;
        let path = sidecar.join(&relative);
        ensure_no_symlink_components(&sidecar, &path)?;
        let bytes = fs::read(&path).map_err(|e| format!("Read checkpoint manifest: {e}"))?;
        let manifest: CheckpointManifest = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid checkpoint manifest: {e}"))?;
        if (manifest.schema_version != CHECKPOINT_SCHEMA_VERSION && manifest.schema_version != 2)
            || manifest.run_id != run_id
        {
            return Err("Checkpoint schema or run ID mismatch".into());
        }
        let actual_digest = manifest_digest(&manifest)?;
        if actual_digest != manifest.manifest_digest || actual_digest != expected_digest {
            return Err("Checkpoint manifest digest mismatch".into());
        }
        let checkpoint_dir = path.parent().ok_or("Manifest has no parent directory")?;
        verify_blob(&self.runs_dir, checkpoint_dir, &manifest.index_blob_hash)?;
        for entry in &manifest.payload_entries {
            if let Some(hash) = &entry.blob_hash {
                let bytes = read_blob(&self.runs_dir, checkpoint_dir, hash)?;
                if bytes.len() as u64 != entry.size {
                    return Err(format!(
                        "Checkpoint payload length mismatch: {}",
                        entry.path
                    ));
                }
            } else if entry.kind == "file"
                || entry.kind.starts_with("symlink")
                || entry.kind == "symlink"
            {
                return Err(format!(
                    "Checkpoint payload reference missing for {}",
                    entry.path
                ));
            }
        }
        for sub in &manifest.submodules {
            if !sub.index_blob_hash.is_empty() {
                verify_blob(&self.runs_dir, checkpoint_dir, &sub.index_blob_hash)?;
            }
            for entry in &sub.payload_entries {
                if let Some(hash) = &entry.blob_hash {
                    let bytes = read_blob(&self.runs_dir, checkpoint_dir, hash)?;
                    if bytes.len() as u64 != entry.size {
                        return Err(format!(
                            "Submodule checkpoint payload length mismatch: {}/{}",
                            sub.node_key, entry.path
                        ));
                    }
                } else if entry.kind == "file"
                    || entry.kind.starts_with("symlink")
                    || entry.kind == "symlink"
                {
                    return Err(format!(
                        "Submodule checkpoint payload reference missing for {}/{}",
                        sub.node_key, entry.path
                    ));
                }
            }
        }
        Ok(manifest)
    }

    pub fn preflight(
        &self,
        run_id: &str,
        journal_revision: u64,
        reference: &str,
        expected_digest: &str,
        project_path: &Path,
    ) -> Result<RecoveryPreflight, String> {
        let manifest = self.load(run_id, reference, expected_digest)?;
        let root = canonical_repo_root(project_path)?;
        let current = inventory_repository(&root)?;
        let current_fingerprint = recursive_fingerprint(&current)?;

        let saved_root_index = read_blob_for_manifest(
            &self.runs_dir,
            run_id,
            reference,
            &manifest.index_blob_hash,
        )?;
        let mut saved_sub_indices = BTreeMap::new();
        for sub in &manifest.submodules {
            if !sub.index_blob_hash.is_empty() {
                let sub_idx_bytes = read_blob_for_manifest(
                    &self.runs_dir,
                    run_id,
                    reference,
                    &sub.index_blob_hash,
                )?;
                saved_sub_indices.insert(sub.node_key.clone(), sub_idx_bytes);
            }
        }

        let saved_fingerprint = manifest_fingerprint(&manifest)?;
        let workspace_matches = current_fingerprint == saved_fingerprint;

        // Collect recursive affected paths
        let mut affected_paths = Vec::new();

        // 1. Root diffs
        let root_delta = path_delta(&manifest.payload_entries, &current.entries);
        affected_paths.extend(root_delta);
        if sha256(&current.index_bytes) != sha256(&saved_root_index) {
            affected_paths.push(".git/index (staged state)".to_string());
        }
        if current.repository.head_oid != manifest.repository.head_oid {
            affected_paths.push("HEAD".to_string());
        }
        if current.repository.refs_snapshot != manifest.repository.refs_snapshot {
            affected_paths.push("refs".to_string());
        }

        // 2. Submodule diffs
        let current_sub_map: BTreeMap<&str, &SubmoduleInventory> = current
            .submodules
            .iter()
            .map(|s| (s.checkpoint.node_key.as_str(), s))
            .collect();
        let manifest_sub_map: BTreeMap<&str, &SubmoduleCheckpoint> = manifest
            .submodules
            .iter()
            .map(|s| (s.node_key.as_str(), s))
            .collect();
        let all_sub_keys: BTreeSet<&str> = current_sub_map
            .keys()
            .chain(manifest_sub_map.keys())
            .copied()
            .collect();

        for sub_key in all_sub_keys {
            let prefix = node_prefix_display(sub_key);
            match (manifest_sub_map.get(sub_key), current_sub_map.get(sub_key)) {
                (Some(m_sub), Some(c_sub)) => {
                    let sub_delta =
                        path_delta(&m_sub.payload_entries, &c_sub.checkpoint.payload_entries);
                    for path in sub_delta {
                        affected_paths.push(format!("{prefix}{path}"));
                    }
                    let saved_idx = saved_sub_indices
                        .get(sub_key)
                        .map(|v| v.as_slice())
                        .unwrap_or(b"");
                    if sha256(&c_sub.index_bytes) != sha256(saved_idx) {
                        affected_paths.push(format!("{prefix}.git/index (staged state)"));
                    }
                    if c_sub.checkpoint.head_oid != m_sub.head_oid {
                        affected_paths.push(format!("{prefix}HEAD"));
                    }
                    if c_sub.checkpoint.repository.refs_snapshot != m_sub.repository.refs_snapshot {
                        affected_paths.push(format!("{prefix}refs"));
                    }
                    if c_sub.checkpoint.gitlink_oid != m_sub.gitlink_oid {
                        affected_paths.push(format!("{prefix}(gitlink)"));
                    }
                }
                (Some(_), None) => {
                    affected_paths.push(format!("{sub_key} (missing submodule)"));
                }
                (None, Some(_)) => {
                    affected_paths.push(format!("{sub_key} (untracked submodule)"));
                }
                (None, None) => {}
            }
        }

        affected_paths.sort();
        affected_paths.dedup();

        // Check restore preconditions: exact submodule structure + HEAD/refs match
        let mut exact_supported = true;
        let mut reason_code = None;

        if manifest.schema_version < 3 && !manifest.submodules.is_empty() && !workspace_matches {
            exact_supported = false;
            reason_code = Some("unsupported_manifest_schema".into());
        } else if !same_repository_identity(&current.repository, &manifest.repository) {
            exact_supported = false;
            reason_code = Some("repository_identity_changed".into());
        } else if manifest.submodules.len() != current.submodules.len() {
            exact_supported = false;
            reason_code = Some("unsupported_submodule_state".into());
        } else {
            for m_sub in &manifest.submodules {
                let Some(c_sub) = current_sub_map.get(m_sub.node_key.as_str()) else {
                    exact_supported = false;
                    reason_code = Some("unsupported_submodule_state".into());
                    break;
                };
                if m_sub.rel_path != c_sub.checkpoint.rel_path
                    || m_sub.head_oid != c_sub.checkpoint.head_oid
                    || m_sub.repository.head_oid != c_sub.checkpoint.repository.head_oid
                    || m_sub.repository.refs_snapshot != c_sub.checkpoint.repository.refs_snapshot
                {
                    exact_supported = false;
                    reason_code = Some("unsupported_submodule_state".into());
                    break;
                }
            }
        }

        let backup_id = backup_candidate_id(run_id, journal_revision, &current_fingerprint);
        let backup_destination = self
            .runs_dir
            .join(run_id)
            .join("backups")
            .join(&backup_id)
            .to_string_lossy()
            .into_owned();

        Ok(RecoveryPreflight {
            run_id: run_id.to_string(),
            journal_revision,
            checkpoint_id: manifest.checkpoint_id.clone(),
            checkpoint_kind: manifest.kind,
            checkpoint_stage: manifest.stage,
            checkpoint_digest: manifest.manifest_digest.clone(),
            backup_id,
            backup_destination,
            current_fingerprint,
            workspace_matches,
            can_resume: false,
            resume_stage: None,
            can_restore: false,
            can_adopt: false,
            affected_paths,
            reason_code: if !exact_supported {
                reason_code
            } else {
                Some("resume_route_unavailable".into())
            },
        })
    }

    pub fn resolve_restore(
        &self,
        run_id: &str,
        journal_revision: u64,
        reference: &str,
        expected_digest: &str,
        expected_current_fingerprint: &str,
        backup_id: &str,
        project_path: &Path,
    ) -> Result<RecoveryResolution, String> {
        Uuid::parse_str(backup_id).map_err(|_| "Invalid Shelve Backup ID".to_string())?;
        let expected_candidate =
            backup_candidate_id(run_id, journal_revision, expected_current_fingerprint);
        if backup_id != expected_candidate {
            return Err(
                "Shelve Backup destination no longer matches this recovery preflight".into(),
            );
        }
        let target = self.load(run_id, reference, expected_digest)?;
        let root = canonical_repo_root(project_path)?;
        let before = inventory_repository(&root)?;
        let before_fingerprint = recursive_fingerprint(&before)?;
        if before_fingerprint != expected_current_fingerprint {
            return Err(
                "Workspace changed after recovery preflight; reopen recovery details".into(),
            );
        }
        if !same_repository_identity(&before.repository, &target.repository) {
            return Err("HEAD or refs differ; checkpoint restore is unsupported".into());
        }
        if target.submodules.len() != before.submodules.len() {
            return Err(
                "Submodule state changed; recursive submodule restore is deferred and no workspace mutation was performed".into(),
            );
        }
        let current_sub_map: BTreeMap<&str, &SubmoduleInventory> = before
            .submodules
            .iter()
            .map(|s| (s.checkpoint.node_key.as_str(), s))
            .collect();
        for m_sub in &target.submodules {
            let Some(c_sub) = current_sub_map.get(m_sub.node_key.as_str()) else {
                return Err(
                    "Submodule state changed; recursive submodule restore is deferred and no workspace mutation was performed".into(),
                );
            };
            if m_sub.rel_path != c_sub.checkpoint.rel_path
                || m_sub.head_oid != c_sub.checkpoint.head_oid
                || m_sub.repository.head_oid != c_sub.checkpoint.repository.head_oid
                || m_sub.repository.refs_snapshot != c_sub.checkpoint.repository.refs_snapshot
            {
                return Err(
                    "Submodule HEAD or refs differ; checkpoint restore is unsupported".into(),
                );
            }
        }

        // Create and verify complete recursive Shelve Backup
        let (backup, backup_ref) = self.capture_with_id(
            run_id,
            target.stage,
            CheckpointKind::ShelveBackup,
            &root,
            None,
            None,
            backup_id.to_string(),
        )?;
        let backup_verified = self.load(run_id, &backup_ref, &backup.manifest_digest)?;
        if backup_verified.checkpoint_id != backup.checkpoint_id {
            return Err("Shelve backup verification mismatch; restore was not started".into());
        }

        // Re-open and verify again immediately before first mutation.
        let latest = inventory_repository(&root)?;
        if recursive_fingerprint(&latest)? != expected_current_fingerprint {
            return Err(
                "Workspace changed while Shelve Backup was created; restore was not started".into(),
            );
        }

        validate_restore_plan(&self.runs_dir, &root, &target, &latest)?;
        restore_inventory(&self.runs_dir, &root, &target, &latest).map_err(|e| {
            format!("Partial restore failed; verified Shelve Backup retained at {backup_ref}: {e}")
        })?;

        let after = inventory_repository(&root)?;
        let restored = recursive_fingerprint(&after)?;
        let expected = manifest_fingerprint(&target)?;
        if restored != expected {
            return Err(format!(
                "Partial restore: post-restore fingerprint mismatch; verified Shelve Backup retained at {backup_ref}"
            ));
        }

        Ok(RecoveryResolution {
            backup_id: Some(backup.checkpoint_id),
            backup_digest: Some(backup.manifest_digest),
            checkpoint_id: Some(target.checkpoint_id),
            resume_stage: Some(target.stage),
            journal_revision,
        })
    }
}

fn inventory_node(root: &Path) -> Result<NodeInventory, String> {
    let head = git(root, &["rev-parse", "HEAD"])?;
    let head_oid = String::from_utf8(head.stdout)
        .map_err(|e| format!("HEAD is not UTF-8: {e}"))?
        .trim()
        .to_string();
    if head_oid.is_empty() {
        return Err("Git returned an empty HEAD".into());
    }
    let refs = git(root, &["for-each-ref", "--format=%(refname) %(objectname)"])?;
    let status = git(
        root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=all",
        ],
    )?;
    let index_tree = git(root, &["write-tree"])?;
    let index_tree = String::from_utf8(index_tree.stdout)
        .map_err(|e| format!("Index tree is not UTF-8: {e}"))?
        .trim()
        .to_string();
    // Read the exact index after Git's semantic inspections, which may update
    // non-semantic stat/cache extensions. Optional Git lock writes are disabled
    // in `git()`; the enclosing capture/preflight performs its own stale-state
    // fingerprint check before any action is authorized.
    let index_path_out = git(root, &["rev-parse", "--git-path", "index"])?;
    let index_path_text = String::from_utf8(index_path_out.stdout)
        .map_err(|e| format!("Git index path is not UTF-8: {e}"))?;
    let index_path = git_path_to_native(root, &index_path_text);
    let index_bytes = fs::read(&index_path)
        .map_err(|e| format!("Cannot read Git index '{}': {e}", index_path.display()))?;
    let staged = git(
        root,
        &[
            "diff",
            "--cached",
            "--binary",
            "--no-ext-diff",
            "--ignore-submodules=all",
            "HEAD",
        ],
    )?;
    let unstaged = git(
        root,
        &[
            "diff",
            "--binary",
            "--no-ext-diff",
            "--ignore-submodules=all",
        ],
    )?;
    let untracked = git(root, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    let tracked_modes = git(root, &["ls-files", "--stage", "-z"])?;
    let mut modes = BTreeMap::<String, u32>::new();
    let mut gitlinks = BTreeMap::<String, String>::new();
    for record in tracked_modes
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
    {
        let tab = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("Malformed git ls-files --stage output")?;
        let (header, rest) = record.split_at(tab);
        let path = &rest[1..];
        let header =
            std::str::from_utf8(header).map_err(|e| format!("Malformed index entry: {e}"))?;
        let mode = u32::from_str_radix(
            header
                .split_whitespace()
                .next()
                .ok_or("Missing index mode")?,
            8,
        )
        .map_err(|e| format!("Invalid index mode: {e}"))?;
        let path = path_to_string(path)?;
        validate_rel_path(&path)?;
        if mode == 0o160000 {
            let oid = header
                .split_whitespace()
                .nth(1)
                .ok_or("Missing gitlink object ID")?
                .to_string();
            gitlinks.insert(path.clone(), oid);
        }
        modes.insert(path, mode);
    }
    let mut paths: BTreeSet<String> = modes.keys().cloned().collect();
    for raw in untracked
        .stdout
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
    {
        let path = path_to_string(raw)?;
        validate_rel_path(&path)?;
        paths.insert(path);
    }
    if paths.len() > MAX_FILES {
        return Err("Checkpoint file count exceeds limit".into());
    }
    let mut entries = Vec::with_capacity(paths.len());
    let mut tracked_changes = Vec::new();
    let mut untracked_files = Vec::new();
    for rel in paths {
        if gitlinks.contains_key(&rel) {
            entries.push(CheckpointPayloadEntry {
                path: rel,
                kind: "submodule".into(),
                mode: 0o160000,
                size: 0,
                blob_hash: None,
                symlink_target: None,
            });
            continue;
        }
        let path = safe_join(root, &rel)?;
        let indexed_mode = modes.get(&rel).copied();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                entries.push(CheckpointPayloadEntry {
                    path: rel.clone(),
                    kind: "missing".into(),
                    mode: indexed_mode.unwrap_or(0),
                    size: 0,
                    blob_hash: None,
                    symlink_target: None,
                });
                tracked_changes.push(TrackedChangeRecord {
                    path: rel,
                    change_type: super::recovery::TrackedChangeType::Deleted,
                    old_path: None,
                    content_hash: None,
                    mode: indexed_mode,
                });
                continue;
            }
            Err(e) => {
                return Err(format!(
                    "Cannot inspect checkpoint path '{}': {e}",
                    path.display()
                ))
            }
        };
        let mode = filesystem_mode(&metadata);
        let (kind, size, target, bytes) = if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)
                .map_err(|e| format!("Cannot read symlink '{}': {e}", path.display()))?;
            let target = target
                .to_str()
                .ok_or_else(|| format!("Non-UTF8 symlink target is unsupported: {}", rel))?
                .to_string();
            let kind = symlink_kind(&path)?;
            let bytes = target.as_bytes().to_vec();
            (kind, bytes.len() as u64, Some(target), bytes)
        } else if metadata.is_file() {
            if metadata.len() > MAX_FILE_BYTES {
                return Err(format!("Checkpoint file exceeds per-file limit: {rel}"));
            }
            let bytes = fs::read(&path)
                .map_err(|e| format!("Cannot read checkpoint file '{}': {e}", path.display()))?;
            ("file".to_string(), bytes.len() as u64, None, bytes)
        } else {
            return Err(format!("Unsupported checkpoint object type: {rel}"));
        };
        let hash = sha256(&bytes);
        entries.push(CheckpointPayloadEntry {
            path: rel.clone(),
            kind: kind.clone(),
            mode,
            size,
            blob_hash: Some(hash.clone()),
            symlink_target: target.clone(),
        });
        if indexed_mode.is_some() {
            tracked_changes.push(TrackedChangeRecord {
                path: rel.clone(),
                change_type: super::recovery::TrackedChangeType::Modified,
                old_path: None,
                content_hash: Some(hash),
                mode: Some(mode),
            });
        } else {
            untracked_files.push(UntrackedCheckpointEntry {
                rel_path: rel,
                file_type: kind,
                content_hash: hash,
                mode,
                symlink_target: target,
            });
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let diff_index = git(
        root,
        &[
            "diff-index",
            "-p",
            "--binary",
            "--ignore-submodules=all",
            "HEAD",
        ],
    )?;
    let refs_snapshot =
        String::from_utf8(refs.stdout).map_err(|e| format!("Refs output is not UTF-8: {e}"))?;
    let mut repository = RepositoryCheckpoint {
        canonical_root: root.to_string_lossy().to_string(),
        head_oid,
        refs_snapshot,
        status_inventory: status.stdout,
        tracked_changes,
        untracked_files,
        integrity_hash: String::new(),
    };
    repository.integrity_hash = repository_integrity(
        &repository,
        &entries,
        &index_tree,
        &staged.stdout,
        &unstaged.stdout,
    )?;
    repository
        .status_inventory
        .extend_from_slice(&staged.stdout);
    repository.status_inventory.push(0);
    repository
        .status_inventory
        .extend_from_slice(&unstaged.stdout);
    repository.status_inventory.push(0);
    repository
        .status_inventory
        .extend_from_slice(index_tree.as_bytes());
    repository.status_inventory.push(0);
    repository
        .status_inventory
        .extend_from_slice(&diff_index.stdout);

    Ok(NodeInventory {
        repository,
        entries,
        gitlinks,
        index_bytes,
    })
}

fn collect_submodules_recursive(
    parent: &Path,
    parent_node_key: &str,
    gitlinks: &BTreeMap<String, String>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<Vec<SubmoduleInventory>, String> {
    let mut result = Vec::new();
    for (relative, gitlink_oid) in gitlinks {
        validate_rel_path(relative)?;
        let child_path = safe_join(parent, relative)?;
        let metadata = fs::symlink_metadata(&child_path)
            .map_err(|e| format!("Inspect submodule '{}': {e}", relative))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(format!(
                "Submodule is uninitialized or redirected: {relative}"
            ));
        }
        let child = fs::canonicalize(&child_path)
            .map_err(|e| format!("Canonicalize submodule '{}': {e}", relative))?;
        if !child.starts_with(parent) {
            return Err(format!(
                "Submodule path escapes parent repository: {relative}"
            ));
        }
        if !visited.insert(child.clone()) {
            return Err(format!(
                "Cycle detected in submodule hierarchy at: {relative}"
            ));
        }

        let prefix_out = git(&child, &["rev-parse", "--show-prefix"])?;
        if !String::from_utf8_lossy(&prefix_out.stdout).trim().is_empty() {
            return Err(format!("Submodule path is not a repository root: {relative}"));
        }

        let child_node = inventory_node(&child)?;
        let child_head_oid = child_node.repository.head_oid.clone();

        let child_node_key = if parent_node_key.is_empty() || parent_node_key == "." {
            relative.clone()
        } else {
            format!("{parent_node_key}::{relative}")
        };

        let sub_cp = SubmoduleCheckpoint {
            node_key: child_node_key.clone(),
            immediate_parent_key: if parent_node_key.is_empty() {
                ".".to_string()
            } else {
                parent_node_key.to_string()
            },
            immediate_parent_path: parent.to_string_lossy().to_string(),
            rel_path: relative.clone(),
            gitlink_oid: gitlink_oid.clone(),
            head_oid: child_head_oid,
            repository: child_node.repository,
            payload_entries: child_node.entries,
            index_blob_hash: String::new(),
        };

        let nested = collect_submodules_recursive(
            &child,
            &child_node_key,
            &child_node.gitlinks,
            visited,
        )?;

        result.push(SubmoduleInventory {
            checkpoint: sub_cp,
            index_bytes: child_node.index_bytes,
        });
        result.extend(nested);
    }

    result.sort_by(|a, b| a.checkpoint.node_key.cmp(&b.checkpoint.node_key));
    Ok(result)
}

fn inventory_repository(root: &Path) -> Result<Inventory, String> {
    let canonical = canonical_repo_root(root)?;
    let root_node = inventory_node(&canonical)?;
    let mut visited = BTreeSet::new();
    visited.insert(canonical.clone());
    let submodules =
        collect_submodules_recursive(&canonical, ".", &root_node.gitlinks, &mut visited)?;
    Ok(Inventory {
        repository: root_node.repository,
        entries: root_node.entries,
        submodules,
        index_bytes: root_node.index_bytes,
    })
}

fn node_display_ancestry(node_key: &str) -> String {
    node_key.replace("::", " / ")
}

fn node_prefix_display(node_key: &str) -> String {
    if node_key.is_empty() || node_key == "." {
        String::new()
    } else {
        let display = node_display_ancestry(node_key);
        format!("{display} / ")
    }
}

fn restore_single_repository_payload(
    runs_dir: &Path,
    checkpoint_dir: &Path,
    repo_root: &Path,
    target_entries: &[CheckpointPayloadEntry],
    current_entries: &[CheckpointPayloadEntry],
    target_index_blob_hash: &str,
) -> Result<(), String> {
    let target_map: BTreeMap<&str, &CheckpointPayloadEntry> =
        target_entries.iter().map(|e| (e.path.as_str(), e)).collect();
    let current_map: BTreeMap<&str, &CheckpointPayloadEntry> = current_entries
        .iter()
        .map(|e| (e.path.as_str(), e))
        .collect();
    let all: BTreeSet<&str> = target_map
        .keys()
        .chain(current_map.keys())
        .copied()
        .collect();

    for rel in all {
        validate_rel_path(rel)?;
        if target_map.get(rel).is_some_and(|e| e.kind == "submodule")
            && current_map.get(rel).is_some_and(|e| e.kind == "submodule")
        {
            continue;
        }
        if let (Some(t), Some(c)) = (target_map.get(rel), current_map.get(rel)) {
            if t == c {
                continue;
            }
        }
        let target_entry = target_map.get(rel).copied();
        match target_entry {
            None => {
                let path = safe_join(repo_root, rel)?;
                if let Some(parent) = path.parent() {
                    ensure_no_symlink_components(repo_root, parent)?;
                }
                remove_checkpoint_leaf(&path, rel)?;
            }
            Some(entry) if entry.kind == "missing" => {
                let path = safe_join(repo_root, rel)?;
                if let Some(parent) = path.parent() {
                    ensure_no_symlink_components(repo_root, parent)?;
                }
                remove_checkpoint_leaf(&path, rel)?;
            }
            Some(entry) => {
                let path = safe_join(repo_root, rel)?;
                ensure_safe_parent_dirs(repo_root, &path)?;
                let Some(hash) = &entry.blob_hash else {
                    return Err(format!("Checkpoint payload missing for {rel}"));
                };
                let bytes = read_blob(runs_dir, checkpoint_dir, hash)?;
                if bytes.len() as u64 != entry.size {
                    return Err(format!("Checkpoint payload length mismatch: {rel}"));
                }
                if entry.kind == "file" {
                    replace_regular_file(&path, &bytes, entry.mode)?;
                } else if entry.kind.starts_with("symlink") || entry.kind == "symlink" {
                    let target = entry
                        .symlink_target
                        .as_deref()
                        .ok_or("Checkpoint symlink target missing")?;
                    create_exact_symlink(&path, target, &entry.kind)?;
                } else {
                    return Err(format!(
                        "Unsupported restore kind '{}' for '{}'; aborting",
                        entry.kind, rel
                    ));
                }
            }
        }
    }

    let index_bytes = read_blob(runs_dir, checkpoint_dir, target_index_blob_hash)?;
    let index_out = git(repo_root, &["rev-parse", "--git-path", "index"])?;
    let index_text =
        String::from_utf8(index_out.stdout).map_err(|e| format!("Index path is not UTF-8: {e}"))?;
    let index = git_path_to_native(repo_root, &index_text);
    replace_regular_file(&index, &index_bytes, 0o600)?;
    Ok(())
}

fn restore_inventory(
    runs_dir: &Path,
    root: &Path,
    target: &CheckpointManifest,
    current: &Inventory,
) -> Result<(), String> {
    if !same_repository_identity(&current.repository, &target.repository) {
        return Err("Repository identity changed before restore".into());
    }

    let checkpoint_dir = runs_dir
        .join(&target.run_id)
        .join("checkpoints")
        .join(&target.checkpoint_id);

    let current_sub_map: BTreeMap<&str, &SubmoduleInventory> = current
        .submodules
        .iter()
        .map(|s| (s.checkpoint.node_key.as_str(), s))
        .collect();

    // 1. Restore child repositories first (bottom-up: nested children before parent)
    let mut sorted_submodules = target.submodules.clone();
    sorted_submodules.sort_by(|a, b| b.node_key.len().cmp(&a.node_key.len()));

    for sub_target in &sorted_submodules {
        let current_sub = current_sub_map
            .get(sub_target.node_key.as_str())
            .ok_or_else(|| {
                format!(
                    "Cannot find submodule '{}' in current workspace",
                    sub_target.node_key
                )
            })?;
        let sub_root = PathBuf::from(&current_sub.checkpoint.repository.canonical_root);
        restore_single_repository_payload(
            runs_dir,
            &checkpoint_dir,
            &sub_root,
            &sub_target.payload_entries,
            &current_sub.checkpoint.payload_entries,
            &sub_target.index_blob_hash,
        )?;
    }

    // 2. Restore root repository
    restore_single_repository_payload(
        runs_dir,
        &checkpoint_dir,
        root,
        &target.payload_entries,
        &current.entries,
        &target.index_blob_hash,
    )?;

    Ok(())
}

fn canonical_repo_root(path: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|e| format!("Resolve project root '{}': {e}", path.display()))?;
    let prefix = git(&canonical, &["rev-parse", "--show-prefix"])?;
    if !String::from_utf8_lossy(&prefix.stdout).trim().is_empty() {
        return Err(
            "Project path must be the canonical Git repository root, not a subdirectory".into(),
        );
    }
    Ok(canonical)
}

fn git(root: &Path, args: &[&str]) -> Result<Output, String> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(|e| format!("Failed to execute git {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed ({}): {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output)
}

fn write_blob(runs_dir: &Path, checkpoint_dir: &Path, bytes: &[u8]) -> Result<String, String> {
    let hash = sha256(bytes);
    let payload_dir = checkpoint_dir.join("payload");
    secure_create_dir_all(runs_dir, &payload_dir)?;
    let path = payload_dir.join(&hash);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if !metadata.file_type().is_file() {
            return Err("Existing payload path is not a regular file".into());
        }
        let existing = fs::read(&path).map_err(|e| format!("Read existing payload blob: {e}"))?;
        if sha256(&existing) != hash {
            return Err("Existing content-addressed payload is corrupt".into());
        }
        return Ok(hash);
    }
    write_protected_atomic(runs_dir, &path, bytes)?;
    Ok(hash)
}

fn read_blob_for_manifest(
    runs_dir: &Path,
    run_id: &str,
    reference: &str,
    hash: &str,
) -> Result<Vec<u8>, String> {
    let relative = validate_manifest_ref(reference)?;
    let path = runs_dir.join(run_id).join(relative);
    let dir = path.parent().ok_or("Invalid checkpoint payload path")?;
    read_blob(runs_dir, dir, hash)
}

fn read_blob(runs_dir: &Path, checkpoint_dir: &Path, hash: &str) -> Result<Vec<u8>, String> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid payload hash path".into());
    }
    let path = checkpoint_dir.join("payload").join(hash);
    ensure_no_symlink_components(runs_dir, &path)?;
    let bytes =
        fs::read(&path).map_err(|e| format!("Read payload blob '{}': {e}", path.display()))?;
    if sha256(&bytes) != hash {
        return Err(format!("Payload digest mismatch for {}", path.display()));
    }
    Ok(bytes)
}

fn verify_blob(runs_dir: &Path, checkpoint_dir: &Path, hash: &str) -> Result<(), String> {
    read_blob(runs_dir, checkpoint_dir, hash).map(|_| ())
}

fn secure_create_dir_all(runs_dir: &Path, dir: &Path) -> Result<(), String> {
    if !runs_dir.exists() {
        fs::create_dir_all(runs_dir).map_err(|e| {
            format!("Create recovery runs directory '{}': {e}", runs_dir.display())
        })?;
    }
    let relative = dir
        .strip_prefix(runs_dir)
        .map_err(|_| "Checkpoint storage escaped runs directory")?;
    let mut current = runs_dir.to_path_buf();
    verify_existing_dir(&current)?;
    for component in relative.components() {
        match component {
            Component::Normal(part) => current.push(part),
            _ => return Err("Invalid checkpoint directory component".into()),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(format!(
                    "Checkpoint directory is not a real directory: {}",
                    current.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(create_error) if create_error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(create_error) => return Err(format!(
                        "Create protected checkpoint directory '{}': {create_error}",
                        current.display()
                    )),
                }
            }
            Err(error) => return Err(format!(
                "Inspect checkpoint directory '{}': {error}",
                current.display()
            )),
        }
        let metadata = fs::symlink_metadata(&current)
            .map_err(|e| format!("Inspect checkpoint directory '{}': {e}", current.display()))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(format!(
                "Checkpoint directory is not a real directory: {}",
                current.display()
            ));
        }
        apply_and_verify_permissions(&current, true)?;
    }
    Ok(())
}

fn ensure_new_checkpoint_target(checkpoint_dir: &Path) -> Result<(), String> {
    let parent = checkpoint_dir
        .parent()
        .ok_or("Checkpoint destination has no parent directory")?;
    if parent.exists() {
        verify_existing_dir(parent)?;
    }
    match fs::symlink_metadata(checkpoint_dir) {
        Ok(_) => Err(format!(
            "Checkpoint destination already exists and is immutable: {}",
            checkpoint_dir.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Cannot inspect checkpoint destination '{}': {error}",
            checkpoint_dir.display()
        )),
    }
}

fn backup_candidate_id(run_id: &str, revision: u64, fingerprint: &str) -> String {
    let digest = sha256(format!("{run_id}\0{revision}\0{fingerprint}").as_bytes());
    format!(
        "{}-{}-{}-{}-{}",
        &digest[0..8],
        &digest[8..12],
        &digest[12..16],
        &digest[16..20],
        &digest[20..32]
    )
}

fn verify_existing_dir(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| {
        format!(
            "Inspect protected recovery directory '{}': {e}",
            path.display()
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!(
            "Protected recovery path is not a real directory: {}",
            path.display()
        ));
    }
    apply_and_verify_permissions(path, true)
}

fn validate_single_repository_restore_plan(
    runs_dir: &Path,
    checkpoint_dir: &Path,
    repo_root: &Path,
    target_entries: &[CheckpointPayloadEntry],
    current_entries: &[CheckpointPayloadEntry],
    target_index_blob_hash: &str,
) -> Result<(), String> {
    let current_map: BTreeMap<&str, &CheckpointPayloadEntry> =
        current_entries.iter().map(|e| (e.path.as_str(), e)).collect();
    let target_map: BTreeMap<&str, &CheckpointPayloadEntry> =
        target_entries.iter().map(|e| (e.path.as_str(), e)).collect();
    let all: BTreeSet<&str> = current_map
        .keys()
        .chain(target_map.keys())
        .copied()
        .collect();

    for rel in all {
        validate_rel_path(rel)?;
        if target_map.get(rel).is_some_and(|e| e.kind == "submodule")
            && current_map.get(rel).is_some_and(|e| e.kind == "submodule")
        {
            continue;
        }
        let path = safe_join(repo_root, rel)?;
        if let Some(parent) = path.parent() {
            ensure_no_symlink_components(repo_root, parent)?;
        }
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
                return Err(format!(
                    "Restore plan contains unsupported current object: {rel}"
                ));
            }
        }
        if let Some(entry) = target_map.get(rel) {
            if entry.kind == "submodule" {
                continue;
            }
            if entry.kind != "missing" {
                if !matches!(
                    entry.kind.as_str(),
                    "file" | "symlink" | "symlink_file" | "symlink_dir"
                ) {
                    return Err(format!(
                        "Restore plan contains unsupported target kind '{}' for {rel}",
                        entry.kind
                    ));
                }
                let hash = entry
                    .blob_hash
                    .as_deref()
                    .ok_or_else(|| format!("Restore payload missing for {rel}"))?;
                let bytes = read_blob(runs_dir, checkpoint_dir, hash)?;
                if bytes.len() as u64 != entry.size {
                    return Err(format!("Restore payload length mismatch for {rel}"));
                }
            }
        }
    }
    read_blob(runs_dir, checkpoint_dir, target_index_blob_hash)?;
    Ok(())
}

fn validate_restore_plan(
    runs_dir: &Path,
    root: &Path,
    target: &CheckpointManifest,
    current: &Inventory,
) -> Result<(), String> {
    let checkpoint_dir = runs_dir
        .join(&target.run_id)
        .join("checkpoints")
        .join(&target.checkpoint_id);

    validate_single_repository_restore_plan(
        runs_dir,
        &checkpoint_dir,
        root,
        &target.payload_entries,
        &current.entries,
        &target.index_blob_hash,
    )?;

    let current_sub_map: BTreeMap<&str, &SubmoduleInventory> = current
        .submodules
        .iter()
        .map(|s| (s.checkpoint.node_key.as_str(), s))
        .collect();

    for sub_target in &target.submodules {
        let current_sub = current_sub_map
            .get(sub_target.node_key.as_str())
            .ok_or_else(|| {
                format!(
                    "Cannot find submodule '{}' in current workspace",
                    sub_target.node_key
                )
            })?;
        let sub_root = PathBuf::from(&current_sub.checkpoint.repository.canonical_root);
        validate_single_repository_restore_plan(
            runs_dir,
            &checkpoint_dir,
            &sub_root,
            &sub_target.payload_entries,
            &current_sub.checkpoint.payload_entries,
            &sub_target.index_blob_hash,
        )?;
    }

    Ok(())
}

fn remove_checkpoint_leaf(path: &Path, rel: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            #[cfg(windows)]
            {
                let remove_result = match symlink_kind(path) {
                    Ok(kind) if kind == "symlink_dir" => fs::remove_dir(path),
                    _ => fs::remove_file(path),
                };
                remove_result
                    .map_err(|e| format!("Restore remove symlink '{}' failed: {e}", rel))?;
            }
            #[cfg(not(windows))]
            fs::remove_file(path)
                .map_err(|e| format!("Restore remove symlink '{}' failed: {e}", rel))?;
            Ok(())
        }
        Ok(meta) if meta.is_file() => {
            fs::remove_file(path).map_err(|e| format!("Restore remove '{}' failed: {e}", rel))
        }
        Ok(_) => Err(format!("Restore refuses to remove non-file path: {rel}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Restore inspect '{}' failed: {e}", rel)),
    }
}

fn write_protected_atomic(runs_dir: &Path, target: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = target.parent().ok_or("Checkpoint file has no parent")?;
    secure_create_dir_all(runs_dir, parent)?;
    let temp = parent.join(format!(".tmp-{}", Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("Create checkpoint temp file: {e}"))?;
    apply_and_verify_permissions(&temp, false)?;
    file.write_all(bytes)
        .map_err(|e| format!("Write checkpoint file: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("Sync checkpoint file: {e}"))?;
    drop(file);
    if target.exists() {
        fs::remove_file(target).map_err(|e| format!("Replace checkpoint target: {e}"))?;
    }
    fs::rename(&temp, target).map_err(|e| format!("Publish checkpoint file: {e}"))?;
    apply_and_verify_permissions(target, false)?;
    Ok(())
}

fn replace_regular_file(path: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Create parent directory: {e}"))?;
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            remove_checkpoint_leaf(path, &path.display().to_string())?;
        } else if meta.is_file() {
            fs::remove_file(path)
                .map_err(|e| format!("Remove existing file '{}': {e}", path.display()))?;
        } else {
            return Err(format!(
                "Refusing to replace non-regular file {}",
                path.display()
            ));
        }
    }
    let temp = path.with_file_name(format!(".ab-restore-{}", Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("Create restore temp file: {e}"))?;
    file.write_all(bytes)
        .map_err(|e| format!("Write restored file: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("Sync restored file: {e}"))?;
    drop(file);
    set_mode(&temp, mode)?;
    fs::rename(&temp, path).map_err(|e| format!("Publish restored file '{}': {e}", path.display()))
}

fn create_exact_symlink(path: &Path, target: &str, kind: &str) -> Result<(), String> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.file_type().is_symlink() {
            return Err(format!(
                "Refusing to replace non-symlink with link: {}",
                path.display()
            ));
        }
        remove_checkpoint_leaf(path, &path.display().to_string())?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, path).map_err(|e| format!("Create symlink: {e}"))?;
    #[cfg(windows)]
    match kind {
        "symlink_file" => std::os::windows::fs::symlink_file(target, path)
            .map_err(|e| format!("Create file symlink: {e}"))?,
        "symlink_dir" => std::os::windows::fs::symlink_dir(target, path)
            .map_err(|e| format!("Create directory symlink: {e}"))?,
        _ => return Err("Unknown Windows symlink kind".into()),
    }
    Ok(())
}

fn ensure_safe_parent_dirs(root: &Path, path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or("Restore path has no parent")?;
    let rel = parent
        .strip_prefix(root)
        .map_err(|_| "Restore parent escaped repository root")?;
    let mut current = root.to_path_buf();
    for component in rel.components() {
        let Component::Normal(part) = component else {
            return Err("Invalid restore parent path".into());
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(format!(
                    "Restore path traverses a non-directory/reparse point: {}",
                    current.display()
                ))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)
                .map_err(|e| format!("Create restore directory '{}': {e}", current.display()))?,
            Err(e) => {
                return Err(format!(
                    "Inspect restore directory '{}': {e}",
                    current.display()
                ))
            }
        }
    }
    Ok(())
}

fn ensure_no_symlink_components(root: &Path, path: &Path) -> Result<(), String> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| "Path escaped expected root")?;
    let mut current = root.to_path_buf();
    for component in rel.components() {
        let Component::Normal(part) = component else {
            return Err("Invalid relative path component".into());
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "Refusing symlink/reparse traversal: {}",
                    current.display()
                ))
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(format!("Inspect path '{}': {e}", current.display())),
        }
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    validate_rel_path(relative)?;
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
    }
    if !path.starts_with(root) {
        return Err("Relative path escaped root".into());
    }
    Ok(path)
}

fn validate_rel_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with('/')
        || path.contains(':')
    {
        return Err(format!("Unsupported or unsafe relative path: {path:?}"));
    }
    let parsed = Path::new(path);
    if parsed
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("Path traversal is forbidden: {path:?}"));
    }
    Ok(())
}

fn validate_manifest_ref(reference: &str) -> Result<PathBuf, String> {
    if reference.starts_with('/')
        || reference.contains('\\')
        || reference.contains(':')
        || reference.contains('\0')
    {
        return Err("Invalid checkpoint manifest reference".into());
    }
    let path = Path::new(reference);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
        || path.file_name().is_none_or(|n| n != "manifest.json")
    {
        return Err("Invalid checkpoint manifest reference".into());
    }
    Ok(path.to_path_buf())
}

fn path_to_string(path: &[u8]) -> Result<String, String> {
    String::from_utf8(path.to_vec()).map_err(|e| format!("Non-UTF8 Git path is unsupported: {e}"))
}

fn git_path_to_native(root: &Path, path_str: &str) -> PathBuf {
    let trimmed = path_str.trim();
    #[cfg(windows)]
    {
        let normalized = trimmed.replace('\\', "/");
        if normalized.starts_with('/') && normalized.len() >= 3 {
            let drive = normalized.chars().nth(1).unwrap();
            let sep = normalized.chars().nth(2).unwrap();
            if drive.is_ascii_alphabetic() && sep == '/' {
                return PathBuf::from(format!("{}:/{}", drive.to_ascii_uppercase(), &normalized[3..]));
            }
        }
    }
    let p = PathBuf::from(trimmed);
    if p.is_absolute() {
        p
    } else {
        root.join(p)
    }
}

fn filesystem_mode(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        if meta.permissions().readonly() {
            0o444
        } else {
            0o644
        }
    }
}

fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| format!("Set file mode: {e}"))
    }
    #[cfg(not(unix))]
    {
        let mut p = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
        p.set_readonly(mode & 0o200 == 0);
        fs::set_permissions(path, p).map_err(|e| format!("Set file mode: {e}"))
    }
}

fn symlink_kind(path: &Path) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path)
            .map_err(|e| format!("Cannot classify symlink kind '{}': {e}", path.display()))?;
        if !metadata.file_type().is_symlink() {
            return Err(format!("Path is not a symlink: {}", path.display()));
        }
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        if metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 {
            Ok("symlink_dir".into())
        } else {
            Ok("symlink_file".into())
        }
    }
    #[cfg(unix)]
    {
        let _ = path;
        Ok("symlink".into())
    }
}

fn now_unix() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| format!("System clock before epoch: {e}"))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn manifest_digest(manifest: &CheckpointManifest) -> Result<String, String> {
    if manifest.schema_version == 2 {
        return Ok(sha256(&serialize_legacy_v2_manifest(manifest, true)?));
    }
    let mut canonical = manifest.clone();
    canonical.manifest_digest.clear();
    let bytes =
        serde_json::to_vec(&canonical).map_err(|e| format!("Serialize canonical manifest: {e}"))?;
    Ok(sha256(&bytes))
}

/// Schema v2's SubmoduleCheckpoint predates recursive payload fields. Serialize
/// through the historical shape so loading a v2 manifest verifies its original
/// digest rather than a v3-expanded representation with default fields added.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacySubmoduleCheckpointV2<'a> {
    immediate_parent_path: &'a str,
    rel_path: &'a str,
    gitlink_oid: &'a str,
    head_oid: &'a str,
    repository: &'a RepositoryCheckpoint,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LegacyCheckpointManifestV2<'a> {
    schema_version: u32,
    checkpoint_id: &'a str,
    run_id: &'a str,
    stage: WorkflowState,
    kind: CheckpointKind,
    created_at_unix: u64,
    repository: &'a RepositoryCheckpoint,
    payload_entries: &'a [CheckpointPayloadEntry],
    index_blob_hash: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_prompt: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage_input: &'a Option<String>,
    submodules: Vec<LegacySubmoduleCheckpointV2<'a>>,
    manifest_digest: &'a str,
}

fn serialize_legacy_v2_manifest(
    manifest: &CheckpointManifest,
    clear_digest: bool,
) -> Result<Vec<u8>, String> {
    let legacy = LegacyCheckpointManifestV2 {
        schema_version: manifest.schema_version,
        checkpoint_id: &manifest.checkpoint_id,
        run_id: &manifest.run_id,
        stage: manifest.stage,
        kind: manifest.kind,
        created_at_unix: manifest.created_at_unix,
        repository: &manifest.repository,
        payload_entries: &manifest.payload_entries,
        index_blob_hash: &manifest.index_blob_hash,
        task_prompt: &manifest.task_prompt,
        stage_input: &manifest.stage_input,
        submodules: manifest
            .submodules
            .iter()
            .map(|sub| LegacySubmoduleCheckpointV2 {
                immediate_parent_path: &sub.immediate_parent_path,
                rel_path: &sub.rel_path,
                gitlink_oid: &sub.gitlink_oid,
                head_oid: &sub.head_oid,
                repository: &sub.repository,
            })
            .collect(),
        manifest_digest: if clear_digest {
            ""
        } else {
            &manifest.manifest_digest
        },
    };
    serde_json::to_vec(&legacy).map_err(|e| format!("Serialize legacy v2 manifest: {e}"))
}

fn repository_integrity(
    repo: &RepositoryCheckpoint,
    entries: &[CheckpointPayloadEntry],
    index_tree: &str,
    staged: &[u8],
    unstaged: &[u8],
) -> Result<String, String> {
    let mut bytes = serde_json::to_vec(&(
        repo.canonical_root.as_str(),
        repo.head_oid.as_str(),
        repo.refs_snapshot.as_str(),
        &repo.status_inventory,
        entries,
        index_tree,
    ))
    .map_err(|e| e.to_string())?;
    bytes.extend_from_slice(staged);
    bytes.push(0);
    bytes.extend_from_slice(unstaged);
    Ok(sha256(&bytes))
}

fn recursive_fingerprint(inv: &Inventory) -> Result<String, String> {
    let mut stable_subs = Vec::with_capacity(inv.submodules.len());
    for sub in &inv.submodules {
        stable_subs.push((
            sub.checkpoint.node_key.as_str(),
            sub.checkpoint.immediate_parent_path.as_str(),
            sub.checkpoint.rel_path.as_str(),
            sub.checkpoint.gitlink_oid.as_str(),
            sub.checkpoint.head_oid.as_str(),
            sub.checkpoint.repository.refs_snapshot.as_str(),
            &sub.checkpoint.repository.status_inventory,
            &sub.checkpoint.payload_entries,
            sha256(&sub.index_bytes),
        ));
    }
    let stable = serde_json::to_vec(&(
        inv.repository.canonical_root.as_str(),
        inv.repository.head_oid.as_str(),
        inv.repository.refs_snapshot.as_str(),
        &inv.repository.status_inventory,
        &inv.entries,
        sha256(&inv.index_bytes),
        stable_subs,
    ))
    .map_err(|e| e.to_string())?;
    Ok(sha256(&stable))
}

fn manifest_fingerprint(manifest: &CheckpointManifest) -> Result<String, String> {
    let mut stable_subs = Vec::with_capacity(manifest.submodules.len());
    for sub in &manifest.submodules {
        stable_subs.push((
            sub.node_key.as_str(),
            sub.immediate_parent_path.as_str(),
            sub.rel_path.as_str(),
            sub.gitlink_oid.as_str(),
            sub.head_oid.as_str(),
            sub.repository.refs_snapshot.as_str(),
            &sub.repository.status_inventory,
            &sub.payload_entries,
            sub.index_blob_hash.as_str(),
        ));
    }
    let stable = serde_json::to_vec(&(
        manifest.repository.canonical_root.as_str(),
        manifest.repository.head_oid.as_str(),
        manifest.repository.refs_snapshot.as_str(),
        &manifest.repository.status_inventory,
        &manifest.payload_entries,
        manifest.index_blob_hash.as_str(),
        stable_subs,
    ))
    .map_err(|e| e.to_string())?;
    Ok(sha256(&stable))
}

fn same_repository_identity(a: &RepositoryCheckpoint, b: &RepositoryCheckpoint) -> bool {
    a.canonical_root == b.canonical_root
        && a.head_oid == b.head_oid
        && a.refs_snapshot == b.refs_snapshot
}

fn path_delta(a: &[CheckpointPayloadEntry], b: &[CheckpointPayloadEntry]) -> Vec<String> {
    let am: BTreeMap<&str, &CheckpointPayloadEntry> =
        a.iter().map(|e| (e.path.as_str(), e)).collect();
    let bm: BTreeMap<&str, &CheckpointPayloadEntry> =
        b.iter().map(|e| (e.path.as_str(), e)).collect();
    am.keys()
        .chain(bm.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|p| am.get(p) != bm.get(p))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn repo() -> (TempDir, PathBuf) {
        let t = TempDir::new().unwrap();
        let root = t.path().join("repo");
        fs::create_dir_all(&root).unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        for (k, v) in [("user.name", "Test"), ("user.email", "test@test.local")] {
            assert!(Command::new("git")
                .args(["config", k, v])
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        fs::write(root.join("base.txt"), b"initial").unwrap();
        assert!(Command::new("git")
            .args(["add", "base.txt"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "base"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        (t, root)
    }

    fn init_submodule(parent: &Path, rel_path: &str) -> PathBuf {
        let sub_temp = TempDir::new().unwrap();
        let sub_origin = sub_temp.path().join("origin");
        fs::create_dir_all(&sub_origin).unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&sub_origin)
            .status()
            .unwrap()
            .success());
        for (k, v) in [("user.name", "Test"), ("user.email", "test@test.local")] {
            assert!(Command::new("git")
                .args(["config", k, v])
                .current_dir(&sub_origin)
                .status()
                .unwrap()
                .success());
        }
        fs::write(sub_origin.join("sub_tracked.txt"), b"sub_initial").unwrap();
        assert!(Command::new("git")
            .args(["add", "sub_tracked.txt"])
            .current_dir(&sub_origin)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "sub_base"])
            .current_dir(&sub_origin)
            .status()
            .unwrap()
            .success());

        let origin_url = sub_origin.to_string_lossy().to_string();
        assert!(Command::new("git")
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &origin_url,
                rel_path,
            ])
            .current_dir(parent)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", &format!("add submodule {rel_path}")])
            .current_dir(parent)
            .status()
            .unwrap()
            .success());
        parent.join(rel_path)
    }

    #[test]
    fn checkpoint_round_trip_verifies_payload_and_detects_same_size_drift() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());
        fs::write(root.join("new file.txt"), b"sample payload").unwrap();
        let (manifest, reference) = store
            .capture(
                "run-1",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                Some("task prompt".into()),
                Some("{\"planText\":\"p\"}".into()),
            )
            .unwrap();
        assert_eq!(manifest.schema_version, CHECKPOINT_SCHEMA_VERSION);
        let loaded = store
            .load("run-1", &reference, &manifest.manifest_digest)
            .unwrap();
        assert_eq!(manifest, loaded);
        let preflight = store
            .preflight(
                "run-1",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(preflight.workspace_matches);
        assert!(preflight.affected_paths.is_empty());

        // Modify file with same size bytes to test exact content hashing drift
        fs::write(root.join("new file.txt"), b"SAMPLE PAYLOAD").unwrap();
        let drift = store
            .preflight(
                "run-1",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(!drift.workspace_matches);
        assert!(drift.affected_paths.contains(&"new file.txt".to_string()));
    }

    #[test]
    fn restore_creates_verified_shelve_backup_and_preserves_head_and_refs() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());
        fs::write(root.join("preserved.txt"), b"expected content").unwrap();
        let (manifest, reference) = store
            .capture(
                "run-restore",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();
        let initial_head = git(&root, &["rev-parse", "HEAD"]).unwrap().stdout;
        fs::write(root.join("preserved.txt"), b"drifted content").unwrap();
        fs::write(root.join("untracked_drift.txt"), b"drift").unwrap();
        let preflight = store
            .preflight(
                "run-restore",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        let resolution = store
            .resolve_restore(
                "run-restore",
                1,
                &reference,
                &manifest.manifest_digest,
                &preflight.current_fingerprint,
                &preflight.backup_id,
                &root,
            )
            .unwrap();
        assert_eq!(resolution.checkpoint_id, Some(manifest.checkpoint_id));
        assert_eq!(
            fs::read(root.join("preserved.txt")).unwrap(),
            b"expected content"
        );
        assert!(!root.join("untracked_drift.txt").exists());
        let final_head = git(&root, &["rev-parse", "HEAD"]).unwrap().stdout;
        assert_eq!(initial_head, final_head);
        let backup_ref = format!(
            "backups/{}/manifest.json",
            resolution.backup_id.as_deref().unwrap()
        );
        let backup = store
            .load(
                "run-restore",
                &backup_ref,
                resolution.backup_digest.as_deref().unwrap(),
            )
            .unwrap();
        assert_eq!(backup.kind, CheckpointKind::ShelveBackup);
    }

    #[test]
    fn restore_rejects_backup_id_not_bound_to_preflight_without_mutation() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());
        let (manifest, reference) = store
            .capture(
                "run-guard",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();
        let preflight = store
            .preflight(
                "run-guard",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        fs::write(root.join("drift.txt"), b"dirty").unwrap();
        let err = store.resolve_restore(
            "run-guard",
            1,
            &reference,
            &manifest.manifest_digest,
            &preflight.current_fingerprint,
            &Uuid::new_v4().to_string(),
            &root,
        );
        assert!(err.is_err());
        assert!(root.join("drift.txt").exists());
    }

    #[test]
    fn test_nested_submodule_checkpoint_and_restore_round_trip() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());

        // 1. Add child submodule
        let sub1_path = init_submodule(&root, "vendor/sub1");
        // 2. Add nested submodule inside sub1
        let _sub2_path = init_submodule(&sub1_path, "nested/sub2");
        assert!(Command::new("git")
            .args(["add", "vendor/sub1"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "update sub1 with nested sub2"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());

        // Add staged and untracked changes across root, sub1, and sub2
        fs::write(root.join("root_extra.txt"), b"root_extra").unwrap();
        fs::write(sub1_path.join("sub1_extra.txt"), b"sub1_extra").unwrap();
        fs::write(
            sub1_path.join("nested/sub2/sub2_extra.txt"),
            b"sub2_extra",
        )
        .unwrap();

        let (manifest, reference) = store
            .capture(
                "run-nested",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        assert_eq!(manifest.submodules.len(), 2);
        assert_eq!(manifest.submodules[0].node_key, "vendor/sub1");
        assert_eq!(
            manifest.submodules[1].node_key,
            "vendor/sub1::nested/sub2"
        );

        let preflight = store
            .preflight(
                "run-nested",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(preflight.workspace_matches);
        assert!(preflight.affected_paths.is_empty());

        // Introduce changes at all nesting levels:
        fs::write(root.join("root_extra.txt"), b"root_modified").unwrap();
        fs::write(sub1_path.join("sub1_extra.txt"), b"sub1_modified").unwrap();
        fs::write(
            sub1_path.join("nested/sub2/sub2_extra.txt"),
            b"sub2_modified",
        )
        .unwrap();
        fs::write(
            sub1_path.join("nested/sub2/sub2_new.txt"),
            b"sub2_untracked",
        )
        .unwrap();

        let drift = store
            .preflight(
                "run-nested",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(!drift.workspace_matches);
        assert!(drift.affected_paths.contains(&"root_extra.txt".to_string()));
        assert!(drift
            .affected_paths
            .contains(&"vendor/sub1 / sub1_extra.txt".to_string()));
        assert!(drift
            .affected_paths
            .contains(&"vendor/sub1 / nested/sub2 / sub2_extra.txt".to_string()));
        assert!(drift
            .affected_paths
            .contains(&"vendor/sub1 / nested/sub2 / sub2_new.txt".to_string()));

        // Restore and verify exact round-trip across all nodes
        let resolution = store
            .resolve_restore(
                "run-nested",
                1,
                &reference,
                &manifest.manifest_digest,
                &drift.current_fingerprint,
                &drift.backup_id,
                &root,
            )
            .unwrap();

        assert_eq!(
            fs::read(root.join("root_extra.txt")).unwrap(),
            b"root_extra"
        );
        assert_eq!(
            fs::read(sub1_path.join("sub1_extra.txt")).unwrap(),
            b"sub1_extra"
        );
        assert_eq!(
            fs::read(sub1_path.join("nested/sub2/sub2_extra.txt")).unwrap(),
            b"sub2_extra"
        );
        assert!(!sub1_path.join("nested/sub2/sub2_new.txt").exists());

        let post_restore_preflight = store
            .preflight(
                "run-nested",
                resolution.journal_revision,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(post_restore_preflight.workspace_matches);
        assert!(post_restore_preflight.affected_paths.is_empty());
    }

    #[test]
    fn test_legacy_v2_manifest_with_submodule_verifies_historical_digest_and_rejects_tampering() {
        let (_t, root) = repo();
        let sub_path = init_submodule(&root, "vendor/sub");
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());
        let (mut manifest, reference) = store
            .capture(
                "run-legacy-v2",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        manifest.schema_version = 2;
        for sub in &mut manifest.submodules {
            sub.node_key.clear();
            sub.immediate_parent_key.clear();
            sub.payload_entries.clear();
            sub.index_blob_hash.clear();
        }
        manifest.manifest_digest = manifest_digest(&manifest).unwrap();
        let manifest_path = runs
            .path()
            .join("run-legacy-v2")
            .join("checkpoints")
            .join(&manifest.checkpoint_id)
            .join("manifest.json");
        fs::write(
            &manifest_path,
            serialize_legacy_v2_manifest(&manifest, false).unwrap(),
        )
        .unwrap();

        let loaded = store
            .load("run-legacy-v2", &reference, &manifest.manifest_digest)
            .unwrap();
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.submodules.len(), 1);
        assert_eq!(loaded.submodules[0].node_key, "");
        assert_eq!(loaded.submodules[0].payload_entries, Vec::new());

        let mut tampered = manifest.clone();
        tampered.manifest_digest = "f".repeat(64);
        fs::write(
            &manifest_path,
            serialize_legacy_v2_manifest(&tampered, false).unwrap(),
        )
        .unwrap();
        assert!(store
            .load("run-legacy-v2", &reference, &manifest.manifest_digest)
            .is_err());
        assert!(sub_path.join("sub_tracked.txt").exists());
    }

    #[test]
    fn test_raw_index_byte_drift_is_detected_at_root_child_and_nested_nodes() {
        let (_t, root) = repo();
        let sub1_path = init_submodule(&root, "vendor/sub1");
        let _sub2_path = init_submodule(&sub1_path, "nested/sub2");
        assert!(Command::new("git")
            .args(["add", "vendor/sub1"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "record nested submodule"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());

        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());
        let (manifest, reference) = store
            .capture(
                "run-index-drift",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        let index_targets = [
            (root.clone(), "base.txt", ".git/index (staged state)".to_string()),
            (
                sub1_path.clone(),
                "sub_tracked.txt",
                "vendor/sub1 / .git/index (staged state)".to_string(),
            ),
            (
                sub1_path.join("nested/sub2"),
                "sub_tracked.txt",
                "vendor/sub1 / nested/sub2 / .git/index (staged state)".to_string(),
            ),
        ];
        for (repository, tracked_path, _) in &index_targets {
            assert!(Command::new("git")
                .args(["update-index", "--assume-unchanged", "--", tracked_path])
                .current_dir(repository)
                .status()
                .unwrap()
                .success());
        }

        let preflight = store
            .preflight(
                "run-index-drift",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(!preflight.workspace_matches);
        for (_, _, index_path) in index_targets {
            assert!(preflight.affected_paths.contains(&index_path));
        }
    }

    #[test]
    fn test_same_filename_in_multiple_submodules_does_not_collide() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());

        let sub1_path = init_submodule(&root, "vendor/sub1");
        let sub2_path = init_submodule(&root, "vendor/sub2");

        // Same filename `shared_name.txt` in root, sub1, and sub2 with different contents
        fs::write(root.join("shared_name.txt"), b"root content").unwrap();
        fs::write(sub1_path.join("shared_name.txt"), b"sub1 content").unwrap();
        fs::write(sub2_path.join("shared_name.txt"), b"sub2 content").unwrap();

        let (manifest, reference) = store
            .capture(
                "run-collision-test",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        // Mess up all three files
        fs::write(root.join("shared_name.txt"), b"corrupt 0").unwrap();
        fs::write(sub1_path.join("shared_name.txt"), b"corrupt 1").unwrap();
        fs::write(sub2_path.join("shared_name.txt"), b"corrupt 2").unwrap();

        let preflight = store
            .preflight(
                "run-collision-test",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(preflight
            .affected_paths
            .contains(&"shared_name.txt".to_string()));
        assert!(preflight
            .affected_paths
            .contains(&"vendor/sub1 / shared_name.txt".to_string()));
        assert!(preflight
            .affected_paths
            .contains(&"vendor/sub2 / shared_name.txt".to_string()));

        store
            .resolve_restore(
                "run-collision-test",
                1,
                &reference,
                &manifest.manifest_digest,
                &preflight.current_fingerprint,
                &preflight.backup_id,
                &root,
            )
            .unwrap();

        assert_eq!(
            fs::read(root.join("shared_name.txt")).unwrap(),
            b"root content"
        );
        assert_eq!(
            fs::read(sub1_path.join("shared_name.txt")).unwrap(),
            b"sub1 content"
        );
        assert_eq!(
            fs::read(sub2_path.join("shared_name.txt")).unwrap(),
            b"sub2 content"
        );
    }

    #[test]
    fn test_submodule_head_ahead_of_gitlink_captured_and_restored() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());

        let sub_path = init_submodule(&root, "vendor/sub");

        // Make a commit inside the submodule without updating parent gitlink
        fs::write(sub_path.join("local_commit.txt"), b"committed in sub").unwrap();
        assert!(Command::new("git")
            .args(["add", "local_commit.txt"])
            .current_dir(&sub_path)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "submodule ahead commit"])
            .current_dir(&sub_path)
            .status()
            .unwrap()
            .success());

        // Add dirty untracked change
        fs::write(sub_path.join("dirty_untracked.txt"), b"dirty").unwrap();

        let (manifest, reference) = store
            .capture(
                "run-head-ahead",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        assert_eq!(manifest.submodules.len(), 1);
        let sub_cp = &manifest.submodules[0];
        assert_ne!(sub_cp.head_oid, sub_cp.gitlink_oid);

        let preflight = store
            .preflight(
                "run-head-ahead",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(preflight.workspace_matches);

        // Edit dirty file
        fs::write(sub_path.join("dirty_untracked.txt"), b"dirty changed").unwrap();
        let drift = store
            .preflight(
                "run-head-ahead",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();
        assert!(!drift.workspace_matches);

        store
            .resolve_restore(
                "run-head-ahead",
                1,
                &reference,
                &manifest.manifest_digest,
                &drift.current_fingerprint,
                &drift.backup_id,
                &root,
            )
            .unwrap();

        assert_eq!(
            fs::read(sub_path.join("dirty_untracked.txt")).unwrap(),
            b"dirty"
        );
    }

    #[test]
    fn test_changed_submodule_head_blocks_restore_fail_closed() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());

        let sub_path = init_submodule(&root, "vendor/sub");
        let (manifest, reference) = store
            .capture(
                "run-head-changed-block",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        // Mutate submodule HEAD after checkpoint
        fs::write(sub_path.join("new_sub_commit.txt"), b"extra").unwrap();
        assert!(Command::new("git")
            .args(["add", "new_sub_commit.txt"])
            .current_dir(&sub_path)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "new commit in sub"])
            .current_dir(&sub_path)
            .status()
            .unwrap()
            .success());

        let preflight = store
            .preflight(
                "run-head-changed-block",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();

        assert!(!preflight.workspace_matches);
        assert_eq!(
            preflight.reason_code,
            Some("unsupported_submodule_state".into())
        );
        assert!(preflight
            .affected_paths
            .contains(&"vendor/sub / HEAD".to_string()));

        let err = store.resolve_restore(
            "run-head-changed-block",
            1,
            &reference,
            &manifest.manifest_digest,
            &preflight.current_fingerprint,
            &preflight.backup_id,
            &root,
        );
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("Submodule HEAD or refs differ"));
    }

    #[test]
    fn test_ignored_files_remain_untouched_and_excluded_from_inventory() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());

        let sub_path = init_submodule(&root, "vendor/sub");

        // Add gitignore at root and submodule
        fs::write(root.join(".gitignore"), b"ignored_root.log\ntarget/\n").unwrap();
        fs::write(sub_path.join(".gitignore"), b"ignored_sub.log\n").unwrap();
        assert!(Command::new("git")
            .args(["add", ".gitignore"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "add root gitignore"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["add", ".gitignore"])
            .current_dir(&sub_path)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "add sub gitignore"])
            .current_dir(&sub_path)
            .status()
            .unwrap()
            .success());

        // Create ignored files and build directory
        fs::write(root.join("ignored_root.log"), b"root log content").unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::write(
            root.join("target/debug/build_cache.bin"),
            b"build cache bytes",
        )
        .unwrap();
        fs::write(sub_path.join("ignored_sub.log"), b"sub log content").unwrap();

        let (manifest, reference) = store
            .capture(
                "run-ignored",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        // Verify ignored files are absent from root and submodule payload entries
        assert!(!manifest
            .payload_entries
            .iter()
            .any(|e| e.path.contains("ignored") || e.path.contains("target")));
        assert!(!manifest.submodules[0]
            .payload_entries
            .iter()
            .any(|e| e.path.contains("ignored")));

        // Make a tracked change to allow restore
        fs::write(root.join("base.txt"), b"modified base").unwrap();

        let preflight = store
            .preflight(
                "run-ignored",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();

        store
            .resolve_restore(
                "run-ignored",
                1,
                &reference,
                &manifest.manifest_digest,
                &preflight.current_fingerprint,
                &preflight.backup_id,
                &root,
            )
            .unwrap();

        // Ignored files and caches must remain exactly untouched on disk
        assert_eq!(
            fs::read(root.join("ignored_root.log")).unwrap(),
            b"root log content"
        );
        assert_eq!(
            fs::read(root.join("target/debug/build_cache.bin")).unwrap(),
            b"build cache bytes"
        );
        assert_eq!(
            fs::read(sub_path.join("ignored_sub.log")).unwrap(),
            b"sub log content"
        );
    }

    #[test]
    fn test_submodule_refs_drift_blocks_restore_precondition() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());

        let sub_path = init_submodule(&root, "vendor/sub");
        let (manifest, reference) = store
            .capture(
                "run-refs-test",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        // Create a new branch ref inside the submodule (HEAD remains at same commit)
        assert!(Command::new("git")
            .args(["branch", "new-feature-branch"])
            .current_dir(&sub_path)
            .status()
            .unwrap()
            .success());

        let preflight = store
            .preflight(
                "run-refs-test",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();

        assert!(!preflight.workspace_matches);
        assert_eq!(
            preflight.reason_code,
            Some("unsupported_submodule_state".into())
        );
        assert!(preflight
            .affected_paths
            .contains(&"vendor/sub / refs".to_string()));

        let err = store.resolve_restore(
            "run-refs-test",
            1,
            &reference,
            &manifest.manifest_digest,
            &preflight.current_fingerprint,
            &preflight.backup_id,
            &root,
        );
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("Submodule HEAD or refs differ"));
    }

    #[test]
    fn test_stale_fingerprint_token_blocks_restore_with_zero_mutation() {
        let (_t, root) = repo();
        let runs = TempDir::new().unwrap();
        let store = CheckpointStore::new(runs.path().to_path_buf());

        let sub_path = init_submodule(&root, "vendor/sub");
        fs::write(sub_path.join("file.txt"), b"initial").unwrap();
        let (manifest, reference) = store
            .capture(
                "run-stale-test",
                WorkflowState::Implementation,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();

        fs::write(sub_path.join("file.txt"), b"drifted1").unwrap();
        let preflight = store
            .preflight(
                "run-stale-test",
                1,
                &reference,
                &manifest.manifest_digest,
                &root,
            )
            .unwrap();

        // Further modify after preflight was issued
        fs::write(sub_path.join("file.txt"), b"drifted2").unwrap();

        let err = store.resolve_restore(
            "run-stale-test",
            1,
            &reference,
            &manifest.manifest_digest,
            &preflight.current_fingerprint,
            &preflight.backup_id,
            &root,
        );
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("Workspace changed after recovery preflight"));
        assert_eq!(fs::read(sub_path.join("file.txt")).unwrap(), b"drifted2");
    }
}
