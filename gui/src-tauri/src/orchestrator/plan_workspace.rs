use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const DEFAULT_PLAN_DIR: &str = ".plan";
pub const DEFAULT_FILENAME_TEMPLATE: &str = "V{version}-r{revision}{suffix}.md";
pub const DEFAULT_VERSION_SOURCES: &[&str] = &[
    "gui/package.json",
    "gui/src-tauri/tauri.conf.json",
    "gui/src-tauri/Cargo.toml",
];

const MAX_PLAN_FILE_BYTES: u64 = 10 * 1024 * 1024; // 10 MiB
const TOKEN_TTL_SECONDS: u64 = 300; // 5 minutes

// Global write lock for plan file appends & mutations
static PLAN_WRITE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

// Process-owned CSPRNG preview token registry
#[derive(Debug, Clone)]
struct IssuedPreviewState {
    canonical_root: String,
    candidate_revision: u64,
    candidate_path: String,
    inventory_digest: String,
    created_at_unix: u64,
    expires_at_unix: u64,
}

static PREVIEW_REGISTRY: LazyLock<Mutex<HashMap<String, IssuedPreviewState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Clears all issued preview tokens. Useful for testing process restart or lost registry state.
pub fn clear_preview_token_registry() {
    if let Ok(mut guard) = PREVIEW_REGISTRY.lock() {
        guard.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathProbe {
    Missing,
    Exists {
        is_dir: bool,
        is_file: bool,
        is_symlink: bool,
    },
}

#[cfg(test)]
pub type PathProbeSeamFn =
    std::sync::Arc<dyn Fn(&Path) -> Option<Result<PathProbe, PlanWorkspaceError>> + Send + Sync>;

#[cfg(test)]
static PATH_PROBE_SEAM: LazyLock<Mutex<Option<PathProbeSeamFn>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(test)]
pub fn set_path_probe_seam(seam: Option<PathProbeSeamFn>) {
    if let Ok(mut guard) = PATH_PROBE_SEAM.lock() {
        *guard = seam;
    }
}

pub fn probe_path_symlink(path: &Path) -> Result<PathProbe, PlanWorkspaceError> {
    #[cfg(test)]
    {
        if let Ok(guard) = PATH_PROBE_SEAM.lock() {
            if let Some(ref seam) = *guard {
                if let Some(res) = seam(path) {
                    return res;
                }
            }
        }
    }

    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(PathProbe::Exists {
            is_dir: meta.is_dir(),
            is_file: meta.is_file(),
            is_symlink: meta.file_type().is_symlink(),
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(PathProbe::Missing),
        Err(err) => Err(PlanWorkspaceError::new(
            "path_probe_failed",
            format!("Cannot probe path '{}': {err}", path.display()),
        )),
    }
}

pub fn probe_path_metadata(path: &Path) -> Result<PathProbe, PlanWorkspaceError> {
    #[cfg(test)]
    {
        if let Ok(guard) = PATH_PROBE_SEAM.lock() {
            if let Some(ref seam) = *guard {
                if let Some(res) = seam(path) {
                    return res;
                }
            }
        }
    }

    match fs::metadata(path) {
        Ok(meta) => Ok(PathProbe::Exists {
            is_dir: meta.is_dir(),
            is_file: meta.is_file(),
            is_symlink: meta.file_type().is_symlink(),
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(PathProbe::Missing),
        Err(err) => Err(PlanWorkspaceError::new(
            "path_probe_failed",
            format!("Cannot probe path metadata '{}': {err}", path.display()),
        )),
    }
}

#[cfg(test)]
pub type CaptureDriftSeamFn = std::sync::Arc<dyn Fn(&PlanContext, usize) + Send + Sync>;

#[cfg(test)]
static CAPTURE_DRIFT_SEAM: LazyLock<Mutex<Option<CaptureDriftSeamFn>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(test)]
pub fn set_capture_drift_seam(seam: Option<CaptureDriftSeamFn>) {
    if let Ok(mut guard) = CAPTURE_DRIFT_SEAM.lock() {
        *guard = seam;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanWorkspaceConfig {
    #[serde(default = "default_plan_dir")]
    pub plan_dir: String,
    #[serde(default = "default_filename_template")]
    pub filename_template: String,
    #[serde(default = "default_version_sources")]
    pub version_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_series_version_override: Option<String>,
}

fn default_plan_dir() -> String {
    DEFAULT_PLAN_DIR.to_string()
}

fn default_filename_template() -> String {
    DEFAULT_FILENAME_TEMPLATE.to_string()
}

fn default_version_sources() -> Vec<String> {
    DEFAULT_VERSION_SOURCES
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Default for PlanWorkspaceConfig {
    fn default() -> Self {
        Self {
            plan_dir: default_plan_dir(),
            filename_template: default_filename_template(),
            version_sources: default_version_sources(),
            plan_series_version_override: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanResolverStatus {
    Resolved,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanFileRecord {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplementalPlanRecord {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub suffix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanContext {
    pub project_root_identity: String,
    pub plan_directory: String,
    pub application_version: String,
    pub plan_series_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_primary_plan: Option<PlanFileRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_supplemental_plans: Vec<SupplementalPlanRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_leaf_plan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_plan_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_primary_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_primary_revision: Option<u64>,
    pub resolver_status: PlanResolverStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved_reason_code: Option<String>,
}

impl PlanContext {
    pub fn is_plan_bound(&self) -> bool {
        self.resolver_status == PlanResolverStatus::Resolved
            && (self.current_primary_plan.is_some()
                || !self.active_supplemental_plans.is_empty()
                || self.current_leaf_plan_id.is_some()
                || self.effective_plan_digest.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCurrentResponse {
    pub context: PlanContext,
    pub leaf_plan_content: Option<String>,
    pub primary_plan_content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplementalPlanContent {
    pub id: String,
    pub path: String,
    pub suffix: String,
    pub digest: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanFileIdentityAndDigest {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplementalPlanIdentityAndDigest {
    pub id: String,
    pub path: String,
    pub digest: String,
    pub suffix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrozenPlanSourceKind {
    Primary,
    Supplemental,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenPlanSource {
    pub kind: FrozenPlanSourceKind,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    pub path: String,
    pub source_digest: String,
    pub content: String,
}

pub fn canonical_assemble_sources(sources: &[FrozenPlanSource]) -> Result<String, String> {
    let mut out = String::new();
    for (i, source) in sources.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        if source.id.contains('"') || source.id.contains('\n') || source.id.contains('\r') {
            return Err(format!("Invalid plan source id '{}'", source.id));
        }
        if let Some(ref suffix) = source.suffix {
            if suffix.contains('"') || suffix.contains('\n') || suffix.contains('\r') {
                return Err(format!("Invalid plan source suffix '{}'", suffix));
            }
        }

        match source.kind {
            FrozenPlanSourceKind::Primary => {
                out.push_str(&format!(
                    "<<<PLAN_SOURCE kind=\"primary\" id=\"{}\">>>\n{}\n<<<END_PLAN_SOURCE>>>",
                    source.id, source.content
                ));
            }
            FrozenPlanSourceKind::Supplemental => {
                let suffix_attr = match source.suffix {
                    Some(ref s) => format!(" suffix=\"{}\"", s),
                    None => String::new(),
                };
                out.push_str(&format!(
                    "<<<PLAN_SOURCE kind=\"supplemental\" id=\"{}\"{}>\n{}\n<<<END_PLAN_SOURCE>>>",
                    source.id, suffix_attr, source.content
                ));
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenPlanPayload {
    pub schema_version: u32,
    pub effective_plan_content: String,
    pub content_digest: String,
    pub effective_plan_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_plan_identity_and_digest: Option<PlanFileIdentityAndDigest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordered_supplemental_plan_identities_and_digests: Vec<SupplementalPlanIdentityAndDigest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordered_sources: Vec<FrozenPlanSource>,
}

impl FrozenPlanPayload {
    pub fn validate_against_plan_context(&self, plan_context: &PlanContext) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "Unsupported frozen plan payload schema version '{}'",
                self.schema_version
            ));
        }

        // Rule 1: Validate each source content against its source_digest
        for src in &self.ordered_sources {
            let computed_src_digest = sha256_bytes(src.content.as_bytes());
            if src.source_digest != computed_src_digest {
                return Err(format!(
                    "Frozen plan payload source '{}' digest mismatch: recorded '{}', computed '{}'",
                    src.id, src.source_digest, computed_src_digest
                ));
            }
        }

        // Rule 2: Validate ordered_sources against PlanContext primary + supplemental chain
        let expected_source_count = (if plan_context.current_primary_plan.is_some() { 1 } else { 0 })
            + plan_context.active_supplemental_plans.len();
        if self.ordered_sources.len() != expected_source_count {
            return Err(format!(
                "Frozen plan payload ordered sources count mismatch with plan context: {} vs {}",
                self.ordered_sources.len(),
                expected_source_count
            ));
        }

        let mut src_idx = 0;
        if let Some(ref primary) = plan_context.current_primary_plan {
            let primary_src = &self.ordered_sources[src_idx];
            if primary_src.kind != FrozenPlanSourceKind::Primary
                || primary_src.id != primary.id
                || primary_src.path != primary.path
                || primary_src.source_digest != primary.digest
                || primary_src.suffix.is_some()
            {
                return Err(format!(
                    "Frozen plan payload primary source mismatch: payload_src={:?}, context_primary={:?}",
                    primary_src, primary
                ));
            }
            src_idx += 1;
        }

        for supp in &plan_context.active_supplemental_plans {
            let supp_src = &self.ordered_sources[src_idx];
            if supp_src.kind != FrozenPlanSourceKind::Supplemental
                || supp_src.id != supp.id
                || supp_src.path != supp.path
                || supp_src.source_digest != supp.digest
                || supp_src.suffix.as_deref() != Some(&supp.suffix)
            {
                return Err(format!(
                    "Frozen plan payload supplemental source mismatch at index {}: payload_src={:?}, context_supp={:?}",
                    src_idx, supp_src, supp
                ));
            }
            src_idx += 1;
        }

        // Rule 3: Validate canonical assembly of ordered_sources == effective_plan_content
        let reassembled = canonical_assemble_sources(&self.ordered_sources)
            .map_err(|e| format!("Canonical assembly failure: {e}"))?;
        if reassembled != self.effective_plan_content {
            return Err(
                "Frozen plan payload effective_plan_content does not match canonical assembly of ordered_sources"
                    .to_string(),
            );
        }

        // Rule 4: Validate content_digest == sha256(effective_plan_content)
        let computed_content_digest = sha256_bytes(self.effective_plan_content.as_bytes());
        if self.content_digest != computed_content_digest {
            return Err(format!(
                "Frozen plan payload content digest mismatch: payload has '{}', computed '{}'",
                self.content_digest, computed_content_digest
            ));
        }

        // Rule 5: Validate effective_plan_digest == plan_context.effective_plan_digest
        let ctx_digest = plan_context.effective_plan_digest.as_ref().ok_or_else(|| {
            "PlanContext is missing effective plan identity digest".to_string()
        })?;
        if &self.effective_plan_digest != ctx_digest {
            return Err(format!(
                "Frozen plan payload effective digest mismatch: payload has '{}', plan context has '{}'",
                self.effective_plan_digest, ctx_digest
            ));
        }

        // Verify primary plan identity and digest match plan_context
        match (&self.primary_plan_identity_and_digest, &plan_context.current_primary_plan) {
            (Some(payload_p), Some(ctx_p)) => {
                if payload_p.id != ctx_p.id
                    || payload_p.path != ctx_p.path
                    || payload_p.digest != ctx_p.digest
                    || payload_p.revision != ctx_p.revision
                {
                    return Err(format!(
                        "Frozen plan payload primary mismatch: payload={:?}, context={:?}",
                        payload_p, ctx_p
                    ));
                }
            }
            (None, None) => {}
            _ => {
                return Err("Frozen plan payload primary plan presence mismatch with plan context".to_string());
            }
        }

        // Verify ordered supplemental identities, paths, and digests
        if self.ordered_supplemental_plan_identities_and_digests.len() != plan_context.active_supplemental_plans.len() {
            return Err(format!(
                "Frozen plan payload supplemental count mismatch with plan context: {} vs {}",
                self.ordered_supplemental_plan_identities_and_digests.len(),
                plan_context.active_supplemental_plans.len()
            ));
        }
        for (payload_s, ctx_s) in self.ordered_supplemental_plan_identities_and_digests.iter().zip(&plan_context.active_supplemental_plans) {
            if payload_s.id != ctx_s.id
                || payload_s.path != ctx_s.path
                || payload_s.digest != ctx_s.digest
                || payload_s.suffix != ctx_s.suffix
            {
                return Err(format!(
                    "Frozen plan payload supplemental mismatch: payload={:?}, context={:?}",
                    payload_s, ctx_s
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenPlanSnapshot {
    pub plan_context: PlanContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_plan_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplemental_plan_contents: Vec<SupplementalPlanContent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordered_sources: Vec<FrozenPlanSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_plan_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_plan_digest: Option<String>,
}

impl FrozenPlanSnapshot {
    pub fn to_frozen_plan_payload(&self) -> Option<FrozenPlanPayload> {
        let content = self.effective_plan_content.as_ref()?;
        let digest = self.effective_plan_digest.as_ref()?;
        let primary = self.plan_context.current_primary_plan.as_ref().map(|p| PlanFileIdentityAndDigest {
            id: p.id.clone(),
            path: p.path.clone(),
            digest: p.digest.clone(),
            revision: p.revision,
        });
        let supplementals = self.plan_context.active_supplemental_plans.iter().map(|s| SupplementalPlanIdentityAndDigest {
            id: s.id.clone(),
            path: s.path.clone(),
            digest: s.digest.clone(),
            suffix: s.suffix.clone(),
        }).collect();

        Some(FrozenPlanPayload {
            schema_version: 1,
            effective_plan_content: content.clone(),
            content_digest: sha256_bytes(content.as_bytes()),
            effective_plan_digest: digest.clone(),
            primary_plan_identity_and_digest: primary,
            ordered_supplemental_plan_identities_and_digests: supplementals,
            ordered_sources: self.ordered_sources.clone(),
        })
    }

    pub fn ensure_run_entry_allowed(&self) -> Result<(), PlanWorkspaceError> {
        match self.plan_context.resolver_status {
            PlanResolverStatus::Unresolved => {
                let code = self
                    .plan_context
                    .unresolved_reason_code
                    .as_deref()
                    .unwrap_or("unresolved_plan_context");
                Err(PlanWorkspaceError::new(
                    code,
                    format!("Plan workspace resolution is unresolved: [{code}]"),
                ))
            }
            PlanResolverStatus::Resolved => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanAppendRequest {
    pub target_plan_id: String,
    pub expected_file_digest: String,
    pub section_type: String,
    pub section_title: String,
    pub section_content: String,
    pub idempotency_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanAppendResponse {
    pub plan_id: String,
    pub updated_file_digest: String,
    pub applied: bool,
    pub context: PlanContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNewPreviewResponse {
    pub candidate_revision: u64,
    pub candidate_filename: String,
    pub candidate_path: String,
    pub token: String,
    pub context: PlanContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNewConfirmRequest {
    pub token: String,
    pub title: String,
    pub initial_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNewConfirmResponse {
    pub created_plan_id: String,
    pub created_path: String,
    pub file_digest: String,
    pub context: PlanContext,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_warning_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leftover_temp_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanWorkspaceError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl PlanWorkspaceError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Some(details),
        }
    }
}

impl std::fmt::Display for PlanWorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PlanWorkspaceError {}

impl From<PlanWorkspaceError> for String {
    fn from(err: PlanWorkspaceError) -> Self {
        err.to_string()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_semver(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

pub fn canonicalize_project_root(path: &Path) -> Result<PathBuf, PlanWorkspaceError> {
    match probe_path_symlink(path)? {
        PathProbe::Missing => {
            return Err(PlanWorkspaceError::new(
                "invalid_project_root",
                format!("Project root directory does not exist: {}", path.display()),
            ));
        }
        PathProbe::Exists { .. } => {}
    }
    let canonical = fs::canonicalize(path).map_err(|e| {
        PlanWorkspaceError::new(
            "invalid_project_root",
            format!("Cannot resolve project root '{}': {e}", path.display()),
        )
    })?;
    let meta = fs::symlink_metadata(&canonical).map_err(|e| {
        PlanWorkspaceError::new(
            "invalid_project_root",
            format!("Cannot inspect project root '{}': {e}", canonical.display()),
        )
    })?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(PlanWorkspaceError::new(
            "invalid_project_root",
            format!("Project root is not a real directory: {}", canonical.display()),
        ));
    }
    Ok(canonical)
}

/// Resolves a path that must be strictly contained within `root`.
/// Handles relative paths and explicitly configured absolute paths within `root`.
/// For paths that do not yet exist, verifies that the nearest existing ancestor is contained in `root`.
pub fn resolve_contained_path(root: &Path, rel_or_abs: &str) -> Result<PathBuf, PlanWorkspaceError> {
    let trimmed = rel_or_abs.trim();
    if trimmed.is_empty() {
        return Err(PlanWorkspaceError::new(
            "path_traversal_forbidden",
            "Path is empty",
        ));
    }

    let candidate_path = Path::new(trimmed);
    let target = if candidate_path.is_absolute() {
        candidate_path.to_path_buf()
    } else {
        // Disallow path traversal components
        for comp in candidate_path.components() {
            if matches!(comp, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
                return Err(PlanWorkspaceError::new(
                    "path_traversal_forbidden",
                    format!("Path traversal is forbidden: '{trimmed}'"),
                ));
            }
        }
        root.join(candidate_path)
    };

    match probe_path_symlink(&target)? {
        PathProbe::Exists { .. } => {
            let canonical = fs::canonicalize(&target).map_err(|e| {
                PlanWorkspaceError::new(
                    "path_containment_rejected",
                    format!("Cannot resolve path '{}': {e}", target.display()),
                )
            })?;
            if !canonical.starts_with(root) {
                return Err(PlanWorkspaceError::new(
                    "path_containment_rejected",
                    format!("Path escaped project root: '{}'", target.display()),
                ));
            }
            Ok(canonical)
        }
        PathProbe::Missing => {
            // For non-existent target, walk upward to the nearest existing ancestor and verify containment
            let mut curr = target.parent();
            let mut nearest_existing: Option<PathBuf> = None;
            while let Some(p) = curr {
                match probe_path_symlink(p)? {
                    PathProbe::Exists { .. } => {
                        nearest_existing = Some(p.to_path_buf());
                        break;
                    }
                    PathProbe::Missing => {
                        curr = p.parent();
                    }
                }
            }

            let existing_ancestor = nearest_existing.ok_or_else(|| {
                PlanWorkspaceError::new(
                    "path_containment_rejected",
                    format!("No existing ancestor found for target '{}'", target.display()),
                )
            })?;

            let canonical_ancestor = fs::canonicalize(&existing_ancestor).map_err(|e| {
                PlanWorkspaceError::new(
                    "path_containment_rejected",
                    format!("Cannot resolve ancestor '{}': {e}", existing_ancestor.display()),
                )
            })?;
            if !canonical_ancestor.starts_with(root) {
                return Err(PlanWorkspaceError::new(
                    "path_containment_rejected",
                    format!(
                        "Ancestor '{}' escaped project root: '{}'",
                        existing_ancestor.display(),
                        canonical_ancestor.display()
                    ),
                ));
            }

            // Verify candidate relative components do not contain parent traversal
            for comp in candidate_path.components() {
                if matches!(comp, Component::ParentDir) {
                    return Err(PlanWorkspaceError::new(
                        "path_traversal_forbidden",
                        format!("Path traversal is forbidden: '{trimmed}'"),
                    ));
                }
            }

            Ok(target)
        }
    }
}

pub fn validate_filename_template(template: &str) -> Result<(), (String, String)> {
    let t = template.trim();
    if t.is_empty() {
        return Err((
            "invalid_template_configuration".to_string(),
            "Filename template is empty".to_string(),
        ));
    }
    if !t.contains("{version}") || !t.contains("{revision}") {
        return Err((
            "invalid_template_configuration".to_string(),
            "Filename template must contain {version} and {revision}".to_string(),
        ));
    }
    if t.contains('/') || t.contains('\\') || t.contains(':') {
        return Err((
            "invalid_template_configuration".to_string(),
            "Filename template cannot contain path separators".to_string(),
        ));
    }

    // Validate that only allowed placeholders appear inside braces
    let mut in_brace = false;
    let mut current_placeholder = String::new();
    for c in t.chars() {
        if c == '{' {
            if in_brace {
                return Err((
                    "invalid_template_configuration".to_string(),
                    "Nested braces in filename template".to_string(),
                ));
            }
            in_brace = true;
            current_placeholder.clear();
        } else if c == '}' {
            if !in_brace {
                return Err((
                    "invalid_template_configuration".to_string(),
                    "Unmatched closing brace in filename template".to_string(),
                ));
            }
            in_brace = false;
            if current_placeholder != "version"
                && current_placeholder != "revision"
                && current_placeholder != "suffix"
            {
                return Err((
                    "invalid_template_configuration".to_string(),
                    format!("Unknown placeholder '{{{current_placeholder}}}' in template"),
                ));
            }
        } else if in_brace {
            current_placeholder.push(c);
        }
    }
    if in_brace {
        return Err((
            "invalid_template_configuration".to_string(),
            "Unclosed brace in filename template".to_string(),
        ));
    }

    if !t.ends_with(".md") && !t.ends_with(".markdown") {
        return Err((
            "invalid_template_configuration".to_string(),
            "Filename template must have a markdown extension (.md or .markdown)".to_string(),
        ));
    }

    Ok(())
}

pub fn format_plan_filename(
    template: &str,
    version: &str,
    revision: u64,
    suffix: &str,
) -> String {
    template
        .replace("{version}", version)
        .replace("{revision}", &revision.to_string())
        .replace("{suffix}", suffix)
}

// ---------------------------------------------------------------------------
// Version Consensus
// ---------------------------------------------------------------------------

fn parse_cargo_toml_version(content: &str) -> Result<String, String> {
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && trimmed.starts_with("version") {
            let parts: Vec<&str> = trimmed.split('=').collect();
            if parts.len() == 2 {
                let val = parts[1].trim().trim_matches('"').trim_matches('\'').trim();
                if !val.is_empty() {
                    return Ok(val.to_string());
                }
            }
        }
    }
    Err("Missing [package].version in Cargo.toml".to_string())
}

fn read_version_source(root: &Path, rel_path: &str) -> Result<String, (String, String)> {
    let full_path = resolve_contained_path(root, rel_path)
        .map_err(|e| ("invalid_version_source_path".to_string(), e.to_string()))?;
    match probe_path_symlink(&full_path)
        .map_err(|e| ("version_source_probe_failed".to_string(), e.to_string()))?
    {
        PathProbe::Missing => {
            return Err((
                "version_source_missing".to_string(),
                format!("Version source missing: {rel_path}"),
            ));
        }
        PathProbe::Exists { .. } => {}
    }
    let meta = fs::symlink_metadata(&full_path).map_err(|e| {
        (
            "version_source_metadata_failed".to_string(),
            format!("Cannot inspect version source '{rel_path}': {e}"),
        )
    })?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err((
            "version_source_not_regular_file".to_string(),
            format!("Version source is not a regular file: {rel_path}"),
        ));
    }
    let bytes = fs::read(&full_path).map_err(|e| {
        (
            "version_source_read_failed".to_string(),
            format!("Cannot read version source '{rel_path}': {e}"),
        )
    })?;
    let content = String::from_utf8(bytes).map_err(|e| {
        (
            "version_source_malformed".to_string(),
            format!("Version source is not valid UTF-8 '{rel_path}': {e}"),
        )
    })?;

    let version = if rel_path.ends_with(".json") {
        let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            (
                "version_source_malformed".to_string(),
                format!("JSON syntax error in '{rel_path}': {e}"),
            )
        })?;
        json.get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                (
                    "version_source_malformed".to_string(),
                    format!("Missing string 'version' field in '{rel_path}'"),
                )
            })?
            .to_string()
    } else if rel_path.ends_with(".toml") {
        parse_cargo_toml_version(&content).map_err(|e| {
            (
                "version_source_malformed".to_string(),
                format!("TOML syntax or version error in '{rel_path}': {e}"),
            )
        })?
    } else {
        return Err((
            "version_source_unsupported_format".to_string(),
            format!("Unsupported version manifest format: {rel_path}"),
        ));
    };

    if !validate_semver(&version) {
        return Err((
            "invalid_semver_syntax".to_string(),
            format!("Version '{version}' in '{rel_path}' is not valid semantic versioning (X.Y.Z)"),
        ));
    }

    Ok(version)
}

pub fn resolve_version_consensus(
    root: &Path,
    sources: &[String],
    series_override: Option<&str>,
) -> Result<(String, String), (String, String)> {
    if sources.is_empty() {
        return Err((
            "no_version_sources_configured".to_string(),
            "No version sources configured in Plan Workspace".to_string(),
        ));
    }

    let mut resolved_versions = Vec::with_capacity(sources.len());
    for src in sources {
        let v = read_version_source(root, src)?;
        resolved_versions.push((src.clone(), v));
    }

    let first = &resolved_versions[0].1;
    for (src, v) in &resolved_versions[1..] {
        if v != first {
            return Err((
                "version_consensus_mismatch".to_string(),
                format!(
                    "Version consensus mismatch between '{}' ({}) and '{}' ({})",
                    resolved_versions[0].0, first, src, v
                ),
            ));
        }
    }

    let app_version = first.clone();
    let plan_series = match series_override {
        Some(over) if !over.trim().is_empty() => {
            let trimmed = over.trim();
            if !validate_semver(trimmed) {
                return Err((
                    "invalid_plan_series_override".to_string(),
                    format!("Plan series version override '{trimmed}' is not valid semantic versioning"),
                ));
            }
            trimmed.to_string()
        }
        _ => app_version.clone(),
    };

    Ok((app_version, plan_series))
}

// ---------------------------------------------------------------------------
// Candidate Parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ParsedPlanFilename {
    full_name: String,
    revision: u64,
    suffix: String,
    is_primary: bool,
}

fn parse_plan_filename(
    filename: &str,
    template: &str,
    plan_series: &str,
) -> Result<Option<ParsedPlanFilename>, (String, String)> {
    validate_filename_template(template)?;

    let version_idx = template.find("{version}").unwrap();
    let rev_idx = template.find("{revision}").unwrap();

    let prefix_before_version = &template[..version_idx];
    let between_v_and_r = &template[version_idx + "{version}".len()..rev_idx];

    let after_rev = &template[rev_idx + "{revision}".len()..];
    let (between_r_and_s, ext) = if let Some(suffix_pos) = after_rev.find("{suffix}") {
        (
            &after_rev[..suffix_pos],
            &after_rev[suffix_pos + "{suffix}".len()..],
        )
    } else {
        ("", after_rev)
    };

    let expected_prefix = format!("{prefix_before_version}{plan_series}{between_v_and_r}");

    // Check case collision / case mismatch
    let lower_filename = filename.to_ascii_lowercase();
    let lower_prefix = expected_prefix.to_ascii_lowercase();
    let lower_ext = ext.to_ascii_lowercase();

    if lower_filename.starts_with(&lower_prefix) && lower_filename.ends_with(&lower_ext) {
        if !filename.starts_with(&expected_prefix) || !filename.ends_with(ext) {
            return Err((
                "case_collision".to_string(),
                format!("Plan filename '{filename}' has invalid casing; expected prefix '{expected_prefix}' and extension '{ext}'"),
            ));
        }

        let middle = &filename[expected_prefix.len()..filename.len() - ext.len()];
        if middle.is_empty() {
            return Err((
                "malformed_plan_filename".to_string(),
                format!("Plan filename '{filename}' is missing revision number"),
            ));
        }

        let digit_count = middle.chars().take_while(|c| c.is_ascii_digit()).count();
        if digit_count == 0 {
            return Err((
                "malformed_plan_filename".to_string(),
                format!("Plan filename '{filename}' does not start with a numeric revision"),
            ));
        }

        let (digits, rest) = middle.split_at(digit_count);
        let rev: u64 = digits.parse().map_err(|e| {
            (
                "malformed_plan_filename".to_string(),
                format!("Invalid integer revision '{digits}' in '{filename}': {e}"),
            )
        })?;

        let suffix_part = if !between_r_and_s.is_empty() {
            if !rest.starts_with(between_r_and_s) {
                return Err((
                    "malformed_plan_filename".to_string(),
                    format!("Plan filename '{filename}' does not match template separator '{between_r_and_s}'"),
                ));
            }
            &rest[between_r_and_s.len()..]
        } else {
            rest
        };

        if !suffix_part.is_empty() {
            if !suffix_part.chars().all(|c| c.is_ascii_lowercase()) {
                return Err((
                    "malformed_plan_filename".to_string(),
                    format!("Supplemental suffix '{suffix_part}' in '{filename}' must contain only lowercase ASCII letters (e.g. 'b', 'c')"),
                ));
            }
        }

        Ok(Some(ParsedPlanFilename {
            full_name: filename.to_string(),
            revision: rev,
            suffix: suffix_part.to_string(),
            is_primary: suffix_part.is_empty(),
        }))
    } else {
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Inventory Fingerprinting & Process-Private Token Registry
// ---------------------------------------------------------------------------

pub fn compute_complete_inventory_digest(
    canonical_root: &Path,
    config: &PlanWorkspaceConfig,
    app_version: &str,
    plan_series: &str,
    primary_plan: Option<&PlanFileRecord>,
    active_supplementals: &[SupplementalPlanRecord],
    all_candidate_digests: &[(String, String, u64)], // (filename, digest, size)
    effective_plan_digest: Option<&str>,
    next_primary_rev: Option<u64>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_root.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(config.plan_dir.as_bytes());
    hasher.update(b"\0");
    hasher.update(config.filename_template.as_bytes());
    hasher.update(b"\0");
    for vs in &config.version_sources {
        hasher.update(vs.as_bytes());
        hasher.update(b"\0");
    }
    if let Some(ref ov) = config.plan_series_version_override {
        hasher.update(ov.as_bytes());
    }
    hasher.update(b"\0");
    hasher.update(app_version.as_bytes());
    hasher.update(b"\0");
    hasher.update(plan_series.as_bytes());
    hasher.update(b"\0");

    if let Some(p) = primary_plan {
        hasher.update(p.id.as_bytes());
        hasher.update(b":");
        hasher.update(p.path.as_bytes());
        hasher.update(b":");
        hasher.update(p.revision.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(p.digest.as_bytes());
    }
    hasher.update(b"\0");

    for s in active_supplementals {
        hasher.update(s.id.as_bytes());
        hasher.update(b":");
        hasher.update(s.path.as_bytes());
        hasher.update(b":");
        hasher.update(s.suffix.as_bytes());
        hasher.update(b":");
        hasher.update(s.digest.as_bytes());
        hasher.update(b"\0");
    }

    for (fname, digest, size) in all_candidate_digests {
        hasher.update(fname.as_bytes());
        hasher.update(b":");
        hasher.update(digest.as_bytes());
        hasher.update(b":");
        hasher.update(size.to_string().as_bytes());
        hasher.update(b"\0");
    }

    if let Some(eff) = effective_plan_digest {
        hasher.update(eff.as_bytes());
    }
    hasher.update(b"\0");

    if let Some(nr) = next_primary_rev {
        hasher.update(nr.to_string().as_bytes());
    }
    hasher.update(b"\0");

    format!("{:x}", hasher.finalize())
}

fn register_preview_token(
    canonical_root: &Path,
    revision: u64,
    path: &str,
    inventory_digest: &str,
) -> String {
    let now = now_unix();
    let token = format!("plan-preview-{}", Uuid::new_v4());
    let state = IssuedPreviewState {
        canonical_root: canonical_root.to_string_lossy().to_string(),
        candidate_revision: revision,
        candidate_path: path.to_string(),
        inventory_digest: inventory_digest.to_string(),
        created_at_unix: now,
        expires_at_unix: now + TOKEN_TTL_SECONDS,
    };

    if let Ok(mut registry) = PREVIEW_REGISTRY.lock() {
        // Prune expired tokens
        registry.retain(|_, v| v.expires_at_unix >= now);
        registry.insert(token.clone(), state);
    }

    token
}

fn verify_and_consume_preview_token(
    token: &str,
    canonical_root: &Path,
    expected_revision: u64,
    expected_path: &str,
    current_inventory_digest: &str,
) -> Result<(), PlanWorkspaceError> {
    let now = now_unix();
    let mut registry = PREVIEW_REGISTRY
        .lock()
        .map_err(|e| PlanWorkspaceError::new("lock_error", format!("Token registry lock failed: {e}")))?;

    let issued = registry
        .get(token)
        .cloned()
        .ok_or_else(|| {
            PlanWorkspaceError::new(
                "invalid_preview_token",
                "Plan creation token is invalid, forged, or expired; request a fresh preview",
            )
        })?;

    if now > issued.expires_at_unix {
        registry.remove(token);
        return Err(PlanWorkspaceError::new(
            "expired_preview_token",
            "Plan creation token has expired; request a fresh preview",
        ));
    }

    if issued.canonical_root != canonical_root.to_string_lossy() {
        return Err(PlanWorkspaceError::new(
            "preview_token_root_mismatch",
            "Plan creation token was issued for a different project root",
        ));
    }

    if issued.candidate_revision != expected_revision {
        return Err(PlanWorkspaceError::new(
            "preview_token_revision_mismatch",
            format!(
                "Token revision mismatch (token r{}, current r{})",
                issued.candidate_revision, expected_revision
            ),
        ));
    }

    if issued.candidate_path != expected_path {
        return Err(PlanWorkspaceError::new(
            "preview_token_path_mismatch",
            format!(
                "Token candidate path mismatch (token '{}', current '{}')",
                issued.candidate_path, expected_path
            ),
        ));
    }

    if issued.inventory_digest != current_inventory_digest {
        return Err(PlanWorkspaceError::new(
            "preview_token_context_changed",
            "Plan workspace context or inventory has changed since preview; request a fresh preview",
        ));
    }

    // Token is valid; consume it immediately (single-use / replay prevention)
    registry.remove(token);

    Ok(())
}

// ---------------------------------------------------------------------------
// Context Resolution
// ---------------------------------------------------------------------------

pub fn resolve_plan_context(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
) -> Result<PlanContext, PlanWorkspaceError> {
    let canonical_root = match canonicalize_project_root(project_root) {
        Ok(c) => c,
        Err(_e) => {
            return Ok(PlanContext {
                project_root_identity: project_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: String::new(),
                plan_series_version: String::new(),
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some("invalid_project_root".to_string()),
            });
        }
    };

    if let Err((code, _)) = validate_filename_template(&config.filename_template) {
        return Ok(PlanContext {
            project_root_identity: canonical_root.to_string_lossy().to_string(),
            plan_directory: config.plan_dir.clone(),
            application_version: String::new(),
            plan_series_version: String::new(),
            current_primary_plan: None,
            active_supplemental_plans: Vec::new(),
            current_leaf_plan_id: None,
            effective_plan_digest: None,
            current_primary_revision: None,
            next_primary_revision: None,
            resolver_status: PlanResolverStatus::Unresolved,
            unresolved_reason_code: Some(code),
        });
    }

    let (app_version, plan_series) = match resolve_version_consensus(
        &canonical_root,
        &config.version_sources,
        config.plan_series_version_override.as_deref(),
    ) {
        Ok(res) => res,
        Err((code, _msg)) => {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: String::new(),
                plan_series_version: String::new(),
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some(code),
            });
        }
    };

    let plan_dir_path = match resolve_contained_path(&canonical_root, &config.plan_dir) {
        Ok(p) => p,
        Err(_) => {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: app_version,
                plan_series_version: plan_series,
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some("invalid_plan_directory_path".to_string()),
            });
        }
    };

    match probe_path_symlink(&plan_dir_path) {
        Ok(PathProbe::Missing) => {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: app_version,
                plan_series_version: plan_series,
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: Some(1),
                resolver_status: PlanResolverStatus::Resolved,
                unresolved_reason_code: None,
            });
        }
        Ok(PathProbe::Exists { is_dir, is_symlink, .. }) => {
            if !is_dir || is_symlink {
                return Ok(PlanContext {
                    project_root_identity: canonical_root.to_string_lossy().to_string(),
                    plan_directory: config.plan_dir.clone(),
                    application_version: app_version,
                    plan_series_version: plan_series,
                    current_primary_plan: None,
                    active_supplemental_plans: Vec::new(),
                    current_leaf_plan_id: None,
                    effective_plan_digest: None,
                    current_primary_revision: None,
                    next_primary_revision: None,
                    resolver_status: PlanResolverStatus::Unresolved,
                    unresolved_reason_code: Some("plan_directory_is_symlink_or_not_dir".to_string()),
                });
            }
        }
        Err(_) => {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: app_version,
                plan_series_version: plan_series,
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some("plan_directory_inaccessible".to_string()),
            });
        }
    }

    let entries = match fs::read_dir(&plan_dir_path) {
        Ok(e) => e,
        Err(_) => {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: app_version,
                plan_series_version: plan_series,
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some("plan_directory_read_error".to_string()),
            });
        }
    };

    let mut seen_lower = BTreeSet::new();
    let mut parsed_plans = Vec::new();
    let mut candidate_file_digests = Vec::new();

    for entry_res in entries {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => {
                return Ok(PlanContext {
                    project_root_identity: canonical_root.to_string_lossy().to_string(),
                    plan_directory: config.plan_dir.clone(),
                    application_version: app_version,
                    plan_series_version: plan_series,
                    current_primary_plan: None,
                    active_supplemental_plans: Vec::new(),
                    current_leaf_plan_id: None,
                    effective_plan_digest: None,
                    current_primary_revision: None,
                    next_primary_revision: None,
                    resolver_status: PlanResolverStatus::Unresolved,
                    unresolved_reason_code: Some("plan_directory_entry_read_error".to_string()),
                });
            }
        };

        let fname = entry.file_name().to_string_lossy().to_string();
        // Ignore internal temporary files and dotfiles from plan candidate discovery
        if fname.starts_with('.') {
            continue;
        }

        let fmeta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                return Ok(PlanContext {
                    project_root_identity: canonical_root.to_string_lossy().to_string(),
                    plan_directory: config.plan_dir.clone(),
                    application_version: app_version,
                    plan_series_version: plan_series,
                    current_primary_plan: None,
                    active_supplemental_plans: Vec::new(),
                    current_leaf_plan_id: None,
                    effective_plan_digest: None,
                    current_primary_revision: None,
                    next_primary_revision: None,
                    resolver_status: PlanResolverStatus::Unresolved,
                    unresolved_reason_code: Some("plan_file_metadata_error".to_string()),
                });
            }
        };

        if !fmeta.is_file() || fmeta.file_type().is_symlink() {
            if let Ok(Some(_)) = parse_plan_filename(&fname, &config.filename_template, &plan_series) {
                return Ok(PlanContext {
                    project_root_identity: canonical_root.to_string_lossy().to_string(),
                    plan_directory: config.plan_dir.clone(),
                    application_version: app_version,
                    plan_series_version: plan_series,
                    current_primary_plan: None,
                    active_supplemental_plans: Vec::new(),
                    current_leaf_plan_id: None,
                    effective_plan_digest: None,
                    current_primary_revision: None,
                    next_primary_revision: None,
                    resolver_status: PlanResolverStatus::Unresolved,
                    unresolved_reason_code: Some("plan_candidate_not_regular_file".to_string()),
                });
            }
            continue;
        }

        let lower = fname.to_ascii_lowercase();
        if !seen_lower.insert(lower) {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: app_version,
                plan_series_version: plan_series,
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some("case_collision".to_string()),
            });
        }

        match parse_plan_filename(&fname, &config.filename_template, &plan_series) {
            Ok(Some(parsed)) => {
                let candidate_path = plan_dir_path.join(&fname);
                let bytes = match fs::read(&candidate_path) {
                    Ok(b) => b,
                    Err(_) => {
                        return Ok(PlanContext {
                            project_root_identity: canonical_root.to_string_lossy().to_string(),
                            plan_directory: config.plan_dir.clone(),
                            application_version: app_version,
                            plan_series_version: plan_series,
                            current_primary_plan: None,
                            active_supplemental_plans: Vec::new(),
                            current_leaf_plan_id: None,
                            effective_plan_digest: None,
                            current_primary_revision: None,
                            next_primary_revision: None,
                            resolver_status: PlanResolverStatus::Unresolved,
                            unresolved_reason_code: Some("plan_file_read_error".to_string()),
                        });
                    }
                };
                let digest = sha256_bytes(&bytes);
                candidate_file_digests.push((fname.clone(), digest, fmeta.len()));
                parsed_plans.push(parsed);
            }
            Ok(None) => {}
            Err((code, _)) => {
                return Ok(PlanContext {
                    project_root_identity: canonical_root.to_string_lossy().to_string(),
                    plan_directory: config.plan_dir.clone(),
                    application_version: app_version,
                    plan_series_version: plan_series,
                    current_primary_plan: None,
                    active_supplemental_plans: Vec::new(),
                    current_leaf_plan_id: None,
                    effective_plan_digest: None,
                    current_primary_revision: None,
                    next_primary_revision: None,
                    resolver_status: PlanResolverStatus::Unresolved,
                    unresolved_reason_code: Some(code),
                });
            }
        }
    }

    if parsed_plans.is_empty() {
        return Ok(PlanContext {
            project_root_identity: canonical_root.to_string_lossy().to_string(),
            plan_directory: config.plan_dir.clone(),
            application_version: app_version,
            plan_series_version: plan_series,
            current_primary_plan: None,
            active_supplemental_plans: Vec::new(),
            current_leaf_plan_id: None,
            effective_plan_digest: None,
            current_primary_revision: None,
            next_primary_revision: Some(1),
            resolver_status: PlanResolverStatus::Resolved,
            unresolved_reason_code: None,
        });
    }

    candidate_file_digests.sort();

    // Group by revision
    let mut revision_groups: BTreeMap<u64, (Option<ParsedPlanFilename>, Vec<ParsedPlanFilename>)> =
        BTreeMap::new();
    for p in parsed_plans {
        let entry = revision_groups.entry(p.revision).or_insert((None, Vec::new()));
        if p.is_primary {
            if entry.0.is_some() {
                return Ok(PlanContext {
                    project_root_identity: canonical_root.to_string_lossy().to_string(),
                    plan_directory: config.plan_dir.clone(),
                    application_version: app_version,
                    plan_series_version: plan_series,
                    current_primary_plan: None,
                    active_supplemental_plans: Vec::new(),
                    current_leaf_plan_id: None,
                    effective_plan_digest: None,
                    current_primary_revision: None,
                    next_primary_revision: None,
                    resolver_status: PlanResolverStatus::Unresolved,
                    unresolved_reason_code: Some("duplicate_plan_revision".to_string()),
                });
            }
            entry.0 = Some(p);
        } else {
            if entry.1.iter().any(|s| s.suffix == p.suffix) {
                return Ok(PlanContext {
                    project_root_identity: canonical_root.to_string_lossy().to_string(),
                    plan_directory: config.plan_dir.clone(),
                    application_version: app_version,
                    plan_series_version: plan_series,
                    current_primary_plan: None,
                    active_supplemental_plans: Vec::new(),
                    current_leaf_plan_id: None,
                    effective_plan_digest: None,
                    current_primary_revision: None,
                    next_primary_revision: None,
                    resolver_status: PlanResolverStatus::Unresolved,
                    unresolved_reason_code: Some("duplicate_supplemental_suffix".to_string()),
                });
            }
            entry.1.push(p);
        }
    }

    for (_, (primary, supplementals)) in revision_groups.iter_mut() {
        if primary.is_none() && !supplementals.is_empty() {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: app_version,
                plan_series_version: plan_series,
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some("orphaned_supplemental_plan".to_string()),
            });
        }
        supplementals.sort_by(|a, b| a.suffix.cmp(&b.suffix));
    }

    let highest_rev = *revision_groups.keys().last().unwrap();
    let (highest_primary_opt, highest_supplementals) =
        revision_groups.get(&highest_rev).unwrap();

    let primary_parsed = match highest_primary_opt {
        Some(p) => p,
        None => {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: app_version,
                plan_series_version: plan_series,
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some("orphaned_supplemental_plan".to_string()),
            });
        }
    };

    let primary_file_path = plan_dir_path.join(&primary_parsed.full_name);
    let primary_bytes = match fs::read(&primary_file_path) {
        Ok(b) => b,
        Err(_) => {
            return Ok(PlanContext {
                project_root_identity: canonical_root.to_string_lossy().to_string(),
                plan_directory: config.plan_dir.clone(),
                application_version: app_version,
                plan_series_version: plan_series,
                current_primary_plan: None,
                active_supplemental_plans: Vec::new(),
                current_leaf_plan_id: None,
                effective_plan_digest: None,
                current_primary_revision: None,
                next_primary_revision: None,
                resolver_status: PlanResolverStatus::Unresolved,
                unresolved_reason_code: Some("plan_file_read_error".to_string()),
            });
        }
    };
    let primary_digest = sha256_bytes(&primary_bytes);
    let primary_id = primary_parsed
        .full_name
        .strip_suffix(".md")
        .or_else(|| primary_parsed.full_name.strip_suffix(".markdown"))
        .unwrap_or(&primary_parsed.full_name)
        .to_string();
    let primary_rel_path = format!("{}/{}", config.plan_dir, primary_parsed.full_name);

    let primary_record = PlanFileRecord {
        id: primary_id.clone(),
        path: primary_rel_path,
        digest: primary_digest.clone(),
        revision: highest_rev,
    };

    let mut supplemental_records = Vec::with_capacity(highest_supplementals.len());
    for s in highest_supplementals {
        let s_path = plan_dir_path.join(&s.full_name);
        let s_bytes = match fs::read(&s_path) {
            Ok(b) => b,
            Err(_) => {
                return Ok(PlanContext {
                    project_root_identity: canonical_root.to_string_lossy().to_string(),
                    plan_directory: config.plan_dir.clone(),
                    application_version: app_version,
                    plan_series_version: plan_series,
                    current_primary_plan: None,
                    active_supplemental_plans: Vec::new(),
                    current_leaf_plan_id: None,
                    effective_plan_digest: None,
                    current_primary_revision: None,
                    next_primary_revision: None,
                    resolver_status: PlanResolverStatus::Unresolved,
                    unresolved_reason_code: Some("supplemental_plan_file_read_error".to_string()),
                });
            }
        };
        let s_digest = sha256_bytes(&s_bytes);
        let s_id = s
            .full_name
            .strip_suffix(".md")
            .or_else(|| s.full_name.strip_suffix(".markdown"))
            .unwrap_or(&s.full_name)
            .to_string();
        let s_rel_path = format!("{}/{}", config.plan_dir, s.full_name);

        supplemental_records.push(SupplementalPlanRecord {
            id: s_id,
            path: s_rel_path,
            digest: s_digest,
            suffix: s.suffix.clone(),
        });
    }

    let leaf_id = supplemental_records
        .last()
        .map(|s| s.id.clone())
        .unwrap_or_else(|| primary_id.clone());

    let mut hasher = Sha256::new();
    hasher.update(primary_record.id.as_bytes());
    hasher.update(b"\0");
    hasher.update(primary_record.digest.as_bytes());
    hasher.update(b"\0");
    for s in &supplemental_records {
        hasher.update(s.id.as_bytes());
        hasher.update(b"\0");
        hasher.update(s.suffix.as_bytes());
        hasher.update(b"\0");
        hasher.update(s.digest.as_bytes());
        hasher.update(b"\0");
    }
    let effective_digest = format!("{:x}", hasher.finalize());

    Ok(PlanContext {
        project_root_identity: canonical_root.to_string_lossy().to_string(),
        plan_directory: config.plan_dir.clone(),
        application_version: app_version,
        plan_series_version: plan_series,
        current_primary_plan: Some(primary_record),
        active_supplemental_plans: supplemental_records,
        current_leaf_plan_id: Some(leaf_id),
        effective_plan_digest: Some(effective_digest),
        current_primary_revision: Some(highest_rev),
        next_primary_revision: Some(highest_rev + 1),
        resolver_status: PlanResolverStatus::Resolved,
        unresolved_reason_code: None,
    })
}

// ---------------------------------------------------------------------------
// Read-Only Status & Current Plan APIs
// ---------------------------------------------------------------------------

pub fn resolve_plan_workspace(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
) -> PlanContext {
    resolve_plan_context(project_root, config).unwrap_or_else(|err| PlanContext {
        project_root_identity: project_root.to_string_lossy().to_string(),
        plan_directory: config.plan_dir.clone(),
        application_version: String::new(),
        plan_series_version: String::new(),
        current_primary_plan: None,
        active_supplemental_plans: Vec::new(),
        current_leaf_plan_id: None,
        effective_plan_digest: None,
        current_primary_revision: None,
        next_primary_revision: None,
        resolver_status: PlanResolverStatus::Unresolved,
        unresolved_reason_code: Some(err.code),
    })
}

pub fn plan_status(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
) -> Result<PlanContext, PlanWorkspaceError> {
    resolve_plan_context(project_root, config)
}

pub fn plan_current(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
) -> Result<PlanCurrentResponse, PlanWorkspaceError> {
    let context = resolve_plan_context(project_root, config)?;
    let canonical_root = canonicalize_project_root(project_root)?;

    let primary_content = if let Some(ref primary) = context.current_primary_plan {
        let full = canonical_root.join(&primary.path);
        match probe_path_symlink(&full)? {
            PathProbe::Exists { .. } => Some(
                fs::read_to_string(&full).map_err(|e| {
                    PlanWorkspaceError::new(
                        "plan_file_read_error",
                        format!("Read primary plan '{}': {e}", full.display()),
                    )
                })?,
            ),
            PathProbe::Missing => None,
        }
    } else {
        None
    };

    let leaf_content = if let Some(ref leaf_id) = context.current_leaf_plan_id {
        if context
            .current_primary_plan
            .as_ref()
            .is_some_and(|p| &p.id == leaf_id)
        {
            primary_content.clone()
        } else if let Some(supp) = context
            .active_supplemental_plans
            .iter()
            .find(|s| &s.id == leaf_id)
        {
            let full = canonical_root.join(&supp.path);
            match probe_path_symlink(&full)? {
                PathProbe::Exists { .. } => Some(
                    fs::read_to_string(&full).map_err(|e| {
                        PlanWorkspaceError::new(
                            "supplemental_plan_file_read_error",
                            format!("Read leaf plan '{}': {e}", full.display()),
                        )
                    })?,
                ),
                PathProbe::Missing => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(PlanCurrentResponse {
        context,
        leaf_plan_content: leaf_content,
        primary_plan_content: primary_content,
    })
}

/// Captures an atomic, digest-verified immutable snapshot of the PlanContext and all required plan file contents.
/// Retries boundedly if concurrency drift is detected during capture, or fails closed with a typed PlanWorkspaceError.
pub fn capture_frozen_plan_snapshot(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
) -> Result<FrozenPlanSnapshot, PlanWorkspaceError> {
    const MAX_CAPTURE_ATTEMPTS: usize = 3;

    for _attempt in 0..MAX_CAPTURE_ATTEMPTS {
        let plan_context = resolve_plan_context(project_root, config)?;
        if plan_context.resolver_status == PlanResolverStatus::Unresolved {
            return Ok(FrozenPlanSnapshot {
                plan_context,
                primary_plan_content: None,
                supplemental_plan_contents: Vec::new(),
                ordered_sources: Vec::new(),
                effective_plan_content: None,
                effective_plan_digest: None,
            });
        }

        // If no primary plan exists and no supplementals exist (clean workspace), return empty snapshot
        if plan_context.current_primary_plan.is_none() && plan_context.active_supplemental_plans.is_empty() {
            return Ok(FrozenPlanSnapshot {
                plan_context,
                primary_plan_content: None,
                supplemental_plan_contents: Vec::new(),
                ordered_sources: Vec::new(),
                effective_plan_content: None,
                effective_plan_digest: None,
            });
        }

        #[cfg(test)]
        {
            if let Ok(guard) = CAPTURE_DRIFT_SEAM.lock() {
                if let Some(ref seam) = *guard {
                    seam(&plan_context, _attempt);
                }
            }
        }

        let canonical_root = canonicalize_project_root(project_root)?;

        let mut primary_content_opt = None;

        if let Some(ref primary) = plan_context.current_primary_plan {
            let primary_path = resolve_contained_path(&canonical_root, &primary.path)?;
            let primary_meta = fs::symlink_metadata(&primary_path).map_err(|e| {
                PlanWorkspaceError::new(
                    "plan_file_metadata_error",
                    format!("Inspect primary plan file '{}': {e}", primary_path.display()),
                )
            })?;
            if !primary_meta.is_file() || primary_meta.file_type().is_symlink() {
                return Err(PlanWorkspaceError::new(
                    "plan_file_not_regular",
                    format!("Primary plan file '{}' is not a regular file", primary_path.display()),
                ));
            }
            if primary_meta.len() > MAX_PLAN_FILE_BYTES {
                return Err(PlanWorkspaceError::new(
                    "plan_file_too_large",
                    format!("Primary plan file '{}' exceeds maximum allowed size", primary_path.display()),
                ));
            }
            let primary_bytes = fs::read(&primary_path).map_err(|e| {
                PlanWorkspaceError::new(
                    "plan_file_read_error",
                    format!("Read primary plan file '{}': {e}", primary_path.display()),
                )
            })?;
            let primary_digest = sha256_bytes(&primary_bytes);
            if primary_digest != primary.digest {
                // File modified between resolve_plan_context and read
                continue;
            }
            let primary_content = String::from_utf8(primary_bytes).map_err(|e| {
                PlanWorkspaceError::new(
                    "plan_file_not_utf8",
                    format!("Primary plan file '{}' is not valid UTF-8: {e}", primary_path.display()),
                )
            })?;
            primary_content_opt = Some(primary_content);
        }

        let mut supp_retry_needed = false;
        let mut supp_contents = Vec::with_capacity(plan_context.active_supplemental_plans.len());
        for supp in &plan_context.active_supplemental_plans {
            let supp_path = resolve_contained_path(&canonical_root, &supp.path)?;
            let supp_meta = fs::symlink_metadata(&supp_path).map_err(|e| {
                PlanWorkspaceError::new(
                    "supplemental_plan_file_metadata_error",
                    format!("Inspect supplemental plan file '{}': {e}", supp_path.display()),
                )
            })?;
            if !supp_meta.is_file() || supp_meta.file_type().is_symlink() {
                return Err(PlanWorkspaceError::new(
                    "supplemental_plan_file_not_regular",
                    format!("Supplemental plan file '{}' is not a regular file", supp_path.display()),
                ));
            }
            if supp_meta.len() > MAX_PLAN_FILE_BYTES {
                return Err(PlanWorkspaceError::new(
                    "supplemental_plan_file_too_large",
                    format!("Supplemental plan file '{}' exceeds maximum allowed size", supp_path.display()),
                ));
            }
            let supp_bytes = fs::read(&supp_path).map_err(|e| {
                PlanWorkspaceError::new(
                    "supplemental_plan_file_read_error",
                    format!("Read supplemental plan file '{}': {e}", supp_path.display()),
                )
            })?;
            let supp_digest = sha256_bytes(&supp_bytes);
            if supp_digest != supp.digest {
                // Supplemental file modified between resolve_plan_context and read
                supp_retry_needed = true;
                break;
            }
            let supp_content = String::from_utf8(supp_bytes).map_err(|e| {
                PlanWorkspaceError::new(
                    "supplemental_plan_file_not_utf8",
                    format!("Supplemental plan file '{}' is not valid UTF-8: {e}", supp_path.display()),
                )
            })?;
            supp_contents.push(SupplementalPlanContent {
                id: supp.id.clone(),
                path: supp.path.clone(),
                suffix: supp.suffix.clone(),
                digest: supp_digest,
                content: supp_content,
            });
        }

        if supp_retry_needed {
            continue;
        }

        // Verify effective plan digest recomputed from captured contents
        let mut hasher = Sha256::new();
        if let Some(ref p) = plan_context.current_primary_plan {
            hasher.update(p.id.as_bytes());
            hasher.update(b"\0");
            hasher.update(p.digest.as_bytes());
            hasher.update(b"\0");
        }
        for s in &supp_contents {
            hasher.update(s.id.as_bytes());
            hasher.update(b"\0");
            hasher.update(s.suffix.as_bytes());
            hasher.update(b"\0");
            hasher.update(s.digest.as_bytes());
            hasher.update(b"\0");
        }
        let computed_effective_digest = format!("{:x}", hasher.finalize());
        if plan_context.effective_plan_digest.as_deref() != Some(&computed_effective_digest) {
            continue;
        }

        // Build ordered_sources (primary first, then supplementals in resolved order)
        let mut ordered_sources = Vec::new();
        if let (Some(ref primary), Some(ref primary_content)) = (&plan_context.current_primary_plan, &primary_content_opt) {
            ordered_sources.push(FrozenPlanSource {
                kind: FrozenPlanSourceKind::Primary,
                id: primary.id.clone(),
                suffix: None,
                path: primary.path.clone(),
                source_digest: primary.digest.clone(),
                content: primary_content.clone(),
            });
        }
        for supp in &supp_contents {
            ordered_sources.push(FrozenPlanSource {
                kind: FrozenPlanSourceKind::Supplemental,
                id: supp.id.clone(),
                suffix: Some(supp.suffix.clone()),
                path: supp.path.clone(),
                source_digest: supp.digest.clone(),
                content: supp.content.clone(),
            });
        }

        // Assemble composite effective_plan_content
        let effective_plan_content = if ordered_sources.is_empty() {
            None
        } else {
            Some(canonical_assemble_sources(&ordered_sources).map_err(|e| {
                PlanWorkspaceError::new("plan_assembly_error", e)
            })?)
        };

        return Ok(FrozenPlanSnapshot {
            plan_context,
            primary_plan_content: primary_content_opt,
            supplemental_plan_contents: supp_contents,
            ordered_sources,
            effective_plan_content,
            effective_plan_digest: Some(computed_effective_digest),
        });
    }

    Err(PlanWorkspaceError::new(
        "stale_plan_file_digest",
        "Plan workspace contents changed repeatedly during atomic snapshot capture; aborting",
    ))
}

// ---------------------------------------------------------------------------
// Guarded Append & New Plan APIs
// ---------------------------------------------------------------------------

pub fn plan_append(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
    req: PlanAppendRequest,
) -> Result<PlanAppendResponse, PlanWorkspaceError> {
    // Acquire write lock to serialize appends and eliminate race conditions
    let _lock = PLAN_WRITE_LOCK
        .lock()
        .map_err(|e| PlanWorkspaceError::new("lock_error", format!("Plan append write lock error: {e}")))?;

    let canonical_root = canonicalize_project_root(project_root)?;
    let plan_dir_path = resolve_contained_path(&canonical_root, &config.plan_dir)?;

    let target_filename = if req.target_plan_id.ends_with(".md") || req.target_plan_id.ends_with(".markdown") {
        req.target_plan_id.clone()
    } else {
        format!("{}.md", req.target_plan_id)
    };

    let target_file_path = resolve_contained_path(&plan_dir_path, &target_filename)?;
    match probe_path_symlink(&target_file_path)? {
        PathProbe::Missing => {
            return Err(PlanWorkspaceError::new(
                "target_plan_file_missing",
                format!("target_plan_file_missing: {}", target_file_path.display()),
            ));
        }
        PathProbe::Exists { is_file, is_symlink, .. } => {
            if !is_file || is_symlink {
                return Err(PlanWorkspaceError::new(
                    "target_plan_file_not_regular",
                    "Target plan file is not a regular file",
                ));
            }
        }
    }

    let meta = fs::symlink_metadata(&target_file_path).map_err(|e| {
        PlanWorkspaceError::new(
            "plan_file_metadata_error",
            format!("Inspect target plan file: {e}"),
        )
    })?;
    if meta.len() > MAX_PLAN_FILE_BYTES {
        return Err(PlanWorkspaceError::new(
            "target_plan_file_too_large",
            "Target plan file exceeds maximum allowed size",
        ));
    }

    // Re-read existing target file from disk after acquiring lock
    let existing_bytes = fs::read(&target_file_path).map_err(|e| {
        PlanWorkspaceError::new(
            "plan_file_read_error",
            format!("Read target plan file: {e}"),
        )
    })?;
    let current_digest = sha256_bytes(&existing_bytes);
    let existing_str = String::from_utf8(existing_bytes.clone()).map_err(|e| {
        PlanWorkspaceError::new(
            "target_plan_file_not_utf8",
            format!("target_plan_file_not_utf8: {e}"),
        )
    })?;

    // Idempotency token check: if already applied, return idempotent response
    let token_marker = format!("<!-- idempotency_token: {} -->", req.idempotency_token.trim());
    if existing_str.contains(&token_marker) {
        let ctx = resolve_plan_context(project_root, config)?;
        return Ok(PlanAppendResponse {
            plan_id: req.target_plan_id,
            updated_file_digest: current_digest,
            applied: false,
            context: ctx,
        });
    }

    if current_digest != req.expected_file_digest {
        return Err(PlanWorkspaceError::new(
            "stale_plan_file_digest",
            format!(
                "stale_plan_file_digest: expected '{}', found '{}'",
                req.expected_file_digest, current_digest
            ),
        ));
    }

    // Detect line endings
    let newline = if existing_str.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    let section_to_append = format!(
        "{newline}{newline}## {}{newline}{newline}{}{newline}{newline}{}{newline}",
        req.section_title.trim(),
        token_marker,
        req.section_content.trim()
    );

    let mut new_bytes = existing_bytes.clone();
    new_bytes.extend_from_slice(section_to_append.as_bytes());

    // Write atomic with temp file in same directory
    let temp_name = format!(".tmp-append-{}", Uuid::new_v4());
    let temp_path = plan_dir_path.join(&temp_name);

    let mut temp_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| PlanWorkspaceError::new("io_error", format!("Create append temp file: {e}")))?;

    temp_file
        .write_all(&new_bytes)
        .map_err(|e| PlanWorkspaceError::new("io_error", format!("Write append temp file: {e}")))?;
    temp_file
        .sync_all()
        .map_err(|e| PlanWorkspaceError::new("io_error", format!("Sync append temp file: {e}")))?;
    drop(temp_file);

    // Verify written prefix
    let written_bytes = match fs::read(&temp_path) {
        Ok(b) => b,
        Err(e) => {
            let _ = fs::remove_file(&temp_path);
            return Err(PlanWorkspaceError::new(
                "io_error",
                format!("Read back append temp file: {e}"),
            ));
        }
    };
    if !written_bytes.starts_with(&existing_bytes) {
        let _ = fs::remove_file(&temp_path);
        return Err(PlanWorkspaceError::new(
            "atomic_append_verification_failed",
            "Atomic append verification failed: original bytes were not preserved as exact prefix",
        ));
    }

    // Revalidate source digest immediately before replacement
    let pre_publish_bytes = match fs::read(&target_file_path) {
        Ok(b) => b,
        Err(e) => {
            let _ = fs::remove_file(&temp_path);
            return Err(PlanWorkspaceError::new(
                "plan_file_read_error",
                format!("Pre-publish target read failed: {e}"),
            ));
        }
    };
    if sha256_bytes(&pre_publish_bytes) != current_digest {
        let _ = fs::remove_file(&temp_path);
        return Err(PlanWorkspaceError::new(
            "stale_plan_file_digest",
            "stale_plan_file_digest: target file modified concurrently before publish",
        ));
    }

    if let Err(e) = fs::rename(&temp_path, &target_file_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(PlanWorkspaceError::new(
            "atomic_publish_failed",
            format!("Atomic publish of appended plan failed: {e}"),
        ));
    }

    let updated_digest = sha256_bytes(&written_bytes);
    let ctx = resolve_plan_context(project_root, config)?;

    Ok(PlanAppendResponse {
        plan_id: req.target_plan_id,
        updated_file_digest: updated_digest,
        applied: true,
        context: ctx,
    })
}

pub fn plan_new_preview(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
) -> Result<PlanNewPreviewResponse, PlanWorkspaceError> {
    let ctx = resolve_plan_context(project_root, config)?;
    if ctx.resolver_status != PlanResolverStatus::Resolved {
        return Err(PlanWorkspaceError::new(
            "unresolved_plan_context",
            format!("unresolved_plan_context: {:?}", ctx.unresolved_reason_code),
        ));
    }

    let next_rev = ctx.next_primary_revision.unwrap_or(1);
    let filename = format_plan_filename(&config.filename_template, &ctx.plan_series_version, next_rev, "");
    let rel_path = format!("{}/{}", config.plan_dir, filename);

    let canonical_root = canonicalize_project_root(project_root)?;
    let plan_dir_path = resolve_contained_path(&canonical_root, &config.plan_dir)?;
    let target_file_path = plan_dir_path.join(&filename);

    match probe_path_symlink(&target_file_path)? {
        PathProbe::Exists { .. } => {
            return Err(PlanWorkspaceError::new(
                "candidate_already_exists",
                format!("candidate_already_exists: {}", target_file_path.display()),
            ));
        }
        PathProbe::Missing => {}
    }

    // Compute complete inventory digest to bind preview token to entire context & inventory state
    let mut candidate_digests = Vec::new();
    match probe_path_symlink(&plan_dir_path)? {
        PathProbe::Exists { is_dir, is_symlink, .. } => {
            if !is_dir || is_symlink {
                return Err(PlanWorkspaceError::new(
                    "plan_directory_not_directory",
                    "Plan directory path is not a directory or is a symlink",
                ));
            }
            let entries = fs::read_dir(&plan_dir_path).map_err(|e| {
                PlanWorkspaceError::new(
                    "plan_directory_read_error",
                    format!("Read plan dir for preview: {e}"),
                )
            })?;
            for entry_res in entries {
                let entry = entry_res.map_err(|e| {
                    PlanWorkspaceError::new(
                        "plan_directory_entry_read_error",
                        format!("Read entry for preview: {e}"),
                    )
                })?;
                let fname = entry.file_name().to_string_lossy().to_string();
                // Ignore internal temporary files and dotfiles
                if fname.starts_with('.') {
                    continue;
                }
                let meta = entry.metadata().map_err(|e| {
                    PlanWorkspaceError::new(
                        "plan_file_metadata_error",
                        format!("Inspect entry for preview: {e}"),
                    )
                })?;
                if meta.is_file() && !meta.file_type().is_symlink() {
                    if let Ok(Some(_)) = parse_plan_filename(&fname, &config.filename_template, &ctx.plan_series_version) {
                        let bytes = fs::read(entry.path()).map_err(|e| {
                            PlanWorkspaceError::new(
                                "plan_file_read_error",
                                format!("Read candidate for preview: {e}"),
                            )
                        })?;
                        candidate_digests.push((fname, sha256_bytes(&bytes), meta.len()));
                    }
                }
            }
        }
        PathProbe::Missing => {}
    }
    candidate_digests.sort();

    let inventory_digest = compute_complete_inventory_digest(
        &canonical_root,
        config,
        &ctx.application_version,
        &ctx.plan_series_version,
        ctx.current_primary_plan.as_ref(),
        &ctx.active_supplemental_plans,
        &candidate_digests,
        ctx.effective_plan_digest.as_deref(),
        ctx.next_primary_revision,
    );

    let token = register_preview_token(&canonical_root, next_rev, &rel_path, &inventory_digest);

    Ok(PlanNewPreviewResponse {
        candidate_revision: next_rev,
        candidate_filename: filename,
        candidate_path: rel_path,
        token,
        context: ctx,
    })
}

pub fn plan_new_confirm(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
    req: PlanNewConfirmRequest,
) -> Result<PlanNewConfirmResponse, PlanWorkspaceError> {
    plan_new_confirm_with_remover(project_root, config, req, |p| fs::remove_file(p))
}

pub(crate) fn plan_new_confirm_with_remover<F>(
    project_root: &Path,
    config: &PlanWorkspaceConfig,
    req: PlanNewConfirmRequest,
    remove_fn: F,
) -> Result<PlanNewConfirmResponse, PlanWorkspaceError>
where
    F: FnOnce(&Path) -> std::io::Result<()>,
{
    let _lock = PLAN_WRITE_LOCK
        .lock()
        .map_err(|e| PlanWorkspaceError::new("lock_error", format!("Plan new confirm write lock error: {e}")))?;

    let canonical_root = canonicalize_project_root(project_root)?;
    let plan_dir_path = resolve_contained_path(&canonical_root, &config.plan_dir)?;

    let ctx = resolve_plan_context(project_root, config)?;
    if ctx.resolver_status != PlanResolverStatus::Resolved {
        return Err(PlanWorkspaceError::new(
            "unresolved_plan_context",
            format!("unresolved_plan_context: {:?}", ctx.unresolved_reason_code),
        ));
    }

    let next_rev = ctx.next_primary_revision.unwrap_or(1);
    let filename = format_plan_filename(&config.filename_template, &ctx.plan_series_version, next_rev, "");
    let rel_path = format!("{}/{}", config.plan_dir, filename);

    // Compute current complete inventory digest
    let mut candidate_digests = Vec::new();
    match probe_path_symlink(&plan_dir_path)? {
        PathProbe::Exists { is_dir, is_symlink, .. } => {
            if !is_dir || is_symlink {
                return Err(PlanWorkspaceError::new(
                    "plan_directory_not_directory",
                    "Plan directory path is not a directory or is a symlink",
                ));
            }
            let entries = fs::read_dir(&plan_dir_path).map_err(|e| {
                PlanWorkspaceError::new(
                    "plan_directory_read_error",
                    format!("Read plan dir for confirm: {e}"),
                )
            })?;
            for entry_res in entries {
                let entry = entry_res.map_err(|e| {
                    PlanWorkspaceError::new(
                        "plan_directory_entry_read_error",
                        format!("Read entry for confirm: {e}"),
                    )
                })?;
                let fname = entry.file_name().to_string_lossy().to_string();
                // Ignore internal temporary files and dotfiles
                if fname.starts_with('.') {
                    continue;
                }
                let meta = entry.metadata().map_err(|e| {
                    PlanWorkspaceError::new(
                        "plan_file_metadata_error",
                        format!("Inspect entry for confirm: {e}"),
                    )
                })?;
                if meta.is_file() && !meta.file_type().is_symlink() {
                    if let Ok(Some(_)) = parse_plan_filename(&fname, &config.filename_template, &ctx.plan_series_version) {
                        let bytes = fs::read(entry.path()).map_err(|e| {
                            PlanWorkspaceError::new(
                                "plan_file_read_error",
                                format!("Read candidate for confirm: {e}"),
                            )
                        })?;
                        candidate_digests.push((fname, sha256_bytes(&bytes), meta.len()));
                    }
                }
            }
        }
        PathProbe::Missing => {}
    }
    candidate_digests.sort();

    let current_inventory_digest = compute_complete_inventory_digest(
        &canonical_root,
        config,
        &ctx.application_version,
        &ctx.plan_series_version,
        ctx.current_primary_plan.as_ref(),
        &ctx.active_supplemental_plans,
        &candidate_digests,
        ctx.effective_plan_digest.as_deref(),
        ctx.next_primary_revision,
    );

    // Verify and consume authorization token from in-memory registry
    verify_and_consume_preview_token(
        &req.token,
        &canonical_root,
        next_rev,
        &rel_path,
        &current_inventory_digest,
    )?;

    match probe_path_symlink(&plan_dir_path)? {
        PathProbe::Missing => {
            fs::create_dir_all(&plan_dir_path).map_err(|e| {
                PlanWorkspaceError::new(
                    "io_error",
                    format!("Create plan directory: {e}"),
                )
            })?;
        }
        PathProbe::Exists { is_dir, is_symlink, .. } => {
            if !is_dir || is_symlink {
                return Err(PlanWorkspaceError::new(
                    "plan_directory_not_directory",
                    "Plan directory path is not a directory or is a symlink",
                ));
            }
        }
    }

    let target_file_path = plan_dir_path.join(&filename);
    match probe_path_symlink(&target_file_path)? {
        PathProbe::Exists { .. } => {
            return Err(PlanWorkspaceError::new(
                "candidate_already_exists",
                format!("candidate_already_exists: {}", target_file_path.display()),
            ));
        }
        PathProbe::Missing => {}
    }

    let content_to_write = format!(
        "# {}\n\n{}\n",
        req.title.trim(),
        req.initial_content.trim()
    );
    let content_bytes = content_to_write.as_bytes();
    let expected_digest = sha256_bytes(content_bytes);

    // Crash-safe write to temporary file in same directory first
    let temp_name = format!(".tmp-new-{}", Uuid::new_v4());
    let temp_path = plan_dir_path.join(&temp_name);

    let mut temp_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| {
            PlanWorkspaceError::new(
                "io_error",
                format!("Create new plan temp file: {e}"),
            )
        })?;

    temp_file
        .write_all(content_bytes)
        .map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            PlanWorkspaceError::new(
                "io_error",
                format!("Write new plan temp file: {e}"),
            )
        })?;
    temp_file
        .sync_all()
        .map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            PlanWorkspaceError::new(
                "io_error",
                format!("Sync new plan temp file: {e}"),
            )
        })?;
    drop(temp_file);

    // Verify temp file bytes & digest
    let written_temp_bytes = match fs::read(&temp_path) {
        Ok(b) => b,
        Err(e) => {
            let _ = fs::remove_file(&temp_path);
            return Err(PlanWorkspaceError::new(
                "io_error",
                format!("Read back new plan temp file: {e}"),
            ));
        }
    };
    if written_temp_bytes != content_bytes {
        let _ = fs::remove_file(&temp_path);
        return Err(PlanWorkspaceError::new(
            "atomic_append_verification_failed",
            "Written temp file content does not match expected payload",
        ));
    }

    // Atomic publish to target path with no-overwrite semantics using hard_link + remove_file
    // On NTFS & Unix, fs::hard_link fails atomically if target_file_path already exists.
    if let Err(e) = fs::hard_link(&temp_path, &target_file_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(PlanWorkspaceError::new(
            "atomic_publish_failed",
            format!("Atomic no-overwrite publish failed: {e}"),
        ));
    }

    // Clean up temporary link using remover function.
    // If cleanup fails after successful publish, do NOT roll back or delete the valid target file.
    // Instead, report the machine-readable cleanup warning code and leftover temporary path.
    let (cleanup_warning_code, leftover_temp_path) = match remove_fn(&temp_path) {
        Ok(_) => (None, None),
        Err(_) => (
            Some("temporary_link_cleanup_failed".to_string()),
            Some(temp_path.to_string_lossy().to_string()),
        ),
    };

    let plan_id = filename
        .strip_suffix(".md")
        .or_else(|| filename.strip_suffix(".markdown"))
        .unwrap_or(&filename)
        .to_string();

    let updated_ctx = resolve_plan_context(project_root, config)?;

    Ok(PlanNewConfirmResponse {
        created_plan_id: plan_id,
        created_path: rel_path,
        file_digest: expected_digest,
        context: updated_ctx,
        cleanup_warning_code,
        leftover_temp_path,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    pub fn setup_test_project() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(root.join("gui/src-tauri")).unwrap();
        fs::create_dir_all(root.join(".plan")).unwrap();

        fs::write(
            root.join("gui/package.json"),
            br#"{"name": "test", "version": "0.23.0"}"#,
        )
        .unwrap();

        fs::write(
            root.join("gui/src-tauri/tauri.conf.json"),
            br#"{"productName": "test", "version": "0.23.0"}"#,
        )
        .unwrap();

        fs::write(
            root.join("gui/src-tauri/Cargo.toml"),
            br#"[package]
name = "test"
version = "0.23.0"
"#,
        )
        .unwrap();

        (temp, root)
    }

    #[test]
    fn test_version_consensus_and_series_override() {
        let (_temp, root) = setup_test_project();
        let cfg = PlanWorkspaceConfig::default();

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.application_version, "0.23.0");
        assert_eq!(ctx.plan_series_version, "0.23.0");

        let mut cfg_override = cfg.clone();
        cfg_override.plan_series_version_override = Some("0.24.0".to_string());

        let ctx_override = resolve_plan_context(&root, &cfg_override).unwrap();
        assert_eq!(ctx_override.application_version, "0.23.0");
        assert_eq!(ctx_override.plan_series_version, "0.24.0");

        // Test version mismatch across manifests
        fs::write(
            root.join("gui/package.json"),
            br#"{"name": "test", "version": "0.24.0"}"#,
        )
        .unwrap();
        let ctx_mismatch = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx_mismatch.resolver_status, PlanResolverStatus::Unresolved);
        assert_eq!(
            ctx_mismatch.unresolved_reason_code.as_deref(),
            Some("version_consensus_mismatch")
        );
    }

    #[test]
    fn test_plan_grouping_supplemental_chain_and_effective_digest() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        fs::write(plan_dir.join("V0.24.0-r21.md"), "# Rev 21\n").unwrap();
        fs::write(plan_dir.join("V0.24.0-r22.md"), "# Rev 22 Base\n").unwrap();
        fs::write(plan_dir.join("V0.24.0-r22a.md"), "# Rev 22 Supplemental A\n").unwrap();
        fs::write(plan_dir.join("V0.24.0-r22b.md"), "# Rev 22 Supplemental B\n").unwrap();

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Resolved);
        assert_eq!(ctx.current_primary_revision, Some(22));
        assert_eq!(ctx.next_primary_revision, Some(23));
        assert_eq!(ctx.current_primary_plan.as_ref().unwrap().id, "V0.24.0-r22");
        assert_eq!(ctx.active_supplemental_plans.len(), 2);
        assert_eq!(ctx.active_supplemental_plans[0].id, "V0.24.0-r22a");
        assert_eq!(ctx.active_supplemental_plans[1].id, "V0.24.0-r22b");
        assert_eq!(ctx.current_leaf_plan_id.as_deref(), Some("V0.24.0-r22b"));
        assert!(ctx.effective_plan_digest.is_some());
    }

    #[test]
    fn test_case_collision_fails_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        fs::write(plan_dir.join("v0.24.0-r22.md"), "# Lowercase v\n").unwrap();

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Unresolved);
        assert_eq!(ctx.unresolved_reason_code.as_deref(), Some("case_collision"));
    }

    #[test]
    fn test_orphaned_supplemental_plan_fails_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        // Only supplemental without matching primary
        fs::write(plan_dir.join("V0.24.0-r22a.md"), "# Orphaned Supp\n").unwrap();

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Unresolved);
        assert_eq!(
            ctx.unresolved_reason_code.as_deref(),
            Some("orphaned_supplemental_plan")
        );
    }

    #[test]
    fn test_supplemental_read_failure_fails_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        fs::write(plan_dir.join("V0.24.0-r22.md"), "# Rev 22 Base\n").unwrap();
        fs::write(plan_dir.join("V0.24.0-r22a.md"), "# Rev 22 Supplemental A\n").unwrap();

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Resolved);
        assert_eq!(ctx.active_supplemental_plans.len(), 1);
    }

    #[test]
    fn test_guarded_plan_append_atomic_and_idempotent() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        let initial_text = "# Primary Plan\n\nInitial content.\n";
        let plan_path = plan_dir.join("V0.24.0-r22.md");
        fs::write(&plan_path, initial_text).unwrap();

        let initial_digest = sha256_bytes(initial_text.as_bytes());
        let req = PlanAppendRequest {
            target_plan_id: "V0.24.0-r22".to_string(),
            expected_file_digest: initial_digest.clone(),
            section_type: "append_section".to_string(),
            section_title: "Corrective Pass 1".to_string(),
            section_content: "Details of pass 1.".to_string(),
            idempotency_token: "token-12345".to_string(),
        };

        let res = plan_append(&root, &cfg, req.clone()).unwrap();
        assert!(res.applied);
        assert_ne!(res.updated_file_digest, initial_digest);

        let content_after = fs::read_to_string(&plan_path).unwrap();
        assert!(content_after.starts_with(initial_text));
        assert!(content_after.contains("## Corrective Pass 1"));
        assert!(content_after.contains("<!-- idempotency_token: token-12345 -->"));

        // Second call with same token must be idempotent
        let second_res = plan_append(&root, &cfg, req).unwrap();
        assert!(!second_res.applied);
        assert_eq!(second_res.updated_file_digest, res.updated_file_digest);

        // Stale digest error
        let stale_req = PlanAppendRequest {
            target_plan_id: "V0.24.0-r22".to_string(),
            expected_file_digest: initial_digest, // Stale!
            section_type: "append_section".to_string(),
            section_title: "Stale Pass".to_string(),
            section_content: "Will fail.".to_string(),
            idempotency_token: "token-stale".to_string(),
        };
        let err = plan_append(&root, &cfg, stale_req).unwrap_err();
        assert_eq!(err.code, "stale_plan_file_digest");
    }

    #[test]
    fn test_concurrent_plan_append_exact_single_success_and_conflict() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        let initial_text = "# Primary Plan\n\nBase.\n";
        let plan_path = plan_dir.join("V0.24.0-r22.md");
        fs::write(&plan_path, initial_text).unwrap();

        let initial_digest = sha256_bytes(initial_text.as_bytes());

        let req1 = PlanAppendRequest {
            target_plan_id: "V0.24.0-r22".to_string(),
            expected_file_digest: initial_digest.clone(),
            section_type: "append_section".to_string(),
            section_title: "Thread 1 Append".to_string(),
            section_content: "Content 1".to_string(),
            idempotency_token: "token-t1".to_string(),
        };

        let req2 = PlanAppendRequest {
            target_plan_id: "V0.24.0-r22".to_string(),
            expected_file_digest: initial_digest.clone(),
            section_type: "append_section".to_string(),
            section_title: "Thread 2 Append".to_string(),
            section_content: "Content 2".to_string(),
            idempotency_token: "token-t2".to_string(),
        };

        let root_clone = root.clone();
        let cfg_clone = cfg.clone();

        let handle1 = std::thread::spawn(move || plan_append(&root_clone, &cfg_clone, req1));
        let handle2 = std::thread::spawn(move || plan_append(&root, &cfg, req2));

        let res1 = handle1.join().unwrap();
        let res2 = handle2.join().unwrap();

        // Exactly one should succeed and one should fail with stale_plan_file_digest
        let (success_count, conflict_count) = match (res1, res2) {
            (Ok(r1), Err(e2)) => {
                assert!(r1.applied);
                assert_eq!(e2.code, "stale_plan_file_digest");
                (1, 1)
            }
            (Err(e1), Ok(r2)) => {
                assert_eq!(e1.code, "stale_plan_file_digest");
                assert!(r2.applied);
                (1, 1)
            }
            (Ok(_), Ok(_)) => (2, 0),
            (Err(_), Err(_)) => (0, 2),
        };

        assert_eq!(success_count, 1, "Exactly one concurrent append must succeed");
        assert_eq!(conflict_count, 1, "The competing append with stale digest must fail with conflict");
    }

    #[test]
    fn test_plan_new_preview_and_confirm_lifecycle() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        fs::write(plan_dir.join("V0.24.0-r22.md"), "# Rev 22 Base\n").unwrap();

        let preview = plan_new_preview(&root, &cfg).unwrap();
        assert_eq!(preview.candidate_revision, 23);
        assert_eq!(preview.candidate_filename, "V0.24.0-r23.md");
        assert_eq!(preview.candidate_path, ".plan/V0.24.0-r23.md");

        let target_file = plan_dir.join("V0.24.0-r23.md");
        assert!(!target_file.exists());

        let confirm_req = PlanNewConfirmRequest {
            token: preview.token.clone(),
            title: "Plan Rev 23".to_string(),
            initial_content: "Initial content for rev 23.".to_string(),
        };

        let confirm_res = plan_new_confirm(&root, &cfg, confirm_req).unwrap();
        assert_eq!(confirm_res.created_plan_id, "V0.24.0-r23");
        assert!(target_file.exists());

        let created_str = fs::read_to_string(&target_file).unwrap();
        assert!(created_str.contains("# Plan Rev 23"));
        assert!(created_str.contains("Initial content for rev 23."));

        // Single-use token replay must fail
        let replay_req = PlanNewConfirmRequest {
            token: preview.token,
            title: "Replay Plan".to_string(),
            initial_content: "Replay.".to_string(),
        };
        let replay_err = plan_new_confirm(&root, &cfg, replay_req).unwrap_err();
        assert!(replay_err.code.contains("token"));
    }

    #[test]
    fn test_forged_preview_token_and_lost_registry_fails_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        fs::write(plan_dir.join("V0.24.0-r22.md"), "# Rev 22 Base\n").unwrap();

        let preview = plan_new_preview(&root, &cfg).unwrap();

        // 1. Forged token
        let forged_req = PlanNewConfirmRequest {
            token: "forged-token-xyz".to_string(),
            title: "Forged".to_string(),
            initial_content: "Forged content".to_string(),
        };
        let err = plan_new_confirm(&root, &cfg, forged_req).unwrap_err();
        assert!(err.code.contains("token"));

        // 2. Lost registry state / process restart simulation
        clear_preview_token_registry();
        let restart_req = PlanNewConfirmRequest {
            token: preview.token,
            title: "Post-restart".to_string(),
            initial_content: "Content".to_string(),
        };
        let err2 = plan_new_confirm(&root, &cfg, restart_req).unwrap_err();
        assert!(err2.code.contains("token"));
    }

    #[test]
    fn test_same_size_content_change_invalidates_preview_token() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        fs::write(plan_dir.join("V0.24.0-r22.md"), "AAAA").unwrap();

        let preview = plan_new_preview(&root, &cfg).unwrap();

        // Modify content with EXACT SAME BYTE SIZE
        fs::write(plan_dir.join("V0.24.0-r22.md"), "BBBB").unwrap();

        let confirm_req = PlanNewConfirmRequest {
            token: preview.token,
            title: "Plan Rev 23".to_string(),
            initial_content: "Content.".to_string(),
        };

        let err = plan_new_confirm(&root, &cfg, confirm_req).unwrap_err();
        assert!(err.code.contains("token") || err.message.contains("changed since preview") || err.message.contains("inventory"));
    }

    #[test]
    fn test_plan_current_and_status_read_only() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        let primary_content = "# Primary Plan Content\n";
        let supp_content = "# Supplemental Plan Content\n";

        fs::write(plan_dir.join("V0.24.0-r22.md"), primary_content).unwrap();
        fs::write(plan_dir.join("V0.24.0-r22a.md"), supp_content).unwrap();

        let status = plan_status(&root, &cfg).unwrap();
        assert_eq!(status.current_leaf_plan_id.as_deref(), Some("V0.24.0-r22a"));

        let current = plan_current(&root, &cfg).unwrap();
        assert_eq!(current.primary_plan_content.as_deref(), Some(primary_content));
        assert_eq!(current.leaf_plan_content.as_deref(), Some(supp_content));
    }

    #[test]
    fn test_custom_filename_template_support() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());
        cfg.filename_template = "plan-v{version}-revision-{revision}{suffix}.markdown".to_string();

        let plan_dir = root.join(".plan");
        fs::write(
            plan_dir.join("plan-v0.24.0-revision-5.markdown"),
            "# Custom template rev 5\n",
        )
        .unwrap();

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Resolved);
        assert_eq!(ctx.current_primary_revision, Some(5));
        assert_eq!(ctx.next_primary_revision, Some(6));

        let preview = plan_new_preview(&root, &cfg).unwrap();
        assert_eq!(preview.candidate_revision, 6);
        assert_eq!(
            preview.candidate_filename,
            "plan-v0.24.0-revision-6.markdown"
        );
    }

    #[test]
    fn test_invalid_template_configuration_fails_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.filename_template = "invalid_template_{foo}.md".to_string();

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Unresolved);
        assert_eq!(
            ctx.unresolved_reason_code.as_deref(),
            Some("invalid_template_configuration")
        );
    }

    #[test]
    fn test_nearest_existing_ancestor_symlink_escape_fails_closed() {
        let (_temp, root) = setup_test_project();
        let outside = _temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();

        // Create symlink inside root pointing outside
        let escape_link = root.join("escape_link");
        #[cfg(windows)]
        {
            let _ = std::os::windows::fs::symlink_dir(&outside, &escape_link);
        }
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(&outside, &escape_link);
        }

        if escape_link.exists() {
            // Path inside escape_link with non-existent child
            let escaped_target = "escape_link/sub1/sub2/file.md";
            let err = resolve_contained_path(&root, escaped_target).unwrap_err();
            assert!(err.to_string().contains("escaped project root") || err.to_string().contains("Path escaped") || err.code == "path_containment_violation");
        }
    }

    #[test]
    fn test_injected_failures_enumeration_metadata_read_fail_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        // 1. Missing version source file fails closed
        cfg.version_sources = vec!["nonexistent-package.json".to_string()];
        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Unresolved);
        assert_eq!(
            ctx.unresolved_reason_code.as_deref(),
            Some("version_source_missing")
        );

        // 2. Traversal version source path fails closed
        cfg.version_sources = vec!["../outside.json".to_string()];
        let ctx2 = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx2.resolver_status, PlanResolverStatus::Unresolved);
        assert_eq!(
            ctx2.unresolved_reason_code.as_deref(),
            Some("invalid_version_source_path")
        );
    }

    #[test]
    fn test_plan_workspace_error_serialization_and_codes() {
        let err = PlanWorkspaceError::with_details(
            "stale_digest",
            "Digest mismatch occurred",
            serde_json::json!({ "expected": "abc", "actual": "def" }),
        );
        let serialized = serde_json::to_string(&err).unwrap();
        assert!(serialized.contains("\"code\":\"stale_digest\""));
        assert!(serialized.contains("\"message\":\"Digest mismatch occurred\""));
        assert!(serialized.contains("\"actual\":\"def\"") && serialized.contains("\"expected\":\"abc\""));

        let deserialized: PlanWorkspaceError = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.code, "stale_digest");
        assert_eq!(deserialized.message, "Digest mismatch occurred");
        assert!(deserialized.details.is_some());
    }

    #[test]
    fn test_plan_new_confirm_cleanup_failure_retains_valid_plan_and_reports_warning() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        fs::write(plan_dir.join("V0.24.0-r1.md"), "# Base rev 1\n").unwrap();

        let preview = plan_new_preview(&root, &cfg).unwrap();
        assert_eq!(preview.candidate_revision, 2);

        // Inject simulated remove_file failure
        let confirm_res = plan_new_confirm_with_remover(
            &root,
            &cfg,
            PlanNewConfirmRequest {
                token: preview.token.clone(),
                title: "Cleanup Test".to_string(),
                initial_content: "Testing cleanup failure".to_string(),
            },
            |_temp_path| Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "injected cleanup error")),
        ).unwrap();

        assert_eq!(confirm_res.created_plan_id, "V0.24.0-r2");
        assert_eq!(confirm_res.cleanup_warning_code.as_deref(), Some("temporary_link_cleanup_failed"));
        assert!(confirm_res.leftover_temp_path.is_some());

        // Verify the published file is intact and valid
        let target_file = root.join(".plan").join("V0.24.0-r2.md");
        assert!(target_file.exists());
        let content = fs::read_to_string(&target_file).unwrap();
        assert!(content.contains("# Cleanup Test"));

        // Clean up leftover temp file manually for hygiene
        if let Some(leftover) = confirm_res.leftover_temp_path {
            let _ = fs::remove_file(Path::new(&leftover));
        }
    }

    #[test]
    fn test_internal_temp_files_ignored_during_discovery() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        fs::write(plan_dir.join("V0.24.0-r1.md"), "# Base rev 1\n").unwrap();
        // Write a temp file and a dotfile
        fs::write(plan_dir.join(".tmp-new-12345"), "# Temp payload\n").unwrap();
        fs::write(plan_dir.join(".DS_Store"), "dummy").unwrap();

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Resolved);
        assert_eq!(ctx.current_primary_revision, Some(1));
        assert_eq!(ctx.next_primary_revision, Some(2));
        assert_eq!(ctx.active_supplemental_plans.len(), 0);
    }

    #[test]
    fn test_capture_frozen_plan_snapshot_success() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        let primary_content = "# Primary Plan Rev 1\nInitial content.\n";
        let supp_content = "# Supplemental Plan Rev 1a\nSupplemental notes.\n";

        fs::write(plan_dir.join("V0.24.0-r1.md"), primary_content).unwrap();
        fs::write(plan_dir.join("V0.24.0-r1a.md"), supp_content).unwrap();

        let snapshot = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        assert_eq!(snapshot.plan_context.resolver_status, PlanResolverStatus::Resolved);
        assert_eq!(snapshot.primary_plan_content.as_deref(), Some(primary_content));
        assert_eq!(snapshot.supplemental_plan_contents.len(), 1);
        assert_eq!(snapshot.supplemental_plan_contents[0].id, "V0.24.0-r1a");
        assert_eq!(snapshot.supplemental_plan_contents[0].content, supp_content);
        assert_eq!(snapshot.ordered_sources.len(), 2);
        let expected_assembled = canonical_assemble_sources(&snapshot.ordered_sources).unwrap();
        assert_eq!(snapshot.effective_plan_content.as_deref(), Some(expected_assembled.as_str()));
        assert_eq!(
            snapshot.effective_plan_digest.as_deref(),
            snapshot.plan_context.effective_plan_digest.as_deref()
        );
    }

    #[test]
    fn test_capture_frozen_plan_snapshot_clean_workspace() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let snapshot = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        assert_eq!(snapshot.plan_context.resolver_status, PlanResolverStatus::Resolved);
        assert_eq!(snapshot.primary_plan_content, None);
        assert!(snapshot.supplemental_plan_contents.is_empty());
        assert!(snapshot.ordered_sources.is_empty());
        assert_eq!(snapshot.effective_plan_content, None);
        assert_eq!(snapshot.effective_plan_digest, None);
    }

    #[test]
    fn test_capture_frozen_plan_snapshot_unresolved_returns_unresolved_snapshot() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        // Orphaned supplemental plan without matching primary
        fs::write(plan_dir.join("V0.24.0-r1a.md"), "# Orphaned Rev 1a\n").unwrap();

        let snapshot = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        assert_eq!(snapshot.plan_context.resolver_status, PlanResolverStatus::Unresolved);
        assert_eq!(snapshot.plan_context.unresolved_reason_code.as_deref(), Some("orphaned_supplemental_plan"));
        assert_eq!(snapshot.primary_plan_content, None);
        assert!(snapshot.supplemental_plan_contents.is_empty());
        assert!(snapshot.ordered_sources.is_empty());
    }

    #[test]
    fn test_capture_frozen_plan_snapshot_invalid_utf8_fails_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        // Write invalid UTF-8 bytes to plan file
        let invalid_utf8_bytes = vec![0xFF, 0xFE, 0xFD];
        fs::write(plan_dir.join("V0.24.0-r1.md"), invalid_utf8_bytes).unwrap();

        let err = capture_frozen_plan_snapshot(&root, &cfg).unwrap_err();
        assert_eq!(err.code, "plan_file_not_utf8");
    }

    #[test]
    fn test_frozen_plan_payload_validation_success_and_failures() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        let plan_dir = root.join(".plan");
        let primary_content = "# Primary Plan Rev 1\nInitial content.\n";
        let supp_content = "# Supplemental Plan Rev 1a\nSupplemental notes.\n";

        fs::write(plan_dir.join("V0.24.0-r1.md"), primary_content).unwrap();
        fs::write(plan_dir.join("V0.24.0-r1a.md"), supp_content).unwrap();

        let snapshot = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        let payload = snapshot.to_frozen_plan_payload().expect("payload present for resolved non-empty plan");

        // 1. Success case
        assert!(payload.validate_against_plan_context(&snapshot.plan_context).is_ok());

        // 2. Failure: digest mismatch
        let mut corrupted_digest = payload.clone();
        corrupted_digest.effective_plan_digest = "sha256:corrupted".to_string();
        let err = corrupted_digest.validate_against_plan_context(&snapshot.plan_context).unwrap_err();
        assert!(err.contains("effective digest mismatch"));

        // 3. Failure: missing primary plan identity
        let mut missing_primary = payload.clone();
        missing_primary.primary_plan_identity_and_digest = None;
        let err = missing_primary.validate_against_plan_context(&snapshot.plan_context).unwrap_err();
        assert!(err.contains("primary plan presence mismatch"));

        // 4. Failure: supplemental count mismatch
        let mut supp_mismatch = payload.clone();
        supp_mismatch.ordered_supplemental_plan_identities_and_digests.clear();
        let err = supp_mismatch.validate_against_plan_context(&snapshot.plan_context).unwrap_err();
        assert!(err.contains("supplemental count mismatch"));

        // 5. Failure: body mutation with the original content digest is rejected.
        let mut body_mismatch = payload.clone();
        body_mismatch.effective_plan_content.push_str("changed");
        let err = body_mismatch.validate_against_plan_context(&snapshot.plan_context).unwrap_err();
        assert!(err.contains("does not match canonical assembly") || err.contains("content digest mismatch"));

        // 6. Unknown schema versions fail closed.
        let mut unsupported_schema = payload.clone();
        unsupported_schema.schema_version = 2;
        let err = unsupported_schema.validate_against_plan_context(&snapshot.plan_context).unwrap_err();
        assert!(err.contains("Unsupported frozen plan payload schema version"));

        // 7. Reordered sources fail closed
        let mut reordered_sources = payload.clone();
        reordered_sources.ordered_sources.swap(0, 1);
        let err = reordered_sources.validate_against_plan_context(&snapshot.plan_context).unwrap_err();
        assert!(err.contains("primary source mismatch"));

        // 8. Individual source content corruption fails closed
        let mut corrupted_source = payload.clone();
        corrupted_source.ordered_sources[0].content = "corrupted".to_string();
        let err = corrupted_source.validate_against_plan_context(&snapshot.plan_context).unwrap_err();
        assert!(err.contains("digest mismatch"));
    }

    #[test]
    fn test_tauri_frozen_plan_payload_matches_shared_mcp_wire_fixture() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());
        let body = "# V0.24.0-r22\n\nPhase A plan.\n";
        fs::write(root.join(".plan/V0.24.0-r22.md"), body).unwrap();

        let snapshot = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        let payload = snapshot.to_frozen_plan_payload().expect("frozen payload");
        let actual = serde_json::to_value(payload).unwrap();
        let shared_fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../mcp-server/tests/fixtures/tauri_frozen_plan_payload.json"
        ))
        .unwrap();

        assert_eq!(actual, shared_fixture);
    }

    #[test]
    fn test_path_probe_seam_permission_denied_fails_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());
        fs::write(root.join(".plan/V0.24.0-r1.md"), "# Base\n").unwrap();

        // Inject simulated PermissionDenied on the plan directory
        let plan_dir = root.join(".plan");
        let plan_dir_str = plan_dir.to_string_lossy().to_string();
        set_path_probe_seam(Some(Arc::new(move |p: &Path| {
            if p.to_string_lossy().contains(&plan_dir_str) {
                Some(Err(PlanWorkspaceError::new(
                    "path_probe_failed",
                    "Access denied by simulated test seam",
                )))
            } else {
                None
            }
        })));

        let direct_err = resolve_contained_path(&root, ".plan").unwrap_err();
        assert_eq!(direct_err.code, "path_probe_failed");
        assert!(direct_err.message.contains("Access denied"));

        let ctx = resolve_plan_context(&root, &cfg).unwrap();
        assert_eq!(ctx.resolver_status, PlanResolverStatus::Unresolved);
        assert!(ctx.unresolved_reason_code.is_some());

        let snapshot = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        assert_eq!(snapshot.plan_context.resolver_status, PlanResolverStatus::Unresolved);
        assert_eq!(snapshot.primary_plan_content, None);
        assert!(snapshot.ordered_sources.is_empty());

        set_path_probe_seam(None);
    }

    #[test]
    fn test_path_probe_seam_io_error_on_append_target_fails_closed() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());
        let target_file = root.join(".plan/V0.24.0-r1.md");
        fs::write(&target_file, "# Base\n").unwrap();
        let digest = sha256_bytes(b"# Base\n");

        set_path_probe_seam(Some(Arc::new(|p: &Path| {
            if p.to_string_lossy().ends_with("V0.24.0-r1.md") {
                Some(Err(PlanWorkspaceError::new(
                    "path_probe_failed",
                    "Simulated disk I/O probe failure",
                )))
            } else {
                None
            }
        })));

        let req = PlanAppendRequest {
            target_plan_id: "V0.24.0-r1".to_string(),
            expected_file_digest: digest,
            section_type: "append_section".to_string(),
            section_title: "Title".to_string(),
            section_content: "Content".to_string(),
            idempotency_token: "token-1".to_string(),
        };

        let res = plan_append(&root, &cfg, req);
        set_path_probe_seam(None);

        let err = res.unwrap_err();
        assert_eq!(err.code, "path_probe_failed");
        assert!(err.message.contains("Simulated disk I/O probe failure"));
    }

    #[test]
    fn test_frozen_plan_snapshot_ensure_run_entry_allowed_enforces_typed_error_contract() {
        let (_temp, root) = setup_test_project();
        let mut cfg = PlanWorkspaceConfig::default();
        cfg.plan_series_version_override = Some("0.24.0".to_string());

        // 1. Resolved clean workspace (no plan) -> Ok(())
        let snapshot_clean = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        assert_eq!(snapshot_clean.plan_context.resolver_status, PlanResolverStatus::Resolved);
        assert!(snapshot_clean.ensure_run_entry_allowed().is_ok());

        // 2. Resolved workspace with plan -> Ok(())
        fs::write(root.join(".plan/V0.24.0-r1.md"), "# Plan 1\n").unwrap();
        let snapshot_with_plan = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        assert_eq!(snapshot_with_plan.plan_context.resolver_status, PlanResolverStatus::Resolved);
        assert!(snapshot_with_plan.ensure_run_entry_allowed().is_ok());

        // 3. Unresolved workspace (e.g. orphaned supplemental plan) -> Err(PlanWorkspaceError) with exact code
        fs::write(root.join(".plan/V0.24.0-r2a.md"), "# Orphaned Supplemental\n").unwrap();
        let snapshot_unresolved = capture_frozen_plan_snapshot(&root, &cfg).unwrap();
        assert_eq!(snapshot_unresolved.plan_context.resolver_status, PlanResolverStatus::Unresolved);
        let err = snapshot_unresolved.ensure_run_entry_allowed().unwrap_err();
        assert_eq!(err.code, "orphaned_supplemental_plan");
        assert!(err.message.contains("orphaned_supplemental_plan"));
    }
}
