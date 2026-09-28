// ---- Orchestrator Types ----

export type ProfileCapability =
  | "reasoning"
  | "review"
  | "workspace_read"
  | "workspace_write"
  | "command_execution";

export type AgentRole =
  | "planner"
  | "plan_reviewer"
  | "implementer"
  | "fixer"
  | "code_reviewer";

export type ExecutionAdapterType =
  | "provider"
  | "ollama"
  | "cli"
  | "mcp";

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
  | "plan_generation"
  | "plan_review"
  | "plan_revision"
  | "implementation"
  | "validation"
  | "failure_analysis"
  | "finding_aggregation"
  | "fix"
  | "code_review"
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

export interface ValidationGateConfig {
  id: string;
  name: string;
  executable: string;
  args: string[];
  enabled: boolean;
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
  | "plan_review"
  | "implementation"
  | "validation"
  | "code_review"
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
}

export interface RunConfigurationSnapshot {
  projectPath: string;
  assignments: Record<AgentRole, OrchestratorProfile>;
  iterationLimits: LoopIterationLimits;
  validationGates: ValidationGateConfig[];
  budgetLimits: Record<string, BudgetConfig>;
  createdAtUnix: number;
}

export interface StepProgressEvent {
  runId: string;
  step: WorkflowState;
  iterationInfo?: string;
  message: string;
  reviewResult?: ReviewResult;
  validationSummary?: ValidationRunSummary;
  planText?: string;
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
  plan_reviewer: ["review"],
  implementer: ["workspace_write"],
  fixer: ["workspace_write"],
  code_reviewer: ["review"],
};

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

export interface DetectedFiles {
  specMd: boolean;
  implementationPlanMd: boolean;
  agentsMd: boolean;
  readmeMd: boolean;
  packageJson: boolean;
  pyprojectToml: boolean;
  cargoToml: boolean;
  git: boolean;
}

export interface ProjectMetadataResponse {
  path: string;
  exists: boolean;
  isDirectory: boolean;
  projectType: string;
  detectedFiles: DetectedFiles;
  /** Legacy persisted/backend spelling accepted during the wire migration. */
  project_type?: string;
  detected_files?: Record<string, boolean>;
}
