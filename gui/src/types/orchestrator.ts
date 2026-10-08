// ---- Orchestrator Types ----

export type ProfileCapability =
  | "reasoning"
  | "review"
  | "workspace_read"
  | "workspace_write"
  | "command_execution";

export type AgentRole =
  | "planner"
  | "plan_integrator"
  | "plan_reviewer"
  | "implementer"
  | "fixer"
  | "code_reviewer";

export type WorkflowType =
  | "full_loop"
  | "human_gated_loop"
  | "plan_only"
  | "implement_only"
  | "review_only";

export type ExecutionAdapterType =
  | "provider"
  | "ollama"
  | "cli"
  | "mcp"
  | "antigravity";

export interface OrchestratorProfile {
  id: string;
  displayName: string;
  adapter: ExecutionAdapterType;
  capabilities: ProfileCapability[];

  // Provider adapter settings
  providerId?: string;
  providerProfileId?: string;
  model?: string;
  thinkingMode?: "normal" | "thinking";
  reasoningEffort?: string;

  // Ollama adapter settings (independent localhost endpoint)
  ollamaModel?: string;
  ollamaEndpoint?: string;

  // CLI adapter settings (direct process invocation)
  executable?: string;
  args?: string[];

  // External MCP adapter settings
  externalMcpServer?: string;
  mcpTool?: string;

  contextWindowTokens?: number;
}

export interface RoleAssignment {
  role: AgentRole;
  profileId: string;
  customPromptSupplement?: string;
  escalationRole?: AgentRole;
}

export type RunControlState = "running" | "paused" | "cancelled";

export type WorkflowState =
  | "idle"
  | "loading_project"
  | "building_context"
  | "plan_draft"
  | "plan_generation"
  | "awaiting_antigravity_claim"
  | "plan_integration"
  | "plan_review"
  | "plan_revision"
  | "implementation"
  | "validation"
  | "failure_analysis"
  | "finding_aggregation"
  | "fix"
  | "code_review"
  | "human_gate"
  | "waiting_for_user"
  | "waiting_for_blocking_resolution"
  | "complete"
  | "failed"
  | "paused"
  | "cancelled";

export interface StartRunResponse {
  runId: string;
}

export interface LoopIterationLimits {
  maxPlanReviewIterations: number;
  maxFixIterations: number;
  maxCodeReviewIterations: number;
}

export type ReviewVerdict =
  | "approved"
  | "changes_required"
  | "needs_clarification"
  | "failed";

export type FindingSeverity = "critical" | "high" | "medium" | "low";

export interface ReviewFinding {
  id: string;
  severity: FindingSeverity;
  category?: string;
  file?: string;
  line?: number;
  issue: string;
  recommendation?: string;
  isBlocking: boolean;
}

export interface ReviewResult {
  verdict: ReviewVerdict;
  findings: ReviewFinding[];
  summary: string;
  rawOutput: string;
}

export type ValidationCategory =
  | "static_check"
  | "tests"
  | "lint"
  | "format_check"
  | "build"
  | "security_audit"
  | "repository_check"
  | "comprehensive_check"
  | "status_check"
  | "custom";

export type GateSuccessCriteria =
  | "exit_code_zero"
  | "empty_output";

export interface ValidationGateConfig {
  id: string;
  name: string;
  executable: string;
  args: string[];
  enabled: boolean;
  category?: ValidationCategory;
  successCriteria?: GateSuccessCriteria;
  workingDir?: string;
  failOnError: boolean;
  isAdvancedCustom?: boolean;
}

export interface AuthorizedCustomGate {
  gateId: string;
  executable: string;
  args: string[];
  canonicalWorkingDir: string;
  commandHash: string;
}

export interface BudgetConfig {
  maxCallsPerRun: number;
  maxConsecutiveCalls: number;
  timeoutSeconds: number;
  fallbackProfileId?: string;
  onRateLimit: "pause" | "fallback" | "escalate" | "stop";
}

