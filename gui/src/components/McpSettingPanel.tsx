import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { GatewayConfig, McpConfig, McpTargetConfig, AntigravityMcpInfo, AntigravityCommandsInfo } from "../types";
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
  const [expandedProviders, setExpandedProviders] = useState<Set<string>>(new Set());
  const [selectedOrProfileId, setSelectedOrProfileId] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [savedMessage, setSavedMessage] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // ── Antigravity Integration State ──
  const [agInfo, setAgInfo] = useState<AntigravityMcpInfo | null>(null);
  const [selectedExePath, setSelectedExePath] = useState<string>("");
  const [agLoading, setAgLoading] = useState(false);
  const [agSavedMessage, setAgSavedMessage] = useState(false);
  const [agError, setAgError] = useState<string | null>(null);

  // ── Antigravity Commands State (Global Skills) ──
  const [commandsInfo, setCommandsInfo] = useState<AntigravityCommandsInfo | null>(null);
  const [commandsLoading, setCommandsLoading] = useState(false);
  const [commandsSavedMessage, setCommandsSavedMessage] = useState(false);
  const [commandsError, setCommandsError] = useState<string | null>(null);

  const refreshAgStatus = useCallback(async () => {
    try {
      const info = await invoke<AntigravityMcpInfo>("get_antigravity_mcp_status");
      setAgInfo(info);
      setSelectedExePath(info.registered_command ?? "");
      setAgError(null);
    } catch (e: unknown) {
      console.error("Failed to load Antigravity MCP status:", e);
      setAgError(String(e));
    }
  }, []);

  const refreshAgCommandsStatus = useCallback(async () => {
    try {
      const info = await invoke<AntigravityCommandsInfo>("get_antigravity_commands_status");
      setCommandsInfo(info);
      setCommandsError(null);
    } catch (e: unknown) {
      console.error("Failed to load Antigravity Commands status:", e);
      setCommandsError(String(e));
    }
  }, []);

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
    void refreshAgStatus();
    void refreshAgCommandsStatus();
  }, [refreshMcp, refreshAgStatus, refreshAgCommandsStatus]);

  const handleOpenAgFolder = useCallback(async () => {
    try {
      await invoke("open_antigravity_mcp_config_folder");
    } catch (e: unknown) {
      console.error("Failed to open Antigravity config folder:", e);
      setAgError(String(e));
    }
  }, []);

  const handleSelectExe = useCallback(async () => {
    try {
      const chosen = await invoke<string | null>("select_executable_dialog");
      if (chosen && chosen.trim().length > 0) {
        setSelectedExePath(chosen.trim());
        setAgError(null);
      }
    } catch (e: unknown) {
      console.error("Failed to select executable:", e);
      setAgError(String(e));
    }
  }, []);

  const handleConfigureAg = useCallback(async () => {
    if (!selectedExePath.trim()) {
      return;
    }
    setAgLoading(true);
    setAgError(null);
    setAgSavedMessage(false);
    try {
      const updated = await invoke<AntigravityMcpInfo>("configure_antigravity_mcp", {
        exePath: selectedExePath.trim(),
      });
      setAgInfo(updated);
      setSelectedExePath(updated.registered_command ?? "");
      setAgSavedMessage(true);
      setTimeout(() => setAgSavedMessage(false), 2500);
    } catch (e: unknown) {
      console.error("Failed to configure Antigravity MCP:", e);
      setAgError(String(e));
    } finally {
      setAgLoading(false);
    }
  }, [selectedExePath]);

  const handleRemoveAg = useCallback(async () => {
    setAgLoading(true);
    setAgError(null);
    setAgSavedMessage(false);
    try {
      const updated = await invoke<AntigravityMcpInfo>("remove_antigravity_mcp");
      setAgInfo(updated);
      setSelectedExePath(updated.registered_command ?? "");
      setAgSavedMessage(true);
      setTimeout(() => setAgSavedMessage(false), 2500);
    } catch (e: unknown) {
      console.error("Failed to remove Antigravity MCP configuration:", e);
      setAgError(String(e));
    } finally {
      setAgLoading(false);
    }
  }, []);

  const handleOpenSkillsFolder = useCallback(async () => {
    try {
      await invoke("open_antigravity_skills_folder");
    } catch (e: unknown) {
      console.error("Failed to open Antigravity skills folder:", e);
      setCommandsError(String(e));
    }
  }, []);

  const handleInstallCommand = useCallback(async (commandName: string) => {
    setCommandsLoading(true);
    setCommandsError(null);
    setCommandsSavedMessage(false);
    try {
      const updated = await invoke<AntigravityCommandsInfo>("install_antigravity_command", { name: commandName });
      setCommandsInfo(updated);
      setCommandsSavedMessage(true);
      setTimeout(() => setCommandsSavedMessage(false), 2500);
    } catch (e: unknown) {
      console.error(`Failed to install ${commandName} command:`, e);
      setCommandsError(String(e));
    } finally {
      setCommandsLoading(false);
    }
  }, []);

  const handleRemoveCommand = useCallback(async (commandName: string) => {
    setCommandsLoading(true);
    setCommandsError(null);
    setCommandsSavedMessage(false);
    try {
      const updated = await invoke<AntigravityCommandsInfo>("remove_antigravity_command", { name: commandName });
      setCommandsInfo(updated);
      setCommandsSavedMessage(true);
      setTimeout(() => setCommandsSavedMessage(false), 2500);
    } catch (e: unknown) {
      console.error(`Failed to remove ${commandName} command:`, e);
      setCommandsError(String(e));
    } finally {
      setCommandsLoading(false);
    }
  }, []);

  const handleInstallAllCommands = useCallback(async () => {
    setCommandsLoading(true);
    setCommandsError(null);
    setCommandsSavedMessage(false);
    try {
      const updated = await invoke<AntigravityCommandsInfo>("install_all_antigravity_commands");
      setCommandsInfo(updated);
      setCommandsSavedMessage(true);
      setTimeout(() => setCommandsSavedMessage(false), 2500);
    } catch (e: unknown) {
      console.error("Failed to install all Antigravity commands:", e);
      setCommandsError(String(e));
    } finally {
      setCommandsLoading(false);
    }
  }, []);

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
    <>
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

    {/* ── Antigravity Integration Section ── */}
    <div className="settings-tile">
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 6 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            flexWrap: "wrap",
          }}
        >
          <h3 style={{ margin: 0, fontSize: 14, fontWeight: 700, color: "var(--text-primary)" }}>
            {t("antigravity.header")}
          </h3>
            <div>
              {agInfo?.status === "invalid" ? (
                <span style={{ fontSize: 12, color: "#cf222e", fontWeight: 600 }}>
                  ⚠ {t("antigravity.statusInvalid")}
                </span>
              ) : selectedExePath.length > 0 && selectedExePath !== (agInfo?.registered_command ?? "") ? (
                <span style={{ fontSize: 12, color: "#9a6700", fontWeight: 600 }}>
                  ⚠ {t("antigravity.statusOutdated")}
                </span>
              ) : agInfo?.status === "configured" && agInfo.registered_command ? (
                <span style={{ display: "inline-flex", alignItems: "center", gap: 4, fontSize: 12, color: "#1a7f37", fontWeight: 600 }}>
                  <span>✓</span> {t("antigravity.statusConfigured")}
                </span>
              ) : (
                <span style={{ fontSize: 12, color: "#656d76", fontWeight: 500 }}>
                  {t("antigravity.statusNotConfigured")}
                </span>
              )}
              {agLoading && <span style={{ fontSize: 12, color: "#656d76", marginLeft: 6 }}>...</span>}
            </div>
          </div>
          {agSavedMessage && (
            <span style={{ fontSize: 12, color: "#1a7f37", fontWeight: 500 }}>
              ✓ {t("antigravity.savedMessage")}
            </span>
          )}
        </div>

        <p style={{ margin: "0 0 12px 0", fontSize: 12, color: "#656d76" }}>
          {t("antigravity.desc")}
        </p>

            {agError && (
              <div style={{ margin: "0 0 12px 0", padding: "8px 12px", background: "#ffebe9", border: "1px solid #ff8182", borderRadius: 6, fontSize: 12, color: "#cf222e" }}>
                {agError}
              </div>
            )}

            <div style={{ background: "#f6f8fa", border: "1px solid #d0d7de", borderRadius: 6, padding: "12px 14px", marginBottom: 14 }}>
              {/* Block 1 Header: MCP Server Registration */}
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span style={{ fontSize: 12, fontWeight: 700, color: "#24292f" }}>
                    {t("antigravity.mcpSectionHeader")}
                  </span>
                  <div>
                    {agInfo?.status === "invalid" ? (
                      <span style={{ fontSize: 11, color: "#cf222e", fontWeight: 600 }}>
                        ⚠ {t("antigravity.statusInvalid")}
                      </span>
                    ) : selectedExePath.length > 0 && selectedExePath !== (agInfo?.registered_command ?? "") ? (
                      <span style={{ fontSize: 11, color: "#9a6700", fontWeight: 600 }}>
                        ⚠ {t("antigravity.statusOutdated")}
                      </span>
                    ) : agInfo?.status === "configured" && agInfo.registered_command ? (
                      <span style={{ display: "inline-flex", alignItems: "center", gap: 3, fontSize: 11, color: "#1a7f37", fontWeight: 600 }}>
                        <span>✓</span> {t("antigravity.statusConfigured")}
                      </span>
                    ) : (
                      <span style={{ fontSize: 11, color: "#656d76", fontWeight: 500 }}>
                        {t("antigravity.statusNotConfigured")}
                      </span>
                    )}
                  </div>
                </div>
              </div>

              {/* Row 1: Antigravity Config File & Actions */}
              <div style={{ marginBottom: 14 }}>
                <div style={{ fontSize: 11, fontWeight: 600, color: "#57606a", marginBottom: 4 }}>
                  {t("antigravity.configPathLabel")}:
                </div>
                <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
                  <input
                    type="text"
                    readOnly
                    value={agInfo?.config_path || "~\\.gemini\\config\\mcp_config.json"}
                    style={{
                      flex: "1 1 240px",
                      fontFamily: "var(--font-mono)",
                      fontSize: 11,
                      padding: "5px 8px",
                      background: "#fff",
                      color: "#24292f",
                      border: "1px solid #d0d7de",
                      borderRadius: 4,
                      outline: "none",
                      boxSizing: "border-box",
                    }}
                  />
                  <button
                    className="btn"
                    style={{ fontSize: 12, padding: "5px 12px" }}
                    onClick={handleOpenAgFolder}
                    disabled={agLoading}
                  >
                    {t("antigravity.btnOpenFolder")}
                  </button>
                  <button
                    className="btn btn-primary"
                    style={{ fontSize: 12, padding: "5px 12px" }}
                    onClick={handleConfigureAg}
                    disabled={
                      agLoading ||
                      agInfo?.status === "invalid" ||
                      !selectedExePath.trim() ||
                      selectedExePath === (agInfo?.registered_command ?? "")
                    }
                  >
                    {t("antigravity.btnUpdate")}
                  </button>
                  {agInfo?.status === "configured" && (
                    <button
                      className="btn btn-danger"
                      style={{ fontSize: 12, padding: "5px 12px" }}
                      onClick={handleRemoveAg}
                      disabled={agLoading}
                    >
                      {t("antigravity.btnRemove")}
                    </button>
                  )}
                </div>
              </div>

              {/* Row 2: Selected / Target Anthro Bridge Executable */}
              <div>
                <div style={{ fontSize: 11, fontWeight: 600, color: "#57606a", marginBottom: 4 }}>
                  {t("antigravity.targetExeLabel")}:
                </div>
                <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
                  <input
                    type="text"
                    readOnly
                    value={selectedExePath}
                    placeholder={t("antigravity.placeholderNotSelected")}
                    style={{
                      flex: "1 1 240px",
                      fontFamily: "var(--font-mono)",
                      fontSize: 11,
                      padding: "5px 8px",
                      background: "#fff",
                      color: selectedExePath ? "#24292f" : "#656d76",
                      border: "1px solid #d0d7de",
                      borderRadius: 4,
                      outline: "none",
                      boxSizing: "border-box",
                    }}
                  />
                  <button
                    className="btn"
                    style={{ fontSize: 12, padding: "5px 12px" }}
                    onClick={handleSelectExe}
                    disabled={agLoading}
                  >
                    {t("antigravity.btnChangeExe")}
                  </button>
                </div>
              </div>
            </div>

            {/* Block 2: Antigravity Commands (/anthro-plan & /anthro-revise) */}
            <div style={{ background: "#f6f8fa", border: "1px solid #d0d7de", borderRadius: 6, padding: "12px 14px", marginBottom: 12 }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 6, flexWrap: "wrap", gap: 8 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                  <span style={{ fontSize: 12, fontWeight: 700, color: "#24292f" }}>
                    {t("antigravity.commandsSectionHeader")}
                  </span>
                  {commandsLoading && <span style={{ fontSize: 11, color: "#656d76" }}>...</span>}
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  {commandsSavedMessage && (
                    <span style={{ fontSize: 12, color: "#1a7f37", fontWeight: 500 }}>
                      ✓ {t("antigravity.savedMessage")}
                    </span>
                  )}
                  <button
                    className="btn"
                    style={{ fontSize: 11, padding: "4px 10px" }}
                    onClick={handleOpenSkillsFolder}
                    disabled={commandsLoading}
                  >
                    {t("antigravity.btnOpenSkillsFolder")}
                  </button>
                  {((commandsInfo?.plan_command.status !== "installed") ||
                    (commandsInfo?.revise_command.status !== "installed")) && (
                    <button
                      className="btn btn-primary"
                      style={{ fontSize: 11, padding: "4px 10px" }}
                      onClick={handleInstallAllCommands}
                      disabled={commandsLoading}
                    >
                      {t("antigravity.btnInstallAll")}
                    </button>
                  )}
                </div>
              </div>

              <p style={{ margin: "0 0 10px 0", fontSize: 12, color: "#656d76" }}>
                {t("antigravity.commandsDesc")}
              </p>

              {agInfo?.status !== "configured" && (
                <div style={{ margin: "0 0 10px 0", padding: "6px 10px", background: "#fff8c5", border: "1px solid #d4a72c", borderRadius: 4, fontSize: 11, color: "#6e4a00" }}>
                  {t("antigravity.commandsMcpWarning")}
                </div>
              )}

              {commandsError && (
                <div style={{ margin: "0 0 10px 0", padding: "6px 10px", background: "#ffebe9", border: "1px solid #ff8182", borderRadius: 4, fontSize: 11, color: "#cf222e" }}>
                  {commandsError}
                </div>
              )}

              {/* Commands List Cards */}
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                {[
                  {
                    item: commandsInfo?.plan_command,
                    titleKey: "antigravity.commandPlanTitle" as const,
                    descKey: "antigravity.commandPlanDesc" as const,
                    fallbackName: "anthro-plan",
                    fallbackCmd: "/anthro-plan",
                  },
                  {
                    item: commandsInfo?.revise_command,
                    titleKey: "antigravity.commandReviseTitle" as const,
                    descKey: "antigravity.commandReviseDesc" as const,
                    fallbackName: "anthro-revise",
                    fallbackCmd: "/anthro-revise",
                  },
                ].map(({ item, titleKey, descKey, fallbackName, fallbackCmd }) => {
                  const cmdName = item?.name ?? fallbackName;
                  const slashCmd = item?.slash_command ?? fallbackCmd;
                  const status = item?.status ?? "not_installed";

                  return (
                    <div
                      key={cmdName}
                      style={{
                        background: "#fff",
                        border: "1px solid #e1e4e8",
                        borderRadius: 6,
                        padding: "8px 12px",
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                        flexWrap: "wrap",
                        gap: 8,
                      }}
                    >
                      <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                          <span style={{ fontFamily: "var(--font-mono)", fontSize: 12, fontWeight: 700, color: "#1f2937" }}>
                            {t(titleKey)}
                          </span>
                          <div>
                            {status === "invalid" ? (
                              <span style={{ fontSize: 11, color: "#cf222e", fontWeight: 600 }}>
                                ⚠ {t("antigravity.commandStatusInvalid")}
                              </span>
                            ) : status === "outdated" ? (
                              <span style={{ fontSize: 11, color: "#9a6700", fontWeight: 600 }}>
                                ⚠ {t("antigravity.commandStatusOutdated")}
                              </span>
                            ) : status === "installed" ? (
                              <span style={{ display: "inline-flex", alignItems: "center", gap: 3, fontSize: 11, color: "#1a7f37", fontWeight: 600 }}>
                                <span>✓</span> {t("antigravity.commandStatusInstalled")}
                              </span>
                            ) : (
                              <span style={{ fontSize: 11, color: "#656d76", fontWeight: 500 }}>
                                {t("antigravity.commandStatusNotInstalled")}
                              </span>
                            )}
                          </div>
                        </div>
                        <div style={{ fontSize: 11, color: "#57606a" }}>
                          {t(descKey)}
                        </div>
                      </div>

                      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                        {status === "not_installed" && (
                          <button
                            className="btn btn-primary"
                            style={{ fontSize: 11, padding: "4px 10px" }}
                            onClick={() => void handleInstallCommand(cmdName)}
                            disabled={commandsLoading}
                          >
                            {t("antigravity.commandBtnInstall")}
                          </button>
                        )}
                        {status === "outdated" && (
                          <button
                            className="btn btn-primary"
                            style={{ fontSize: 11, padding: "4px 10px" }}
                            onClick={() => void handleInstallCommand(cmdName)}
                            disabled={commandsLoading}
                          >
                            {t("antigravity.commandBtnUpdate")}
                          </button>
                        )}
                        {(status === "installed" || status === "outdated") && (
                          <button
                            className="btn btn-danger"
                            style={{ fontSize: 11, padding: "4px 10px" }}
                            onClick={() => void handleRemoveCommand(cmdName)}
                            disabled={commandsLoading}
                          >
                            {t("antigravity.commandBtnRemove")}
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
      </div>
    </>
  );
}
