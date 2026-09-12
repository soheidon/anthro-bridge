// UI-only display name mapping for Direct DeepSeek models.
// Internal model IDs and routing are never modified — this helper
// is used exclusively for user-visible labels in dropdowns and tables.

const DIRECT_DEEPSEEK_PROVIDER_ID = "deepseek";

const DIRECT_DEEPSEEK_DISPLAY_NAMES: Record<string, string> = {
  "deepseek-flash": "DeepSeek V4.1 Flash",
  "deepseek-v4-pro": "DeepSeek V4 Pro 0813",
};

/**
 * Returns the user-facing display name for a model.
 *
 * For the `deepseek` (Direct DeepSeek) provider, maps internal model IDs to
 * dated release display names. All other providers and unmapped IDs are
 * returned verbatim — including OpenRouter models.
 *
 * @param modelId   Internal model identifier (e.g. "deepseek-flash")
 * @param providerId Provider identifier (e.g. "deepseek", "openrouter")
 */
export function getModelDisplayName(modelId: string, providerId?: string): string {
  if (providerId === DIRECT_DEEPSEEK_PROVIDER_ID) {
    return DIRECT_DEEPSEEK_DISPLAY_NAMES[modelId] ?? modelId;
  }
  return modelId;
}
