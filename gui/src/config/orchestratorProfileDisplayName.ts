import type { OrchestratorProfile } from "../types/orchestrator";
import { getOpenRouterModelDisplayName } from "./builtinOpenRouter";
import { getModelDisplayName } from "./modelDisplayNames";

const PROVIDER_NAMES: Record<string, string> = {
  deepseek: "DeepSeek",
  kimi: "Kimi",
  kimi_code: "Kimi Code",
  "kimi-code": "Kimi Code",
  minimax: "MiniMax",
  mimo: "MiMo",
  openrouter: "OpenRouter",
};

const OLLAMA_MODEL_NAMES: Record<string, string> = {
  "mimo-v2.6:9b": "MiMo-V2.6-9B",
  "qwen2.5-coder:14b": "Qwen2.5-Coder 14B",
};

function formatModelName(modelId: string): string {
  const aliases: Record<string, string> = {
    "kimi-k3": "Kimi K3",
    "kimi-for-coding": "Kimi for Coding",
    "mimo-v2.6-pro": "MiMo-V2.6-Pro",
    "mimo-v2.6-flash": "MiMo-V2.6-Flash",
  };
  const normalized = modelId.trim();
  if (aliases[normalized.toLowerCase()]) return aliases[normalized.toLowerCase()];

  return normalized
    .split(/[-_:\s]+/)
    .map((part) => {
      const token = part.toLowerCase();
      if (token === "gpt") return "GPT";
      if (token === "mimo") return "MiMo";
      if (token === "deepseek") return "DeepSeek";
      if (token === "qwen") return "Qwen";
      if (token === "for") return "for";
      if (/^\d+[a-z]$/.test(token)) return token.slice(0, -1) + token.slice(-1).toUpperCase();
      if (/^v\d/.test(token)) return `V${token.slice(1)}`;
      return token ? token[0].toUpperCase() + token.slice(1) : part;
    })
    .join(" ");
}

function providerName(providerId: string): string {
  return PROVIDER_NAMES[providerId.toLowerCase()]
    ?? providerId.replace(/[_-]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

export function getOrchestratorProfileProviderLabel(profile: OrchestratorProfile): string {
  if (profile.adapter === "provider") return providerName(profile.providerId ?? "Provider");
  if (profile.adapter === "ollama") return "Ollama";
  if (profile.adapter === "cli") return "Codex CLI";
  return "MCP";
}

/** Canonical, read-only profile label derived from adapter/provider/model settings. */
export function getOrchestratorProfileDisplayName(profile: OrchestratorProfile): string {
  if (profile.adapter === "provider") {
    const providerId = profile.providerId ?? "provider";
    const provider = providerName(providerId);
    const model = profile.model?.trim();
    if (providerId.toLowerCase() === "openrouter") {
      return model ? `OpenRouter ${getOpenRouterModelDisplayName(model)}` : "OpenRouter Profile";
    }
    if (!model) return `${provider} Profile (Direct API)`;

    const canonicalModel = getModelDisplayName(model, providerId);
    const modelLabel = canonicalModel !== model ? canonicalModel : formatModelName(model);
    const name = modelLabel.toLowerCase().startsWith(provider.toLowerCase())
      ? modelLabel
      : `${provider} ${modelLabel}`;
    return `${name} (Direct API)`;
  }

  if (profile.adapter === "ollama") {
    const model = profile.ollamaModel?.trim();
    if (!model) return "Ollama (Local)";
    return `Ollama ${OLLAMA_MODEL_NAMES[model.toLowerCase()] ?? formatModelName(model)} (Local)`;
  }

  if (profile.adapter === "cli") return "Codex CLI (Local Agent)";

  const mcpName = profile.mcpTool?.trim() || profile.externalMcpServer?.trim();
  return mcpName ? `${mcpName} (MCP)` : "MCP Tool";
}
