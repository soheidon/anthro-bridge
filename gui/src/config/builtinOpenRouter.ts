import type { ModelCapabilities, ThinkingOption } from "../modelCapabilities";

// ═══════════════════════════════════════════════════════════════════
// Single source of truth for built-in OpenRouter default models.
// Feeds both MODEL_CAPABILITIES and MODEL_PRICING to avoid duplication.
//
// Pricing Policy:
// 1. Base/headline price is taken from the official OpenRouter model page.
// 2. Limited-time promotions display regular headline price (strikethrough)
//    and current discounted headline price (bold).
// 3. Single-provider / Flex / batch rates are excluded from static model headlines.
// ═══════════════════════════════════════════════════════════════════

export interface BuiltinOpenRouterPricing {
  inputPerMillionUsd: number;
  outputPerMillionUsd: number;
  cacheReadPerMillionUsd?: number;
  // OpenAI's current revised standard price, not the original launch price.
  regularInputPerMillionUsd?: number;
  regularOutputPerMillionUsd?: number;
  regularCacheReadPerMillionUsd?: number;
}

export interface BuiltinOpenRouterEntry {
  displayName: string;
  vendor: string;
  pricingNoteKey?: string;
  pricingNoteKeys?: string[];
  pricingUpdatedAt: string;
  capabilities: ModelCapabilities;
  pricing?: BuiltinOpenRouterPricing;
}