export interface OrchestratorPreset {
  id: string;
  name: string;
  description: string;
  assignments: Record<AgentRole, string>; // role -> profileId
  iterationLimits?: LoopIterationLimits;
  validationGates?: ValidationGateConfig[];
  budgetLimits?: Record<string, BudgetConfig>;
}

export type OrchestratorStep =
  | "planning"
  | "plan_integration"
  | "plan_review"
  | "implementation"
  | "validation"
  | "code_review"
  | "human_gate"
  | "fixing"
  | "completed";

export interface OrchestratorConfig {
  projectPath?: string;
  activeWorkflowId?: string;
  activePresetId?: string;
  profiles?: OrchestratorProfile[];
  assignments?: Record<AgentRole, RoleAssignment>;
  iterationLimits?: LoopIterationLimits;
  validationGates?: ValidationGateConfig[];
  budgetLimits?: Record<string, BudgetConfig>;
  customPresets?: OrchestratorPreset[];
  authorizedCustomGates?: AuthorizedCustomGate[];
  quickSlots?: OrchestratorQuickSlot[];
  autoValidationEnabled?: boolean;
  leanAntigravityMode?: boolean;
}

export interface OrchestratorQuickSlot {
  id: string;
  profileId: string;
  label: string;
  visible: boolean;
  order: number;
}

export interface RunTransientOverrides {
  gateOverrides?: Record<string, boolean>;
  limitOverrides?: Partial<LoopIterationLimits>;
}

export interface RunConfigurationSnapshot {
  projectPath: string;
  assignments: Record<AgentRole, OrchestratorProfile>;
  iterationLimits: LoopIterationLimits;
  validationGates: ValidationGateConfig[];
  budgetLimits: Record<string, BudgetConfig>;
  createdAtUnix: number;
  leanAntigravityMode?: boolean;
}

export type RunRecoveryStatus = "active" | "interrupted" | "complete" | "failed" | "cancelled";
export interface RunRecoverySummary {
  runId: string;
  workflowType: string;
  projectPath: string;
  status: RunRecoveryStatus;
  currentState: WorkflowState;
  lastSuccessfulState?: WorkflowState | null;
  revision: number;
  createdAtUnix: number;
  updatedAtUnix: number;
  isResumable: boolean;
  error?: string | null;
}

export interface RecoveryPreflight {
  runId: string;
  journalRevision: number;
  checkpointId: string;
  checkpointKind: "entry_baseline" | "stage_checkpoint" | "adopt_baseline" | "shelve_backup";
  checkpointStage: WorkflowState;
  resumeStage?: WorkflowState | null;
  checkpointDigest: string;
  backupId: string;
  backupDestination: string;
  currentFingerprint: string;
  workspaceMatches: boolean;
  canResume: boolean;
  canRestore: boolean;
  canAdopt: boolean;
  affectedPaths: string[];
  reasonCode?: string | null;
}

export interface RecoveryResolution {
  backupId?: string | null;
  checkpointId?: string | null;
  resumeStage?: WorkflowState | null;
  journalRevision: number;
}

/** Per-run archive folder for the approved plan; never persisted in OrchestratorConfig. */
export interface PlanArchiveOptions {
  directory: string;
}

/** Advisory next archive filename derived by the backend. */
export interface PlanArchivePreview {
  nextFileName: string;
}

export interface StepProgressEvent {
  runId: string;
  step: WorkflowState;
  iterationInfo?: string;
  message: string;
  reviewResult?: ReviewResult;
  validationSummary?: ValidationRunSummary;
  planText?: string;
  antigravityDispatches?: number;
  antigravityDispatchLimit?: number;
  budgetScope?: "task" | "run";
  waitingReason?: "budget_exhausted" | "worker_disconnected" | "clarification_required" | string;
}

