import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { GatewayConfig, McpConfig, McpStatus, AllApiKeyStatus } from "../types";
import { getMcpTargetKey } from "../types";
import { getVisibleOpenRouterProfiles } from "../dashboardTiles";
import { getDeepSeekPricingStatus } from "../config/deepseekSchedule";

interface McpPanelProps {
  config: GatewayConfig | null;
  refreshConfig: () => Promise<void>;
}

const MCP_PROVIDER_ORDER = ["deepseek", "openrouter", "minimax", "mimo", "kimi"];

interface McpTileItem {
  id: string;
  providerId: string;
  profileId: string | null;
  displayName: string;
  modelSummary: string;
  thinkingSummary: string;
  isActive: boolean;
}

export default function McpPanel({ config, refreshConfig }: McpPanelProps) {
  const { t } = useTranslation();
  const [mcpStatus, setMcpStatus] = useState<McpStatus | null>(null);
  const [mcpConfig, setMcpConfig] = useState<McpConfig>({});
  const [allKeyStatus, setAllKeyStatus] = useState<AllApiKeyStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // DeepSeek peak/valley state
  const [now, setNow] = useState(() => new Date());

  // Update now every minute (aligned to minute boundary)
  useEffect(() => {
    let intervalId: number | undefined;
    let timeoutId: number | undefined;

    const updateNow = () => setNow(new Date());
    const msToNextMinute = 60_000 - (Date.now() % 60_000);

    timeoutId = window.setTimeout(() => {
      updateNow();
      intervalId = window.setInterval(updateNow, 60_000);
    }, msToNextMinute);

    return () => {
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
      if (intervalId !== undefined) window.clearInterval(intervalId);
    };
  }, []);

  // Load MCP status, config, and all API keys
  const refreshMcp = useCallback(async () => {
    try {
      const [status, cfg, keys] = await Promise.all([
        invoke<McpStatus>("get_mcp_status"),
        invoke<McpConfig>("get_mcp_config"),
        invoke<AllApiKeyStatus>("check_all_api_keys"),
      ]);
      setMcpStatus(status);
      setMcpConfig(cfg);
      setAllKeyStatus(keys);
    } catch (e: unknown) {
      console.error("Failed to load MCP status/config/keys:", e);
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refreshMcp();
  }, [refreshMcp]);

  const selectedProviderId = mcpConfig.provider ?? config?.active_provider ?? "deepseek";

  // OpenRouter visible profiles
  const orVisibleProfiles = useMemo(() => {
    const orProvider = config?.providers?.["openrouter"];
    if (!orProvider?.profiles) return [];
    return getVisibleOpenRouterProfiles(orProvider.profiles) ?? orProvider.profiles;
  }, [config]);

  const selectedProfileId =
    selectedProviderId === "openrouter"
      ? mcpConfig.profile_id ??
        config?.active_openrouter_profile_id ??
        orVisibleProfiles[0]?.id ??
        ""
      : undefined;

  const handleSave = async (updated: McpConfig) => {
    setSaving(true);
    setError(null);
    try {
      await invoke("update_mcp_config", { mcp: updated });
      setMcpConfig(updated);
      await refreshConfig();
      await refreshMcp();
    } catch (err) {
      console.error("Save MCP config failed:", err);
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleTileClick = (tile: McpTileItem) => {
    if (tile.isActive || saving) return;
    const targetKey = getMcpTargetKey(tile.providerId, tile.profileId);
    const targetSetting = mcpConfig.targets?.[targetKey];

    // Mirror target settings to top-level if saved in targets, otherwise fallback to defaults
    let model = targetSetting?.model;
    let thinking_mode = targetSetting?.thinking_mode;
    let reasoning_effort = targetSetting?.reasoning_effort;

    if (!model) {
      if (tile.providerId === "openrouter") {
        const prof = orVisibleProfiles.find((p) => p.id === tile.profileId);
        model = prof?.models?.["claude-opus-5"]?.upstream_model ?? "deepseek/deepseek-r1";
        thinking_mode = prof?.models?.["claude-opus-5"]?.thinking_mode ?? "thinking";
        reasoning_effort = prof?.models?.["claude-opus-5"]?.reasoning_effort ?? "high";
      } else {
        const p = config?.providers?.[tile.providerId];
        model = p?.models?.["claude-opus-5"]?.upstream_model ?? p?.default_model ?? "deepseek-v4-pro";
        thinking_mode = p?.models?.["claude-opus-5"]?.thinking_mode ?? "thinking";
        reasoning_effort = p?.models?.["claude-opus-5"]?.reasoning_effort ?? (tile.providerId === "kimi" ? "max" : "high");
      }
    }

    const next: McpConfig = {
      ...mcpConfig,
      provider: tile.providerId,
      profile_id: tile.profileId ?? undefined,
      model,
      thinking_mode,
      reasoning_effort,
    };
    void handleSave(next);
  };

  // Build card tiles matching Gateway's card list
  const tiles = useMemo<McpTileItem[]>(() => {
    if (!config) return [];
    const list: McpTileItem[] = [];

    // Direct providers
    const directProviderIds = ["deepseek", "mimo", "minimax", "kimi"];
    for (const pid of directProviderIds) {
      const p = config.providers[pid];
      if (!p || p.hidden) continue;

      const isCurrentActive = selectedProviderId === pid;
      const targetKey = getMcpTargetKey(pid);
      const targetSaved = mcpConfig.targets?.[targetKey];

      const model = isCurrentActive && mcpConfig.model
        ? mcpConfig.model
        : targetSaved?.model ?? p.models?.["claude-opus-5"]?.upstream_model ?? p.default_model ?? "—";

      const thinkingMode = isCurrentActive && mcpConfig.thinking_mode
        ? mcpConfig.thinking_mode
        : targetSaved?.thinking_mode ?? p.models?.["claude-opus-5"]?.thinking_mode ?? "thinking";

      const reasoningEffort = isCurrentActive && mcpConfig.reasoning_effort
        ? mcpConfig.reasoning_effort
        : targetSaved?.reasoning_effort ?? p.models?.["claude-opus-5"]?.reasoning_effort ?? (pid === "kimi" ? "max" : "high");

      const thinkingSummary = thinkingMode === "thinking"
        ? `${t("mcp.thinkingLabel")}: ${reasoningEffort}`
        : t("popup.mode.disabled");

      list.push({
        id: pid,
        providerId: pid,
        profileId: null,
        displayName: p.display_name,
        modelSummary: model,
        thinkingSummary,
        isActive: isCurrentActive,
      });
    }

    // OpenRouter profiles
    const orProvider = config.providers["openrouter"];
    if (orProvider && !orProvider.hidden && orVisibleProfiles.length > 0) {
      for (const prof of orVisibleProfiles) {
        if (prof.hidden) continue;
        const isCurrentActive = selectedProviderId === "openrouter" && selectedProfileId === prof.id;
        const targetKey = getMcpTargetKey("openrouter", prof.id);
        const targetSaved = mcpConfig.targets?.[targetKey];

        const model = isCurrentActive && mcpConfig.model
          ? mcpConfig.model
          : targetSaved?.model ?? prof.models?.["claude-opus-5"]?.upstream_model ?? "—";

        const thinkingMode = isCurrentActive && mcpConfig.thinking_mode
          ? mcpConfig.thinking_mode
          : targetSaved?.thinking_mode ?? prof.models?.["claude-opus-5"]?.thinking_mode ?? "thinking";

        const reasoningEffort = isCurrentActive && mcpConfig.reasoning_effort
          ? mcpConfig.reasoning_effort
          : targetSaved?.reasoning_effort ?? prof.models?.["claude-opus-5"]?.reasoning_effort ?? "high";

        const thinkingSummary = thinkingMode === "thinking"
          ? `${t("mcp.thinkingLabel")}: ${reasoningEffort}`
          : t("popup.mode.disabled");

        list.push({
          id: `openrouter:${prof.id}`,
          providerId: "openrouter",
          profileId: prof.id,
          displayName: prof.display_name || `OpenRouter: ${prof.id}`,
          modelSummary: model,
          thinkingSummary,
          isActive: isCurrentActive,
        });
      }
    }

    return list;
  }, [config, selectedProviderId, selectedProfileId, mcpConfig, orVisibleProfiles, t]);

  return (
    <div className="dashboard-page" style={{ overflowY: "auto" }}>
      {/* Gateway-style Provider Cards Grid (Top) */}
      <div className="dashboard-section" style={{ marginBottom: 8 }}>
        <h3>{t("mcp.destinationHeader")}</h3>
        <div className="provider-tile-grid">
          {tiles.map((tile) => (
            <div
              key={tile.id}
              className={`provider-tile${tile.isActive ? " selected" : ""}`}
              style={saving ? { opacity: 0.6, pointerEvents: "none", cursor: "default" } : { cursor: "pointer" }}
              onClick={() => handleTileClick(tile)}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
                <div className="provider-tile-name">{tile.displayName}</div>
                {tile.providerId === "deepseek" && (() => {
                  const dsStatus = getDeepSeekPricingStatus(now);
                  return (
                    <span className={`provider-tile-pricing-badge ${dsStatus.period.type === "PEAK" ? "peak" : "valley"}`}>
                      {t(`peakValley.${dsStatus.period.type.toLowerCase() as "peak" | "valley"}`)}
                    </span>
                  );
                })()}
              </div>
              <div className="provider-tile-routes-simple">
                <div title={tile.modelSummary}><span className="up-mono">{tile.modelSummary}</span></div>
                <div title={tile.thinkingSummary}><span className="up-mono" style={{ color: "var(--text-muted)", fontSize: 11 }}>{tile.thinkingSummary}</span></div>
              </div>
              <div className="provider-tile-badge">{t("statusPanel.tileActive")}</div>
            </div>
          ))}
        </div>
      </div>

      {/* Server & Tool Status Section (Bottom) */}
      <div className="panel">
        <div className="panel-header">
          <span>{t("mcp.statusHeader")}</span>
        </div>
        <div className="panel-content">
          <div className="status-grid" style={{ gridTemplateColumns: "1fr 2fr" }}>
            <div className="status-card">
              <div className="status-card-label">{t("mcp.toolStatus")}</div>
              <div className="status-card-value green" style={{ fontSize: 13, display: "flex", alignItems: "center", gap: 6 }}>
                <span style={{ display: "inline-block", width: 8, height: 8, borderRadius: "50%", background: "#107c10" }} />
                <span>{t("mcp.toolReady")}</span>
              </div>
            </div>

            <div className="status-card">
              <div className="status-card-label">{t("statusPanel.apiKey")}</div>
              {allKeyStatus && config ? (
                <div style={{ display: "flex", gap: 12, flexWrap: "wrap", fontSize: 11 }}>
                  {MCP_PROVIDER_ORDER.map((id) => {
                    const status = allKeyStatus[id];
                    const name = config.providers[id]?.display_name ?? id;
                    const isSet = status?.set ?? false;
                    return (
                      <span
                        key={id}
                        style={{
                          color: isSet ? "#107c10" : "var(--error)",
                          fontWeight: 600,
                          whiteSpace: "nowrap",
                        }}
                      >
                        {name}: {isSet ? "✓" : "✗"}
                      </span>
                    );
                  })}
                </div>
              ) : (
                <div className="loading" />
              )}
            </div>
          </div>

          {error && <div style={{ marginTop: 8, fontSize: 12, color: "var(--error)" }}>{error}</div>}
        </div>
      </div>
    </div>
  );
}



