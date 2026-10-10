use super::parser::sha256_hex;
use super::types::{PlanCandidate, PlanConvergenceError, ReviewVerdict};
use crate::orchestrator::plan_workspace::FrozenPlanSnapshot;
use crate::orchestrator::recovery::{apply_and_verify_permissions, validate_run_id};
use std::fs;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

pub const MAX_ARTIFACT_BYTES: usize = 1024 * 1024; // 1 MiB

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenRunSnapshotArtifact {
    pub schema_version: u32,
    pub run_id: String,
    pub snapshot: FrozenPlanSnapshot,
}

pub const FROZEN_SNAPSHOT_ARTIFACT_REF: &str = "artifacts/context/frozen_plan.json";

/// Persist the exact run-owned plan snapshot before any role is dispatched.
pub fn save_frozen_run_snapshot(
    runs_dir: &Path,
    run_id: &str,
    snapshot: &FrozenPlanSnapshot,
) -> Result<(String, String), PlanConvergenceError> {
    validate_run_id(run_id).map_err(|e| PlanConvergenceError::new("PC_INVALID_RUN_ID", e))?;
    let payload = snapshot.to_frozen_plan_payload().ok_or_else(|| {
        PlanConvergenceError::new("PC_INVALID_FROZEN_PLAN", "Frozen plan payload is unavailable")
    })?;
    payload.validate_against_plan_context(&snapshot.plan_context).map_err(|e| {
        PlanConvergenceError::new("PC_INVALID_FROZEN_PLAN", e)
    })?;

    let sidecar_dir = runs_dir.join(run_id);
    let context_dir = sidecar_dir.join("artifacts").join("context");
    fs::create_dir_all(&context_dir).map_err(|e| {
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Failed to create context artifact directory: {e}"))
    })?;
    apply_and_verify_permissions(&context_dir, true).map_err(|e| {
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Context artifact permission check failed: {e}"))
    })?;

    let artifact = FrozenRunSnapshotArtifact {
        schema_version: 1,
        run_id: run_id.to_string(),
        snapshot: snapshot.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&artifact).map_err(|e| {
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Failed to serialize frozen context artifact: {e}"))
    })?;
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(PlanConvergenceError::new(
            "PC_FROZEN_PLAN_OVERSIZED",
            format!("Frozen context artifact size {} exceeds 1 MiB limit", bytes.len()),
        ));
    }
    let digest = sha256_hex(&bytes);
    let target = context_dir.join("frozen_plan.json");
    if target.exists() {
        apply_and_verify_permissions(&target, false).map_err(|e| {
            PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Frozen context target permission check failed: {e}"))
        })?;
        let existing = fs::read(&target).map_err(|e| {
            PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Failed to read existing frozen context artifact: {e}"))
        })?;
        if existing == bytes {
            return Ok((FROZEN_SNAPSHOT_ARTIFACT_REF.to_string(), digest));
        }
        return Err(PlanConvergenceError::new("PC_ARTIFACT_CONFLICT", "Frozen context artifact already exists with different bytes"));
    }
    let temp = context_dir.join(format!(".tmp-frozen-{}", Uuid::new_v4()));
    fs::write(&temp, b"").map_err(|e| PlanConvergenceError::new("PC_PERSISTENCE_FAILED", e.to_string()))?;
    apply_and_verify_permissions(&temp, false).map_err(|e| {
        let _ = fs::remove_file(&temp);
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Frozen context temp permission check failed: {e}"))
    })?;
    fs::write(&temp, &bytes).map_err(|e| {
        let _ = fs::remove_file(&temp);
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Failed to write frozen context artifact: {e}"))
    })?;
    fs::rename(&temp, &target).map_err(|e| {
        let _ = fs::remove_file(&temp);
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Failed to publish frozen context artifact: {e}"))
    })?;
    apply_and_verify_permissions(&target, false).map_err(|e| {
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Frozen context target permission check failed: {e}"))
    })?;
    Ok((FROZEN_SNAPSHOT_ARTIFACT_REF.to_string(), digest))
}

