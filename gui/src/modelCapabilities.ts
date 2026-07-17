// Centralized model capabilities — single source of truth for:
//  - ApiKeyPanel model editor (what caps to display when a known model is selected)
//  - ProviderTiles popover (what caps to display)
//
// ⚠️ SYNC: When adding/editing a model, update BOTH this map AND
//          gui/src-tauri/src/proxy.rs resolve_model_capabilities() simultaneously.
//          The two definitions must stay in agreement.
//
// When adding a new known upstream model, just add it here.

export type ThinkingModePolicy = "toggleable" | "thinking_only" | "unknown";

export interface ModelCapabilities {
  supports_vision: boolean;
  supports_video: boolean;
  supports_image_url: boolean;
  supports_image_base64: boolean;
  supports_video_url: boolean;
  supports_video_base64: boolean;
  force_thinking: boolean;
  thinking: string; // "default" | "disabled"
  thinkingModePolicy: ThinkingModePolicy;
  supportsReasoningEffort: boolean;
  suppressThinkingParameter?: boolean; // K3: do not send thinking parameter upstream
  forcedReasoningEffort?: "max";       // K3: max is the only allowed effort
}

const KIMI_K27_CODE_CAPS: ModelCapabilities = {
  supports_vision: true,
  supports_video: true,
  supports_image_url: false,
  supports_image_base64: true,
  supports_video_url: false,
  supports_video_base64: true,
  force_thinking: true,
  thinking: "default",
  thinkingModePolicy: "thinking_only",
  supportsReasoningEffort: false,
};

export const MODEL_CAPABILITIES: Record<string, ModelCapabilities> = {
  // ── DeepSeek ──
  "deepseek-v4-pro": {
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
  },
  "deepseek-v4-flash": {
    supports_vision: false,
    supports_video: false,
    supports_image_url: false,
    supports_image_base64: false,
    supports_video_url: false,
    supports_video_base64: false,
    force_thinking: false,
    thinking: "default",
    thinkingModePolicy: "toggleable",
    supportsReasoningEffort: false,
  },

  // ── MiniMax ──
  "MiniMax-M3": {
    supports_vision: true,
    supports_video: true,
    supports_image_url: true,
    supports_image_base64: true,
    supports_video_url: true,
    supports_video_base64: true,
    force_thinking: true,
    thinking: "default",
    thinkingModePolicy: "thinking_only",
    supportsReasoningEffort: false,
  },
  "MiniMax-M2.7": {
    supports_vision: true,
    supports_video: true,
    supports_image_url: true,
    supports_image_base64: true,
    supports_video_url: true,
    supports_video_base64: true,
    force_thinking: false,
    thinking: "default",
    thinkingModePolicy: "toggleable",
    supportsReasoningEffort: false,
  },
  "MiniMax-M2.7-highspeed": {
    supports_vision: false,
    supports_video: false,
    supports_image_url: false,
    supports_image_base64: false,
    supports_video_url: false,
    supports_video_base64: false,
    force_thinking: true,
    thinking: "default",
    thinkingModePolicy: "thinking_only",
    supportsReasoningEffort: false,
  },

  // ── Kimi / Moonshot ──
  "kimi-k3": {
    supports_vision: true,
    supports_video: true,
    supports_image_url: false,
    supports_image_base64: true,
    supports_video_url: false,       // ms:// file ID only, no direct URL support
    supports_video_base64: false,    // no file upload→ms:// conversion in proxy
    force_thinking: true,
    thinking: "default",
    thinkingModePolicy: "thinking_only",
    supportsReasoningEffort: true,
    suppressThinkingParameter: true,
    forcedReasoningEffort: "max",
  },
  "kimi-k2.7-code": KIMI_K27_CODE_CAPS,
  "kimi-k2.7-code-highspeed": { ...KIMI_K27_CODE_CAPS },
  "kimi-k2.6": {
    supports_vision: true,
    supports_video: true,
    supports_image_url: false,
    supports_image_base64: true,
    supports_video_url: false,
    supports_video_base64: true,
    force_thinking: false,
    thinking: "disabled",
    thinkingModePolicy: "toggleable",
    supportsReasoningEffort: false,
  },
  "kimi-k2.5": {
    supports_vision: true,
    supports_video: true,
    supports_image_url: false,
    supports_image_base64: true,
    supports_video_url: false,
    supports_video_base64: true,
    force_thinking: false,
    thinking: "disabled",
    thinkingModePolicy: "toggleable",
    supportsReasoningEffort: false,
  },

  // ── MiMo ──
  "mimo-v2.5-pro": {
    supports_vision: false,
    supports_video: false,
    supports_image_url: false,
    supports_image_base64: false,
    supports_video_url: false,
    supports_video_base64: false,
    force_thinking: false,
    thinking: "default",
    thinkingModePolicy: "toggleable",
    supportsReasoningEffort: false,
  },
  "mimo-v2.5": {
    supports_vision: true,
    supports_video: false,
    supports_image_url: true,
    supports_image_base64: true,
    supports_video_url: false,
    supports_video_base64: false,
    force_thinking: false,
    thinking: "default",
    thinkingModePolicy: "toggleable",
    supportsReasoningEffort: false,
  },
};

// Per-provider model lists for dropdown
export const PROVIDER_MODELS: Record<string, string[]> = {
  deepseek: ["deepseek-v4-pro", "deepseek-v4-flash"],
  minimax: ["MiniMax-M3", "MiniMax-M2.7-highspeed"],
  kimi: ["kimi-k3", "kimi-k2.7-code", "kimi-k2.7-code-highspeed", "kimi-k2.6", "kimi-k2.5"],
  mimo: ["mimo-v2.5-pro", "mimo-v2.5"],
};

export const CUSTOM_MODEL_SENTINEL = "__custom__";

export const CUSTOM_MODEL_DEFAULTS: ModelCapabilities = {
  supports_vision: false,
  supports_video: false,
  supports_image_url: false,
  supports_image_base64: false,
  supports_video_url: false,
  supports_video_base64: false,
  force_thinking: false,
  thinking: "default",
  thinkingModePolicy: "unknown",
  supportsReasoningEffort: false,
};

export function isKnownModel(upstreamModel: string): boolean {
  return upstreamModel in MODEL_CAPABILITIES;
}

// For the ApiKeyPanel: get the selectable models for a provider
export function getProviderModels(providerId: string): string[] {
  return PROVIDER_MODELS[providerId] ?? [];
}
