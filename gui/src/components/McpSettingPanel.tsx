import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { GatewayConfig, McpConfig, McpTargetConfig } from "../types";
import { getMcpTargetKey } from "../types";
import { MODEL_CAPABILITIES, getProviderModels, isKnownModel } from "../modelCapabilities";
import { getVisibleOpenRouterProfiles } from "../dashboardTiles";

interface McpSettingPanelProps {
  config: GatewayConfig | null;
  refreshConfig: () => Promise<void>;
}

const MCP_PROVIDERS = [
  { id: "deepseek", name: "DeepSeek" },
  { id: "minimax", name: "MiniMax" },
  { id: "kimi", name: "Kimi" },
  { id: "mimo", name: "MiMo" },
  { id: "openrouter", name: "OpenRouter" },
];

const COL_STYLE: React.CSSProperties = {
  padding: "6px 4px",
  fontSize: 12,
  color: "#1f2937",
  whiteSpace: "nowrap",
};

export default function McpSettingPanel({ config, refreshConfig }: McpSettingPanelProps) {
  const { t } = useTranslation();
  const [mcpConfig, setMcpConfig] = useState<McpConfig>({});
  const [expandedProviders, setExpandedProviders] = useState<Set<string>>(new Set(["deepseek"]));
  const [selectedOrProfileId, setSelectedOrProfileId] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [savedMessage, setSavedMessage] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshMcp = useCallback(async () => {
    try {
      const cfg = await invoke<McpConfig>("get_mcp_config");
      setMcpConfig(cfg);
    } catch (e: unknown) {
      console.error("Failed to load MCP config:", e);
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refreshMcp();
  }, [refreshMcp]);

  // Active top-level target key
  const activeProviderId = mcpConfig.provider ?? config?.active_provider ?? "deepseek";
  const activeProfileId =
    activeProviderId === "openrouter"
      ? mcpConfig.profile_id ?? config?.active_openrouter_profile_id ?? ""
      : null;
  const activeTargetKey = getMcpTargetKey(activeProviderId, activeProfileId);

  // OpenRouter visible profiles
  const orProfiles = useMemo(() => {
    const orProvider = config?.providers?.["openrouter"];
    if (!orProvider?.profiles) return [];
    return getVisibleOpenRouterProfiles(orProvider.profiles) ?? orProvider.profiles;
  }, [config]);

  // Set initial selected OpenRouter profile if empty
  useEffect(() => {
    if (!selectedOrProfileId && orProfiles.length > 0) {
      const initial = activeProfileId || orProfiles[0].id;
      setSelectedOrProfileId(initial);
    }
  }, [orProfiles, activeProfileId, selectedOrProfileId]);

  /**
   * Helper: Resolve effective setting for a targetKey following user's resolution order:
   * 1. targets[targetKey]
   * 2. If targetKey == activeTargetKey, existing top-level mcpConfig
   * 3. Gateway config defaults
   */
  const resolveTargetSetting = useCallback(
    (targetKey: string, providerId: string, profileId?: string | null): {
      model: string;
      thinking_mode: string;
      reasoning_effort: string;
    } => {
      const saved = mcpConfig.targets?.[targetKey];
      const isActive = targetKey === activeTargetKey;

      // 1. Saved targets entry
      let model = saved?.model;
      let thinking_mode = saved?.thinking_mode;
      let reasoning_effort = saved?.reasoning_effort;

      // 2. Active top-level mirror fallback
      if (isActive) {
        model = model || mcpConfig.model;
        thinking_mode = thinking_mode || mcpConfig.thinking_mode;
        reasoning_effort = reasoning_effort || mcpConfig.reasoning_effort;
      }

      // 3. Provider/Profile defaults from config
      if (providerId === "openrouter") {
        const prof = orProfiles.find((p) => p.id === (profileId || selectedOrProfileId)) ?? orProfiles[0];
        const defaultOpus = prof?.models?.["claude-opus-5"];
        model = model || defaultOpus?.upstream_model || "deepseek/deepseek-r1";
        thinking_mode = thinking_mode || defaultOpus?.thinking_mode || "thinking";
        reasoning_effort = reasoning_effort || defaultOpus?.reasoning_effort || "high";
      } else {
        const p = config?.providers?.[providerId];
        const defaultOpus = p?.models?.["claude-opus-5"];
        model = model || defaultOpus?.upstream_model || p?.default_model || "deepseek-v4-pro";
        thinking_mode = thinking_mode || defaultOpus?.thinking_mode || "thinking";
        reasoning_effort = reasoning_effort || defaultOpus?.reasoning_effort || (providerId === "kimi" ? "max" : "high");
      }

      return { model, thinking_mode, reasoning_effort };
    },
    [mcpConfig, activeTargetKey, orProfiles, selectedOrProfileId, config]
  );

  // Save changes to targets[targetKey] and mirror to top-level if targetKey is active
  const handleSaveTarget = async (
    targetKey: string,
    updatedTarget: McpTargetConfig
  ) => {
    setSaving(true);
    setError(null);
    try {
      const nextTargets = {
        ...(mcpConfig.targets || {}),
        [targetKey]: {
          ...(mcpConfig.targets?.[targetKey] || {}),
          ...updatedTarget,
        },
      };

      const nextMcp: McpConfig = {
        ...mcpConfig,
        targets: nextTargets,
      };

      // If editing currently active target, mirror to top-level
      if (targetKey === activeTargetKey) {
        if (updatedTarget.model !== undefined) nextMcp.model = updatedTarget.model;
        if (updatedTarget.thinking_mode !== undefined) nextMcp.thinking_mode = updatedTarget.thinking_mode;
        if (updatedTarget.reasoning_effort !== undefined) nextMcp.reasoning_effort = updatedTarget.reasoning_effort;
      }

      await invoke("update_mcp_config", { mcp: nextMcp });
      setMcpConfig(nextMcp);
      await refreshConfig();
      setSavedMessage(true);
      setTimeout(() => setSavedMessage(false), 2000);
    } catch (err) {
      console.error("Save MCP config failed:", err);
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="settings-tile" style={{ marginBottom: 16 }}>
      <h3>{t("mcp.destinationDetailedHeader")}</h3>

      {/* Provider rows enclosed in a rounded bordered card */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          border: "1px solid #e5e7eb",
          borderRadius: 6,
          overflow: "hidden",
        }}
      >
        {MCP_PROVIDERS.map((provider, idx) => {
          const isExpanded = expandedProviders.has(provider.id);
          const providerId = provider.id;
          const isLast = idx === MCP_PROVIDERS.length - 1;

          const toggleExpand = () => {
            setExpandedProviders((prev) => {
              const next = new Set(prev);
              if (next.has(provider.id)) {
                next.delete(provider.id);
              } else {
                next.add(provider.id);
              }
              return next;
            });
          };

          // For summary calculation
          const targetKey =
            providerId === "openrouter"
              ? getMcpTargetKey("openrouter", selectedOrProfileId || orProfiles[0]?.id)
              : getMcpTargetKey(providerId);

          const setting = resolveTargetSetting(
            targetKey,
            providerId,
            providerId === "openrouter" ? selectedOrProfileId || orProfiles[0]?.id : undefined
          );

          // Get available models
          const availableModels: string[] = [];
          if (providerId === "openrouter") {
            const prof = orProfiles.find((p) => p.id === (selectedOrProfileId || orProfiles[0]?.id));
            if (prof?.models) {
              const list = Object.values(prof.models).map((m) => m.upstream_model);
              availableModels.push(...Array.from(new Set(list.filter(Boolean))));
            }
            if (availableModels.length === 0) {
              availableModels.push("deepseek/deepseek-r1", "google/gemini-3.7-flash", "anthropic/claude-3.7-sonnet");
            }
          } else {
            const list = getProviderModels(providerId);
            const p = config?.providers?.[providerId];
            if (p?.models) {
              for (const m of Object.values(p.models)) {
                if (m.upstream_model && !list.includes(m.upstream_model)) {
                  list.push(m.upstream_model);
                }
              }
            }
            availableModels.push(...list);
          }

          const caps = MODEL_CAPABILITIES[setting.model];
          const policy = caps?.thinkingModePolicy ?? "toggleable";
          const supportsReasoningEffort = caps?.supportsReasoningEffort || !!caps?.forcedReasoningEffort;
          const forcedOptions = caps?.forcedThinkingOptions;

          // Summary string for header
          const summaryText =
            setting.thinking_mode === "thinking"
              ? `${setting.model} (${setting.reasoning_effort})`
              : `${setting.model} (Normal)`;

          return (
            <div key={provider.id} style={{ borderBottom: isLast ? "none" : "1px solid #e5e7eb" }}>
              {/* Clickable header row */}
              <div
                role="button"
                tabIndex={0}
                aria-expanded={isExpanded}
                onClick={toggleExpand}
                style={{
                  display: "flex",
                  alignItems: "center",
                  background: "#ffffff",
                  cursor: "pointer",
                  transition: "background 0.1s",
                  padding: 0,
                }}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLElement).style.background = "#f8f9fa";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.background = "#ffffff";
                }}
              >
                <div style={{ ...COL_STYLE, fontSize: 14, color: "#6b7280", userSelect: "none", padding: "6px 4px 6px 8px", minWidth: 28 }}>
                  {isExpanded ? "▾" : "▸"}
                </div>

                <div style={{ ...COL_STYLE, fontWeight: 600, minWidth: 130, fontSize: 13, padding: "6px 4px" }}>
                  {provider.name}
                </div>

                <div style={{ ...COL_STYLE, fontFamily: "var(--font-mono)", fontSize: 11, color: "var(--text-muted)", flex: 1, padding: "6px 8px" }}>
                  {summaryText}
                </div>
              </div>

              {/* Expandable edit area */}
              {isExpanded && (
                <div
                  style={{
                    background: "#fafafa",
                    borderTop: "1px solid #e5e7eb",
                    padding: "12px 16px 14px 38px",
                    display: "flex",
                    flexDirection: "column",
                    gap: 10,
                  }}
                  onClick={(e) => e.stopPropagation()}
                >
                  {/* OpenRouter Profile Selector */}
                  {providerId === "openrouter" && (
                    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                      <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
                        {t("mcp.profileLabel")}:
                      </span>
                      <select
                        style={{
                          padding: "4px 8px",
                          fontSize: 11,
                          fontFamily: "var(--font-mono)",
                          background: "#fff",
                          color: "#1f2937",
                          border: "1px solid #d0d7de",
                          borderRadius: 4,
                          outline: "none",
                          minWidth: 220,
                        }}
                        value={selectedOrProfileId || orProfiles[0]?.id}
                        onChange={(e) => setSelectedOrProfileId(e.target.value)}
                        disabled={saving}
                      >
                        {orProfiles.map((prof) => (
                          <option key={prof.id} value={prof.id}>
                            {prof.display_name || prof.id}
                          </option>
                        ))}
                      </select>
                    </div>
                  )}

                  {/* Model, Thinking Mode, and Reasoning Effort in a single row */}
                  <div style={{ display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap" }}>
                    {/* Model Selector */}
                    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", whiteSpace: "nowrap" }}>
                        {t("mcp.modelLabel")}:
                      </span>
                      <select
                        style={{
                          padding: "4px 8px",
                          fontSize: 11,
                          fontFamily: "var(--font-mono)",
                          background: "#fff",
                          color: "#1f2937",
                          border: "1px solid #d0d7de",
                          borderRadius: 4,
                          outline: "none",
                          minWidth: 180,
                        }}
                        value={setting.model}
                        onChange={(e) => {
                          void handleSaveTarget(targetKey, { model: e.target.value });
                        }}
                        disabled={saving}
                      >
                        {availableModels.map((m) => (
                          <option key={m} value={m}>
                            {m}
                          </option>
                        ))}
                      </select>
                    </div>

                    {/* Thinking Mode */}
                    {policy !== "none" && (
                      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                        <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", whiteSpace: "nowrap" }}>
                          {t("mcp.thinkingLabel")}:
                        </span>
                        <select
                          style={{
                            padding: "4px 8px",
                            fontSize: 11,
                            fontFamily: "var(--font-mono)",
                            background: "#fff",
                            color: "#1f2937",
                            border: "1px solid #d0d7de",
                            borderRadius: 4,
                            outline: "none",
                            minWidth: 100,
                          }}
                          value={setting.thinking_mode}
                          onChange={(e) => {
                            void handleSaveTarget(targetKey, { thinking_mode: e.target.value });
                          }}
                          disabled={saving || policy === "forced" || policy === "thinking_only"}
                        >
                          <option value="thinking">{t("apiKeyPanel.thinkingModeOn")}</option>
                          <option value="normal">{t("apiKeyPanel.normalMode")}</option>
                        </select>
                      </div>
                    )}

                    {/* Reasoning Effort */}
                    {setting.thinking_mode === "thinking" && supportsReasoningEffort && (
                      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                        <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", whiteSpace: "nowrap" }}>
                          {t("mcp.effortLabel")}:
                        </span>
                        <select
                          style={{
                            padding: "4px 8px",
                            fontSize: 11,
                            fontFamily: "var(--font-mono)",
                            background: "#fff",
                            color: "#1f2937",
                            border: "1px solid #d0d7de",
                            borderRadius: 4,
                            outline: "none",
                            minWidth: 80,
                          }}
                          value={setting.reasoning_effort}
                          onChange={(e) => {
                            void handleSaveTarget(targetKey, { reasoning_effort: e.target.value });
                          }}
                          disabled={saving}
                        >
                          {forcedOptions ? (
                            forcedOptions.map((opt) => (
                              <option key={opt} value={opt}>
                                {opt === "max" ? t("apiKeyPanel.reasoningEffortMaxFixed") : opt === "high" ? t("apiKeyPanel.reasoningEffortHigh") : t("apiKeyPanel.reasoningEffortLow")}
                              </option>
                            ))
                          ) : (
                            <>
                              <option value="low">{t("apiKeyPanel.reasoningEffortLow")}</option>
                              <option value="medium">{t("apiKeyPanel.reasoningEffortMedium")}</option>
                              <option value="high">{t("apiKeyPanel.reasoningEffortHigh")}</option>
                              <option value="max">{t("apiKeyPanel.reasoningEffortMaxFixed")}</option>
                            </>
                          )}
                        </select>
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}


