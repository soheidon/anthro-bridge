import { describe, it, expect, vi } from "vitest";

// Vite `?raw` import: resolved relative to this module by the bundler, so the
// path stays correct regardless of CWD (and needs no Node builtins/types).
import contextWindowsRaw from "../../src-tauri/resources/model_context_windows.json?raw";
import templateConfigRaw from "../../src-tauri/resources/config.json?raw";

// test-setup.ts globally mocks ./config/builtinOpenRouter with a reduced
// registry. This test asserts coverage against the REAL built-in model list,
// so the mock must be removed before the module is imported.
vi.unmock("./builtinOpenRouter");

import { PROVIDER_MODELS } from "../modelCapabilities";
import { BUILTIN_OPENROUTER_MODELS } from "./builtinOpenRouter";

interface ContextWindowEntry {
  context_length: number;
  source: string;
  verified_at?: string;
}

const contextWindows = JSON.parse(contextWindowsRaw) as {
  schema_version: number;
  models: Record<string, ContextWindowEntry>;
};
const models = contextWindows.models;

// Same lookup precedence as lookup_static_context_window in
// gui/src-tauri/src/model_capabilities.rs: `provider:model` key wins, the bare
// `model` key is the generic fallback.
function resolveWindow(providerId: string, model: string): number | undefined {
  return (
    models[`${providerId}:${model}`]?.context_length ??
    models[model]?.context_length
  );
}

describe("model_context_windows.json coverage", () => {
  it("every direct provider model has context metadata", () => {
    const directProviders = ["deepseek", "minimax", "kimi", "mimo"];
    for (const providerId of directProviders) {
      const ids = PROVIDER_MODELS[providerId] ?? [];
      expect(ids.length, `${providerId} must list at least one model`).toBeGreaterThan(0);
      for (const model of ids) {
        expect(
          resolveWindow(providerId, model),
          `${providerId}:${model} must have context metadata`,
        ).toBeDefined();
      }
    }
  });

  it("covers every upstream model referenced by direct providers in the template config", () => {
    // Some direct-provider models (e.g. MiniMax-M2.7) exist only in the config
    // (model_map / models[*].upstream_model) and never in PROVIDER_MODELS, so
    // the dropdown-driven test above cannot detect their removal. Scan the real
    // template config to cover them too.
    const templateConfig = JSON.parse(templateConfigRaw) as {
      providers: Record<string, {
        models?: Record<string, { upstream_model?: string }>;
        model_map?: Record<string, string>;
      }>;
    };
    const directProviders = ["deepseek", "minimax", "kimi", "mimo"];
    const referenced: Array<[string, string]> = [];
    for (const providerId of directProviders) {
      const provider = templateConfig.providers?.[providerId];
      expect(provider, `${providerId} must exist in template config`).toBeDefined();
      for (const route of Object.values(provider.models ?? {})) {
        if (route.upstream_model) {
          referenced.push([providerId, route.upstream_model]);
        }
      }
      for (const upstream of Object.values(provider.model_map ?? {})) {
        referenced.push([providerId, upstream]);
      }
    }
    expect(referenced.length, "template config must reference upstream models").toBeGreaterThan(0);
    for (const [providerId, model] of referenced) {
      expect(
        resolveWindow(providerId, model),
        `${providerId}:${model} (referenced by template config) must have context metadata`,
      ).toBeDefined();
    }
  });

  it("every builtin openrouter model has context metadata", () => {
    for (const model of Object.keys(BUILTIN_OPENROUTER_MODELS)) {
      expect(
        resolveWindow("openrouter", model),
        `${model} must have context metadata`,
      ).toBeDefined();
    }
  });

  it("free variants match their base model", () => {
    for (const model of Object.keys(BUILTIN_OPENROUTER_MODELS)) {
      if (!model.endsWith(":free")) continue;
      const base = model.slice(0, -":free".length);
      if (!(base in BUILTIN_OPENROUTER_MODELS)) continue; // e.g. ling-3.0-flash:free
      expect(resolveWindow("openrouter", model), `${model} must be registered`).toBeDefined();
      expect(resolveWindow("openrouter", model)).toBe(
        resolveWindow("openrouter", base),
      );
    }
  });

  it("gpt56 pro variants match their base model", () => {
    for (const model of Object.keys(BUILTIN_OPENROUTER_MODELS)) {
      if (!model.endsWith("-pro")) continue;
      const base = model.slice(0, -"-pro".length);
      if (!(base in BUILTIN_OPENROUTER_MODELS)) continue;
      expect(resolveWindow("openrouter", model), `${model} must be registered`).toBeDefined();
      expect(resolveWindow("openrouter", model)).toBe(
        resolveWindow("openrouter", base),
      );
    }
  });

  it("laguna all four ids are 262144", () => {
    for (const model of [
      "poolside/laguna-s-2.1",
      "poolside/laguna-s-2.1:free",
      "poolside/laguna-xs-2.1",
      "poolside/laguna-xs-2.1:free",
    ]) {
      expect(resolveWindow("openrouter", model)).toBe(262_144);
    }
  });
});
