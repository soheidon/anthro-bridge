import { describe, expect, it } from "vitest";
import type { OrchestratorProfile } from "../types/orchestrator";
import { getOrchestratorProfileDisplayName } from "./orchestratorProfileDisplayName";

describe("getOrchestratorProfileDisplayName", () => {
  it.each([
    [
      { id: "deepseek", adapter: "provider", providerId: "deepseek", model: "deepseek-v4.1-flash" },
      "DeepSeek V4.1 Flash (Direct API)",
    ],
    [
      { id: "mimo", adapter: "provider", providerId: "mimo", model: "mimo-v2.6-pro" },
      "MiMo-V2.6-Pro (Direct API)",
    ],
    [
      { id: "openrouter", adapter: "provider", providerId: "openrouter", model: "openai/gpt-5.6-sol" },
      "OpenRouter GPT-5.6 Sol",
    ],
    [
      { id: "ollama", adapter: "ollama", ollamaModel: "mimo-v2.6:9b" },
      "Ollama MiMo-V2.6-9B (Local)",
    ],
    [{ id: "cli", adapter: "cli" }, "Codex CLI (Local Agent)"],
  ] satisfies Array<[Partial<OrchestratorProfile>, string]>) (
      "derives %s from its adapter/provider/model settings",
    (profile, expected) => {
      expect(getOrchestratorProfileDisplayName({
        displayName: "Stale legacy label",
        capabilities: [],
        ...profile,
        id: profile.id ?? "test",
      } as OrchestratorProfile)).toBe(expected);
    },
  );
});
