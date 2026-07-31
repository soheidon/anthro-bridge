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
      groupOpenAI: "OpenAI",
      groupOtherModels: "Other",
      groupRouter: "OpenRouter",
      selectModel: "Select a model",
      selectVendor: "Select vendor",
      thinkingMode: "Thinking",
      customModelShort: "Custom",
      customModelPlaceholder: "Enter custom model ID...",
      confirm: "Confirm",
      loading: "Loading...",
      modeStandard: "Standard",
      modePro: "Pro",
      modeLabel: "Mode",
      reasoningLow: "Reasoning: Low",
      reasoningMedium: "Reasoning: Medium",
      reasoningHigh: "Reasoning: High",
      reasoningExtraHigh: "Reasoning: Extra High",
      reasoningMax: "Reasoning: Max",
    },
    apiKeyPanel: {
      normalMode: "Normal",
      thinkingMode: "Mode",
    },
    modelPricing: {
      header: "Model Pricing",
      usdLabel: "(USD / 1M tokens)",
      colProvider: "Provider",
      colModel: "Model",
      colInput: "Input/1M",
      colOutput: "Output/1M",
      colCachedInput: "Cache/1M",
      colNotes: "Notes",
      disclaimer: "Pricing is approximate",
      pricingDate: "Prices as of July 31, 2026",
      notes: {
        openRouterPricing: "OpenRouter pricing",
        gpt56StandardPrice: "OpenAI revised standard price; no discount.",
        gpt56Promotion: "Limited-time 50% provider discount on OpenRouter. No end date announced.",
        gpt56LongContext: "Prompts of 272K tokens or more use long-context pricing.",
      },
      discountedPriceAria: "Current price {current}, revised standard price {regular}",
    },
    openRouterProfile: {
      dragHandle: "Drag to reorder",
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
            result = result.replace(`{${k}}`, v);
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
    "openai/gpt-5.6-sol": {
      displayName: "GPT-5.6 Sol",
      vendor: "openai",
      contextLength: 1_050_000,
      pricingUpdatedAt: "2026-08-01",
      pricing: { inputPerMillionUsd: 5, outputPerMillionUsd: 30, cacheReadPerMillionUsd: 0.5 },
      pricingNoteKeys: ["modelPricing.notes.openrouterPricing", "modelPricing.notes.gpt56StandardPrice", "modelPricing.notes.gpt56LongContext"],
      capabilities: {
        supports_vision: true, supports_video: false,
        force_thinking: false, thinking: "reasoning_effort",
        thinkingModePolicy: "forced", supportsReasoningEffort: true,
        forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
      },
    },
    "openai/gpt-5.6-sol-pro": {
      displayName: "GPT-5.6 Sol Pro",
      vendor: "openai",
      contextLength: 1_050_000,
      pricingUpdatedAt: "2026-08-01",
      pricing: { inputPerMillionUsd: 5, outputPerMillionUsd: 30, cacheReadPerMillionUsd: 0.5 },
      pricingNoteKeys: ["modelPricing.notes.openrouterPricing", "modelPricing.notes.gpt56StandardPrice", "modelPricing.notes.gpt56LongContext"],
      capabilities: {
        supports_vision: true, supports_video: false,
        force_thinking: false, thinking: "reasoning_effort",
        thinkingModePolicy: "forced", supportsReasoningEffort: true,
        forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
      },
    },
    "openai/gpt-5.6-terra": {
      displayName: "GPT-5.6 Terra",
      vendor: "openai",
      contextLength: 1_050_000,
      pricingUpdatedAt: "2026-08-01",
      pricing: { inputPerMillionUsd: 1, outputPerMillionUsd: 6, cacheReadPerMillionUsd: 0.1, regularInputPerMillionUsd: 2, regularOutputPerMillionUsd: 12, regularCacheReadPerMillionUsd: 0.2 },
      pricingNoteKeys: ["modelPricing.notes.openrouterPricing", "modelPricing.notes.gpt56Promotion", "modelPricing.notes.gpt56LongContext"],
      capabilities: {
        supports_vision: true, supports_video: false,
        force_thinking: false, thinking: "reasoning_effort",
        thinkingModePolicy: "forced", supportsReasoningEffort: true,
        forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
      },
    },
    "openai/gpt-5.6-terra-pro": {
      displayName: "GPT-5.6 Terra Pro",
      vendor: "openai",
      contextLength: 1_050_000,
      pricingUpdatedAt: "2026-08-01",
      pricing: { inputPerMillionUsd: 1, outputPerMillionUsd: 6, cacheReadPerMillionUsd: 0.1, regularInputPerMillionUsd: 2, regularOutputPerMillionUsd: 12, regularCacheReadPerMillionUsd: 0.2 },
      pricingNoteKeys: ["modelPricing.notes.openrouterPricing", "modelPricing.notes.gpt56Promotion", "modelPricing.notes.gpt56LongContext"],
      capabilities: {
        supports_vision: true, supports_video: false,
        force_thinking: false, thinking: "reasoning_effort",
        thinkingModePolicy: "forced", supportsReasoningEffort: true,
        forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
      },
    },
    "openai/gpt-5.6-luna": {
      displayName: "GPT-5.6 Luna",
      vendor: "openai",
      contextLength: 1_050_000,
      pricingUpdatedAt: "2026-08-01",
      pricing: { inputPerMillionUsd: 0.1, outputPerMillionUsd: 0.6, cacheReadPerMillionUsd: 0.01, regularInputPerMillionUsd: 0.2, regularOutputPerMillionUsd: 1.2, regularCacheReadPerMillionUsd: 0.02 },
      pricingNoteKeys: ["modelPricing.notes.openrouterPricing", "modelPricing.notes.gpt56Promotion", "modelPricing.notes.gpt56LongContext"],
      capabilities: {
        supports_vision: true, supports_video: false,
        force_thinking: false, thinking: "reasoning_effort",
        thinkingModePolicy: "forced", supportsReasoningEffort: true,
        forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
      },
    },
    "openai/gpt-5.6-luna-pro": {
      displayName: "GPT-5.6 Luna Pro",
      vendor: "openai",
      contextLength: 1_050_000,
      pricingUpdatedAt: "2026-08-01",
      pricing: { inputPerMillionUsd: 0.1, outputPerMillionUsd: 0.6, cacheReadPerMillionUsd: 0.01, regularInputPerMillionUsd: 0.2, regularOutputPerMillionUsd: 1.2, regularCacheReadPerMillionUsd: 0.02 },
      pricingNoteKeys: ["modelPricing.notes.openrouterPricing", "modelPricing.notes.gpt56Promotion", "modelPricing.notes.gpt56LongContext"],
      capabilities: {
        supports_vision: true, supports_video: false,
        force_thinking: false, thinking: "reasoning_effort",
        thinkingModePolicy: "forced", supportsReasoningEffort: true,
        forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
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
