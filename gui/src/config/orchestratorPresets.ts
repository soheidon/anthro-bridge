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
    id: "minimax-m3",
    displayName: "MiniMax M3 (Direct API)",
    adapter: "provider",
    providerId: "minimax",
    model: "MiniMax-M3",
    thinkingMode: "thinking",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 1000000,
  },
  {
    id: "kimi-k3",
    displayName: "Kimi K3 (Direct API)",
    adapter: "provider",
    providerId: "kimi",
    model: "kimi-k3",
    thinkingMode: "thinking",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 1048576,
  },
  {
    id: "kimi-for-coding",
    displayName: "Kimi for Coding (Direct API)",
    adapter: "provider",
    providerId: "kimi-code",
    model: "kimi-for-coding",
    thinkingMode: "thinking",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 200000,
  },
  {
    id: "openrouter-gpt-56-sol",
    displayName: "OpenRouter GPT-5.6 Sol",
    adapter: "provider",
    providerId: "openrouter",
    model: "openai/gpt-5.6-sol",
    thinkingMode: "thinking",
    reasoningEffort: "high",
    capabilities: ["reasoning", "review", "workspace_read"],
    contextWindowTokens: 1050000,
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
  {
    id: "antigravity-harness",
    displayName: "Google Antigravity Harness (MCP Mailbox)",
    adapter: "antigravity",
    capabilities: [
      "workspace_read",
      "workspace_write",
      "command_execution",
      "reasoning",
    ],
  },
];

export const DEFAULT_VALIDATION_GATES: ValidationGateConfig[] = [
  {
    id: "typecheck",
    name: "TypeScript Check (tsc)",
    executable: "npx",
    args: ["tsc", "--noEmit"],
    enabled: true,
    category: "static_check",
    failOnError: true,
  },
  {
    id: "test",
    name: "Test Suite",
    executable: "npm",
    args: ["test", "--", "--run"],
    enabled: true,
    category: "tests",
    failOnError: true,
  },
  {
    id: "git-status",
    name: "Git Status Check",
    executable: "git",
    args: ["status", "--short"],
    enabled: true,
    category: "repository_check",
    successCriteria: "empty_output",
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
    id: "human-gated-development-loop",
    name: "Human-Gated Development Loop (DeepSeek + Antigravity + Codex)",
    description: "Complete autonomous development loop with DeepSeek planning, Antigravity implementation & fixing, disposable worktree Codex review, and interactive Human Gate.",
    assignments: {
      planner: "deepseek-v41-flash",
      plan_integrator: "antigravity-harness",
      plan_reviewer: "deepseek-v41-flash",
      implementer: "antigravity-harness",
      fixer: "antigravity-harness",
      code_reviewer: "codex-cli",
    },
    iterationLimits: {
      maxPlanReviewIterations: 3,
      maxFixIterations: 5,
      maxCodeReviewIterations: 3,
    },
    validationGates: DEFAULT_VALIDATION_GATES,
  },
  {
    id: "balanced",
    name: "Balanced Agentic Development",
    description: "MiMo Pro for planning, DeepSeek for critical review, Codex CLI for implementation, fixing, and diff review.",
    assignments: {
      planner: "mimo-v26-pro",
      plan_integrator: "antigravity-harness",
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
      plan_integrator: "antigravity-harness",
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
      plan_integrator: "antigravity-harness",
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
      plan_integrator: "antigravity-harness",
      plan_reviewer: "codex-cli",
      implementer: "codex-cli",
      fixer: "codex-cli",
      code_reviewer: "codex-cli",
    },
    iterationLimits: DEFAULT_ITERATION_LIMITS,
    validationGates: DEFAULT_VALIDATION_GATES,
  },
];

export function getDefaultPresetIdForWorkflow(workflowId: string): string {
  if (workflowId === "human_gated_loop") {
    return "human-gated-development-loop";
  }
  return "balanced";
}

export function getDefaultRoleAssignments(presetId = "balanced"): Record<AgentRole, RoleAssignment> {
  const preset =
    BUILTIN_ORCHESTRATOR_PRESETS.find((p) => p.id === presetId) ||
    BUILTIN_ORCHESTRATOR_PRESETS[0];

  return {
    planner: { role: "planner", profileId: preset.assignments.planner },
    plan_integrator: { role: "plan_integrator", profileId: preset.assignments.plan_integrator || "antigravity-harness" },
    plan_reviewer: { role: "plan_reviewer", profileId: preset.assignments.plan_reviewer },
    implementer: { role: "implementer", profileId: preset.assignments.implementer },
    fixer: { role: "fixer", profileId: preset.assignments.fixer, escalationRole: "implementer" },
    code_reviewer: { role: "code_reviewer", profileId: preset.assignments.code_reviewer },
  };
}

