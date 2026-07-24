export interface OpenRouterPricing {
  prompt?: string;
  completion?: string;
  image?: string;
  request?: string;
  audio?: string;
  webSearch?: string;
  internalReasoning?: string;
  inputCacheRead?: string;
  inputCacheWrite?: string;
}

export interface OpenRouterModel {
  id: string;
  canonicalSlug?: string;
  displayName: string;
  description?: string;
  contextLength?: number;
  maxCompletionTokens?: number;
  inputModalities: string[];
  outputModalities: string[];
  supportedParameters: string[];
  pricing: OpenRouterPricing;
}

export interface OpenRouterModelsResult {
  models: OpenRouterModel[];
  fetchedAt: string;
  source: string;
  stale: boolean;
  warning?: string;
}
