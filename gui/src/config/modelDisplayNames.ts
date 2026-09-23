// UI-only display name mapping for Direct DeepSeek and Direct MiMo models.
// Internal model IDs and routing are never modified — this helper
// is used exclusively for user-visible labels in dropdowns and tables.

const DIRECT_DEEPSEEK_PROVIDER_ID = "deepseek";
const DIRECT_MIMO_PROVIDER_ID = "mimo";

const DIRECT_DEEPSEEK_DISPLAY_NAMES: Record<string, string> = {
  "deepseek-flash": "DeepSeek V4.1 Flash",
  "deepseek-v4-pro": "DeepSeek V4 Pro 0813",
};

const DIRECT_MIMO_DISPLAY_NAMES: Record<string, string> = {
  "mimo-v2.6-flash": "MiMo-V2.6-Flash",
  "mimo-v2.6-pro": "MiMo-V2.6-Pro",
  "mimo-v2.6-pro-ultraspeed": "MiMo-V2.6-Pro-UltraSpeed",
  "mimo-v2.5-pro": "MiMo-V2.5-Pro",
  "mimo-v2.5": "MiMo-V2.5",
  "mimo-v2.5-pro-ultraspeed": "MiMo-V2.5-Pro-UltraSpeed",
};

/**
 * Returns the user-facing display name for a model.
 *
 * For the `deepseek` (Direct DeepSeek) provider, maps internal model IDs to
 * dated release display names. For `mimo` (Direct MiMo), maps V2.6 model IDs
 * to TitleCase display names. All other providers and unmapped IDs are
 * returned verbatim — including OpenRouter models.
 *
 * @param modelId   Internal model identifier (e.g. "deepseek-flash", "mimo-v2.6-flash")
 * @param providerId Provider identifier (e.g. "deepseek", "mimo", "openrouter")
 */
export function getModelDisplayName(modelId: string, providerId?: string): string {
  if (providerId === DIRECT_DEEPSEEK_PROVIDER_ID) {
    return DIRECT_DEEPSEEK_DISPLAY_NAMES[modelId] ?? modelId;
  }
  if (providerId === DIRECT_MIMO_PROVIDER_ID) {
    return DIRECT_MIMO_DISPLAY_NAMES[modelId] ?? modelId;
  }
  return modelId;
}