export interface ValidationGateResult {
  gateId: string;
  gateName: string;
  executable: string;
  args: string[];
  exitCode: number;
  success: boolean;
  stdout: string;
  stderr: string;
  isTruncated: boolean;
  durationMs: number;
  failOnError: boolean;
  timedOut: boolean;
  cancelled: boolean;
}

export interface ValidationRunSummary {
  passed: boolean;
  totalGatesRun: number;
  failedGateNames: string[];
  results: ValidationGateResult[];
  formattedDiagnostics: string;
}

export interface RunLogEvent {
  runId: string;
  message: string;
}

export function shouldAcceptRunEvent(
  eventRunId: string,
  activeRunId: string | null,
  startPending = false,
): boolean {
  return !startPending && activeRunId !== null && eventRunId === activeRunId;
}

export interface CapabilityValidationError {
  role: AgentRole;
  profileId: string;
  profileName: string;
  missingCapabilities: ProfileCapability[];
  message: string;
}

export const ROLE_REQUIRED_CAPABILITIES: Record<AgentRole, ProfileCapability[]> = {
  planner: ["reasoning"],
  plan_integrator: ["workspace_write"],
  plan_reviewer: ["review"],
  implementer: ["workspace_write"],
  fixer: ["workspace_write"],
  code_reviewer: ["review"],
};

export function getActiveRolesForWorkflow(workflowId: string): AgentRole[] {
  switch (workflowId) {
    case "full_loop":
      return ["planner", "plan_reviewer", "implementer", "fixer", "code_reviewer"];
    case "human_gated_loop":
      return ["planner", "plan_integrator", "plan_reviewer", "implementer", "fixer", "code_reviewer"];
    case "plan_only":
      return ["planner", "plan_reviewer"];
    case "implement_only":
      return ["implementer", "fixer"];
    case "review_only":
      return ["code_reviewer"];
    default:
      return [];
  }
}

export type HumanGateDecision =
  | { type: "approve" }
  | { type: "request_changes"; feedback: string }
  | { type: "abort" };

export interface OrchestratorTaskEnvelope {
  runId: string;
  taskId: string;
  stage: WorkflowState;
  role: AgentRole;
  epoch: number;
  projectPath: string;
  approvedPlan?: string;
  taskPrompt?: string;
  reviewFeedback?: string;
  validationSummary?: string;
}

export function validateReviewOnlyProfile(
  profile: OrchestratorProfile,
): CapabilityValidationError | null {
  if (profile.adapter !== "provider" && profile.adapter !== "ollama") {
    return {
      role: "code_reviewer",
      profileId: profile.id,
      profileName: profile.displayName,
      missingCapabilities: [],
      message: `Review-only workflow requires a read-only adapter (Provider or Ollama). Profile "${profile.displayName}" uses adapter "${profile.adapter}".`,
    };
  }
  if (!profile.capabilities.includes("review")) {
    return {
      role: "code_reviewer",
      profileId: profile.id,
      profileName: profile.displayName,
      missingCapabilities: ["review"],
      message: `Role "code_reviewer" cannot run with profile "${profile.displayName}": missing capability [review].`,
    };
  }
  return null;
}

export function validateWorkflowRoleCapabilities(
  workflowType: string,
  role: AgentRole,
  profile: OrchestratorProfile | undefined,
): CapabilityValidationError | null {
  if (!profile) {
    return {
      role,
      profileId: "",
      profileName: "Unknown",
      missingCapabilities: ROLE_REQUIRED_CAPABILITIES[role] || [],
      message: `Role "${role}" has no assigned profile.`,
    };
  }
  if (workflowType === "review_only" && role === "code_reviewer") {
    return validateReviewOnlyProfile(profile);
  }
  return validateRoleCapabilities(role, profile);
}

export function getCompatibleProfiles(
  profiles: OrchestratorProfile[],
  role: AgentRole,
  workflowId?: string,
): OrchestratorProfile[] {
  return profiles.filter(
    (profile) => validateWorkflowRoleCapabilities(workflowId || "full_loop", role, profile) === null,
  );
}

