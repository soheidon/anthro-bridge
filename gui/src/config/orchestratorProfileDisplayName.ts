import type { OrchestratorProfile } from "../types/orchestrator";
import { MODEL_CATALOG } from "../modelCapabilities";

export function formatThinkingSuffix(
  thinkingMode?: string,
  reasoningEffort?: string,
): string {
  if (thinkingMode !== "thinking" && thinkingMode !== "thinking_only") {
    return "";
  }
  const reasonLabel = reasoningEffort === "max" ? "Max"
    : reasoningEffort === "xhigh" ? "XHigh"
    : reasoningEffort === "high" ? "High"
    : reasoningEffort === "medium" ? "Medium"
    : reasoningEffort === "low" ? "Low"
    : reasoningEffort ? reasoningEffort : null;

  if (reasonLabel) {
    return ` + thinking: ${reasonLabel}`;
  }
  return " + thinking";
}

function providerName(providerId: string): string {
  const normalized = providerId.toLowerCase().replace(/_/g, "-");
  return MODEL_CATALOG.providers.find((provider) => provider.id === normalized)?.label
    ?? providerId.replace(/[_-]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

export function getOrchestratorProfileProviderLabel(profile: OrchestratorProfile): string {
  if (profile.adapter === "provider") return providerName(profile.providerId ?? "Provider");
  if (profile.adapter === "ollama") return "Ollama";
  if (profile.adapter === "cli") return "Codex CLI";
  if (profile.adapter === "antigravity") return "Google Antigravity";
  return "MCP";
}

/** Canonical, read-only profile configuration label (e.g. `deepseek-flash + thinking: High`). */
export function getOrchestratorProfileDisplayName(profile: OrchestratorProfile): string {
  if (profile.adapter === "provider") {
    const model = profile.model?.trim() || "model";
    return `${model}${formatThinkingSuffix(profile.thinkingMode, profile.reasoningEffort)}`;
  }

  if (profile.adapter === "ollama") {
    const model = profile.ollamaModel?.trim() || "ollama";
    return `${model}${formatThinkingSuffix(profile.thinkingMode, profile.reasoningEffort)}`;
  }

  if (profile.adapter === "cli") {
    const executable = profile.executable?.trim() || "codex";
    return `${executable}-cli`;
  }

  if (profile.adapter === "antigravity") {
    return "Google Antigravity Harness (MCP Mailbox)";
  }

  const mcpName = profile.mcpTool?.trim() || profile.externalMcpServer?.trim();
  return mcpName ? `mcp: ${mcpName}` : "mcp-tool";
}
