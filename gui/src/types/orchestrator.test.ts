import { describe, it, expect } from "vitest";
import {
  validateRoleCapabilities,
  validateReviewOnlyProfile,
  validateWorkflowRoleCapabilities,
  ROLE_REQUIRED_CAPABILITIES,
  shouldAcceptRunEvent,
  type OrchestratorProfile,
} from "./orchestrator";
import {
  DEFAULT_ORCHESTRATOR_PROFILES,
  BUILTIN_ORCHESTRATOR_PRESETS,
  getDefaultRoleAssignments,
  hasGateConfigChanged,
  hasUnappliedSuggestedValidationGates,
  isSuggestedGateReflected,
  mergeSuggestedValidationGates,
} from "../config/orchestratorPresets";

describe("Orchestrator Role Capability Validation", () => {
  const pureApiModel: OrchestratorProfile = {
    id: "test-api-model",
    displayName: "Test API Model",
    adapter: "provider",
    capabilities: ["reasoning", "review", "workspace_read"],
  };

  const cliAgent: OrchestratorProfile = {
    id: "test-cli-agent",
    displayName: "Test CLI Agent",
    adapter: "cli",
    capabilities: [
      "reasoning",
      "review",
      "workspace_read",
      "workspace_write",
      "command_execution",
    ],
  };

  it("validates planner requires reasoning capability", () => {
    expect(ROLE_REQUIRED_CAPABILITIES.planner).toEqual(["reasoning"]);
    expect(validateRoleCapabilities("planner", pureApiModel)).toBeNull();
  });

  it("validates plan_reviewer and code_reviewer require review capability", () => {
    expect(ROLE_REQUIRED_CAPABILITIES.plan_reviewer).toEqual(["review"]);
    expect(ROLE_REQUIRED_CAPABILITIES.code_reviewer).toEqual(["review"]);
    expect(validateRoleCapabilities("plan_reviewer", pureApiModel)).toBeNull();
    expect(validateRoleCapabilities("code_reviewer", pureApiModel)).toBeNull();
  });

  it("rejects implementer when profile lacks workspace_write capability", () => {
    const error = validateRoleCapabilities("implementer", pureApiModel);
    expect(error).not.toBeNull();
    expect(error?.missingCapabilities).toContain("workspace_write");
    expect(error?.message).toContain('Role "implementer" cannot run with profile "Test API Model"');
  });

  it("rejects fixer when profile lacks workspace_write capability", () => {
    const error = validateRoleCapabilities("fixer", pureApiModel);
    expect(error).not.toBeNull();
    expect(error?.missingCapabilities).toContain("workspace_write");
  });

  it("accepts implementer and fixer when profile has workspace_write capability", () => {
    expect(validateRoleCapabilities("implementer", cliAgent)).toBeNull();
    expect(validateRoleCapabilities("fixer", cliAgent)).toBeNull();
  });

  it("returns error if profile is undefined", () => {
    const error = validateRoleCapabilities("planner", undefined);
    expect(error).not.toBeNull();
    expect(error?.message).toContain('Role "planner" has no assigned profile');
  });
});

describe("validateReviewOnlyProfile", () => {
  it("accepts provider profile with review capability even if it has workspace_write or command_execution", () => {
    const providerProfile: OrchestratorProfile = {
      id: "deepseek-direct",
      displayName: "DeepSeek Direct",
      adapter: "provider",
      capabilities: ["reasoning", "review", "workspace_read", "workspace_write", "command_execution"],
    };
    expect(validateReviewOnlyProfile(providerProfile)).toBeNull();
    expect(validateWorkflowRoleCapabilities("review_only", "code_reviewer", providerProfile)).toBeNull();
  });

  it("accepts ollama profile with review capability", () => {
    const ollamaProfile: OrchestratorProfile = {
      id: "ollama-qwen",
      displayName: "Ollama Qwen",
      adapter: "ollama",
      capabilities: ["review", "workspace_read"],
    };
    expect(validateReviewOnlyProfile(ollamaProfile)).toBeNull();
    expect(validateWorkflowRoleCapabilities("review_only", "code_reviewer", ollamaProfile)).toBeNull();
  });

  it("rejects profile missing review capability", () => {
    const noReviewProfile: OrchestratorProfile = {
      id: "no-review",
      displayName: "No Review Provider",
      adapter: "provider",
      capabilities: ["reasoning", "workspace_read"],
    };
    const error = validateReviewOnlyProfile(noReviewProfile);
    expect(error).not.toBeNull();
    expect(error?.missingCapabilities).toContain("review");
    expect(error?.message).toContain("missing capability [review]");
  });

  it("rejects profile with non-allowed adapter (e.g. cli)", () => {
    const cliProfile: OrchestratorProfile = {
      id: "cli-agent",
      displayName: "CLI Agent",
      adapter: "cli",
      capabilities: ["review", "workspace_read"],
    };
    const error = validateReviewOnlyProfile(cliProfile);
    expect(error).not.toBeNull();
    expect(error?.message).toContain("Review-only workflow requires a read-only adapter");
  });
});