pub fn load_and_verify_frozen_run_snapshot(
    runs_dir: &Path,
    run_id: &str,
    artifact_ref: &str,
    expected_digest: &str,
) -> Result<FrozenPlanSnapshot, PlanConvergenceError> {
    validate_run_id(run_id).map_err(|e| PlanConvergenceError::new("PC_INVALID_RUN_ID", e))?;
    if artifact_ref != FROZEN_SNAPSHOT_ARTIFACT_REF {
        return Err(PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", "Unexpected frozen context artifact reference"));
    }
    let path = resolve_safe_sidecar_path(&runs_dir.join(run_id), artifact_ref)?;
    let bytes = fs::read(&path).map_err(|e| PlanConvergenceError::new("PC_ARTIFACT_NOT_FOUND", e.to_string()))?;
    if sha256_hex(&bytes) != expected_digest {
        return Err(PlanConvergenceError::new("PC_DIGEST_MISMATCH", "Frozen context artifact digest mismatch"));
    }
    let artifact: FrozenRunSnapshotArtifact = serde_json::from_slice(&bytes).map_err(|e| {
        PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", format!("Failed to parse frozen context artifact: {e}"))
    })?;
    if artifact.schema_version != 1 || artifact.run_id != run_id {
        return Err(PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", "Frozen context artifact identity mismatch"));
    }
    let payload = artifact.snapshot.to_frozen_plan_payload().ok_or_else(|| {
        PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", "Frozen context artifact has no effective payload")
    })?;
    payload.validate_against_plan_context(&artifact.snapshot.plan_context).map_err(|e| {
        PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", e)
    })?;
    Ok(artifact.snapshot)
}

/// Verifies that a path is strictly contained within `base_dir` and has no traversal components.
fn resolve_safe_sidecar_path(base_dir: &Path, rel_path: &str) -> Result<PathBuf, PlanConvergenceError> {
    let candidate = Path::new(rel_path);
    for comp in candidate.components() {
        if matches!(comp, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            return Err(PlanConvergenceError::new(
                "PC_PATH_TRAVERSAL",
                format!("Path traversal forbidden in artifact reference: '{rel_path}'"),
            ));
        }
    }
    let target = base_dir.join(candidate);
    Ok(target)
}
/// Saves an immutable PlanCandidate artifact to the run's protected sidecar directory.
pub fn save_candidate_artifact(
    runs_dir: &Path,
    run_id: &str,
    candidate: &PlanCandidate,
) -> Result<(String, String), PlanConvergenceError> {
    validate_run_id(run_id).map_err(|e| PlanConvergenceError::new("PC_INVALID_RUN_ID", e))?;

    let sidecar_dir = runs_dir.join(run_id);
    let candidates_dir = sidecar_dir.join("artifacts").join("candidates");
    fs::create_dir_all(&candidates_dir).map_err(|e| {
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to create candidates artifact directory: {e}"),
        )
    })?;
    apply_and_verify_permissions(&candidates_dir, true).map_err(|e| {
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Permission check failed: {e}"))
    })?;

    let filename = format!("{}_{}.json", candidate.sequence, candidate.candidate_id);
    let target_path = candidates_dir.join(&filename);
    let artifact_ref = format!("artifacts/candidates/{filename}");

    let serialized = serde_json::to_vec_pretty(candidate).map_err(|e| {
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to serialize candidate artifact: {e}"),
        )
    })?;

    if serialized.len() > MAX_ARTIFACT_BYTES {
        return Err(PlanConvergenceError::new(
            "PC_CANDIDATE_OVERSIZED",
            format!("Candidate artifact size {} exceeds 1 MiB limit", serialized.len()),
        ));
    }

    let digest = sha256_hex(&serialized);

    // Atomic write via temp file
    let tmp_path = candidates_dir.join(format!(".tmp-candidate-{}", Uuid::new_v4()));
    fs::write(&tmp_path, &serialized).map_err(|e| {
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to write candidate temp file: {e}"),
        )
    })?;
    apply_and_verify_permissions(&tmp_path, false).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Permission check failed: {e}"))
    })?;

    fs::rename(&tmp_path, &target_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to rename candidate temp file to target: {e}"),
        )
    })?;

    apply_and_verify_permissions(&target_path, false).map_err(|e| {
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Permission check failed: {e}"))
    })?;

    Ok((artifact_ref, digest))
}