export function isCustomGate(gate: ValidationGateConfig): boolean {
  return gate.isAdvancedCustom === true || gate.category === "custom";
}

export function isSuggestedGateReflected(
  existingGate: ValidationGateConfig | undefined,
  suggestedGate: ValidationGateConfig
): boolean {
  if (!existingGate) return false;
  if (isCustomGate(existingGate)) return false;
  if (existingGate.id !== suggestedGate.id) return false;
  if (existingGate.executable !== suggestedGate.executable) return false;
  if (
    existingGate.args.length !== suggestedGate.args.length ||
    !existingGate.args.every((arg, idx) => arg === suggestedGate.args[idx])
  ) {
    return false;
  }
  if ((existingGate.workingDir ?? undefined) !== (suggestedGate.workingDir ?? undefined)) {
    return false;
  }
  if ((existingGate.successCriteria ?? undefined) !== (suggestedGate.successCriteria ?? undefined)) {
    return false;
  }
  return true;
}

export function hasUnappliedSuggestedValidationGates(
  existingGates: ValidationGateConfig[],
  suggestedGates: ValidationGateConfig[]
): boolean {
  if (!suggestedGates.length) return false;

  return suggestedGates.some((sg) => {
    // If there is any custom gate with this ID, the suggested gate is not cleanly reflected
    const hasCollidingCustom = existingGates.some((g) => isCustomGate(g) && g.id === sg.id);
    if (hasCollidingCustom) return true;

    // Must find exactly one matching built-in gate
    const matchingBuiltins = existingGates.filter((g) => !isCustomGate(g) && g.id === sg.id);
    if (matchingBuiltins.length !== 1) return true;

    return !isSuggestedGateReflected(matchingBuiltins[0], sg);
  });
}

export function hasGateConfigChanged(
  oldGates: ValidationGateConfig[],
  newGates: ValidationGateConfig[]
): boolean {
  if (oldGates.length !== newGates.length) return true;
  for (let i = 0; i < oldGates.length; i++) {
    const a = oldGates[i];
    const b = newGates[i];
    if (
      a.id !== b.id ||
      a.name !== b.name ||
      a.executable !== b.executable ||
      a.enabled !== b.enabled ||
      (a.workingDir ?? undefined) !== (b.workingDir ?? undefined) ||
      (a.failOnError ?? true) !== (b.failOnError ?? true) ||
      (a.isAdvancedCustom ?? false) !== (b.isAdvancedCustom ?? false) ||
      a.category !== b.category ||
      (a.successCriteria ?? undefined) !== (b.successCriteria ?? undefined) ||
      a.args.length !== b.args.length ||
      !a.args.every((arg, idx) => arg === b.args[idx])
    ) {
      return true;
    }
  }
  return false;
}

export function mergeSuggestedValidationGates(
  existingGates: ValidationGateConfig[],
  suggestedGates: ValidationGateConfig[]
): ValidationGateConfig[] {
  // 1. Collect all custom gates from existingGates in order to guarantee none are lost
  const existingCustomGates = existingGates.filter(isCustomGate);

  // 2. Map suggested gates:
  // For each suggested gate:
  // - If there is a matching existing built-in gate, update its definition while preserving user toggles
  // - If there is no matching built-in gate and no custom gate with that ID, add the suggested gate
  // - If there is a colliding custom gate and no matching built-in gate, do not add a duplicate built-in gate
  const updatedSuggestedGates: ValidationGateConfig[] = [];
  const processedBuiltinIds = new Set<string>();

  for (const sg of suggestedGates) {
    const matchingBuiltin = existingGates.find((g) => !isCustomGate(g) && g.id === sg.id);
    const hasCollidingCustom = existingCustomGates.some((g) => g.id === sg.id);

    if (matchingBuiltin) {
      processedBuiltinIds.add(sg.id);
      updatedSuggestedGates.push({
        ...sg,
        enabled: matchingBuiltin.enabled,
        workingDir: matchingBuiltin.workingDir ?? sg.workingDir,
        failOnError: matchingBuiltin.failOnError ?? sg.failOnError,
      });
    } else if (!hasCollidingCustom) {
      processedBuiltinIds.add(sg.id);
      updatedSuggestedGates.push(sg);
    }
  }

  // 3. Preserve existing built-in gates not in the suggested set (non-destructive for absent subprojects)
  for (const g of existingGates) {
    if (!isCustomGate(g) && !processedBuiltinIds.has(g.id)) {
      processedBuiltinIds.add(g.id);
      updatedSuggestedGates.push(g);
    }
  }

  // 4. Preserve ALL existing custom gates (including those with colliding IDs or duplicate IDs)
  return [...updatedSuggestedGates, ...existingCustomGates];
}