describe("run-scoped event filtering", () => {
  it("accepts only events belonging to the active run, including logs", () => {
    expect(shouldAcceptRunEvent("run-current", "run-current")).toBe(true);
    expect(shouldAcceptRunEvent("run-old", "run-current")).toBe(false);
    expect(shouldAcceptRunEvent("run-old", null)).toBe(false);
  });

  it("rejects the previous run during a pending start and after a failed start", () => {
    const previousRunId = "run-A";
    const pending = { activeRunId: null as string | null, startPending: true };
    expect(shouldAcceptRunEvent(previousRunId, pending.activeRunId, pending.startPending)).toBe(false);

    const started = { activeRunId: "run-B", startPending: false };
    expect(shouldAcceptRunEvent(previousRunId, started.activeRunId, started.startPending)).toBe(false);
    expect(shouldAcceptRunEvent("run-B", started.activeRunId, started.startPending)).toBe(true);

    const failed = { activeRunId: null as string | null, startPending: false };
    expect(shouldAcceptRunEvent(previousRunId, failed.activeRunId, failed.startPending)).toBe(false);
  });
});

describe("Orchestrator Presets", () => {
  it("includes canonical provider profiles for MiniMax, Kimi, Kimi Code, and OpenRouter GPT-5.6 Sol", () => {
    const expected = [
      ["minimax-m3", "minimax", "MiniMax-M3", 1_000_000],
      ["kimi-k3", "kimi", "kimi-k3", 1_048_576],
      ["kimi-for-coding", "kimi-code", "kimi-for-coding", 200_000],
      ["openrouter-gpt-56-sol", "openrouter", "openai/gpt-5.6-sol", 1_050_000],
    ] as const;
    for (const [id, providerId, model, contextWindowTokens] of expected) {
      const profile = DEFAULT_ORCHESTRATOR_PROFILES.find((candidate) => candidate.id === id);
      expect(profile).toMatchObject({
        adapter: "provider",
        providerId,
        model,
        thinkingMode: "thinking",
        contextWindowTokens,
        capabilities: ["reasoning", "review", "workspace_read"],
      });
    }
    expect(DEFAULT_ORCHESTRATOR_PROFILES.find((profile) => profile.id === "openrouter-gpt-56-sol")?.reasoningEffort).toBe("high");
  });

  it("ensures all builtin presets have executable capability assignments", () => {
    for (const preset of BUILTIN_ORCHESTRATOR_PRESETS) {
      const assignments = getDefaultRoleAssignments(preset.id);
      for (const [role, assignment] of Object.entries(assignments) as [any, any][]) {
        const profile = DEFAULT_ORCHESTRATOR_PROFILES.find((p) => p.id === assignment.profileId);
        expect(profile, `Profile ${assignment.profileId} for role ${role} in preset ${preset.id} must exist`).toBeDefined();
        const error = validateRoleCapabilities(role, profile);
        expect(error, `Preset ${preset.id} role ${role} should have valid capability`).toBeNull();
      }
    }
  });
});

