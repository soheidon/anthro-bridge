use serde::{Deserialize, Serialize};
use std::fmt;

/// Discriminated operation union for planner proposals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum PlannerOperationProposal {
    AppendSection {
        #[serde(alias = "targetPlanId")]
        target_plan_id: String,
        #[serde(alias = "sectionType")]
        section_type: String,
        #[serde(alias = "sectionTitle")]
        section_title: String,
        #[serde(alias = "sectionContent")]
        section_content: String,
    },
    NewPrimaryPlan {
        #[serde(alias = "proposedRevision")]
        proposed_revision: u64,
        title: String,
        #[serde(alias = "initialContent")]
        initial_content: String,
    },
}

/// Model-authored proposal returned by the Planner role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerProposal {
    #[serde(alias = "schema_version")]
    pub schema_version: u32,
    pub operation: PlannerOperationProposal,
}

/// Runtime-owned immutable record representing a plan candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCandidate {
    #[serde(alias = "schema_version")]
    pub schema_version: u32,
    #[serde(alias = "run_id")]
    pub run_id: String,
    pub sequence: u32,
    #[serde(alias = "candidate_id")]
    pub candidate_id: String,
    pub operation: PlannerOperationProposal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "target_plan_id")]
    pub target_plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "base_plan_digest")]
    pub base_plan_digest: Option<String>,
    #[serde(alias = "plan_context_digest")]
    pub plan_context_digest: String,
    #[serde(alias = "operation_payload_digest")]
    pub operation_payload_digest: String,
    #[serde(alias = "producer_role")]
    pub producer_role: String,
    #[serde(alias = "producer_profile_id")]
    pub producer_profile_id: String,
    #[serde(alias = "producer_model")]
    pub producer_model: String,
    #[serde(alias = "created_at_unix")]
    pub created_at_unix: u64,
}

/// Review decision enumeration with exact uppercase wire serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewDecision {
    #[serde(rename = "APPROVE")]
    Approve,
    #[serde(rename = "REQUEST_CHANGES")]
    RequestChanges,
    #[serde(rename = "ESCALATE")]
    Escalate,
}

impl fmt::Display for ReviewDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Approve => write!(f, "APPROVE"),
            Self::RequestChanges => write!(f, "REQUEST_CHANGES"),
            Self::Escalate => write!(f, "ESCALATE"),
        }
    }
}

/// Scope effect enumeration with exact uppercase wire serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeEffect {
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "CURRENT_PLAN_EXPANSION")]
    CurrentPlanExpansion,
    #[serde(rename = "NEW_REVISION_CANDIDATE")]
    NewRevisionCandidate,
}

impl fmt::Display for ScopeEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "NONE"),
            Self::CurrentPlanExpansion => write!(f, "CURRENT_PLAN_EXPANSION"),
            Self::NewRevisionCandidate => write!(f, "NEW_REVISION_CANDIDATE"),
        }
    }
}

/// Structured finding authored by the PlanReviewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    #[serde(alias = "finding_id")]
    pub finding_id: String,
    #[serde(alias = "affected_requirement")]
    pub affected_requirement: String,
    pub problem: String,
    #[serde(alias = "why_blocking")]
    pub why_blocking: String,
    #[serde(alias = "required_change")]
    pub required_change: String,
    #[serde(alias = "scope_effect")]
    pub scope_effect: ScopeEffect,
    pub blocking: bool,
}

/// Candidate binding tuple returned by the reviewer to ensure review freshness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewedCandidateBinding {
    pub sequence: u32,
    #[serde(alias = "candidate_id")]
    pub candidate_id: String,
    #[serde(alias = "operation_payload_digest")]
    pub operation_payload_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "base_plan_digest")]
    pub base_plan_digest: Option<String>,
    #[serde(alias = "plan_context_digest")]
    pub plan_context_digest: String,
}

/// Model-authored review proposal returned by the PlanReviewer role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerResponseProposal {
    #[serde(alias = "schema_version")]
    pub schema_version: u32,
    pub decision: ReviewDecision,
    #[serde(alias = "reviewed_candidate")]
    pub reviewed_candidate: ReviewedCandidateBinding,
    pub summary: String,
    pub findings: Vec<ReviewFinding>,
}

/// Runtime-owned record representing a validated review verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewVerdict {
    #[serde(alias = "schema_version")]
    pub schema_version: u32,
    pub decision: ReviewDecision,
    #[serde(alias = "reviewed_candidate")]
    pub reviewed_candidate: ReviewedCandidateBinding,
    pub summary: String,
    pub findings: Vec<ReviewFinding>,
    #[serde(alias = "reviewer_role")]
    pub reviewer_role: String,
    #[serde(alias = "reviewer_profile_id")]
    pub reviewer_profile_id: String,
    #[serde(alias = "reviewer_model")]
    pub reviewer_model: String,
    #[serde(alias = "duration_ms")]
    pub duration_ms: u64,
}

