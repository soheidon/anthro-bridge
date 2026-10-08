//! Durable, content-addressed workspace checkpoints for explicit run recovery.
//!
//! This module intentionally supports only a clean, initialized submodule graph.
//! It never changes HEAD or refs. Restore is path-scoped and refuses ambiguous
//! repository states before creating a backup or changing the workspace.

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
struct Inventory {
    repository: RepositoryCheckpoint,
    entries: Vec<CheckpointPayloadEntry>,
    submodules: Vec<SubmoduleCheckpoint>,
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
        let starting_fingerprint = repository_fingerprint(
            &inventory.repository,
            &inventory.entries,
            &inventory.submodules,
            &inventory.index_bytes,
        )?;
        let checkpoint_dir = self
            .sidecar_dir(run_id)?
            .join(match kind {
                CheckpointKind::ShelveBackup => "backups",
                _ => "checkpoints",
            })
            .join(&checkpoint_id);
        ensure_new_checkpoint_target(&checkpoint_dir)?;
        secure_create_dir_all(&self.runs_dir, &checkpoint_dir)?;

        let mut entries = inventory.entries;
        let mut total_bytes = 0u64;
        for entry in &mut entries {
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
        // The workspace may be edited while payloads are copied. Re-inventory
        // after the last read and refuse to publish a manifest for a mixed-time
        // snapshot. Orphaned content-addressed blobs are harmless and retained.
        let after_capture = inventory_repository(&root)?;
        let ending_fingerprint = repository_fingerprint(
            &after_capture.repository,
            &after_capture.entries,
            &after_capture.submodules,
            &after_capture.index_bytes,
        )?;
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
            payload_entries: entries,
            index_blob_hash,
            task_prompt,
            stage_input,
            submodules: inventory.submodules,
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
        if manifest.schema_version != CHECKPOINT_SCHEMA_VERSION || manifest.run_id != run_id {
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
        let current_fingerprint = repository_fingerprint(
            &current.repository,
            &current.entries,
            &current.submodules,
            &current.index_bytes,
        )?;
        let saved_index = read_blob_for_manifest(
            &self.runs_dir,
            run_id,
            reference,
            &manifest.index_blob_hash,
        )?;
        let saved_fingerprint = repository_fingerprint(
            &manifest.repository,
            &manifest.payload_entries,
            &manifest.submodules,
            &saved_index,
        )?;
        let workspace_matches = current_fingerprint == saved_fingerprint;
        let mut affected_paths = path_delta(&manifest.payload_entries, &current.entries);
        if sha256(&current.index_bytes) != sha256(&saved_index) {
            affected_paths.push(".git/index (staged state)".to_string());
        }
        affected_paths.sort();
        affected_paths.dedup();
        let exact_supported = manifest.submodules == current.submodules;
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
            // The command layer fills this only for a fully validated route.
            can_resume: false,
            resume_stage: None,
            // Mutation choices stay disabled until the corresponding durable
            // resume route exists. Restoring/adopting without a continuation
            // path would strand the interrupted run after changing user data.
            can_restore: false,
            can_adopt: false,
            affected_paths,
            reason_code: if !exact_supported {
                Some("unsupported_submodule_state".into())
            } else if !same_repository_identity(&current.repository, &manifest.repository) {
                Some("repository_identity_changed".into())
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
        let expected_candidate = backup_candidate_id(run_id, journal_revision, expected_current_fingerprint);
        if backup_id != expected_candidate {
            return Err("Shelve Backup destination no longer matches this recovery preflight".into());
        }
        let target = self.load(run_id, reference, expected_digest)?;
        let root = canonical_repo_root(project_path)?;
        let before = inventory_repository(&root)?;
        let before_fingerprint = repository_fingerprint(
            &before.repository,
            &before.entries,
            &before.submodules,
            &before.index_bytes,
        )?;
        if before_fingerprint != expected_current_fingerprint {
            return Err(
                "Workspace changed after recovery preflight; reopen recovery details".into(),
            );
        }
        if !same_repository_identity(&before.repository, &target.repository) {
            return Err("HEAD or refs differ; checkpoint restore is unsupported".into());
        }
        if target.submodules != before.submodules {
            return Err("Submodule state changed; recursive submodule restore is deferred and no workspace mutation was performed".into());
        }
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
        if repository_fingerprint(
            &latest.repository,
            &latest.entries,
            &latest.submodules,
            &latest.index_bytes,
        )? != expected_current_fingerprint
        {
            return Err(
                "Workspace changed while Shelve Backup was created; restore was not started".into(),
            );
        }
        validate_restore_plan(&self.runs_dir, &root, &target, &latest)?;
        restore_inventory(&self.runs_dir, &root, &target, &latest).map_err(|e| {
            format!("Partial restore failed; verified Shelve Backup retained at {backup_ref}: {e}")
        })?;
        let after = inventory_repository(&root)?;
        let restored = repository_fingerprint(
            &after.repository,
            &after.entries,
            &after.submodules,
            &after.index_bytes,
        )?;
        let expected = repository_fingerprint(
            &target.repository,
            &target.payload_entries,
            &target.submodules,
            &read_blob_for_manifest(&self.runs_dir, run_id, reference, &target.index_blob_hash)?,
        )?;
        if restored != expected {
            return Err(format!("Partial restore: post-restore fingerprint mismatch; verified Shelve Backup retained at {backup_ref}"));
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

fn inventory_repository(root: &Path) -> Result<Inventory, String> {
    verify_submodule_graph_clean(root)?;
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
    let index_path_out = git(root, &["rev-parse", "--git-path", "index"])?;
    let index_path_text = String::from_utf8(index_path_out.stdout)
        .map_err(|e| format!("Git index path is not UTF-8: {e}"))?;
    let index_path = PathBuf::from(index_path_text.trim());
    let index_path = if index_path.is_absolute() {
        index_path
    } else {
        root.join(index_path)
    };
    let index_bytes = fs::read(&index_path)
        .map_err(|e| format!("Cannot read Git index '{}': {e}", index_path.display()))?;

    let staged = git(
        root,
        &["diff", "--cached", "--binary", "--no-ext-diff", "HEAD"],
    )?;
    let unstaged = git(root, &["diff", "--binary", "--no-ext-diff"])?;
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
    // Preserve full index/worktree staged semantics in the integrity inventory.
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
    let submodules = collect_submodule_checkpoints(root, &gitlinks)?;
    Ok(Inventory {
        repository,
        entries,
        submodules,
        index_bytes,
    })
}

fn collect_submodule_checkpoints(
    parent: &Path,
    gitlinks: &BTreeMap<String, String>,
) -> Result<Vec<SubmoduleCheckpoint>, String> {
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
        let head_out = git(&child, &["rev-parse", "HEAD"])?;
        let head_oid = String::from_utf8(head_out.stdout)
            .map_err(|e| format!("Submodule HEAD is not UTF-8: {e}"))?
            .trim()
            .to_string();
        if &head_oid != gitlink_oid {
            return Err(format!(
                "Submodule HEAD differs from immediate parent gitlink: {relative}"
            ));
        }
        let child_inventory = inventory_repository(&child)?;
        result.push(SubmoduleCheckpoint {
            immediate_parent_path: parent.to_string_lossy().to_string(),
            rel_path: relative.clone(),
            gitlink_oid: gitlink_oid.clone(),
            head_oid,
            repository: child_inventory.repository,
        });
        result.extend(child_inventory.submodules);
    }
    result.sort_by(|a, b| {
        (a.immediate_parent_path.as_str(), a.rel_path.as_str())
            .cmp(&(b.immediate_parent_path.as_str(), b.rel_path.as_str()))
    });
    Ok(result)
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
    let target_map: BTreeMap<&str, &CheckpointPayloadEntry> = target
        .payload_entries
        .iter()
        .map(|e| (e.path.as_str(), e))
        .collect();
    let current_map: BTreeMap<&str, &CheckpointPayloadEntry> = current
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e))
        .collect();
    let all: BTreeSet<&str> = target_map
        .keys()
        .chain(current_map.keys())
        .copied()
        .collect();
    let checkpoint_dir = runs_dir
        .join(&target.run_id)
        .join("checkpoints")
        .join(&target.checkpoint_id);
    for rel in all {
        validate_rel_path(rel)?;
        if target_map.get(rel).is_some_and(|e| e.kind == "submodule")
            && current_map.get(rel).is_some_and(|e| e.kind == "submodule")
        {
            continue;
        }
        let target_entry = target_map.get(rel).copied();
        match target_entry {
            None => {
                let path = safe_join(root, rel)?;
                if let Some(parent) = path.parent() {
                    ensure_no_symlink_components(root, parent)?;
                }
                remove_checkpoint_leaf(&path, rel)?;
            }
            Some(entry) if entry.kind == "missing" => {
                let path = safe_join(root, rel)?;
                if let Some(parent) = path.parent() {
                    ensure_no_symlink_components(root, parent)?;
                }
                remove_checkpoint_leaf(&path, rel)?;
            }
            Some(entry) => {
                let path = safe_join(root, rel)?;
                ensure_safe_parent_dirs(root, &path)?;
                let Some(hash) = &entry.blob_hash else {
                    return Err(format!("Checkpoint payload missing for {rel}"));
                };
                let bytes = read_blob(runs_dir, &checkpoint_dir, hash)?;
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
    let index_bytes = read_blob(runs_dir, &checkpoint_dir, &target.index_blob_hash)?;
    let index_out = git(root, &["rev-parse", "--git-path", "index"])?;
    let index_text =
        String::from_utf8(index_out.stdout).map_err(|e| format!("Index path is not UTF-8: {e}"))?;
    let index = PathBuf::from(index_text.trim());
    let index = if index.is_absolute() {
        index
    } else {
        root.join(index)
    };
    replace_regular_file(&index, &index_bytes, 0o600)?;
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
    // Avoid converting Git-for-Windows' /c/... display paths back into native
    // paths. `--show-prefix` proves the supplied canonical path is the root.
    Ok(canonical)
}

fn verify_submodule_graph_clean(root: &Path) -> Result<(), String> {
    let out = git(root, &["submodule", "status", "--recursive"])?;
    let text =
        String::from_utf8(out.stdout).map_err(|e| format!("Submodule status is not UTF-8: {e}"))?;
    for line in text.lines().filter(|line| !line.is_empty()) {
        if !line.starts_with(' ') {
            return Err(format!(
                "Submodule state is not clean/initialized: {}",
                line.trim()
            ));
        }
    }
    let child_status = git(
        root,
        &[
            "submodule",
            "foreach",
            "--quiet",
            "--recursive",
            "git status --porcelain=v1 --untracked-files=all",
        ],
    )?;
    if !child_status.stdout.is_empty() {
        return Err("Dirty submodule worktree is not resumable".into());
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<Output, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
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

fn validate_restore_plan(
    runs_dir: &Path,
    root: &Path,
    target: &CheckpointManifest,
    current: &Inventory,
) -> Result<(), String> {
    let current_map: BTreeMap<&str, &CheckpointPayloadEntry> = current
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e))
        .collect();
    let target_map: BTreeMap<&str, &CheckpointPayloadEntry> = target
        .payload_entries
        .iter()
        .map(|e| (e.path.as_str(), e))
        .collect();
    let all: BTreeSet<&str> = current_map
        .keys()
        .chain(target_map.keys())
        .copied()
        .collect();
    let checkpoint_dir = runs_dir
        .join(&target.run_id)
        .join("checkpoints")
        .join(&target.checkpoint_id);
    for rel in all {
        validate_rel_path(rel)?;
        if target_map.get(rel).is_some_and(|e| e.kind == "submodule")
            && current_map.get(rel).is_some_and(|e| e.kind == "submodule")
        {
            continue;
        }
        let path = safe_join(root, rel)?;
        if let Some(parent) = path.parent() {
            ensure_no_symlink_components(root, parent)?;
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
                let bytes = read_blob(runs_dir, &checkpoint_dir, hash)?;
                if bytes.len() as u64 != entry.size {
                    return Err(format!("Restore payload length mismatch for {rel}"));
                }
            }
        }
    }
    read_blob(runs_dir, &checkpoint_dir, &target.index_blob_hash)?;
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
    let mut canonical = manifest.clone();
    canonical.manifest_digest.clear();
    let bytes =
        serde_json::to_vec(&canonical).map_err(|e| format!("Serialize canonical manifest: {e}"))?;
    Ok(sha256(&bytes))
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
fn repository_fingerprint(
    repo: &RepositoryCheckpoint,
    entries: &[CheckpointPayloadEntry],
    submodules: &[SubmoduleCheckpoint],
    index_bytes: &[u8],
) -> Result<String, String> {
    let stable = serde_json::to_vec(&(
        repo.canonical_root.as_str(),
        repo.head_oid.as_str(),
        repo.refs_snapshot.as_str(),
        &repo.status_inventory,
        entries,
        submodules,
        sha256(index_bytes),
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
        fs::create_dir(&root).unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&root)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.invalid"])
            .current_dir(&root)
            .status()
            .unwrap();
        fs::write(root.join("tracked.txt"), b"base").unwrap();
        assert!(Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "initial"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        (t, root)
    }

    #[test]
    fn checkpoint_round_trip_verifies_payload_and_detects_same_size_drift() {
        let (temp, root) = repo();
        fs::write(root.join("tracked.txt"), b"edit").unwrap();
        fs::write(root.join("new file.txt"), b"AAAA").unwrap();
        let store = CheckpointStore::new(temp.path().join("runs"));
        let (manifest, reference) = store
            .capture(
                "run-1",
                WorkflowState::Implementation,
                CheckpointKind::EntryBaseline,
                &root,
                Some("task".into()),
                None,
            )
            .unwrap();
        let loaded = store
            .load("run-1", &reference, &manifest.manifest_digest)
            .unwrap();
        assert_eq!(loaded.payload_entries.len(), 2);
        let exact = store
            .preflight("run-1", 1, &reference, &manifest.manifest_digest, &root)
            .unwrap();
        assert!(exact.workspace_matches);
        // A matching fingerprint alone is not enough; this build does not yet
        // install a resume execution route or fresh worker lifecycle.
        assert!(!exact.can_resume);
        assert!(!exact.can_restore);
        assert!(!exact.can_adopt);
        assert_eq!(exact.reason_code.as_deref(), Some("resume_route_unavailable"));
        fs::write(root.join("new file.txt"), b"BBBB").unwrap();
        let drift = store
            .preflight("run-1", 1, &reference, &manifest.manifest_digest, &root)
            .unwrap();
        assert!(!drift.workspace_matches);
        assert!(!drift.can_restore);
        assert!(!drift.can_adopt);
        assert!(drift.affected_paths.contains(&"new file.txt".to_string()));
    }

    #[test]
    fn restore_creates_verified_shelve_backup_and_preserves_head_and_refs() {
        let (temp, root) = repo();
        fs::write(root.join("tracked.txt"), b"checkpoint").unwrap();
        let store = CheckpointStore::new(temp.path().join("runs"));
        let (target, reference) = store
            .capture(
                "run-2",
                WorkflowState::Fix,
                CheckpointKind::StageCheckpoint,
                &root,
                None,
                None,
            )
            .unwrap();
        fs::write(root.join("tracked.txt"), b"other").unwrap();
        let current = store
            .preflight("run-2", 7, &reference, &target.manifest_digest, &root)
            .unwrap();
        let head_before = git(&root, &["rev-parse", "HEAD"]).unwrap().stdout;
        let resolution = store
            .resolve_restore(
                "run-2",
                7,
                &reference,
                &target.manifest_digest,
                &current.current_fingerprint,
                &current.backup_id,
                &root,
            )
            .unwrap();
        assert_eq!(fs::read(root.join("tracked.txt")).unwrap(), b"checkpoint");
        assert!(resolution.backup_id.is_some());
        assert_eq!(
            git(&root, &["rev-parse", "HEAD"]).unwrap().stdout,
            head_before
        );
        let backup_manifest_path = store
            .sidecar_dir("run-2")
            .unwrap()
            .join("backups")
            .join(&current.backup_id)
            .join("manifest.json");
        let retained_backup_bytes = fs::read(&backup_manifest_path).unwrap();
        let replayed_resolution = store.resolve_restore(
            "run-2",
            7,
            &reference,
            &target.manifest_digest,
            &current.current_fingerprint,
            &current.backup_id,
            &root,
        );
        assert!(replayed_resolution.is_err());
        assert_eq!(fs::read(&backup_manifest_path).unwrap(), retained_backup_bytes);
        let replay = store.capture_with_id(
            "run-2",
            WorkflowState::Fix,
            CheckpointKind::ShelveBackup,
            &root,
            None,
            None,
            current.backup_id.clone(),
        );
        assert!(replay.unwrap_err().contains("destination already exists and is immutable"));
        assert_eq!(fs::read(root.join("tracked.txt")).unwrap(), b"checkpoint");
    }

    #[test]
    fn restore_rejects_backup_id_not_bound_to_preflight_without_mutation() {
        let (temp, root) = repo();
        fs::write(root.join("tracked.txt"), b"checkpoint").unwrap();
        let store = CheckpointStore::new(temp.path().join("runs"));
        let (target, reference) = store
            .capture(
                "run-token",
                WorkflowState::Validation,
                CheckpointKind::EntryBaseline,
                &root,
                None,
                None,
            )
            .unwrap();
        fs::write(root.join("tracked.txt"), b"drifted").unwrap();
        let preflight = store
            .preflight("run-token", 9, &reference, &target.manifest_digest, &root)
            .unwrap();
        let before = current_fingerprint(&root);
        let mut mismatched = Uuid::new_v4().to_string();
        while mismatched == preflight.backup_id {
            mismatched = Uuid::new_v4().to_string();
        }
        let result = store.resolve_restore(
            "run-token",
            9,
            &reference,
            &target.manifest_digest,
            &preflight.current_fingerprint,
            &mismatched,
            &root,
        );
        assert!(result
            .unwrap_err()
            .contains("no longer matches this recovery preflight"));
        assert_eq!(current_fingerprint(&root), before);
        assert_eq!(fs::read(root.join("tracked.txt")).unwrap(), b"drifted");
    }

    fn current_fingerprint(root: &Path) -> String {
        let inventory = inventory_repository(root).unwrap();
        repository_fingerprint(
            &inventory.repository,
            &inventory.entries,
            &inventory.submodules,
            &inventory.index_bytes,
        )
        .unwrap()
    }
}
