use super::parser::{canonical_operation_bytes, sha256_hex};
use super::types::{
    ConvergenceWaitingReason, PlanCandidate, PlanConvergenceError, PlannerOperationProposal,
    ReviewDecision, ReviewFinding, ReviewVerdict, ScopeEffect,
};
use crate::orchestrator::plan_workspace::PlanContext;

/// Action resulting from pure deterministic evaluation of the Policy Gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyGateAction {
    /// In-scope AppendSection approved and verified; eligible for guarded workspace append.
    AcceptAppendSection {
        target_plan_id: String,
        base_plan_digest: String,
    },
    /// New primary plan candidate approved by reviewer; routes to WaitingForUser for human confirmation.
    HumanGateNewPrimaryPlan,
    /// Reviewer requested changes with valid blocking findings and review budget remains.
    RejectToRevise {
        blocking_findings: Vec<ReviewFinding>,
    },
    /// Execution must pause at WaitingForUser due to the specified typed reason.
    WaitingRequired {
        reason: ConvergenceWaitingReason,
        details: String,
    },
}

pub struct PolicyGate;

impl PolicyGate {
    /// Pure deterministic evaluation of the candidate, verdict, and frozen run bindings.
    pub fn evaluate(
        candidate: &PlanCandidate,
        verdict: &ReviewVerdict,
        frozen_plan_context: &PlanContext,
        opt_in: bool,
        planner_profile_id: &str,
        reviewer_profile_id: &str,
        review_count: u32,
        max_plan_reviews: u32,
        deadline_expired: bool,
    ) -> Result<PolicyGateAction, PlanConvergenceError> {
        // 1. Validate schema, size limits, candidate sequence/ID uniqueness, and operation shape
        if candidate.schema_version != 1 {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_PROPOSAL",
                format!("Unsupported candidate schema_version {}", candidate.schema_version),
            ));
        }
        if verdict.schema_version != 1 {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                format!("Unsupported verdict schema_version {}", verdict.schema_version),
            ));
        }
        if candidate.sequence == 0 {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_PROPOSAL",
                "Candidate sequence must be greater than zero",
            ));
        }
        if uuid::Uuid::parse_str(&candidate.candidate_id).is_err() {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_PROPOSAL",
                format!("Candidate ID '{}' is not a valid UUID", candidate.candidate_id),
            ));
        }

        // 2. Recompute operation payload digest from canonical serialization and compare
        let canonical_bytes = canonical_operation_bytes(&candidate.operation);
        let recomputed_op_digest = sha256_hex(&canonical_bytes);
        if recomputed_op_digest != candidate.operation_payload_digest {
            return Err(PlanConvergenceError::new(
                "PC_DIGEST_MISMATCH",
                format!(
                    "Operation payload digest mismatch: expected '{}', recomputed '{}'",
                    candidate.operation_payload_digest, recomputed_op_digest
                ),
            ));
        }

        // 3. Verify verdict decision and candidate binding tuple equality
        if verdict.reviewed_candidate.sequence != candidate.sequence {
            return Err(PlanConvergenceError::new(
                "PC_STALE_BINDING",
                format!(
                    "Verdict sequence {} does not match candidate sequence {}",
                    verdict.reviewed_candidate.sequence, candidate.sequence
                ),
            ));
        }
        if verdict.reviewed_candidate.candidate_id != candidate.candidate_id {
            return Err(PlanConvergenceError::new(
                "PC_STALE_BINDING",
                format!(
                    "Verdict candidate_id '{}' does not match candidate '{}'",
                    verdict.reviewed_candidate.candidate_id, candidate.candidate_id
                ),
            ));
        }
        if verdict.reviewed_candidate.operation_payload_digest != candidate.operation_payload_digest {
            return Err(PlanConvergenceError::new(
                "PC_STALE_BINDING",
                format!(
                    "Verdict operation_payload_digest '{}' does not match candidate '{}'",
                    verdict.reviewed_candidate.operation_payload_digest, candidate.operation_payload_digest
                ),
            ));
        }
        if verdict.reviewed_candidate.base_plan_digest != candidate.base_plan_digest {
            return Err(PlanConvergenceError::new(
                "PC_STALE_BINDING",
                format!(
                    "Verdict base_plan_digest '{:?}' does not match candidate '{:?}'",
                    verdict.reviewed_candidate.base_plan_digest, candidate.base_plan_digest
                ),
            ));
        }
        if verdict.reviewed_candidate.plan_context_digest != candidate.plan_context_digest {
            return Err(PlanConvergenceError::new(
                "PC_STALE_BINDING",
                format!(
                    "Verdict plan_context_digest '{}' does not match candidate '{}'",
                    verdict.reviewed_candidate.plan_context_digest, candidate.plan_context_digest
                ),
            ));
        }

        // 4. Decision semantics and scope effect checks
        match verdict.decision {
            ReviewDecision::Escalate => {
                return Ok(PolicyGateAction::WaitingRequired {
                    reason: ConvergenceWaitingReason::ReviewerEscalated,
                    details: verdict.summary.clone(),
                });
            }
            ReviewDecision::RequestChanges => {
                if verdict.findings.iter().any(|f| f.scope_effect != ScopeEffect::None) {
                    return Ok(PolicyGateAction::WaitingRequired {
                        reason: ConvergenceWaitingReason::PolicyGateBlocked,
                        details: "Reviewer finding requests scope expansion or new revision".to_string(),
                    });
                }
                let blocking_findings: Vec<ReviewFinding> = verdict
                    .findings
                    .iter()
                    .filter(|f| f.blocking)
                    .cloned()
                    .collect();

                if blocking_findings.is_empty() {
                    return Err(PlanConvergenceError::new(
                        "PC_INVALID_VERDICT",
                        "REQUEST_CHANGES decision must contain at least one blocking finding",
                    ));
                }

                if review_count >= max_plan_reviews {
                    return Ok(PolicyGateAction::WaitingRequired {
                        reason: ConvergenceWaitingReason::ReviewLimitReached,
                        details: format!("Review count {review_count} reached maximum {max_plan_reviews}"),
                    });
                }

                return Ok(PolicyGateAction::RejectToRevise { blocking_findings });
            }
            ReviewDecision::Approve => {
                if verdict.findings.iter().any(|f| f.blocking) {
                    return Err(PlanConvergenceError::new(
                        "PC_GATE_BLOCKED",
                        "APPROVE decision contains blocking findings",
                    ));
                }
                if verdict.findings.iter().any(|f| f.scope_effect != ScopeEffect::None) {
                    return Err(PlanConvergenceError::new(
                        "PC_GATE_BLOCKED",
                        "APPROVE decision contains non-NONE scope effect findings",
                    ));
                }
            }
        }

        // 5. Verify candidate PlanContext digest and base-plan digest match frozen snapshot
        let frozen_context_digest = frozen_plan_context
            .effective_plan_digest
            .as_deref()
            .unwrap_or("");

        if candidate.plan_context_digest != frozen_context_digest {
            return Ok(PolicyGateAction::WaitingRequired {
                reason: ConvergenceWaitingReason::PlanContextStale,
                details: format!(
                    "Candidate plan_context_digest '{}' differs from frozen context '{}'",
                    candidate.plan_context_digest, frozen_context_digest
                ),
            });
        }

        match &candidate.operation {
            PlannerOperationProposal::AppendSection { target_plan_id, .. } => {
                let expected_leaf_id = frozen_plan_context.current_leaf_plan_id.as_deref().unwrap_or("");
                let norm_target = target_plan_id.strip_suffix(".md").unwrap_or(target_plan_id);
                let norm_expected_leaf = expected_leaf_id.strip_suffix(".md").unwrap_or(expected_leaf_id);
                if norm_target != norm_expected_leaf {
                    return Ok(PolicyGateAction::WaitingRequired {
                        reason: ConvergenceWaitingReason::PlanContextStale,
                        details: format!(
                            "AppendSection target '{}' does not match frozen leaf plan '{}'",
                            target_plan_id, expected_leaf_id
                        ),
                    });
                }

                // Verify base_plan_digest matches leaf plan's exact original digest
                let expected_leaf_digest = if let Some(ref primary) = frozen_plan_context.current_primary_plan {
                    if primary.id == norm_target {
                        Some(primary.digest.as_str())
                    } else if let Some(supp) = frozen_plan_context.active_supplemental_plans.iter().find(|s| s.id == norm_target) {
                        Some(supp.digest.as_str())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let expected_digest = expected_leaf_digest.ok_or_else(|| {
                    PlanConvergenceError::new(
                        "PC_GATE_BLOCKED",
                        format!("Target plan '{target_plan_id}' not found in frozen plan context"),
                    )
                })?;

                if candidate.base_plan_digest.as_deref() != Some(expected_digest) {
                    return Ok(PolicyGateAction::WaitingRequired {
                        reason: ConvergenceWaitingReason::PlanContextStale,
                        details: format!(
                            "Candidate base_plan_digest '{:?}' differs from frozen leaf digest '{}'",
                            candidate.base_plan_digest, expected_digest
                        ),
                    });
                }
            }
            PlannerOperationProposal::NewPrimaryPlan { .. } => {
                if candidate.base_plan_digest.is_some() {
                    return Err(PlanConvergenceError::new(
                        "PC_INVALID_PROPOSAL",
                        "NewPrimaryPlan candidate must have null base_plan_digest",
                    ));
                }
            }
        }

        // 6. Verify opt-in, distinct assignments, review count limits, and deadline
        if !opt_in {
            return Err(PlanConvergenceError::new(
                "PC_NOT_OPTED_IN",
                "Plan convergence loop is not opted in",
            ));
        }

        if planner_profile_id.trim().is_empty()
            || reviewer_profile_id.trim().is_empty()
            || planner_profile_id == reviewer_profile_id
        {
            return Err(PlanConvergenceError::new(
                "PC_DISTINCT_ROLES_REQUIRED",
                format!(
                    "Distinct Planner and PlanReviewer profiles are required (planner='{planner_profile_id}', reviewer='{reviewer_profile_id}')"
                ),
            ));
        }

        if review_count > max_plan_reviews {
            return Ok(PolicyGateAction::WaitingRequired {
                reason: ConvergenceWaitingReason::ReviewLimitReached,
                details: format!("Review count {review_count} exceeds limit {max_plan_reviews}"),
            });
        }

        if deadline_expired {
            return Err(PlanConvergenceError::new(
                "PC_TIMEOUT_EXCEEDED",
                "Convergence absolute deadline expired before acceptance",
            ));
        }

        // 7. Permit automatic acceptance ONLY for AppendSection
        match &candidate.operation {
            PlannerOperationProposal::AppendSection { target_plan_id, .. } => {
                let base_digest = candidate
                    .base_plan_digest
                    .clone()
                    .unwrap_or_default();
                Ok(PolicyGateAction::AcceptAppendSection {
                    target_plan_id: target_plan_id.clone(),
                    base_plan_digest: base_digest,
                })
            }
            PlannerOperationProposal::NewPrimaryPlan { .. } => {
                Ok(PolicyGateAction::HumanGateNewPrimaryPlan)
            }
        }
    }
}