describe("mergeSuggestedValidationGates", () => {
  it("preserves user enabled toggles and retains custom gates while applying suggested gates", () => {
    const existingGates = [
      {
        id: "cargo-check",
        name: "Static Check (cargo check)",
        category: "static_check" as const,
        executable: "cargo",
        args: ["check"],
        enabled: false, // user disabled it
        failOnError: true,
      },
      {
        id: "custom-gate-1",
        name: "My Custom Gate",
        category: "custom" as const,
        executable: "bash",
        args: ["./verify.sh"],
        enabled: true,
        failOnError: true,
        isAdvancedCustom: true,
      },
    ];

    const suggestedGates = [
      {
        id: "cargo-check",
        name: "Static Check (cargo check)",
        category: "static_check" as const,
        executable: "cargo",
        args: ["check"],
        enabled: true, // suggested default
        failOnError: true,
      },
      {
        id: "cargo-test",
        name: "Tests (cargo test)",
        category: "tests" as const,
        executable: "cargo",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];

    const merged = mergeSuggestedValidationGates(existingGates, suggestedGates);

    // 1. cargo-check should preserve user's enabled: false
    const cargoCheck = merged.find((g) => g.id === "cargo-check");
    expect(cargoCheck).toBeDefined();
    expect(cargoCheck?.enabled).toBe(false);

    // 2. cargo-test should be newly added with enabled: true
    const cargoTest = merged.find((g) => g.id === "cargo-test");
    expect(cargoTest).toBeDefined();
    expect(cargoTest?.enabled).toBe(true);

    // 3. custom-gate-1 should be retained
    const customGate = merged.find((g) => g.id === "custom-gate-1");
    expect(customGate).toBeDefined();
    expect(customGate?.executable).toBe("bash");
    expect(customGate?.enabled).toBe(true);
  });

  it("handles empty suggested gates non-destructively by retaining both built-in and custom gates", () => {
    const existingGates = [
      {
        id: "cargo-check",
        name: "Static Check",
        category: "static_check" as const,
        executable: "cargo",
        args: ["check"],
        enabled: true,
        failOnError: true,
      },
      {
        id: "my-custom",
        name: "Custom Gate",
        category: "custom" as const,
        executable: "echo",
        args: ["1"],
        enabled: true,
        failOnError: true,
      },
    ];

    const merged = mergeSuggestedValidationGates(existingGates, []);
    expect(merged).toHaveLength(2);
    expect(merged.find((g) => g.id === "cargo-check")).toBeDefined();
    expect(merged.find((g) => g.id === "my-custom")).toBeDefined();
  });

  it("updates executable and args from suggestion on ID match without duplicating entries", () => {
    const existingGates = [
      {
        id: "typecheck",
        name: "Old Type Check",
        category: "static_check" as const,
        executable: "npx",
        args: ["tsc"],
        enabled: false,
        failOnError: true,
      },
    ];

    const suggestedGates = [
      {
        id: "typecheck",
        name: "Static Check (npm run typecheck)",
        category: "static_check" as const,
        executable: "npm",
        args: ["run", "typecheck"],
        enabled: true,
        failOnError: true,
      },
    ];

    const merged = mergeSuggestedValidationGates(existingGates, suggestedGates);
    expect(merged).toHaveLength(1);
    expect(merged[0].id).toBe("typecheck");
    expect(merged[0].executable).toBe("npm");
    expect(merged[0].args).toEqual(["run", "typecheck"]);
    expect(merged[0].enabled).toBe(false); // preserved
  });

  it("strictly preserves custom gates (command, args, isAdvancedCustom, workingDir) even when ID collides with a suggested gate", () => {
    const existingGates = [
      {
        id: "test",
        name: "Custom Test Runner",
        category: "custom" as const,
        executable: "./run-custom-tests.sh",
        args: ["--strict", "--coverage"],
        workingDir: "tests/custom",
        enabled: true,
        failOnError: true,
        isAdvancedCustom: true,
      },
    ];

    const suggestedGates = [
      {
        id: "test",
        name: "Tests (cargo test)",
        category: "tests" as const,
        executable: "cargo",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];

    const merged = mergeSuggestedValidationGates(existingGates, suggestedGates);
    expect(merged).toHaveLength(1);
    expect(merged[0].id).toBe("test");
    // Custom gate attributes must NOT be overwritten by suggested gate
    expect(merged[0].executable).toBe("./run-custom-tests.sh");
    expect(merged[0].args).toEqual(["--strict", "--coverage"]);
    expect(merged[0].workingDir).toBe("tests/custom");
    expect(merged[0].isAdvancedCustom).toBe(true);
    expect(merged[0].category).toBe("custom");
    expect(merged[0].name).toBe("Custom Test Runner");
  });

  it("preserves both the updated built-in gate and all custom gates when built-in and custom gates share the same ID", () => {
    const existingGates = [
      {
        id: "test",
        name: "Old Built-in Test",
        category: "tests" as const,
        executable: "cargo",
        args: ["check", "--tests"],
        enabled: false,
        failOnError: true,
      },
      {
        id: "test",
        name: "Custom Integration Test Runner",
        category: "custom" as const,
        executable: "./run-integration-tests.sh",
        args: ["--all"],
        workingDir: "tests",
        enabled: true,
        failOnError: true,
        isAdvancedCustom: true,
      },
    ];

    const suggestedGates = [
      {
        id: "test",
        name: "Tests (cargo test)",
        category: "tests" as const,
        executable: "cargo",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];

    const merged = mergeSuggestedValidationGates(existingGates, suggestedGates);
    expect(merged).toHaveLength(2);

    const builtin = merged.find((g) => !g.isAdvancedCustom && g.category === "tests");
    expect(builtin).toBeDefined();
    expect(builtin?.id).toBe("test");
    expect(builtin?.executable).toBe("cargo");
    expect(builtin?.args).toEqual(["test"]);
    expect(builtin?.enabled).toBe(false); // preserved user toggle

    const custom = merged.find((g) => g.isAdvancedCustom);
    expect(custom).toBeDefined();
    expect(custom?.id).toBe("test");
    expect(custom?.executable).toBe("./run-integration-tests.sh");
    expect(custom?.args).toEqual(["--all"]);
    expect(custom?.workingDir).toBe("tests");
  });

  it("preserves all custom gates when multiple custom gates have the same ID", () => {
    const existingGates = [
      {
        id: "test",
        name: "Custom Test 1",
        category: "custom" as const,
        executable: "./test1.sh",
        args: [],
        enabled: true,
        failOnError: true,
        isAdvancedCustom: true,
      },
      {
        id: "test",
        name: "Custom Test 2",
        category: "custom" as const,
        executable: "./test2.sh",
        args: [],
        enabled: false,
        failOnError: true,
        isAdvancedCustom: true,
      },
    ];

    const suggestedGates = [
      {
        id: "test",
        name: "Tests (cargo test)",
        category: "tests" as const,
        executable: "cargo",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];

    const merged = mergeSuggestedValidationGates(existingGates, suggestedGates);
    expect(merged).toHaveLength(2);
    expect(merged[0].name).toBe("Custom Test 1");
    expect(merged[0].executable).toBe("./test1.sh");
    expect(merged[1].name).toBe("Custom Test 2");
    expect(merged[1].executable).toBe("./test2.sh");
  });

  it("preserves existing built-in gates not in the suggested set (non-destructive for absent subprojects)", () => {
    const existingGates = [
      {
        id: "gui:typecheck",
        name: "Static Check (gui)",
        category: "static_check" as const,
        executable: "npm",
        args: ["run", "typecheck"],
        workingDir: "gui",
        enabled: true,
        failOnError: true,
      },
      {
        id: "mcp-server:cargo-check",
        name: "Static Check (mcp-server)",
        category: "static_check" as const,
        executable: "cargo",
        args: ["check"],
        workingDir: "mcp-server",
        enabled: false,
        failOnError: true,
      },
    ];

    // Suppose rescan only found gui, temporarily missing mcp-server
    const suggestedGates = [
      {
        id: "gui:typecheck",
        name: "Static Check (gui)",
        category: "static_check" as const,
        executable: "npm",
        args: ["run", "typecheck"],
        workingDir: "gui",
        enabled: true,
        failOnError: true,
      },
    ];

    const merged = mergeSuggestedValidationGates(existingGates, suggestedGates);
    expect(merged).toHaveLength(2);
    expect(merged.find((g) => g.id === "gui:typecheck")).toBeDefined();
    const mcpGate = merged.find((g) => g.id === "mcp-server:cargo-check");
    expect(mcpGate).toBeDefined();
    expect(mcpGate?.enabled).toBe(false); // preserved
  });
});

describe("hasGateConfigChanged", () => {
  it("returns false for identical gate arrays", () => {
    const gates = [
      {
        id: "t1",
        name: "Test",
        category: "tests" as const,
        executable: "npm",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];
    expect(hasGateConfigChanged(gates, [{ ...gates[0], args: ["test"] }])).toBe(false);
  });

  it("returns true when length or properties differ", () => {
    const gates = [
      {
        id: "t1",
        name: "Test",
        category: "tests" as const,
        executable: "npm",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];
    expect(hasGateConfigChanged(gates, [])).toBe(true);
    expect(hasGateConfigChanged(gates, [{ ...gates[0], enabled: false }])).toBe(true);
    expect(hasGateConfigChanged(gates, [{ ...gates[0], args: ["test", "--run"] }])).toBe(true);
  });
});

describe("isSuggestedGateReflected and hasUnappliedSuggestedValidationGates", () => {
  const baseSuggested = {
    id: "typecheck",
    name: "Static Check (npm run typecheck)",
    category: "static_check" as const,
    executable: "npm",
    args: ["run", "typecheck"],
    workingDir: "gui",
    enabled: true,
    failOnError: true,
  };

  it("reflects exactly matching built-in gate even if user disabled it or failOnError differs", () => {
    const existing = {
      ...baseSuggested,
      enabled: false,
      failOnError: false,
    };
    expect(isSuggestedGateReflected(existing, baseSuggested)).toBe(true);
    expect(hasUnappliedSuggestedValidationGates([existing], [baseSuggested])).toBe(false);
  });

  it("detects unapplied gate when gate ID is missing from existing gates", () => {
    expect(hasUnappliedSuggestedValidationGates([], [baseSuggested])).toBe(true);
  });

  it("detects unapplied gate when ID matches but executable differs", () => {
    const existing = {
      ...baseSuggested,
      executable: "npx",
    };
    expect(isSuggestedGateReflected(existing, baseSuggested)).toBe(false);
    expect(hasUnappliedSuggestedValidationGates([existing], [baseSuggested])).toBe(true);
  });

  it("detects unapplied gate when ID matches but args differ", () => {
    const existing = {
      ...baseSuggested,
      args: ["tsc", "--noEmit"],
    };
    expect(isSuggestedGateReflected(existing, baseSuggested)).toBe(false);
    expect(hasUnappliedSuggestedValidationGates([existing], [baseSuggested])).toBe(true);
  });

  it("detects unapplied gate when ID matches but workingDir differs", () => {
    const existing = {
      ...baseSuggested,
      workingDir: "other-dir",
    };
    expect(isSuggestedGateReflected(existing, baseSuggested)).toBe(false);
    expect(hasUnappliedSuggestedValidationGates([existing], [baseSuggested])).toBe(true);
  });

  it("detects unapplied gate when existing gate is a custom gate colliding with suggested ID", () => {
    const customGate = {
      id: "typecheck",
      name: "My Custom Checker",
      category: "custom" as const,
      executable: "npm",
      args: ["run", "typecheck"],
      workingDir: "gui",
      enabled: true,
      failOnError: true,
      isAdvancedCustom: true,
    };
    // Because custom gates are protected from merge overwrites, the suggested gate cannot be considered reflected
    expect(isSuggestedGateReflected(customGate, baseSuggested)).toBe(false);
    expect(hasUnappliedSuggestedValidationGates([customGate], [baseSuggested])).toBe(true);
  });

  it("detects unapplied gate when both built-in and custom gates coexist with the same ID", () => {
    const existingBuiltin = {
      ...baseSuggested,
    };
    const existingCustom = {
      id: "typecheck",
      name: "Custom Checker",
      category: "custom" as const,
      executable: "./custom.sh",
      args: [],
      enabled: true,
      failOnError: true,
      isAdvancedCustom: true,
    };
    expect(hasUnappliedSuggestedValidationGates([existingBuiltin, existingCustom], [baseSuggested])).toBe(true);
  });

  it("detects unapplied gate when duplicate built-in gates exist with the same ID", () => {
    const duplicate1 = { ...baseSuggested };
    const duplicate2 = { ...baseSuggested };
    expect(hasUnappliedSuggestedValidationGates([duplicate1, duplicate2], [baseSuggested])).toBe(true);
  });

  it("returns false for empty suggested gates", () => {
    expect(hasUnappliedSuggestedValidationGates([baseSuggested], [])).toBe(false);
  });
});