/// Saves an immutable ReviewVerdict artifact to the run's protected sidecar directory.
pub fn save_verdict_artifact(
    runs_dir: &Path,
    run_id: &str,
    verdict: &ReviewVerdict,
) -> Result<(String, String), PlanConvergenceError> {
    validate_run_id(run_id).map_err(|e| PlanConvergenceError::new("PC_INVALID_RUN_ID", e))?;

    let sidecar_dir = runs_dir.join(run_id);
    let reviews_dir = sidecar_dir.join("artifacts").join("reviews");
    fs::create_dir_all(&reviews_dir).map_err(|e| {
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to create reviews artifact directory: {e}"),
        )
    })?;
    apply_and_verify_permissions(&reviews_dir, true).map_err(|e| {
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Permission check failed: {e}"))
    })?;

    let filename = format!(
        "{}_{}.json",
        verdict.reviewed_candidate.sequence, verdict.reviewed_candidate.candidate_id
    );
    let target_path = reviews_dir.join(&filename);
    let artifact_ref = format!("artifacts/reviews/{filename}");

    let serialized = serde_json::to_vec_pretty(verdict).map_err(|e| {
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to serialize verdict artifact: {e}"),
        )
    })?;

    if serialized.len() > MAX_ARTIFACT_BYTES {
        return Err(PlanConvergenceError::new(
            "PC_VERDICT_OVERSIZED",
            format!("Verdict artifact size {} exceeds 1 MiB limit", serialized.len()),
        ));
    }

    let digest = sha256_hex(&serialized);

    let tmp_path = reviews_dir.join(format!(".tmp-verdict-{}", Uuid::new_v4()));
    fs::write(&tmp_path, &serialized).map_err(|e| {
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to write verdict temp file: {e}"),
        )
    })?;
    apply_and_verify_permissions(&tmp_path, false).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Permission check failed: {e}"))
    })?;

    fs::rename(&tmp_path, &target_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to rename verdict temp file to target: {e}"),
        )
    })?;

    apply_and_verify_permissions(&target_path, false).map_err(|e| {
        PlanConvergenceError::new("PC_PERSISTENCE_FAILED", format!("Permission check failed: {e}"))
    })?;

    Ok((artifact_ref, digest))
}

/// Loads and verifies a candidate artifact by path reference and SHA-256 digest.
pub fn load_and_verify_candidate(
    runs_dir: &Path,
    run_id: &str,
    artifact_ref: &str,
    expected_digest: &str,
) -> Result<PlanCandidate, PlanConvergenceError> {
    validate_run_id(run_id).map_err(|e| PlanConvergenceError::new("PC_INVALID_RUN_ID", e))?;
    let sidecar_dir = runs_dir.join(run_id);
    let path = resolve_safe_sidecar_path(&sidecar_dir, artifact_ref)?;

    if !path.exists() {
        return Err(PlanConvergenceError::new(
            "PC_ARTIFACT_NOT_FOUND",
            format!("Candidate artifact not found at '{}'", path.display()),
        ));
    }

    let bytes = fs::read(&path).map_err(|e| {
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to read candidate artifact '{}': {e}", path.display()),
        )
    })?;

    let actual_digest = sha256_hex(&bytes);
    if actual_digest != expected_digest {
        return Err(PlanConvergenceError::new(
            "PC_DIGEST_MISMATCH",
            format!(
                "Candidate artifact digest mismatch: expected '{expected_digest}', found '{actual_digest}'"
            ),
        ));
    }

    let candidate: PlanCandidate = serde_json::from_slice(&bytes).map_err(|e| {
        PlanConvergenceError::new(
            "PC_CORRUPT_ARTIFACT",
            format!("Failed to deserialize candidate artifact: {e}"),
        )
    })?;

    if candidate.schema_version != 1 || candidate.run_id != run_id {
        return Err(PlanConvergenceError::new(
            "PC_CORRUPT_ARTIFACT",
            format!("Unsupported candidate schema_version {}", candidate.schema_version),
        ));
    }

    Ok(candidate)
}

