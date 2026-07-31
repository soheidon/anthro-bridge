import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// ── @tauri-apps/api mock ──────────────────────────────────────────
// All Tauri commands go through `invoke`.  Tests override via
// vi.mocked(invoke).mockImplementation(...) per-suite.

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

// ── i18n mock ─────────────────────────────────────────────────────

vi.mock("./i18n", () => {
  const en: Record<string, unknown> = {
    openRouterModels: {
      title: "OpenRouter Models",
      groupSentinelOther: "Other...",
      groupSentinelBack: "← Back to Groups",
      groupCustom: "Custom",
      saveOkRefreshFailed: "Refresh failed: {{error}}",
      saveOkRestartFailed: "Restart failed: {{error}}",
      groupPoolside: "Poolside",
      groupTencent: "Tencent",
      groupInclusionAI: "InclusionAI",
      groupStepFun: "StepFun",
      groupOtherModels: "Other",
      groupRouter: "OpenRouter",
      selectModel: "Select a model",
      selectVendor: "Select vendor",
      thinkingMode: "Thinking",
      customModelShort: "Custom",
      customModelPlaceholder: "Enter custom model ID...",
      confirm: "Confirm",
      loading: "Loading...",
    },
    apiKeyPanel: {
      normalMode: "Normal",
      thinkingMode: "Mode",
    },
    modelPricing: {
      notes: { openRouterPricing: "OpenRouter pricing" },
    },
  };

  return {
    useTranslation: () => ({
      t: (key: string, vars?: Record<string, string>) => {
        let obj: unknown = en;
        for (const part of key.split(".")) {
          obj = (obj as Record<string, unknown>)?.[part];
        }
        let result = (typeof obj === "string" ? obj : key) as string;
        if (vars) {
          for (const [k, v] of Object.entries(vars)) {
            result = result.replace(`{{${k}}}`, v);
          }
        }
        return result;
      },
      lang: "en",
    }),
    LanguageProvider: ({ children }: { children: React.ReactNode }) => children,
    LanguageContext: { Provider: ({ children }: { children: React.ReactNode }) => children },
    AVAILABLE_LANGS: ["en"],
  };
});

// ── builtinOpenRouter mock ────────────────────────────────────────

vi.mock("./config/builtinOpenRouter", () => {
  // Minimal registry — just enough for tests to find models.
  const BUILTIN_OPENROUTER_MODELS: Record<string, unknown> = {
    "poolside/laguna-s-2.1": {
      displayName: "Laguna S 2.1",
      vendor: "poolside",
      contextLength: 131_072,
      pricingUpdatedAt: "2026-07-25",
      capabilities: {
        supports_vision: false,
        supports_video: false,
        force_thinking: false,
        thinking: "thinking_mode",
        thinkingModePolicy: "forced",
        supportsReasoningEffort: true,
        forcedThinkingOptions: ["max", "off"],
      },
    },
    "poolside/laguna-xs-2.1": {
      displayName: "Laguna XS 2.1",
      vendor: "poolside",
      contextLength: 131_072,
      pricingUpdatedAt: "2026-07-25",
      capabilities: {
        supports_vision: false,
        supports_video: false,
        force_thinking: false,
        thinking: "thinking_mode",
        thinkingModePolicy: "forced",
        supportsReasoningEffort: false,
        forcedThinkingOptions: ["on", "off"],
      },
    },
    "poolside/laguna-s-2.1:free": {
      displayName: "Laguna S 2.1 (Free)",
      vendor: "poolside",
      contextLength: 131_072,
      pricingUpdatedAt: "2026-07-25",
      capabilities: {
        supports_vision: false, supports_video: false,
        force_thinking: false, thinking: "thinking_mode",
        thinkingModePolicy: "forced", supportsReasoningEffort: true,
        forcedThinkingOptions: ["max", "off"],
      },
    },
    "poolside/laguna-xs-2.1:free": {
      displayName: "Laguna XS 2.1 (Free)",
      vendor: "poolside",
      contextLength: 131_072,
      pricingUpdatedAt: "2026-07-25",
      capabilities: {
        supports_vision: false, supports_video: false,
        force_thinking: false, thinking: "thinking_mode",
        thinkingModePolicy: "forced", supportsReasoningEffort: false,
        forcedThinkingOptions: ["on", "off"],
      },
    },
    "tencent/hy3": {
      displayName: "Hy3",
      vendor: "tencent",
      contextLength: 131_072,
      pricingUpdatedAt: "2026-07-01",
      capabilities: {
        supportsReasoningEffort: true,
        thinking: "thinking_mode",
        thinkingModePolicy: "forced",
        forcedThinkingOptions: ["low", "high", "off"],
      },
    },
    "tencent/hy3:free": {
      displayName: "Hy3 (Free)",
      vendor: "tencent",
      contextLength: 131_072,
      pricingUpdatedAt: "2026-07-01",
      capabilities: {
        supportsReasoningEffort: true,
        thinking: "thinking_mode",
        thinkingModePolicy: "forced",
        forcedThinkingOptions: ["low", "high", "off"],
      },
    },
    "openrouter/auto": {
      displayName: "Auto",
      vendor: "openrouter",
      contextLength: 200_000,
      pricingUpdatedAt: "2026-01-01",
      capabilities: {},
    },
  };

  return { BUILTIN_OPENROUTER_MODELS };
});