export const BUILTIN_OPENROUTER_MODELS: Record<string, BuiltinOpenRouterEntry> = {
  // ── DeepSeek V4.1 Flash ──
  "deepseek/deepseek-v4.1-flash": {
    displayName: "DeepSeek V4.1 Flash",
    vendor: "DeepSeek",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-09-10",
    capabilities: {
      supports_vision: true,
      supports_video: false,
      supports_image_url: true,
      supports_image_base64: true,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "default",
      thinkingModePolicy: "toggleable",
      supportsReasoningEffort: true,
      reasoningEffortOptions: ["low", "high", "max"],
    },
    pricing: {
      inputPerMillionUsd: 0.15,
      outputPerMillionUsd: 0.60,
    },
  },

  // ── DeepSeek V4 Flash 0731 ──
  "deepseek/deepseek-v4-flash-0731": {
    displayName: "DeepSeek V4 Flash 0731",
    vendor: "DeepSeek",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-08-14",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "default",
      thinkingModePolicy: "toggleable",
      supportsReasoningEffort: true,
      reasoningEffortOptions: ["high", "max"],
    },
    pricing: {
      inputPerMillionUsd: 0.05,
      outputPerMillionUsd: 0.16,
      cacheReadPerMillionUsd: 0.013,
    },
  },

  // ── DeepSeek V4 Pro 0813 ──
  "deepseek/deepseek-v4-pro-0813": {
    displayName: "DeepSeek V4 Pro 0813",
    vendor: "DeepSeek",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.deepseekV4ProGoingAway",
    ],
    pricingUpdatedAt: "2026-08-14",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "default",
      thinkingModePolicy: "toggleable",
      supportsReasoningEffort: true,
      reasoningEffortOptions: ["high", "max"],
    },
    pricing: {
      inputPerMillionUsd: 0.66,
      outputPerMillionUsd: 1.98,
    },
  },

  // ── Poolside Laguna S 2.1 ──
  "poolside/laguna-s-2.1": {
    displayName: "Laguna S 2.1",
    vendor: "Poolside",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56Promotion",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "thinking_mode",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["max", "off"],
    },
    pricing: {
      inputPerMillionUsd: 0.09,
      outputPerMillionUsd: 0.18,
      cacheReadPerMillionUsd: 0.009,
      regularInputPerMillionUsd: 0.10,
      regularOutputPerMillionUsd: 0.20,
      regularCacheReadPerMillionUsd: 0.01,
    },
  },
  "poolside/laguna-s-2.1:free": {
    displayName: "Laguna S 2.1 (Free)",
    vendor: "Poolside",
    pricingUpdatedAt: "2026-07-25",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "thinking_mode",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["max", "off"],
    },
  },

  // ── Poolside Laguna XS 2.1 ──
  "poolside/laguna-xs-2.1": {
    displayName: "Laguna XS 2.1",
    vendor: "Poolside",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-07-25",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "thinking_mode",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["on", "off"],
    },
    pricing: {
      inputPerMillionUsd: 0.06,
      outputPerMillionUsd: 0.12,
      cacheReadPerMillionUsd: 0.03,
    },
  },
  "poolside/laguna-xs-2.1:free": {
    displayName: "Laguna XS 2.1 (Free)",
    vendor: "Poolside",
    pricingUpdatedAt: "2026-07-25",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "thinking_mode",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["on", "off"],
    },
  },

  // ── Tencent Hy3 ──
  "tencent/hy3": {
    displayName: "Hy3",
    vendor: "Tencent",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "thinking_mode",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["off", "low", "high"],
    },
    pricing: {
      inputPerMillionUsd: 0.0825,
      outputPerMillionUsd: 0.33,
      cacheReadPerMillionUsd: 0.02063,
    },
  },
  "tencent/hy3:free": {
    displayName: "Hy3 (Free)",
    vendor: "Tencent",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-07-31",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "thinking_mode",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["off", "low", "high"],
    },
  },

  // ── InclusionAI ──
  "inclusionai/ring-2.6-1t": {
    displayName: "Ring 2.6 1T",
    vendor: "InclusionAI",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-07-31",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["high", "xhigh"],
    },
    pricing: {
      inputPerMillionUsd: 0.075,
      outputPerMillionUsd: 0.625,
      cacheReadPerMillionUsd: 0.015,
    },
  },
  "inclusionai/ling-2.6-1t": {
    displayName: "Ling 2.6 1T",
    vendor: "InclusionAI",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-07-31",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "none",
      thinkingModePolicy: "none",
      supportsReasoningEffort: false,
    },
    pricing: {
      inputPerMillionUsd: 0.075,
      outputPerMillionUsd: 0.625,
      cacheReadPerMillionUsd: 0.015,
    },
  },
  "inclusionai/ling-2.6-flash": {
    displayName: "Ling 2.6 Flash",
    vendor: "InclusionAI",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-07-31",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "none",
      thinkingModePolicy: "none",
      supportsReasoningEffort: false,
    },
    pricing: {
      inputPerMillionUsd: 0.010,
      outputPerMillionUsd: 0.030,
      cacheReadPerMillionUsd: 0.002,
    },
  },
  "inclusionai/ling-3.0-flash:free": {
    displayName: "Ling 3.0 Flash (Free)",
    vendor: "InclusionAI",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-07-31",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "thinking_mode",
      thinkingModePolicy: "optional",
      supportsReasoningEffort: false,
      forcedThinkingOptions: ["off", "on"],
    },
  },


  // ── Google Gemini ──
  "google/gemini-3.1-pro-preview": {
    displayName: "Gemini 3.1 Pro Preview",
    vendor: "Google",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-08-18",
    capabilities: {
      supports_vision: true,
      supports_video: false,
      supports_image_url: true,
      supports_image_base64: true,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["low", "medium", "high"],
    },
    pricing: {
      inputPerMillionUsd: 2.0,
      outputPerMillionUsd: 12.0,
      cacheReadPerMillionUsd: 0.2,
    },
  },
  "google/gemini-3.7-flash": {
    displayName: "Gemini 3.7 Flash",
    vendor: "Google",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56Promotion",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true,
      supports_video: false,
      supports_image_url: true,
      supports_image_base64: true,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["low", "medium", "high"],
    },
    pricing: {
      inputPerMillionUsd: 0.75,
      outputPerMillionUsd: 3.75,
      cacheReadPerMillionUsd: 0.075,
      regularInputPerMillionUsd: 1.50,
      regularOutputPerMillionUsd: 7.50,
      regularCacheReadPerMillionUsd: 0.15,
    },
  },
  "google/gemini-3.8-flash": {
    displayName: "Gemini 3.8 Flash",
    vendor: "Google",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56Promotion",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true,
      supports_video: false,
      supports_image_url: true,
      supports_image_base64: true,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["low", "medium", "high"],
    },
    pricing: {
      inputPerMillionUsd: 0.75,
      outputPerMillionUsd: 3.75,
      cacheReadPerMillionUsd: 0.075,
      regularInputPerMillionUsd: 1.50,
      regularOutputPerMillionUsd: 7.50,
      regularCacheReadPerMillionUsd: 0.15,
    },
  },
  // ── OpenAI GPT-6 Astra ──
  "openai/gpt-6-astra": {
    displayName: "GPT-6 Astra",
    vendor: "OpenAI",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true,
      supports_video: false,
      supports_image_url: true,
      supports_image_base64: true,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["low", "medium", "high", "xhigh", "max"],
    },
    pricing: {
      inputPerMillionUsd: 10.0,
      outputPerMillionUsd: 50.0,
    },
  },
  "openai/gpt-6-astra-pro": {
    displayName: "GPT-6 Astra Pro",
    vendor: "OpenAI",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true,
      supports_video: false,
      supports_image_url: true,
      supports_image_base64: true,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: true,
      thinking: "default",
      thinkingModePolicy: "thinking_only",
      supportsReasoningEffort: false,
    },
    pricing: {
      inputPerMillionUsd: 10.0,
      outputPerMillionUsd: 50.0,
    },
  },
  "openai/gpt-astra-latest": {
    displayName: "GPT Astra Latest",
    vendor: "OpenAI",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true,
      supports_video: false,
      supports_image_url: true,
      supports_image_base64: true,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["low", "medium", "high", "xhigh", "max"],
    },
    pricing: {
      inputPerMillionUsd: 10.0,
      outputPerMillionUsd: 50.0,
    },
  },

  // ── OpenAI GPT-5.6 ──
  // Context length: ~1.05M tokens across all variants (OpenRouter metadata).
  // Static fallback prices for offline display.
  // Live OpenRouter metadata is authoritative and takes precedence.
  // Review these values when OpenRouter promotions or provider pricing change.
  "openai/gpt-5.6-sol": {
    displayName: "GPT-5.6 Sol",
    vendor: "OpenAI",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56Promotion",
      "modelPricing.notes.gpt56LongContext",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true, supports_video: false,
      supports_image_url: true, supports_image_base64: true,
      supports_video_url: false, supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
    },
    pricing: {
      inputPerMillionUsd: 2.0,
      outputPerMillionUsd: 10.0,
      cacheReadPerMillionUsd: 0.2,
      regularInputPerMillionUsd: 4.0,
      regularOutputPerMillionUsd: 20.0,
      regularCacheReadPerMillionUsd: 0.4,
    },
  },
  "openai/gpt-5.6-sol-pro": {
    displayName: "GPT-5.6 Sol Pro",
    vendor: "OpenAI",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56Promotion",
      "modelPricing.notes.gpt56LongContext",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true, supports_video: false,
      supports_image_url: true, supports_image_base64: true,
      supports_video_url: false, supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
    },
    pricing: {
      inputPerMillionUsd: 2.0,
      outputPerMillionUsd: 10.0,
      cacheReadPerMillionUsd: 0.2,
      regularInputPerMillionUsd: 4.0,
      regularOutputPerMillionUsd: 20.0,
      regularCacheReadPerMillionUsd: 0.4,
    },
  },
  "openai/gpt-5.6-terra": {
    displayName: "GPT-5.6 Terra",
    vendor: "OpenAI",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56LongContext",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true, supports_video: false,
      supports_image_url: true, supports_image_base64: true,
      supports_video_url: false, supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
    },
    pricing: {
      inputPerMillionUsd: 2.0,
      outputPerMillionUsd: 12.0,
      cacheReadPerMillionUsd: 0.2,
    },
  },
  "openai/gpt-5.6-terra-pro": {
    displayName: "GPT-5.6 Terra Pro",
    vendor: "OpenAI",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56LongContext",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true, supports_video: false,
      supports_image_url: true, supports_image_base64: true,
      supports_video_url: false, supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
    },
    pricing: {
      inputPerMillionUsd: 2.0,
      outputPerMillionUsd: 12.0,
      cacheReadPerMillionUsd: 0.2,
    },
  },
  "openai/gpt-5.6-luna": {
    displayName: "GPT-5.6 Luna",
    vendor: "OpenAI",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56LongContext",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true, supports_video: false,
      supports_image_url: true, supports_image_base64: true,
      supports_video_url: false, supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
    },
    pricing: {
      inputPerMillionUsd: 0.2,
      outputPerMillionUsd: 1.2,
      cacheReadPerMillionUsd: 0.02,
    },
  },
  "openai/gpt-5.6-luna-pro": {
    displayName: "GPT-5.6 Luna Pro",
    vendor: "OpenAI",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56LongContext",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true, supports_video: false,
      supports_image_url: true, supports_image_base64: true,
      supports_video_url: false, supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["off", "low", "medium", "high", "xhigh", "max"],
    },
    pricing: {
      inputPerMillionUsd: 0.2,
      outputPerMillionUsd: 1.2,
      cacheReadPerMillionUsd: 0.02,
    },
  },

  // ── StepFun ──
  "stepfun/step-3.7-flash": {
    displayName: "Step 3.7 Flash",
    vendor: "StepFun",
    pricingNoteKeys: [
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.gpt56Promotion",
    ],
    pricingUpdatedAt: "2026-09-12",
    capabilities: {
      supports_vision: true,
      supports_video: false,
      supports_image_url: true,
      supports_image_base64: true,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "reasoning_effort",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: true,
      forcedThinkingOptions: ["low", "medium", "high"],
    },
    pricing: {
      inputPerMillionUsd: 0.16,
      outputPerMillionUsd: 0.92,
      regularInputPerMillionUsd: 0.20,
      regularOutputPerMillionUsd: 1.15,
    },
  },
  "stepfun/step-3.5-flash": {
    displayName: "Step 3.5 Flash",
    vendor: "StepFun",
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-07-31",
    capabilities: {
      supports_vision: false,
      supports_video: false,
      supports_image_url: false,
      supports_image_base64: false,
      supports_video_url: false,
      supports_video_base64: false,
      force_thinking: false,
      thinking: "thinking_mode",
      thinkingModePolicy: "forced",
      supportsReasoningEffort: false,
      forcedThinkingOptions: ["on"],
    },
    pricing: {
      inputPerMillionUsd: 0.10,
      outputPerMillionUsd: 0.30,
    },
  },
};

