import { describe, it, expect } from "vitest";
import {
  DEFAULT_ORCHESTRATOR_PROFILES,
  resolvePresetRoleAssignments,
} from "./orchestratorPresets";
import type { OrchestratorProfile } from "../types/orchestrator";

describe("resolvePresetRoleAssignments", () => {
  it("resolves canonical antigravity-harness for all three write roles in human-gated preset when present", () => {
    const res = resolvePresetRoleAssignments(
      "human-gated-development-loop",
      DEFAULT_ORCHESTRATOR_PROFILES,
      "human_gated_loop"
    );

    expect(res.success).toBe(true);
    expect(res.assignments).toBeDefined();
    expect(res.assignments?.planner.profileId).toBe("deepseek-v41-flash");
    expect(res.assignments?.plan_integrator.profileId).toBe("antigravity-harness");
    expect(res.assignments?.plan_reviewer.profileId).toBe("deepseek-v41-flash");
    expect(res.assignments?.implementer.profileId).toBe("antigravity-harness");
    expect(res.assignments?.fixer.profileId).toBe("antigravity-harness");
    expect(res.assignments?.code_reviewer.profileId).toBe("codex-cli");
    expect(res.iterationLimits).toEqual({
      maxPlanReviewIterations: 3,
      maxFixIterations: 5,
      maxCodeReviewIterations: 3,
    });
  });

  it("prefers canonical antigravity-harness when both canonical and custom Antigravity profiles exist", () => {
    const customAntigravity: OrchestratorProfile = {
      id: "custom-antigravity-123",
      displayName: "Custom Antigravity",
      adapter: "antigravity",
      capabilities: ["workspace_read", "workspace_write", "command_execution", "reasoning"],
    };
    const profiles = [...DEFAULT_ORCHESTRATOR_PROFILES, customAntigravity];

    const res = resolvePresetRoleAssignments(
      "human-gated-development-loop",
      profiles,
      "human_gated_loop"
    );

    expect(res.success).toBe(true);
    expect(res.assignments?.plan_integrator.profileId).toBe("antigravity-harness");
    expect(res.assignments?.implementer.profileId).toBe("antigravity-harness");
    expect(res.assignments?.fixer.profileId).toBe("antigravity-harness");
  });

  it("resolves to compatible custom Antigravity profile when canonical antigravity-harness is absent", () => {
    const customAntigravity: OrchestratorProfile = {
      id: "custom-antigravity-456",
      displayName: "Custom Antigravity",
      adapter: "antigravity",
      capabilities: ["workspace_read", "workspace_write", "command_execution", "reasoning"],
    };
    // Exclude canonical antigravity-harness
    const profiles = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.id !== "antigravity-harness").concat(customAntigravity);

    const res = resolvePresetRoleAssignments(
      "human-gated-development-loop",
      profiles,
      "human_gated_loop"
    );

    expect(res.success).toBe(true);
    expect(res.assignments?.plan_integrator.profileId).toBe("custom-antigravity-456");
    expect(res.assignments?.implementer.profileId).toBe("custom-antigravity-456");
    expect(res.assignments?.fixer.profileId).toBe("custom-antigravity-456");
    expect(res.assignments?.code_reviewer.profileId).toBe("codex-cli");
  });

  it("does not select incompatible custom Antigravity profile as fallback when lacking required write capability", () => {
    const incompatibleAntigravity: OrchestratorProfile = {
      id: "custom-antigravity-readonly",
      displayName: "Readonly Antigravity",
      adapter: "antigravity",
      capabilities: ["workspace_read", "reasoning"], // missing workspace_write!
    };
    const profiles = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.id !== "antigravity-harness").concat(incompatibleAntigravity);

    const res = resolvePresetRoleAssignments(
      "human-gated-development-loop",
      profiles,
      "human_gated_loop"
    );

    expect(res.success).toBe(false);
    expect(res.assignments).toBeUndefined();
    expect(res.errorCode).toBe("missing_antigravity_profile");
  });

  it("fails atomically when no Antigravity profile exists", () => {
    const profiles = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.adapter !== "antigravity");

    const res = resolvePresetRoleAssignments(
      "human-gated-development-loop",
      profiles,
      "human_gated_loop"
    );

    expect(res.success).toBe(false);
    expect(res.assignments).toBeUndefined();
    expect(res.errorCode).toBe("missing_antigravity_profile");
  });

  it("fails atomically when a non-Antigravity profile required by the preset is missing", () => {
    // Missing codex-cli
    const profiles = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.id !== "codex-cli");

    const res = resolvePresetRoleAssignments(
      "human-gated-development-loop",
      profiles,
      "human_gated_loop"
    );

    expect(res.success).toBe(false);
    expect(res.assignments).toBeUndefined();
    expect(res.errorCode).toBe("profile_not_found");
    expect(res.errorRole).toBe("code_reviewer");
    expect(res.errorProfileId).toBe("codex-cli");
  });

  it("fails atomically when a non-Antigravity profile lacks required capabilities", () => {
    // Codex CLI stripped of review capability
    const strippedCodex: OrchestratorProfile = {
      id: "codex-cli",
      displayName: "Stripped Codex",
      adapter: "cli",
      capabilities: ["workspace_write", "command_execution"], // missing review!
    };
    const profiles = DEFAULT_ORCHESTRATOR_PROFILES.map((p) => (p.id === "codex-cli" ? strippedCodex : p));

    const res = resolvePresetRoleAssignments(
      "human-gated-development-loop",
      profiles,
      "human_gated_loop"
    );

    expect(res.success).toBe(false);
    expect(res.assignments).toBeUndefined();
    expect(res.errorCode).toBe("incompatible_profile");
    expect(res.errorRole).toBe("code_reviewer");
  });

  it("rejects a standard preset when an inactive plan_integrator assignment references a missing Antigravity profile", () => {
    const profiles = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.id !== "antigravity-harness");
    const res = resolvePresetRoleAssignments("balanced", profiles, "full_loop");

    expect(res.success).toBe(false);
    expect(res.assignments).toBeUndefined();
    expect(res.errorCode).toBe("profile_not_found");
    expect(res.errorRole).toBe("plan_integrator");
    expect(res.errorProfileId).toBe("antigravity-harness");
  });

  it("resolves a standard preset when its inactive Antigravity profile exists", () => {
    const res = resolvePresetRoleAssignments("balanced", DEFAULT_ORCHESTRATOR_PROFILES, "full_loop");

    expect(res.success).toBe(true);
    for (const assignment of Object.values(res.assignments ?? {})) {
      expect(DEFAULT_ORCHESTRATOR_PROFILES.some((profile) => profile.id === assignment.profileId)).toBe(true);
    }
  });

  it("rejects a standard preset when another inactive assignment references a missing profile", () => {
    const profiles = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.id !== "deepseek-v41-flash");
    const res = resolvePresetRoleAssignments("balanced", profiles, "implement_only");

    expect(res.success).toBe(false);
    expect(res.assignments).toBeUndefined();
    expect(res.errorCode).toBe("profile_not_found");
    expect(res.errorRole).toBe("plan_reviewer");
    expect(res.errorProfileId).toBe("deepseek-v41-flash");
  });

  it("guarantees every resolved assignment references an existing profile", () => {
    const profiles = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.id !== "antigravity-harness").concat({
      id: "custom-antigravity-all-roles",
      displayName: "Custom Antigravity",
      adapter: "antigravity" as const,
      capabilities: ["workspace_read", "workspace_write", "command_execution", "reasoning"],
    });
    const res = resolvePresetRoleAssignments("human-gated-development-loop", profiles, "human_gated_loop");

    expect(res.success).toBe(true);
    for (const assignment of Object.values(res.assignments ?? {})) {
      expect(profiles.some((profile) => profile.id === assignment.profileId)).toBe(true);
    }
  });
});
