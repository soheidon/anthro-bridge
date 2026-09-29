// UI-only labels from the shared static catalog. IDs are never rewritten.
import modelCatalog from "../shared/model_catalog.json";

const catalogModels = (modelCatalog as {
  models: Record<string, { providerId: string; displayName: string }>;
}).models;

/**
 * Returns the user-facing display name for a model.
 *
 * Known non-OpenRouter provider/model pairs use the catalog's canonical label.
 * OpenRouter's provider-qualified IDs intentionally remain verbatim here;
 * callers that need OpenRouter labels use its dedicated catalog helper.
 *
 * @param modelId   Internal model identifier (e.g. "deepseek-flash", "mimo-v2.6-flash")
 * @param providerId Provider identifier (e.g. "deepseek", "mimo", "openrouter")
 */
export function getModelDisplayName(modelId: string, providerId?: string): string {
  if (!providerId) return modelId;
  // OpenRouter has its own provider-qualified display helper; keep this
  // generic legacy helper from stripping/relabeling those IDs.
  if (providerId === "openrouter") return modelId;
  const model = catalogModels[modelId];
  return model && model.providerId === providerId ? model.displayName : modelId;
}
