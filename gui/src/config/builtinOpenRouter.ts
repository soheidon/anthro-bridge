import modelCatalog from "../shared/model_catalog.json";
import openRouterPricing from "./openrouterPricing.json";
import type { ModelCapabilities } from "../modelCapabilities";

export interface BuiltinOpenRouterPricing {
  inputPerMillionUsd: number;
  outputPerMillionUsd: number;
  cacheReadPerMillionUsd?: number;
  regularInputPerMillionUsd?: number;
  regularOutputPerMillionUsd?: number;
  regularCacheReadPerMillionUsd?: number;
}

interface OpenRouterPricingMetadata {
  pricingNoteKey?: string;
  pricingNoteKeys?: string[];
  pricingUpdatedAt?: string;
  pricing?: BuiltinOpenRouterPricing;
}

export interface BuiltinOpenRouterEntry extends OpenRouterPricingMetadata {
  displayName: string;
  vendor: string;
  capabilities: ModelCapabilities;
}

type CatalogModel = {
  providerId: string;
  displayName: string;
  vendor?: string;
  capabilities: {
    supportsVision: boolean;
    supportsVideo: boolean;
    supportsImageUrl: boolean;
    supportsImageBase64: boolean;
    supportsVideoUrl: boolean;
    supportsVideoBase64: boolean;
    forceThinking: boolean;
    thinkingDefault: string;
    thinkingPolicy: ModelCapabilities["thinkingModePolicy"];
    supportsReasoningEffort: boolean;
    suppressThinkingParameter?: boolean;
    forcedReasoningEffort?: "max";
    forcedThinkingOptions?: ModelCapabilities["forcedThinkingOptions"];
    reasoningEffortOptions?: ModelCapabilities["reasoningEffortOptions"];
  };
};

const catalogModels = (modelCatalog as { models: Record<string, CatalogModel> }).models;
const pricingMetadata = openRouterPricing as Record<string, OpenRouterPricingMetadata>;

function modelCapabilities(model: CatalogModel): ModelCapabilities {
  return {
    supports_vision: model.capabilities.supportsVision,
    supports_video: model.capabilities.supportsVideo,
    supports_image_url: model.capabilities.supportsImageUrl,
    supports_image_base64: model.capabilities.supportsImageBase64,
    supports_video_url: model.capabilities.supportsVideoUrl,
    supports_video_base64: model.capabilities.supportsVideoBase64,
    force_thinking: model.capabilities.forceThinking,
    thinking: model.capabilities.thinkingDefault,
    thinkingModePolicy: model.capabilities.thinkingPolicy,
    supportsReasoningEffort: model.capabilities.supportsReasoningEffort,
    suppressThinkingParameter: model.capabilities.suppressThinkingParameter,
    forcedReasoningEffort: model.capabilities.forcedReasoningEffort,
    forcedThinkingOptions: model.capabilities.forcedThinkingOptions,
    reasoningEffortOptions: model.capabilities.reasoningEffortOptions,
  };
}

// Canonical model identity, label, vendor, and static capabilities come from
// the shared catalog. This module only augments OpenRouter-specific pricing.
export const BUILTIN_OPENROUTER_MODELS: Record<string, BuiltinOpenRouterEntry> = Object.fromEntries(
  Object.entries(catalogModels)
    .filter(([, model]) => model.providerId === "openrouter" && model.vendor)
    .map(([id, model]) => [id, {
      displayName: model.displayName,
      vendor: model.vendor!,
      capabilities: modelCapabilities(model),
      ...pricingMetadata[id],
    }]),
);

export function getOpenRouterModelDisplayName(modelId: string): string {
  return catalogModels[modelId]?.providerId === "openrouter"
    ? catalogModels[modelId].displayName
    : modelId;
}

export function getOpenRouterVendorModels(vendorId: string): string[] {
  const normalizedVendor = vendorId.toLowerCase();
  return Object.entries(BUILTIN_OPENROUTER_MODELS)
    .filter(([id, entry]) => entry.vendor.toLowerCase() === normalizedVendor && !id.includes(":batch"))
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
  if (profile.models) upstreamList.push(...Object.values(profile.models).map((model) => model?.upstream_model || ""));
  if (profile.model_map) upstreamList.push(...Object.values(profile.model_map));
  for (const model of upstreamList) {
    if (!model) continue;
    const vendor = Object.entries(BUILTIN_OPENROUTER_MODELS)
      .find(([id]) => id === model)?.[1]?.vendor;
    if (vendor) return vendor.toLowerCase();

    // Preserve support for saved/custom OpenRouter IDs that are not in the
    // static catalog. The catalog remains authoritative for known models.
    const prefix = model.split("/", 1)[0]?.toLowerCase();
    if (prefix) {
      const knownVendor = Object.values(BUILTIN_OPENROUTER_MODELS)
        .find((entry) => entry.vendor.toLowerCase() === prefix)?.vendor;
      if (knownVendor) return knownVendor.toLowerCase();
    }
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
  const models: string[] = vendor ? getOpenRouterVendorModels(vendor) : [];
  if (profile?.models) {
    for (const model of Object.values(profile.models).map((entry) => entry?.upstream_model)
      .filter((entry): entry is string => typeof entry === "string" && entry.length > 0 && !entry.includes(":batch"))) {
      if (!models.includes(model)) models.push(model);
    }
  }
  if (models.length === 0) models.push("deepseek/deepseek-r1", "google/gemini-3.7-flash", "anthropic/claude-3.7-sonnet");
  return models;
}
