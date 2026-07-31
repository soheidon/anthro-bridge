import type { ModelCapabilities, ThinkingOption } from "../modelCapabilities";

// ═══════════════════════════════════════════════════════════════════
// Single source of truth for built-in OpenRouter default models.
// Feeds both MODEL_CAPABILITIES and MODEL_PRICING to avoid duplication.
// source: openrouter-api
// ═══════════════════════════════════════════════════════════════════

export interface BuiltinOpenRouterPricing {
  inputPerMillionUsd: number;
  outputPerMillionUsd: number;
  cacheReadPerMillionUsd?: number;
}

export interface BuiltinOpenRouterEntry {
  displayName: string;
  vendor: string;
  contextLength: number;
  pricingNoteKey?: string;
  pricingUpdatedAt: string;
  capabilities: ModelCapabilities;
  pricing?: BuiltinOpenRouterPricing;
}

export const BUILTIN_OPENROUTER_MODELS: Record<string, BuiltinOpenRouterEntry> = {
  // ── Poolside Laguna S 2.1 ──
  "poolside/laguna-s-2.1": {
    displayName: "Laguna S 2.1",
    vendor: "Poolside",
    contextLength: 131_072,
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
      forcedThinkingOptions: ["max", "off"],
    },
    pricing: {
      inputPerMillionUsd: 0.10,
      outputPerMillionUsd: 0.20,
      cacheReadPerMillionUsd: 0.01,
    },
  },
  "poolside/laguna-s-2.1:free": {
    displayName: "Laguna S 2.1 (Free)",
    vendor: "Poolside",
    contextLength: 131_072,
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
    contextLength: 131_072,
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
    contextLength: 131_072,
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
    contextLength: 262_144,
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
    pricing: {
      inputPerMillionUsd: 0.132,
      outputPerMillionUsd: 0.528,
      cacheReadPerMillionUsd: 0.033,
    },
  },
  "tencent/hy3:free": {
    displayName: "Hy3 (Free)",
    vendor: "Tencent",
    contextLength: 262_144,
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
    contextLength: 262_144,
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
    contextLength: 262_144,
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
    contextLength: 262_144,
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
    contextLength: 262_144,
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

  // ── StepFun ──
  "stepfun/step-3.7-flash": {
    displayName: "Step 3.7 Flash",
    vendor: "StepFun",
    contextLength: 262_144,
    pricingNoteKey: "modelPricing.notes.openrouterPricing",
    pricingUpdatedAt: "2026-07-31",
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
      inputPerMillionUsd: 0.20,
      outputPerMillionUsd: 1.15,
      cacheReadPerMillionUsd: 0.04,
    },
  },
  "stepfun/step-3.5-flash": {
    displayName: "Step 3.5 Flash",
    vendor: "StepFun",
    contextLength: 262_144,
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
