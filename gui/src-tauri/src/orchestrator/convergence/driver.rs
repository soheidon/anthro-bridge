use super::artifacts::{save_candidate_artifact, save_verdict_artifact};
use super::parser::{
    canonical_operation_bytes, parse_planner_proposal, parse_reviewer_proposal, sha256_hex,
};
use super::policy_gate::{PolicyGate, PolicyGateAction};
use super::types::{
    ConvergenceOutcome, ConvergenceWaitingReason, PlanCandidate, PlanConvergenceAuditEntry,
    PlanConvergenceConfig, PlannerOperationProposal, ReviewDecision, ReviewFinding, ReviewVerdict,
};
use crate::orchestrator::plan_workspace::{
    plan_append_with_context_validation, FrozenPlanPayload, FrozenPlanSnapshot, PlanAppendRequest,
    PlanContext, PlanWorkspaceConfig,
};
use crate::orchestrator::types::{AgentRole, OrchestratorProfile};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Injected callback to durably persist a convergence audit record to the run journal.
pub type AuditPersistenceFn =
    Arc<dyn Fn(PlanConvergenceAuditEntry) -> Result<(), String> + Send + Sync>;

/// Typed live progress emitted while the convergence loop runs.
///
/// Carries only typed identities and counters so the presentation layer never
/// depends on backend prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvergenceProgressEvent {
    /// About to dispatch the Planner for `sequence`.
    PlannerDispatch { sequence: u32 },
    /// About to consume one review slot for `sequence`.
    ReviewerReserve { sequence: u32 },
    /// About to dispatch the PlanReviewer for `sequence`.
    ReviewerDispatch { sequence: u32 },
    /// A validated reviewer decision was recorded for `sequence`.
    Verdict {
        sequence: u32,
        decision: ReviewDecision,
        reviews_used: u32,
        reviews_limit: u32,
    },
}

/// Injected callback that surfaces convergence progress to the production event path.
///
/// Progress is delivered through the same durable run-event callback as the rest
/// of the run, so a failure here is a persistence failure that stops the loop
/// rather than a discarded result.
pub type ConvergenceProgressCallback =
    Arc<dyn Fn(ConvergenceProgressEvent) -> Result<(), String> + Send + Sync>;