/// Typed waiting reason for human gate or operator resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvergenceWaitingReason {
    #[serde(rename = "NEW_PRIMARY_PLAN_CONFIRMATION")]
    NewPrimaryPlanConfirmation,
    #[serde(rename = "REVIEWER_ESCALATED")]
    ReviewerEscalated,
    #[serde(rename = "REVIEW_LIMIT_REACHED")]
    ReviewLimitReached,
    #[serde(rename = "PLAN_CONTEXT_STALE")]
    PlanContextStale,
    #[serde(rename = "POLICY_GATE_BLOCKED")]
    PolicyGateBlocked,
    #[serde(rename = "INVALID_MODEL_RESPONSE")]
    InvalidModelResponse,
}

impl fmt::Display for ConvergenceWaitingReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NewPrimaryPlanConfirmation => write!(f, "NEW_PRIMARY_PLAN_CONFIRMATION"),
            Self::ReviewerEscalated => write!(f, "REVIEWER_ESCALATED"),
            Self::ReviewLimitReached => write!(f, "REVIEW_LIMIT_REACHED"),
            Self::PlanContextStale => write!(f, "PLAN_CONTEXT_STALE"),
            Self::PolicyGateBlocked => write!(f, "POLICY_GATE_BLOCKED"),
            Self::InvalidModelResponse => write!(f, "INVALID_MODEL_RESPONSE"),
        }
    }
}

/// Terminal or suspended outcome of the Planner–Reviewer convergence loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConvergenceOutcome {
    Accepted {
        #[serde(alias = "candidateId")]
        candidate_id: String,
        #[serde(alias = "targetPlanId")]
        target_plan_id: String,
        #[serde(alias = "acceptedFileDigest")]
        accepted_file_digest: String,
        #[serde(alias = "reviewsUsed")]
        reviews_used: u32,
    },
    WaitingForUser {
        reason: ConvergenceWaitingReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(alias = "diagnosticCode")]
        diagnostic_code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(alias = "candidateArtifactRef")]
        candidate_artifact_ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(alias = "latestVerdictArtifactRef")]
        latest_verdict_artifact_ref: Option<String>,
    },
    Failed {
        #[serde(alias = "stableErrorCode")]
        stable_error_code: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(alias = "safeDetails")]
        safe_details: Option<String>,
    },
    Cancelled,
}

/// Typed error returned by the convergence engine and parsers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanConvergenceError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl PlanConvergenceError {
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

impl fmt::Display for PlanConvergenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PlanConvergenceError {}

impl From<PlanConvergenceError> for String {
    fn from(err: PlanConvergenceError) -> Self {
        err.to_string()
    }
}

/// Durable audit record appended to the run journal during convergence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanConvergenceAuditEntry {
    #[serde(alias = "timestamp_unix")]
    pub timestamp_unix: u64,
    #[serde(alias = "event_type")]
    pub event_type: String,
    pub sequence: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "candidate_id")]
    pub candidate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "artifact_ref")]
    pub artifact_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "artifact_digest")]
    pub artifact_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "profile_id")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<ReviewDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "waiting_reason")]
    pub waiting_reason: Option<ConvergenceWaitingReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "error_code")]
    pub error_code: Option<String>,
}

/// Durable, candidate-bound intent recorded immediately before a converged
/// `NewPrimaryPlan` is published through the guarded Plan Workspace writer.
///
/// Every field is runtime-owned and none is taken from model output. The two
/// revision fields make the protocol explicit: `request_revision` is the
/// `WaitingForUser` journal revision the operator actually reviewed and
/// submitted, while `intent_record_revision` is the revision the journal
/// reached by durably recording this intent. Because that write advances the
/// journal, a retry is validated against the intent's own revision pair rather
/// than by comparing the original UI revision with the journal's current one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanConvergenceConfirmationIntent {
    #[serde(alias = "run_id")]
    pub run_id: String,
    /// `WaitingForUser` journal revision the operator reviewed and submitted.
    #[serde(alias = "request_revision")]
    pub request_revision: u64,
    /// Journal revision produced by the write that recorded this intent.
    #[serde(alias = "intent_record_revision")]
    pub intent_record_revision: u64,
    #[serde(alias = "candidate_id")]
    pub candidate_id: String,
    #[serde(alias = "candidate_sequence")]
    pub candidate_sequence: u32,
    #[serde(alias = "candidate_artifact_ref")]
    pub candidate_artifact_ref: String,
    #[serde(alias = "candidate_artifact_digest")]
    pub candidate_artifact_digest: String,
    /// Discriminated operation kind: `new_primary_plan`.
    #[serde(alias = "operation_kind")]
    pub operation_kind: String,
    /// Deterministic digest of the canonical serialized operation payload.
    #[serde(alias = "operation_payload_digest")]
    pub operation_payload_digest: String,
    /// Base plan digest bound to the candidate; absent for a new primary plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "base_plan_digest")]
    pub base_plan_digest: Option<String>,
    /// Effective PlanContext digest frozen for the reviewed candidate.
    #[serde(alias = "plan_context_digest")]
    pub plan_context_digest: String,
    #[serde(alias = "target_revision")]
    pub target_revision: u64,
    #[serde(alias = "target_plan_id")]
    pub target_plan_id: String,
    /// Canonical project-relative path of the publication target.
    #[serde(alias = "target_path")]
    pub target_path: String,
    /// SHA-256 of the exact bytes confirmation publishes for this candidate.
    #[serde(alias = "content_digest")]
    pub content_digest: String,
    #[serde(alias = "idempotency_key")]
    pub idempotency_key: String,
}

