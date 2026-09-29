import { describe, expect, it } from "vitest";
import type { OrchestratorProfile } from "../types/orchestrator";
import {
  canonicalizeProfileExecutionFields,
  canonicalExecutionFieldsForGroup,
  inferProfileGroupKey,
} from "./orchestratorProviderGroups";

describe("Orchestrator provider-group mapping", () => {
  it.each([
    ["deepseek", "provider", "deepseek"],
    ["minimax", "provider", "minimax"],
    ["kimi", "provider", "kimi"],
    ["kimi-code", "provider", "kimi-code"],
    ["mimo", "provider", "mimo"],
    ["openrouter", "provider", "openrouter"],
    ["ollama", "ollama", undefined],
    ["cli", "cli", undefined],
  ] as const)("group %s maps to fixed execution fields", (group, adapter, providerId) => {
    expect(canonicalExecutionFieldsForGroup(group)).toEqual({ adapter, providerId });
  });

  it("canonicalizes known legacy Profile execution metadata only on edit", () => {
    const legacy = {
      id: "legacy", displayName: "Legacy", adapter: "provider", providerId: "deepseek",
      model: "deepseek-flash", capabilities: [],
    } as OrchestratorProfile;
    expect(inferProfileGroupKey(legacy)).toBe("deepseek");
    expect(canonicalizeProfileExecutionFields(legacy)).toMatchObject({
      adapter: "provider", providerId: "deepseek",
    });
  });

  it("does not rewrite unknown provider profiles merely because they are unsupported", () => {
    const unknown = {
      id: "future", displayName: "Future", adapter: "provider", providerId: "future-provider",
      model: "future-model", capabilities: [],
    } as OrchestratorProfile;
    expect(inferProfileGroupKey(unknown)).toBe("other");
    expect(canonicalizeProfileExecutionFields(unknown)).toEqual(unknown);
  });
});