export function getQuickSlotButtons(
  quickSlots: OrchestratorQuickSlot[],
  profiles: OrchestratorProfile[],
  role: AgentRole,
  workflowId?: string,
): Array<{ slot: OrchestratorQuickSlot; profile: OrchestratorProfile }> {
  const byId = new Map(profiles.map((profile) => [profile.id, profile]));
  return [...quickSlots]
    .filter((slot) => slot.visible)
    .sort((left, right) => left.order - right.order)
    .flatMap((slot) => {
      const profile = byId.get(slot.profileId);
      return profile && validateWorkflowRoleCapabilities(workflowId || "full_loop", role, profile) === null
        ? [{ slot, profile }]
        : [];
    });
}

export function getOtherProfiles(
  quickSlots: OrchestratorQuickSlot[],
  profiles: OrchestratorProfile[],
  role: AgentRole,
  workflowId?: string,
): OrchestratorProfile[] {
  const shownProfileIds = new Set(
    getQuickSlotButtons(quickSlots, profiles, role, workflowId).map(({ profile }) => profile.id),
  );
  return getCompatibleProfiles(profiles, role, workflowId).filter((profile) => !shownProfileIds.has(profile.id));
}

export function validateRoleCapabilities(
  role: AgentRole,
  profile: OrchestratorProfile | undefined,
): CapabilityValidationError | null {
  if (!profile) {
    return {
      role,
      profileId: "",
      profileName: "Unknown",
      missingCapabilities: ROLE_REQUIRED_CAPABILITIES[role],
      message: `Role "${role}" has no assigned profile.`,
    };
  }

  const required = ROLE_REQUIRED_CAPABILITIES[role] || [];
  const missing = required.filter((cap) => !profile.capabilities.includes(cap));

  if (missing.length > 0) {
    return {
      role,
      profileId: profile.id,
      profileName: profile.displayName,
      missingCapabilities: missing,
      message: `Role "${role}" cannot run with profile "${profile.displayName}": missing capability [${missing.join(", ")}].`,
    };
  }

  return null;
}

export function validateAllRoleCapabilities(
  assignments: Record<string, string>,
  profiles: OrchestratorProfile[]
): CapabilityValidationError[] {
  const errors: CapabilityValidationError[] = [];
  for (const [roleStr, profileId] of Object.entries(assignments)) {
    const role = roleStr as AgentRole;
    const profile = profiles.find((p) => p.id === profileId);
    const err = validateRoleCapabilities(role, profile);
    if (err) errors.push(err);
  }
  return errors;
}

export function validateActiveRoleCapabilities(
  workflowType: string,
  assignments: Record<string, string>,
  profiles: OrchestratorProfile[],
): CapabilityValidationError[] {
  const activeRoles = getActiveRolesForWorkflow(workflowType);
  const errors: CapabilityValidationError[] = [];
  for (const role of activeRoles) {
    const profileId = assignments[role];
    const profile = profiles.find((p) => p.id === profileId);
    const err = validateWorkflowRoleCapabilities(workflowType, role, profile);
    if (err) errors.push(err);
  }
  return errors;
}

export interface DetectedFiles {
  specMd: boolean;
  implementationPlanMd: boolean;
  agentsMd: boolean;
  readmeMd: boolean;
  packageJson: boolean;
  tsconfigJson: boolean;
  pyprojectToml: boolean;
  requirementsTxt: boolean;
  cargoToml: boolean;
  goMod: boolean;
  description: boolean;
  renvLock: boolean;
  claspJson: boolean;
  git: boolean;
}

export interface ProjectMetadataResponse {
  path: string;
  exists: boolean;
  isDirectory: boolean;
  projectType: string;
  detectedFiles: DetectedFiles;
  suggestedGates?: ValidationGateConfig[];
  /** Legacy persisted/backend spelling accepted during the wire migration. */
  project_type?: string;
  detected_files?: Record<string, boolean>;
  suggested_gates?: ValidationGateConfig[];
}