impl PlanConvergenceConfirmationIntent {
    /// Idempotency token for one candidate's publication, derived from runtime
    /// identities only and never from model output.
    pub fn idempotency_key_for(run_id: &str, candidate_id: &str) -> String {
        format!("plan-confirm:{run_id}:{candidate_id}")
    }

    /// True when the supplied transaction matches this intent on every binding
    /// field other than the two journal revisions, which are validated by the
    /// caller against the journal's durable state.
    pub fn matches_transaction(
        &self,
        run_id: &str,
        candidate_id: &str,
        candidate_sequence: u32,
        candidate_artifact_ref: &str,
        candidate_artifact_digest: &str,
        operation_payload_digest: &str,
        content_digest: &str,
    ) -> bool {
        self.run_id == run_id
            && self.candidate_id == candidate_id
            && self.candidate_sequence == candidate_sequence
            && self.candidate_artifact_ref == candidate_artifact_ref
            && self.candidate_artifact_digest == candidate_artifact_digest
            && self.operation_payload_digest == operation_payload_digest
            && self.content_digest == content_digest
            && self.idempotency_key == Self::idempotency_key_for(run_id, candidate_id)
    }
}

/// Configuration parameters for opt-in convergence execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanConvergenceConfig {
    #[serde(default)]
    pub opt_in: bool,
    #[serde(default = "default_total_timeout_secs")]
    pub total_timeout_secs: u64,
}

/// Presentation payload for a convergence run that stopped at a human gate.
///
/// Carries the reviewed candidate identity so the operator can bind a
/// confirmation to one exact candidate instead of re-reading backend prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanConvergenceWaitingCandidate {
    #[serde(alias = "run_id")]
    pub run_id: String,
    /// Run-journal revision the waiting state was observed at.
    pub revision: u64,
    #[serde(alias = "candidate_id")]
    pub candidate_id: String,
    /// Candidate sequence inside the convergence loop.
    pub sequence: u32,
    pub title: String,
    #[serde(alias = "plan_text")]
    pub plan_text: String,
    /// `new_primary` or `append_section`.
    pub intent: String,
    /// Proposed revision for `new_primary` candidates; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "proposed_revision")]
    pub proposed_revision: Option<u64>,
}

/// Result of an explicit human decision on a converged `NewPrimaryPlan` candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum PlanConvergenceCommandResult {
    Created {
        #[serde(alias = "run_id")]
        run_id: String,
        #[serde(alias = "created_plan_id")]
        created_plan_id: String,
        #[serde(alias = "created_path")]
        created_path: String,
        #[serde(alias = "file_digest")]
        file_digest: String,
    },
    Rejected {
        #[serde(alias = "run_id")]
        run_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanConvergenceRecoveryPreview {
    pub run_id: String,
    pub journal_revision: u64,
    pub candidate_id: String,
    pub sequence: u32,
    pub target_plan_id: String,
    pub section_title: String,
    pub section_content: String,
    pub operation_digest: String,
    pub plan_context_digest: String,
    pub may_apply_unpublished_append: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanConvergenceRecoveryResult {
    pub run_id: String,
    pub candidate_id: String,
    pub target_plan_id: String,
    pub updated_file_digest: String,
    pub already_applied: bool,
}

impl PlanConvergenceCommandResult {
    pub fn run_id(&self) -> &str {
        match self {
            Self::Created { run_id, .. } | Self::Rejected { run_id } => run_id,
        }
    }
}

fn default_total_timeout_secs() -> u64 {
    900
}

impl Default for PlanConvergenceConfig {
    fn default() -> Self {
        Self {
            opt_in: false,
            total_timeout_secs: default_total_timeout_secs(),
        }
    }
}
