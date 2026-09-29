import { describe, it, expect } from "vitest";
import {
  validateRoleCapabilities,
  ROLE_REQUIRED_CAPABILITIES,
  shouldAcceptRunEvent,
  type OrchestratorProfile,
} from "./orchestrator";
import {
  DEFAULT_ORCHESTRATOR_PROFILES,
  BUILTIN_ORCHESTRATOR_PRESETS,
  getDefaultRoleAssignments,
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
