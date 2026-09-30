import { describe, expect, it } from "vitest";
import { MODEL_CATALOG, MODEL_CAPABILITIES, getProviderModels } from "../modelCapabilities";
import { getModelDisplayName } from "./modelDisplayNames";
import { getOpenRouterModelDisplayName } from "./builtinOpenRouter";
import { getOrchestratorProfileDisplayName } from "./orchestratorProfileDisplayName";

describe("shared model catalog", () => {
  it("is the sole source for MCP and Orchestrator provider model choices", () => {
    expect(MODEL_CATALOG.providers.map(({ id }) => id)).toEqual([
      "deepseek", "minimax", "kimi", "kimi-code", "mimo", "openrouter",
    ]);
    for (const provider of MODEL_CATALOG.providers) {
        expect(getProviderModels(provider.id)).toEqual(provider.modelIds);
        for (const modelId of provider.modelIds) {
          expect(MODEL_CATALOG.models[modelId]?.providerId).toBe(provider.id);
          expect(MODEL_CAPABILITIES[modelId]).toBeDefined();
        }
    }
  });

  it("provides canonical model labels for direct and OpenRouter display", () => {
    expect(getModelDisplayName("deepseek-v4.1-flash", "deepseek")).toBe("DeepSeek V4.1 Flash");
    expect(getModelDisplayName("MiniMax-M3", "minimax")).toBe("MiniMax M3");
    expect(getModelDisplayName("kimi-k3", "kimi")).toBe("Kimi K3");
    expect(getOpenRouterModelDisplayName("openai/gpt-5.6-sol")).toBe("GPT-5.6 Sol");
    expect(MODEL_CATALOG.models["openai/gpt-5.6-sol"].providerId).toBe("openrouter");
  });

  it("keeps catalog data limited to model facts and provider model lists", () => {
    expect(Object.keys(MODEL_CATALOG)).toEqual([
      "schemaVersion", "providers", "models", "contextWindows",
    ]);
    expect(Object.keys(MODEL_CATALOG.providers[0])).toEqual(["id", "label", "modelIds"]);
    expect(MODEL_CATALOG.models["deepseek-flash"]).not.toHaveProperty("selectable");
    expect(MODEL_CATALOG.models["deepseek-flash"]).not.toHaveProperty("proxyCapabilities");
    expect(MODEL_CATALOG.models["deepseek-flash"]).not.toHaveProperty("quickSlots");
  });

  it("uses canonical configuration name for Profile display names", () => {
    expect(getOrchestratorProfileDisplayName({
      id: "kimi-profile", displayName: "Old name", adapter: "provider", providerId: "kimi",
      model: "kimi-k3", capabilities: [],
    })).toBe("kimi-k3");
  });

  it("keeps non-cloud runtime adapters outside the static catalog", () => {
    expect(MODEL_CATALOG.providers.some(({ id }) => id === "ollama" || id === "cli")).toBe(false);
  });
});
