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
}

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
  },

  // ── MiniMax ──
  "MiniMax-M3": {
    supports_vision: true,
    supports_video: true,
    supports_image_url: true,
    supports_image_base64: true,
    supports_video_url: true,
    supports_video_base64: true,
    force_thinking: false,
    thinking: "default",
    thinkingModePolicy: "toggleable",
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
  },

  // ── Kimi / Moonshot ──
  "kimi-k2.7-code": {
    supports_vision: true,
    supports_video: true,
    supports_image_url: false,
    supports_image_base64: true,
    supports_video_url: false,
    supports_video_base64: true,
    force_thinking: true,
    thinking: "default",
    thinkingModePolicy: "thinking_only",
  },
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
  },
};

// Per-provider model lists for dropdown
export const PROVIDER_MODELS: Record<string, string[]> = {
  deepseek: ["deepseek-v4-pro", "deepseek-v4-flash"],
  minimax: ["MiniMax-M3", "MiniMax-M2.7-highspeed"],
  kimi: ["kimi-k2.7-code", "kimi-k2.6"],
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
};

export function isKnownModel(upstreamModel: string): boolean {
  return upstreamModel in MODEL_CAPABILITIES;
}

// For the ApiKeyPanel: get the selectable models for a provider
export function getProviderModels(providerId: string): string[] {
  return PROVIDER_MODELS[providerId] ?? [];
}
