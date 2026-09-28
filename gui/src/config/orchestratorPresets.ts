import type {
  OrchestratorProfile,
  OrchestratorPreset,
  ValidationGateConfig,
  LoopIterationLimits,
  RoleAssignment,
  AgentRole,
  OrchestratorQuickSlot,
} from "../types/orchestrator";

export const DEFAULT_ORCHESTRATOR_QUICK_SLOTS: OrchestratorQuickSlot[] = [
  { id: "slot-1", profileId: "mimo-v26-pro", label: "MiMo Pro", visible: true, order: 0 },
  { id: "slot-2", profileId: "deepseek-v41-flash", label: "DeepSeek Flash", visible: true, order: 1 },
  { id: "slot-3", profileId: "ollama-mimo-9b", label: "Local MiMo 9B", visible: true, order: 2 },
  { id: "slot-4", profileId: "codex-cli", label: "Codex CLI", visible: true, order: 3 },
];

export const DEFAULT_ORCHESTRATOR_PROFILES: OrchestratorProfile[] = [
  {
    id: "mimo-v26-pro",
    displayName: "MiMo-V2.6-Pro (Direct API)",
    adapter: "provider",
    providerId: "mimo",
    model: "mimo-v2.6-pro",
    thinkingMode: "thinking",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 1000000,
  },
  {
    id: "mimo-v26-flash",
    displayName: "MiMo-V2.6-Flash (Direct API)",
    adapter: "provider",
    providerId: "mimo",
    model: "mimo-v2.6-flash",
    thinkingMode: "thinking",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 1000000,
  },
  {
    id: "deepseek-v41-flash",
    displayName: "DeepSeek V4.1 Flash (Direct API)",
    adapter: "provider",
    providerId: "deepseek",
    model: "deepseek-v4.1-flash",
    thinkingMode: "thinking",
    reasoningEffort: "high",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 128000,
  },
  {
    id: "ollama-mimo-9b",
    displayName: "Ollama MiMo-V2.6-9B (Local)",
    adapter: "ollama",
    ollamaModel: "mimo-v2.6:9b",
    ollamaEndpoint: "http://127.0.0.1:11434",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 32768,
  },
  {
    id: "ollama-qwen-coder",
    displayName: "Ollama Qwen2.5-Coder 14B (Local)",
    adapter: "ollama",
    ollamaModel: "qwen2.5-coder:14b",
    ollamaEndpoint: "http://127.0.0.1:11434",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 32768,
  },
  {
    id: "codex-cli",
    displayName: "Codex CLI (Local Agent)",
    adapter: "cli",
    executable: "codex",
    args: ["exec"],
    capabilities: [
      "reasoning",
      "review",
      "workspace_read",
      "workspace_write",
      "command_execution",
    ],
    contextWindowTokens: 200000,
  },
];

export const DEFAULT_VALIDATION_GATES: ValidationGateConfig[] = [
  {
    id: "typecheck",
    name: "TypeScript Check (tsc)",
    executable: "npx",
    args: ["tsc", "--noEmit"],
    enabled: true,
    failOnError: true,
  },
  {
    id: "test",
    name: "Test Suite",
    executable: "npm",
    args: ["test", "--", "--run"],
    enabled: true,
    failOnError: true,
  },
  {
    id: "git-status",
    name: "Git Status Check",
    executable: "git",
    args: ["status", "--short"],
    enabled: true,
    failOnError: false,
  },
];

export const DEFAULT_ITERATION_LIMITS: LoopIterationLimits = {
  maxPlanReviewIterations: 2,
  maxFixIterations: 3,
  maxCodeReviewIterations: 2,
};

export const BUILTIN_ORCHESTRATOR_PRESETS: OrchestratorPreset[] = [
  {
    id: "balanced",
    name: "Balanced Agentic Development",
    description: "MiMo Pro for planning, DeepSeek for critical review, Codex CLI for implementation, fixing, and diff review.",
    assignments: {
      planner: "mimo-v26-pro",
      plan_reviewer: "deepseek-v41-flash",
      implementer: "codex-cli",
      fixer: "codex-cli",
      code_reviewer: "codex-cli",
    },
    iterationLimits: DEFAULT_ITERATION_LIMITS,
    validationGates: DEFAULT_VALIDATION_GATES,
  },
  {
    id: "cheap-hybrid",
    name: "Cheap Hybrid (Local Reviewer)",
    description: "Cloud API for planning, local Ollama for plan and code reviews, Codex CLI for code modifications.",
    assignments: {
      planner: "mimo-v26-pro",
      plan_reviewer: "ollama-mimo-9b",
      implementer: "codex-cli",
      fixer: "codex-cli",
      code_reviewer: "ollama-mimo-9b",
    },
    iterationLimits: DEFAULT_ITERATION_LIMITS,
    validationGates: DEFAULT_VALIDATION_GATES,
  },
  {
    id: "gemini-light",
    name: "Gemini-Light",
    description: "Conserves Gemini capacity by utilizing API models for planning and DeepSeek for review.",
    assignments: {
      planner: "mimo-v26-pro",
      plan_reviewer: "deepseek-v41-flash",
      implementer: "codex-cli",
      fixer: "codex-cli",
      code_reviewer: "codex-cli",
    },
    iterationLimits: DEFAULT_ITERATION_LIMITS,
    validationGates: DEFAULT_VALIDATION_GATES,
  },
  {
    id: "high-quality",
    name: "High-Quality (Strict)",
    description: "MiMo Pro planning with full Codex CLI validation across review, implementation, and fixing.",
    assignments: {
      planner: "mimo-v26-pro",
      plan_reviewer: "codex-cli",
      implementer: "codex-cli",
      fixer: "codex-cli",
      code_reviewer: "codex-cli",
    },
    iterationLimits: DEFAULT_ITERATION_LIMITS,
    validationGates: DEFAULT_VALIDATION_GATES,
  },
];

export function getDefaultRoleAssignments(presetId = "balanced"): Record<AgentRole, RoleAssignment> {
  const preset =
    BUILTIN_ORCHESTRATOR_PRESETS.find((p) => p.id === presetId) ||
    BUILTIN_ORCHESTRATOR_PRESETS[0];

  return {
    planner: { role: "planner", profileId: preset.assignments.planner },
    plan_reviewer: { role: "plan_reviewer", profileId: preset.assignments.plan_reviewer },
    implementer: { role: "implementer", profileId: preset.assignments.implementer },
    fixer: { role: "fixer", profileId: preset.assignments.fixer, escalationRole: "implementer" },
    code_reviewer: { role: "code_reviewer", profileId: preset.assignments.code_reviewer },
  };
}