/// Loads and verifies a verdict artifact by path reference and SHA-256 digest.
pub fn load_and_verify_verdict(
    runs_dir: &Path,
    run_id: &str,
    artifact_ref: &str,
    expected_digest: &str,
) -> Result<ReviewVerdict, PlanConvergenceError> {
    validate_run_id(run_id).map_err(|e| PlanConvergenceError::new("PC_INVALID_RUN_ID", e))?;
    let sidecar_dir = runs_dir.join(run_id);
    let path = resolve_safe_sidecar_path(&sidecar_dir, artifact_ref)?;

    if !path.exists() {
        return Err(PlanConvergenceError::new(
            "PC_ARTIFACT_NOT_FOUND",
            format!("Verdict artifact not found at '{}'", path.display()),
        ));
    }

    let bytes = fs::read(&path).map_err(|e| {
        PlanConvergenceError::new(
            "PC_PERSISTENCE_FAILED",
            format!("Failed to read verdict artifact '{}': {e}", path.display()),
        )
    })?;

    let actual_digest = sha256_hex(&bytes);
    if actual_digest != expected_digest {
        return Err(PlanConvergenceError::new(
            "PC_DIGEST_MISMATCH",
            format!(
                "Verdict artifact digest mismatch: expected '{expected_digest}', found '{actual_digest}'"
            ),
        ));
    }

    let verdict: ReviewVerdict = serde_json::from_slice(&bytes).map_err(|e| {
        PlanConvergenceError::new(
            "PC_CORRUPT_ARTIFACT",
            format!("Failed to deserialize verdict artifact: {e}"),
        )
    })?;

    if verdict.schema_version != 1 {
        return Err(PlanConvergenceError::new(
            "PC_CORRUPT_ARTIFACT",
            format!("Unsupported verdict schema_version {}", verdict.schema_version),
        ));
    }

    Ok(verdict)
}

/// Loads and verifies all candidate and verdict artifacts referenced in an audit trail.
pub fn load_and_verify_run_artifacts(
    runs_dir: &Path,
    run_id: &str,
    audit_trail: &[super::types::PlanConvergenceAuditEntry],
) -> Result<(), PlanConvergenceError> {
    for entry in audit_trail {
        if matches!(entry.event_type.as_str(), "candidate_created" | "verdict_recorded" | "policy_gate_approved" | "frozen_plan_snapshot_recorded")
            && (entry.artifact_ref.is_none() || entry.artifact_digest.is_none()) {
            return Err(PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", "Missing required artifact binding"));
        }
        if let (Some(ref artifact_ref), Some(ref digest)) =
            (&entry.artifact_ref, &entry.artifact_digest)
        {
            if artifact_ref == FROZEN_SNAPSHOT_ARTIFACT_REF {
                let _ = load_and_verify_frozen_run_snapshot(runs_dir, run_id, artifact_ref, digest)?;
            } else if artifact_ref.starts_with("artifacts/candidates/") {
                let candidate = load_and_verify_candidate(runs_dir, run_id, artifact_ref, digest)?;
                if candidate.sequence != entry.sequence || Some(&candidate.candidate_id) != entry.candidate_id.as_ref() {
                    return Err(PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", "Candidate audit binding mismatch"));
                }
            } else if artifact_ref.starts_with("artifacts/reviews/") {
                let verdict = load_and_verify_verdict(runs_dir, run_id, artifact_ref, digest)?;
                if verdict.reviewed_candidate.sequence != entry.sequence
                    || Some(&verdict.reviewed_candidate.candidate_id) != entry.candidate_id.as_ref() {
                    return Err(PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", "Verdict audit binding mismatch"));
                }
            } else {
                return Err(PlanConvergenceError::new("PC_CORRUPT_ARTIFACT", "Unknown artifact namespace"));
            }
        }
    }
    Ok(())
}