export function getOpenRouterModelDisplayName(modelId: string): string {
  return BUILTIN_OPENROUTER_MODELS[modelId]?.displayName ?? modelId;
}

export function getOpenRouterVendorModels(vendorId: string): string[] {
  const normVendor = vendorId.toLowerCase();
  return Object.entries(BUILTIN_OPENROUTER_MODELS)
    .filter(([id, entry]) => entry.vendor.toLowerCase() === normVendor && !id.includes(":batch"))
    .map(([id]) => id);
}

export function getOpenRouterProfileVendor(profile?: {
  id?: string;
  display_name?: string;
  models?: Record<string, { upstream_model?: string }>;
  model_map?: Record<string, string>;
}): string | null {
  if (!profile) return null;
  const name = (profile.display_name || "").toLowerCase();
  if (name.includes("chatgpt") || name.includes("openai")) return "openai";
  if (name.includes("gemini") || name.includes("google")) return "google";
  if (name.includes("deepseek")) return "deepseek";
  if (name.includes("laguna") || name.includes("poolside")) return "poolside";
  if (name.includes("hy3") || name.includes("tencent")) return "tencent";
  if (name.includes("inclusionai") || name.includes("ring") || name.includes("ling")) return "inclusionai";
  if (name.includes("stepfun") || name.includes("step")) return "stepfun";

  const upstreamList: string[] = [];
  if (profile.models) {
    upstreamList.push(...Object.values(profile.models).map((m) => m?.upstream_model || ""));
  }
  if (profile.model_map) {
    upstreamList.push(...Object.values(profile.model_map));
  }
  for (const m of upstreamList) {
    if (!m) continue;
    if (m.startsWith("openai/")) return "openai";
    if (m.startsWith("google/")) return "google";
    if (m.startsWith("deepseek/")) return "deepseek";
    if (m.startsWith("poolside/")) return "poolside";
    if (m.startsWith("tencent/")) return "tencent";
    if (m.startsWith("inclusionai/")) return "inclusionai";
    if (m.startsWith("stepfun/")) return "stepfun";
  }
  return null;
}

export function getOpenRouterModelsForProfile(profile?: {
  id?: string;
  display_name?: string;
  models?: Record<string, { upstream_model?: string }>;
  model_map?: Record<string, string>;
}): string[] {
  const vendor = getOpenRouterProfileVendor(profile);
  const models: string[] = [];
  if (vendor) {
    models.push(...getOpenRouterVendorModels(vendor));
  }
  if (profile?.models) {
    const list = Object.values(profile.models)
      .map((m) => m?.upstream_model)
      .filter((m): m is string => typeof m === "string" && m.length > 0 && !m.includes(":batch"));
    for (const m of list) {
      if (!models.includes(m)) {
        models.push(m);
      }
    }
  }
  if (models.length === 0) {
    models.push("deepseek/deepseek-r1", "google/gemini-3.7-flash", "anthropic/claude-3.7-sonnet");
  }
  return models;
}