/// Injected callback to execute an agent role adapter.
pub trait ConvergenceAdapterExecutor: Send + Sync {
    async fn execute_role(
        &self,
        role: AgentRole,
        profile: &OrchestratorProfile,
        system_prompt: &str,
        user_prompt: &str,
        project_path: &Path,
        mcp_servers: Option<&HashMap<String, crate::orchestrator::types::McpServerConfig>>,
        plan_context: Option<&PlanContext>,
        frozen_plan: Option<&FrozenPlanPayload>,
        timeout: Duration,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<String, String>;
}

/// Standard engine-backed adapter executor used in production.
pub struct EngineAdapterExecutor {
    pub engine: crate::orchestrator::engine::OrchestratorEngine,
}

impl ConvergenceAdapterExecutor for EngineAdapterExecutor {
    async fn execute_role(
        &self,
        role: AgentRole,
        profile: &OrchestratorProfile,
        system_prompt: &str,
        user_prompt: &str,
        project_path: &Path,
        mcp_servers: Option<&HashMap<String, crate::orchestrator::types::McpServerConfig>>,
        plan_context: Option<&PlanContext>,
        frozen_plan: Option<&FrozenPlanPayload>,
        timeout: Duration,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<String, String> {
        let event_cb: crate::orchestrator::engine::EventCallback = Arc::new(|_| Ok(()));
        let timeout_token = CancellationToken::new();
        let effective_cancel = match cancel_token {
            Some(ct) => {
                let cloned_ct = ct.clone();
                let child = timeout_token.clone();
                tokio::spawn(async move {
                    cloned_ct.cancelled().await;
                    child.cancel();
                });
                timeout_token.clone()
            }
            None => timeout_token.clone(),
        };

        let exec_future = self.engine.execute_adapter(
            role,
            profile,
            system_prompt,
            user_prompt,
            project_path,
            mcp_servers,
            plan_context,
            frozen_plan,
            Some(&effective_cancel),
            &event_cb,
        );

        match tokio::time::timeout(timeout, exec_future).await {
            Ok(Ok(output)) => Ok(output.content),
            Ok(Err(err)) => Err(err),
            Err(_) => {
                effective_cancel.cancel();
                Err("Adapter execution timed out".to_string())
            }
        }
    }
}

pub struct ConvergenceDriver {
    pub runs_dir: PathBuf,
    pub run_id: String,
    pub project_path: PathBuf,
    pub task_prompt: String,
    pub planner_profile: OrchestratorProfile,
    pub reviewer_profile: OrchestratorProfile,
    pub plan_workspace_config: PlanWorkspaceConfig,
    pub frozen_plan_snapshot: FrozenPlanSnapshot,
    pub mcp_servers: HashMap<String, crate::orchestrator::types::McpServerConfig>,
    pub max_plan_review_iterations: u32,
    pub convergence_config: PlanConvergenceConfig,
}

impl ConvergenceDriver {
    pub async fn run<E: ConvergenceAdapterExecutor>(
        &self,
        executor: &E,
        audit_callback: AuditPersistenceFn,
        progress_callback: Option<ConvergenceProgressCallback>,
        cancel_token: Option<&CancellationToken>,
    ) -> ConvergenceOutcome {
        let loop_start = Instant::now();
        let total_timeout = Duration::from_secs(self.convergence_config.total_timeout_secs);

        // Progress is best-effort for the UI but flows through the durable run
        // event callback, so a failure to emit it stops the loop instead of
        // being silently discarded.
        let emit_progress = |event: ConvergenceProgressEvent| -> Option<ConvergenceOutcome> {
            let callback = progress_callback.as_ref()?;
            match callback(event) {
                Ok(()) => None,
                Err(error) => Some(ConvergenceOutcome::Failed {
                    stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                    safe_details: Some(format!("Failed to emit convergence progress: {error}")),
                }),
            }
        };

        // Same-run convergence replay is not implemented. Never silently reset
        // a persisted reservation or repeat an already accepted operation.
        let journal_path = self.runs_dir.join(format!("{}.json", self.run_id));
        match std::fs::symlink_metadata(&journal_path) {
            Ok(_) => {
                let manager = crate::orchestrator::recovery::JournalManager::new(self.runs_dir.clone());
                let history = manager.read_journal(&self.run_id).and_then(|journal| {
                    let count = crate::orchestrator::recovery::replay_convergence_review_count(&journal)?;
                    super::artifacts::load_and_verify_run_artifacts(
                        &self.runs_dir, &self.run_id, &journal.plan_convergence_audit,
                    ).map_err(|error| error.to_string())?;
                    Ok((count, journal))
                });
                match history {
                    Err(error) => return ConvergenceOutcome::Failed {
                        stable_error_code: "PC_INVALID_REVIEW_HISTORY".into(), safe_details: Some(error),
                    },
                    Ok((count, journal)) if !journal.plan_convergence_audit.is_empty()
                        || journal.status != crate::orchestrator::recovery::RunRecoveryStatus::Active => {
                        return ConvergenceOutcome::WaitingForUser {
                            reason: if count >= self.max_plan_review_iterations {
                                ConvergenceWaitingReason::ReviewLimitReached
                            } else { ConvergenceWaitingReason::PolicyGateBlocked },
                            diagnostic_code: Some("PC_RESTART_REQUIRES_NEW_RUN".into()),
                            candidate_artifact_ref: None, latest_verdict_artifact_ref: None,
                        };
                    }
                    Ok(_) => {}
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return ConvergenceOutcome::Failed {
                stable_error_code: "PC_PERSISTENCE_FAILED".into(), safe_details: Some(error.to_string()),
            },
        }

        // Preflight validations
        if !self.convergence_config.opt_in {
            return ConvergenceOutcome::Failed {
                stable_error_code: "PC_NOT_OPTED_IN".to_string(),
                safe_details: Some("Plan convergence loop is not opted in".to_string()),
            };
        }

        if self.planner_profile.id.trim().is_empty()
            || self.reviewer_profile.id.trim().is_empty()
            || self.planner_profile.id == self.reviewer_profile.id
        {
            return ConvergenceOutcome::Failed {
                stable_error_code: "PC_DISTINCT_ROLES_REQUIRED".to_string(),
                safe_details: Some(format!(
                    "Planner ('{}') and PlanReviewer ('{}') must have distinct assigned profiles",
                    self.planner_profile.id, self.reviewer_profile.id
                )),
            };
        }

        if self.max_plan_review_iterations == 0 {
            return ConvergenceOutcome::Failed {
                stable_error_code: "PC_INVALID_LIMITS".to_string(),
                safe_details: Some("max_plan_review_iterations must be greater than zero".to_string()),
            };
        }

        let frozen_context = &self.frozen_plan_snapshot.plan_context;
        if frozen_context.resolver_status != crate::orchestrator::plan_workspace::PlanResolverStatus::Resolved {
            return ConvergenceOutcome::WaitingForUser {
                reason: ConvergenceWaitingReason::PlanContextStale,
                diagnostic_code: Some("PC_UNRESOLVED_CONTEXT".to_string()),
                candidate_artifact_ref: None,
                latest_verdict_artifact_ref: None,
            };
        }

        let frozen_payload = self.frozen_plan_snapshot.to_frozen_plan_payload();
        let plan_context_digest = frozen_context
            .effective_plan_digest
            .clone()
            .unwrap_or_default();

        let mut sequence = 1u32;
        let mut review_count = 0u32;
        let mut previous_blocking_findings: Option<Vec<ReviewFinding>> = None;
        let mut previous_candidate: Option<PlanCandidate> = None;
        let mut latest_candidate_ref: Option<String> = None;
        let mut latest_verdict_ref: Option<String> = None;

        loop {
            // Check cancellation
            if let Some(ct) = cancel_token {
                if ct.is_cancelled() {
                    return ConvergenceOutcome::Cancelled;
                }
            }

            // Check overall timeout
            let elapsed = loop_start.elapsed();
            if elapsed >= total_timeout {
                return ConvergenceOutcome::Failed {
                    stable_error_code: "PC_TIMEOUT_EXCEEDED".to_string(),
                    safe_details: Some(format!(
                        "Convergence loop exceeded total timeout of {} seconds",
                        self.convergence_config.total_timeout_secs
                    )),
                };
            }
            let remaining_time = total_timeout.saturating_sub(elapsed);

            // 1. Build Planner Prompt
            let system_prompt = "You are the Planner in an autonomous software engineering system. Your job is to produce a structured, actionable implementation plan or amendment according to user requirements and plan workspace context. You must respond with EXACTLY ONE strict JSON object adhering to the canonical camelCase PlannerProposal wire schema (schemaVersion: 1) with no markdown code blocks, no leading/trailing prose, and no extra keys.";

            let user_prompt = if sequence == 1 {
                let leaf_id = frozen_context
                    .current_leaf_plan_id
                    .as_deref()
                    .unwrap_or("none");
                let next_rev = frozen_context
                    .next_primary_revision
                    .unwrap_or(1);
                format!(
                    "Task prompt:\n{}\n\nPlan Context Digest: {}\nCurrent Leaf Plan ID: {}\nNext Primary Revision: {}\n\nRespond with exactly one canonical camelCase JSON object of schemaVersion: 1 containing an operation (either append_section to the current leaf plan or new_primary_plan).",
                    self.task_prompt, plan_context_digest, leaf_id, next_rev
                )
            } else {
                let findings_json = serde_json::to_string_pretty(
                    previous_blocking_findings.as_ref().unwrap_or(&vec![]),
                )
                .unwrap_or_default();
                let prev_cand_json = serde_json::to_string_pretty(
                    previous_candidate.as_ref().unwrap(),
                )
                .unwrap_or_default();
                format!(
                    "Task prompt:\n{}\n\nPrevious plan candidate (sequence {}) was reviewed and changes were requested.\n\nBlocking findings to address:\n{}\n\nPrevious candidate:\n{}\n\nRespond with exactly one updated canonical camelCase JSON object of schemaVersion: 1 addressing all blocking findings.",
                    self.task_prompt, sequence - 1, findings_json, prev_cand_json
                )
            };

            if let Some(outcome) =
                emit_progress(ConvergenceProgressEvent::PlannerDispatch { sequence })
            {
                return outcome;
            }

            // Dispatch Planner adapter
            let planner_adapter_timeout = remaining_time.min(Duration::from_secs(180));
            let raw_planner_output = match executor
                .execute_role(
                    AgentRole::Planner,
                    &self.planner_profile,
                    system_prompt,
                    &user_prompt,
                    &self.project_path,
                    Some(&self.mcp_servers),
                    Some(frozen_context),
                    frozen_payload.as_ref(),
                    planner_adapter_timeout,
                    cancel_token,
                )
                .await
            {
                Ok(out) => out,
                Err(err) => {
                    if let Some(ct) = cancel_token {
                        if ct.is_cancelled() {
                            return ConvergenceOutcome::Cancelled;
                        }
                    }
                    return ConvergenceOutcome::Failed {
                        stable_error_code: "PC_ADAPTER_EXECUTION_FAILED".to_string(),
                        safe_details: Some(format!("Planner adapter error: {err}")),
                    };
                }
            };

            // Parse proposal strictly
            let proposal = match parse_planner_proposal(&raw_planner_output) {
                Ok(p) => p,
                Err(err) => {
                    if let Err(error) = audit_callback(PlanConvergenceAuditEntry {
                        timestamp_unix: now_unix(),
                        event_type: "planner_proposal_invalid".to_string(),
                        sequence,
                        candidate_id: None,
                        artifact_ref: None,
                        artifact_digest: None,
                        role: Some("planner".to_string()),
                        profile_id: Some(self.planner_profile.id.clone()),
                        model: self.planner_profile.model.clone(),
                        decision: None,
                        waiting_reason: Some(ConvergenceWaitingReason::InvalidModelResponse),
                        error_code: Some(err.code.clone()),
                    }) {
                        return ConvergenceOutcome::Failed {
                            stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                            safe_details: Some(error),
                        };
                    }
                    return ConvergenceOutcome::WaitingForUser {
                        reason: ConvergenceWaitingReason::InvalidModelResponse,
                        diagnostic_code: Some(err.code),
                        candidate_artifact_ref: latest_candidate_ref,
                        latest_verdict_artifact_ref: latest_verdict_ref,
                    };
                }
            };

            // Create runtime PlanCandidate
            let candidate_id = Uuid::new_v4().to_string();
            let canonical_bytes = canonical_operation_bytes(&proposal.operation);
            let operation_payload_digest = sha256_hex(&canonical_bytes);

            let (target_plan_id, base_plan_digest) = match &proposal.operation {
                PlannerOperationProposal::AppendSection { target_plan_id, .. } => {
                    let norm_target = target_plan_id.strip_suffix(".md").unwrap_or(target_plan_id);
                    let base_digest = if let Some(ref primary) = frozen_context.current_primary_plan {
                        if primary.id == norm_target {
                            Some(primary.digest.clone())
                        } else if let Some(supp) = frozen_context.active_supplemental_plans.iter().find(|s| s.id == norm_target) {
                            Some(supp.digest.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    (Some(target_plan_id.clone()), base_digest)
                }
                PlannerOperationProposal::NewPrimaryPlan { .. } => (None, None),
            };

            let candidate = PlanCandidate {
                schema_version: 1,
                run_id: self.run_id.clone(),
                sequence,
                candidate_id: candidate_id.clone(),
                operation: proposal.operation.clone(),
                target_plan_id,
                base_plan_digest,
                plan_context_digest: plan_context_digest.clone(),
                operation_payload_digest: operation_payload_digest.clone(),
                producer_role: "planner".to_string(),
                producer_profile_id: self.planner_profile.id.clone(),
                producer_model: self.planner_profile.model.clone().unwrap_or_else(|| self.planner_profile.id.clone()),
                created_at_unix: now_unix(),
            };

            // Save candidate artifact
            let (c_ref, c_digest) = match save_candidate_artifact(&self.runs_dir, &self.run_id, &candidate) {
                Ok(res) => res,
                Err(err) => {
                    return ConvergenceOutcome::Failed {
                        stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                        safe_details: Some(format!("Failed to save candidate artifact: {err}")),
                    };
                }
            };
            latest_candidate_ref = Some(c_ref.clone());

            // Persist candidate audit record in journal
            if let Err(err) = audit_callback(PlanConvergenceAuditEntry {
                timestamp_unix: now_unix(),
                event_type: "candidate_created".to_string(),
                sequence,
                candidate_id: Some(candidate_id.clone()),
                artifact_ref: Some(c_ref.clone()),
                artifact_digest: Some(c_digest.clone()),
                role: Some("planner".to_string()),
                profile_id: Some(self.planner_profile.id.clone()),
                model: self.planner_profile.model.clone(),
                decision: None,
                waiting_reason: None,
                error_code: None,
            }) {
                return ConvergenceOutcome::Failed {
                    stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                    safe_details: Some(format!("Failed to persist journal candidate record: {err}")),
                };
            }

            // Durably increment review attempt count and reserve in journal before dispatching reviewer
            if review_count >= self.max_plan_review_iterations {
                if let Err(err) = audit_callback(PlanConvergenceAuditEntry {
                    timestamp_unix: now_unix(),
                    event_type: "waiting_for_user".to_string(),
                    sequence,
                    candidate_id: Some(candidate_id.clone()),
                    artifact_ref: latest_candidate_ref.clone(),
                    artifact_digest: None,
                    role: None,
                    profile_id: None,
                    model: None,
                    decision: None,
                    waiting_reason: Some(ConvergenceWaitingReason::ReviewLimitReached),
                    error_code: Some("PC_REVIEW_LIMIT_REACHED".to_string()),
                }) {
                    return ConvergenceOutcome::Failed {
                        stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                        safe_details: Some(format!("Failed to persist review limit waiting record: {err}")),
                    };
                }
                return ConvergenceOutcome::WaitingForUser {
                    reason: ConvergenceWaitingReason::ReviewLimitReached,
                    diagnostic_code: Some("PC_REVIEW_LIMIT_REACHED".to_string()),
                    candidate_artifact_ref: latest_candidate_ref,
                    latest_verdict_artifact_ref: latest_verdict_ref,
                };
            }
            if let Some(outcome) =
                emit_progress(ConvergenceProgressEvent::ReviewerReserve { sequence })
            {
                return outcome;
            }
            review_count += 1;

            if let Err(err) = audit_callback(PlanConvergenceAuditEntry {
                timestamp_unix: now_unix(),
                event_type: "review_attempt_reserved".to_string(),
                sequence,
                candidate_id: Some(candidate_id.clone()),
                artifact_ref: latest_candidate_ref.clone(),
                artifact_digest: None,
                role: Some("plan_reviewer".to_string()),
                profile_id: Some(self.reviewer_profile.id.clone()),
                model: self.reviewer_profile.model.clone(),
                decision: None,
                waiting_reason: None,
                error_code: None,
            }) {
                return ConvergenceOutcome::Failed {
                    stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                    safe_details: Some(format!("Failed to persist review attempt reservation: {err}")),
                };
            }

            // 2. Build PlanReviewer Prompt
            let reviewer_system_prompt = "You are the Plan Reviewer in an autonomous software engineering system. Your job is to audit proposed plan candidates against task requirements, codebase context, and architectural constraints. You must respond with EXACTLY ONE strict JSON object adhering to the canonical camelCase ReviewerResponseProposal wire schema (schemaVersion: 1) with no markdown code blocks, no leading/trailing prose, and no extra keys.";

            let op_json = serde_json::to_string_pretty(&candidate.operation).unwrap_or_default();
            let base_plan_digest_str = match &candidate.base_plan_digest {
                Some(s) => format!("\"{}\"", s),
                None => "null".to_string(),
            };

            let reviewer_user_prompt = format!(
                "Task prompt:\n{}\n\nPlan Candidate under review:\nSequence: {}\nCandidate ID: {}\nOperation:\n{}\nOperation Payload Digest: {}\nBase Plan Digest: {}\nPlan Context Digest: {}\n\nAudit this plan candidate and return your structured verdict JSON object of schemaVersion: 1 with reviewedCandidate binding, summary, and findings, using canonical camelCase field names only.",
                self.task_prompt, candidate.sequence, candidate.candidate_id, op_json, candidate.operation_payload_digest, base_plan_digest_str, candidate.plan_context_digest
            );

            if let Some(outcome) =
                emit_progress(ConvergenceProgressEvent::ReviewerDispatch { sequence })
            {
                return outcome;
            }

            // Dispatch PlanReviewer adapter
            let reviewer_start = Instant::now();
            let elapsed_now = loop_start.elapsed();
            let remaining_for_reviewer = total_timeout.saturating_sub(elapsed_now).min(Duration::from_secs(180));

            let raw_reviewer_output = match executor
                .execute_role(
                    AgentRole::PlanReviewer,
                    &self.reviewer_profile,
                    reviewer_system_prompt,
                    &reviewer_user_prompt,
                    &self.project_path,
                    Some(&self.mcp_servers),
                    Some(frozen_context),
                    frozen_payload.as_ref(),
                    remaining_for_reviewer,
                    cancel_token,
                )
                .await
            {
                Ok(out) => out,
                Err(err) => {
                    if let Some(ct) = cancel_token {
                        if ct.is_cancelled() {
                            return ConvergenceOutcome::Cancelled;
                        }
                    }
                    return ConvergenceOutcome::Failed {
                        stable_error_code: "PC_ADAPTER_EXECUTION_FAILED".to_string(),
                        safe_details: Some(format!("PlanReviewer adapter error: {err}")),
                    };
                }
            };
            let review_duration_ms = reviewer_start.elapsed().as_millis() as u64;

            // Parse reviewer verdict strictly
            let reviewer_proposal = match parse_reviewer_proposal(&raw_reviewer_output) {
                Ok(vp) => vp,
                Err(err) => {
                    if let Err(audit_err) = audit_callback(PlanConvergenceAuditEntry {
                        timestamp_unix: now_unix(),
                        event_type: "verdict_invalid".to_string(),
                        sequence,
                        candidate_id: Some(candidate_id.clone()),
                        artifact_ref: None,
                        artifact_digest: None,
                        role: Some("plan_reviewer".to_string()),
                        profile_id: Some(self.reviewer_profile.id.clone()),
                        model: self.reviewer_profile.model.clone(),
                        decision: None,
                        waiting_reason: Some(ConvergenceWaitingReason::InvalidModelResponse),
                        error_code: Some(err.code.clone()),
                    }) {
                        return ConvergenceOutcome::Failed {
                            stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                            safe_details: Some(format!("Failed to persist invalid verdict audit: {audit_err}")),
                        };
                    }
                    return ConvergenceOutcome::WaitingForUser {
                        reason: ConvergenceWaitingReason::InvalidModelResponse,
                        diagnostic_code: Some(err.code),
                        candidate_artifact_ref: latest_candidate_ref,
                        latest_verdict_artifact_ref: latest_verdict_ref,
                    };
                }
            };

            if let Some(outcome) = emit_progress(ConvergenceProgressEvent::Verdict {
                sequence,
                decision: reviewer_proposal.decision,
                reviews_used: review_count,
                reviews_limit: self.max_plan_review_iterations,
            }) {
                return outcome;
            }

            // Build runtime ReviewVerdict
            let verdict = ReviewVerdict {
                schema_version: 1,
                decision: reviewer_proposal.decision,
                reviewed_candidate: reviewer_proposal.reviewed_candidate,
                summary: reviewer_proposal.summary,
                findings: reviewer_proposal.findings,
                reviewer_role: "plan_reviewer".to_string(),
                reviewer_profile_id: self.reviewer_profile.id.clone(),
                reviewer_model: self.reviewer_profile.model.clone().unwrap_or_else(|| self.reviewer_profile.id.clone()),
                duration_ms: review_duration_ms,
            };

            // Save verdict artifact
            let (v_ref, v_digest) = match save_verdict_artifact(&self.runs_dir, &self.run_id, &verdict) {
                Ok(res) => res,
                Err(err) => {
                    return ConvergenceOutcome::Failed {
                        stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                        safe_details: Some(format!("Failed to save verdict artifact: {err}")),
                    };
                }
            };
            latest_verdict_ref = Some(v_ref.clone());

            // Persist verdict audit record in journal
            if let Err(err) = audit_callback(PlanConvergenceAuditEntry {
                timestamp_unix: now_unix(),
                event_type: "verdict_recorded".to_string(),
                sequence,
                candidate_id: Some(candidate_id.clone()),
                artifact_ref: Some(v_ref.clone()),
                artifact_digest: Some(v_digest.clone()),
                role: Some("plan_reviewer".to_string()),
                profile_id: Some(self.reviewer_profile.id.clone()),
                model: self.reviewer_profile.model.clone(),
                decision: Some(verdict.decision),
                waiting_reason: None,
                error_code: None,
            }) {
                return ConvergenceOutcome::Failed {
                    stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                    safe_details: Some(format!("Failed to persist journal verdict record: {err}")),
                };
            }

            // 3. Evaluate Policy Gate
            let is_deadline_expired = loop_start.elapsed() >= total_timeout;
            let gate_action = match PolicyGate::evaluate(
                &candidate,
                &verdict,
                frozen_context,
                self.convergence_config.opt_in,
                &self.planner_profile.id,
                &self.reviewer_profile.id,
                review_count,
                self.max_plan_review_iterations,
                is_deadline_expired,
            ) {
                Ok(action) => action,
                Err(err) => {
                    if let Err(audit_err) = audit_callback(PlanConvergenceAuditEntry {
                        timestamp_unix: now_unix(),
                        event_type: "policy_gate_error".to_string(),
                        sequence,
                        candidate_id: Some(candidate_id.clone()),
                        artifact_ref: Some(v_ref.clone()),
                        artifact_digest: Some(v_digest.clone()),
                        role: None,
                        profile_id: None,
                        model: None,
                        decision: Some(verdict.decision),
                        waiting_reason: Some(ConvergenceWaitingReason::PolicyGateBlocked),
                        error_code: Some(err.code.clone()),
                    }) {
                        return ConvergenceOutcome::Failed {
                            stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                            safe_details: Some(format!("Failed to persist policy gate error: {audit_err}")),
                        };
                    }
                    return ConvergenceOutcome::WaitingForUser {
                        reason: ConvergenceWaitingReason::PolicyGateBlocked,
                        diagnostic_code: Some(err.code),
                        candidate_artifact_ref: latest_candidate_ref,
                        latest_verdict_artifact_ref: latest_verdict_ref,
                    };
                }
            };

            // 4. Act on PolicyGate outcome
            match gate_action {
                PolicyGateAction::AcceptAppendSection {
                    target_plan_id,
                    base_plan_digest,
                } => {
                    // Extract section details
                    let (section_type, section_title, section_content) = match &candidate.operation {
                        PlannerOperationProposal::AppendSection {
                            section_type,
                            section_title,
                            section_content,
                            ..
                        } => (section_type.clone(), section_title.clone(), section_content.clone()),
                        _ => unreachable!(),
                    };

                    let append_req = PlanAppendRequest {
                        target_plan_id: target_plan_id.clone(),
                        expected_file_digest: base_plan_digest,
                        section_type,
                        section_title,
                        section_content,
                        // Bind idempotency to this persisted candidate transaction
                        // rather than content alone, so equal payloads from distinct
                        // candidates remain independent authorized operations.
                        idempotency_token: format!("planconv-{}-{}", candidate.run_id, candidate.candidate_id),
                        idempotency_operation_digest: Some(candidate.operation_payload_digest.clone()),
                    };

                    // Atomic guarded append under shared PLAN_WRITE_LOCK verifying full PlanContext freshness
                    let append_resp = match plan_append_with_context_validation(
                        &self.project_path,
                        &self.plan_workspace_config,
                        append_req,
                        &candidate.plan_context_digest,
                        &target_plan_id,
                    ) {
                        Ok(resp) => resp,
                        Err(err) => {
                            if err.code == "stale_plan_context"
                                || err.code == "stale_plan_leaf"
                                || err.code == "stale_plan_file_digest"
                            {
                                if let Err(audit_err) = audit_callback(PlanConvergenceAuditEntry {
                                    timestamp_unix: now_unix(),
                                    event_type: "context_drift_detected".to_string(),
                                    sequence,
                                    candidate_id: Some(candidate.candidate_id.clone()),
                                    artifact_ref: latest_candidate_ref.clone(),
                                    artifact_digest: None,
                                    role: None,
                                    profile_id: None,
                                    model: None,
                                    decision: None,
                                    waiting_reason: Some(ConvergenceWaitingReason::PlanContextStale),
                                    error_code: Some("PC_CONTEXT_DRIFT".to_string()),
                                }) {
                                    return ConvergenceOutcome::Failed {
                                        stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                                        safe_details: Some(format!("Failed to persist context drift record: {audit_err}")),
                                    };
                                }
                                return ConvergenceOutcome::WaitingForUser {
                                    reason: ConvergenceWaitingReason::PlanContextStale,
                                    diagnostic_code: Some("PC_CONTEXT_DRIFT".to_string()),
                                    candidate_artifact_ref: latest_candidate_ref,
                                    latest_verdict_artifact_ref: latest_verdict_ref,
                                };
                            }
                            return ConvergenceOutcome::Failed {
                                stable_error_code: "PC_WRITE_FAILED".to_string(),
                                safe_details: Some(format!("Plan append failed: {err}")),
                            };
                        }
                    };

                    // Persist acceptance audit
                    if let Err(err) = audit_callback(PlanConvergenceAuditEntry {
                        timestamp_unix: now_unix(),
                        event_type: "acceptance_succeeded".to_string(),
                        sequence,
                        candidate_id: Some(candidate.candidate_id.clone()),
                        artifact_ref: latest_candidate_ref.clone(),
                        artifact_digest: None,
                        role: None,
                        profile_id: None,
                        model: None,
                        decision: Some(ReviewDecision::Approve),
                        waiting_reason: None,
                        error_code: None,
                    }) {
                        return ConvergenceOutcome::Failed {
                            stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                            safe_details: Some(format!("Failed to persist accepted outcome to journal: {err}")),
                        };
                    }

                    return ConvergenceOutcome::Accepted {
                        candidate_id: candidate.candidate_id,
                        target_plan_id,
                        accepted_file_digest: append_resp.updated_file_digest,
                        reviews_used: review_count,
                    };
                }
                PolicyGateAction::HumanGateNewPrimaryPlan => {
                    if let Err(err) = audit_callback(PlanConvergenceAuditEntry {
                        timestamp_unix: now_unix(),
                        event_type: "human_gate_new_primary_plan".to_string(),
                        sequence,
                        candidate_id: Some(candidate_id.clone()),
                        artifact_ref: latest_candidate_ref.clone(),
                        artifact_digest: None,
                        role: None,
                        profile_id: None,
                        model: None,
                        decision: Some(ReviewDecision::Approve),
                        waiting_reason: Some(ConvergenceWaitingReason::NewPrimaryPlanConfirmation),
                        error_code: Some("PC_HUMAN_CONFIRMATION_REQUIRED".to_string()),
                    }) {
                        return ConvergenceOutcome::Failed {
                            stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                            safe_details: Some(format!("Failed to persist human gate record: {err}")),
                        };
                    }
                    return ConvergenceOutcome::WaitingForUser {
                        reason: ConvergenceWaitingReason::NewPrimaryPlanConfirmation,
                        diagnostic_code: Some("PC_HUMAN_CONFIRMATION_REQUIRED".to_string()),
                        candidate_artifact_ref: latest_candidate_ref,
                        latest_verdict_artifact_ref: latest_verdict_ref,
                    };
                }
                PolicyGateAction::RejectToRevise { blocking_findings } => {
                    previous_blocking_findings = Some(blocking_findings);
                    previous_candidate = Some(candidate);
                    sequence += 1;
                    continue;
                }
                PolicyGateAction::WaitingRequired { reason, details } => {
                    let diag_code = match reason {
                        ConvergenceWaitingReason::ReviewLimitReached => "PC_REVIEW_LIMIT_REACHED".to_string(),
                        ConvergenceWaitingReason::ReviewerEscalated => "PC_REVIEWER_ESCALATED".to_string(),
                        ConvergenceWaitingReason::PlanContextStale => "PC_CONTEXT_DRIFT".to_string(),
                        ConvergenceWaitingReason::NewPrimaryPlanConfirmation => "PC_HUMAN_CONFIRMATION_REQUIRED".to_string(),
                        ConvergenceWaitingReason::InvalidModelResponse => "PC_INVALID_PROPOSAL".to_string(),
                        ConvergenceWaitingReason::PolicyGateBlocked => details.clone(),
                    };
                    if let Err(err) = audit_callback(PlanConvergenceAuditEntry {
                        timestamp_unix: now_unix(),
                        event_type: "waiting_for_user".to_string(),
                        sequence,
                        candidate_id: Some(candidate_id.clone()),
                        artifact_ref: latest_candidate_ref.clone(),
                        artifact_digest: None,
                        role: None,
                        profile_id: None,
                        model: None,
                        decision: Some(verdict.decision),
                        waiting_reason: Some(reason),
                        error_code: Some(diag_code.clone()),
                    }) {
                        return ConvergenceOutcome::Failed {
                            stable_error_code: "PC_PERSISTENCE_FAILED".to_string(),
                            safe_details: Some(format!("Failed to persist waiting record: {err}")),
                        };
                    }
                    return ConvergenceOutcome::WaitingForUser {
                        reason,
                        diagnostic_code: Some(diag_code),
                        candidate_artifact_ref: latest_candidate_ref,
                        latest_verdict_artifact_ref: latest_verdict_ref,
                    };
                }
            }
        }
    }
}
