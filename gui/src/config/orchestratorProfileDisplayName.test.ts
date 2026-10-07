import { describe, expect, it } from "vitest";
import type { OrchestratorProfile } from "../types/orchestrator";
import { formatThinkingSuffix, getOrchestratorProfileDisplayName } from "./orchestratorProfileDisplayName";

describe("formatThinkingSuffix", () => {
  it("returns an empty string when thinking is disabled or not active", () => {
    expect(formatThinkingSuffix("normal")).toBe("");
    expect(formatThinkingSuffix("disabled")).toBe("");
    expect(formatThinkingSuffix(undefined)).toBe("");
    expect(formatThinkingSuffix("normal", "high")).toBe("");
  });

  it("formats thinking mode and effort with exact Gateway casing rules", () => {
    expect(formatThinkingSuffix("thinking")).toBe(" + thinking");
    expect(formatThinkingSuffix("thinking", "")).toBe(" + thinking");
    expect(formatThinkingSuffix("thinking", "max")).toBe(" + thinking: Max");
    expect(formatThinkingSuffix("thinking", "xhigh")).toBe(" + thinking: XHigh");
    expect(formatThinkingSuffix("thinking", "high")).toBe(" + thinking: High");
    expect(formatThinkingSuffix("thinking", "medium")).toBe(" + thinking: Medium");
    expect(formatThinkingSuffix("thinking", "low")).toBe(" + thinking: Low");
    expect(formatThinkingSuffix("thinking", "custom-effort")).toBe(" + thinking: custom-effort");
    expect(formatThinkingSuffix("thinking_only", "high")).toBe(" + thinking: High");
    expect(formatThinkingSuffix("thinking_only")).toBe(" + thinking");
  });
});

describe("getOrchestratorProfileDisplayName", () => {
  it.each([
    [
      { id: "deepseek-max", adapter: "provider", providerId: "deepseek", model: "deepseek-flash", thinkingMode: "thinking", reasoningEffort: "max" },
      "deepseek-flash + thinking: Max",
    ],
    [
      { id: "deepseek-high", adapter: "provider", providerId: "deepseek", model: "deepseek-v4.1-flash", thinkingMode: "thinking", reasoningEffort: "high" },
      "deepseek-v4.1-flash + thinking: High",
    ],
    [
      { id: "deepseek-normal", adapter: "provider", providerId: "deepseek", model: "deepseek-chat", thinkingMode: "normal" },
      "deepseek-chat",
    ],
    [
      { id: "minimax", adapter: "provider", providerId: "minimax", model: "minimax-m3", thinkingMode: "thinking" },
      "minimax-m3 + thinking",
    ],
    [
      { id: "kimi", adapter: "provider", providerId: "kimi", model: "kimi-k3", thinkingMode: "thinking" },
      "kimi-k3 + thinking",
    ],
    [
      { id: "mimo", adapter: "provider", providerId: "mimo", model: "mimo-v2.6-pro", thinkingMode: "thinking" },
      "mimo-v2.6-pro + thinking",
    ],
    [
      { id: "openrouter", adapter: "provider", providerId: "openrouter", model: "openai/gpt-5.6-sol" },
      "openai/gpt-5.6-sol",
    ],
    [
      { id: "ollama", adapter: "ollama", ollamaModel: "mimo-v2.6:9b" },
      "mimo-v2.6:9b",
    ],
    [{ id: "cli", adapter: "cli" }, "codex-cli"],
    [{ id: "antigravity", adapter: "antigravity" }, "Google Antigravity Harness (MCP Mailbox)"],
    [{ id: "mcp", adapter: "mcp", mcpTool: "plan" }, "mcp: plan"],
  ] satisfies Array<[Partial<OrchestratorProfile>, string]>) (
      "derives %s as canonical configuration name",
    (profile, expected) => {
      expect(getOrchestratorProfileDisplayName({
        displayName: "Stale legacy label",
        capabilities: [],
        ...profile,
        id: profile.id ?? "test",
      } as OrchestratorProfile)).toBe(expected);
    },
  );
});
