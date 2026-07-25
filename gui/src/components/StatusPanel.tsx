import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { GatewayStatus, AllApiKeyStatus, GatewayConfig } from "../types";
import type { OpenRouterModelsResult } from "../types/openrouter";
import { MODEL_PRICING } from "../config/modelPricing";
import { getOpenRouterModelsCached, parsePerMillionUsd } from "./OpenRouterModelSelector";

interface StatusPanelProps {
  health: GatewayStatus | null;
  healthError: string | null;
  healthLoading: boolean;
  refreshKey?: number;
}

const LAGUNA_S_IDS = new Set([
  "poolside/laguna-s-2.1", "poolside/laguna-s-2.1:free",
]);
const LAGUNA_XS_IDS = new Set([
  "poolside/laguna-xs-2.1", "poolside/laguna-xs-2.1:free",
]);

const SHELL_MODELS = [
  { name: "claude-opus-5", role: "Opus 5" },
  { name: "claude-sonnet-5", role: "Sonnet 5" },
  { name: "claude-haiku-4-5", role: "Haiku 4.5" },
];

export default function StatusPanel({ health, healthError, healthLoading, refreshKey }: StatusPanelProps) {
  const { t } = useTranslation();
  const [allKeyStatus, setAllKeyStatus] = useState<AllApiKeyStatus | null>(null);
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [orPricing, setOrPricing] = useState<Map<string, { input: number | null; output: number | null }>>(new Map());

  const refreshPricing = useCallback(() => {
    getOpenRouterModelsCached(false).then((result: OpenRouterModelsResult) => {
      const map = new Map<string, { input: number | null; output: number | null }>();
      for (const m of result.models) {
        map.set(m.id, {
          input: parsePerMillionUsd(m.pricing.prompt),
          output: parsePerMillionUsd(m.pricing.completion),
        });
      }
      setOrPricing(map);
    }).catch(() => {});
  }, []);

  const refresh = useCallback(() => {
    invoke<AllApiKeyStatus>("check_all_api_keys")
      .then(setAllKeyStatus)
      .catch(() => setAllKeyStatus(null));
    invoke<GatewayConfig>("read_config")
      .then(setConfig)
      .catch(() => {});
    refreshPricing();
  }, [refreshPricing]);

  useEffect(() => {
    refresh();
  }, [refresh, refreshKey]);

  const activeProviderId = config?.active_provider ?? "deepseek";
  const activeProvider = config?.providers[activeProviderId];

  // Build routing table rows
  interface RoutedModelRow {
    gateway: string;
    upstream: string;
    role: string;
    thinking: string;
    supports_image_url: boolean | null;
    supports_image_base64: boolean | null;
    supports_video_url: boolean | null;
    supports_video_base64: boolean | null;
    sanitizedVision: boolean | null;
    inputPrice: number | null;
    outputPrice: number | null;
  }
  const routedModels: RoutedModelRow[] = [];
  if (activeProvider?.models) {
    for (const shell of SHELL_MODELS) {
      const entry = activeProvider.models[shell.name];
      if (entry) {
        const isOr = activeProviderId === "openrouter";
        const vis = entry.supports_vision ?? activeProvider.supports_vision;
        const vid = entry.supports_video ?? activeProvider.supports_video;
        const thinkingMode = entry.thinking_mode;
        const reasoningEffort = entry.reasoning_effort;
        const upstream = entry.upstream_model;
        const thinking: string = entry.thinking === "disabled" ? "DISABLED"
          : entry.force_thinking ? "FORCE"
          : thinkingMode === "thinking" && reasoningEffort === "max" ? "MAX"
          : thinkingMode === "thinking" ? "ON"
          : thinkingMode === "normal" ? "OFF"
          // Normalize "DEFAULT" for OpenRouter Laguna models to their known defaults
          : LAGUNA_S_IDS.has(upstream) ? "MAX"
          : LAGUNA_XS_IDS.has(upstream) ? "THINKING"
          : "DEFAULT";
        // For OpenRouter models without explicit per-model capability flags in config,
        // show "—" (unknown) rather than falling back to provider defaults.
        const resolveImgUrl = isOr && entry.supports_image_url == null ? null : (entry.supports_image_url ?? vis);
        const resolveImgB64 = isOr && entry.supports_image_base64 == null ? null : (entry.supports_image_base64 ?? vis);
        const resolveVidUrl = isOr && entry.supports_video_url == null ? null : (entry.supports_video_url ?? vid);
        const resolveVidB64 = isOr && entry.supports_video_base64 == null ? null : (entry.supports_video_base64 ?? vid);
        const pricing = MODEL_PRICING[entry.upstream_model];
        const orPrice = orPricing.get(entry.upstream_model);
        routedModels.push({
          gateway: shell.name,
          upstream: entry.upstream_model,
          role: shell.role,
          thinking,
          supports_image_url: resolveImgUrl,
          supports_image_base64: resolveImgB64,
          supports_video_url: resolveVidUrl,
          supports_video_base64: resolveVidB64,
          sanitizedVision: resolveImgUrl == null ? null : !resolveImgUrl,
          inputPrice: pricing?.inputPerMillionUsd ?? orPrice?.input ?? null,
          outputPrice: pricing?.outputPerMillionUsd ?? orPrice?.output ?? null,
        });
      }
    }
  }
  const capBadge = (val: boolean | null) => {
    if (val === null) return <span className="badge badge-gray">—</span>;
    return val
      ? <span className="badge badge-green">{t("statusPanel.yes")}</span>
      : <span className="badge badge-gray">{t("statusPanel.no")}</span>;
  };

  return (
    <div className="panel status-panel">
      <div className="panel-header">
        <span>{t("statusPanel.header")}</span>
      </div>
      <div className="panel-content">
        {/* ---- Status cards ---- */}
        <div className="status-grid">
          {/* Port 4000 card */}
          <div className="status-card">
            <div className="status-card-label">{t("statusPanel.port4000")}</div>
            {healthLoading ? (
              <div className="loading" />
            ) : healthError ? (
              <div className="error-text">{healthError}</div>
            ) : health?.port_listening ? (
              <div className="status-card-value green">
                {t("statusPanel.listening")}
              </div>
            ) : (
              <div className="status-card-value muted">{t("statusPanel.notListening")}</div>
            )}
          </div>

          {/* Gateway URL card */}
          <div className="status-card">
            <div className="status-card-label">{t("statusPanel.gatewayUrl")}</div>
            <div className="status-card-value" style={{ fontSize: 12 }}>
              {t("statusPanel.gatewayUrlValue")}
            </div>
          </div>

          {/* API keys card */}
          <div className="status-card">
            <div className="status-card-label">{t("statusPanel.apiKey")}</div>
            {allKeyStatus && config ? (
              <div style={{ display: "flex", gap: 12, flexWrap: "wrap", fontSize: 11 }}>
                {Object.entries(allKeyStatus).map(([id, status]) => {
                  const name = config.providers[id]?.display_name ?? id;
                  return (
                    <span key={id} style={{ color: status.set ? "#107c10" : "var(--error)", fontWeight: 600, whiteSpace: "nowrap" }}>
                      {name}: {status.set ? "✓" : "✗"}
                    </span>
                  );
                })}
              </div>
            ) : (
              <div className="loading" />
            )}
          </div>
        </div>

        {/* ---- Routing table ---- */}
        {routedModels.length > 0 && (
          <div style={{ marginTop: 12 }}>
            <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text-muted)", marginBottom: 6 }}>
              {t("statusPanel.availableModels")}
              {" — "}
              {activeProvider?.display_name ?? activeProviderId}
            </div>
            <div style={{ overflowX: "auto" }}>
              <table className="model-routing-table">
                <thead>
                  <tr>
                    <th>{t("statusPanel.colGateway")}</th>
                    <th>{t("statusPanel.colUpstream")}</th>
                    <th>{t("statusPanel.colRole")}</th>
                    <th>{t("statusPanel.colImgUrl")}</th>
                    <th>{t("statusPanel.colImgB64")}</th>
                    <th>{t("statusPanel.colVidUrl")}</th>
                    <th>{t("statusPanel.colVidB64")}</th>
                    <th>{t("statusPanel.colThinking")}</th>
                    <th>{t("statusPanel.colInputPrice")}</th>
                    <th>{t("statusPanel.colOutputPrice")}</th>
                  </tr>
                </thead>
                <tbody>
                  {routedModels.map(({ gateway, upstream, role, thinking, supports_image_url, supports_image_base64, supports_video_url, supports_video_base64, sanitizedVision, inputPrice, outputPrice }) => (
                    <tr key={gateway}>
                      <td className="mono">{gateway}</td>
                      <td className="mono" style={{ color: "var(--text-muted)" }}>{upstream}</td>
                      <td style={{ fontWeight: 600 }}>{role}</td>
                      <td>
                        {sanitizedVision != null && supports_image_url != null && !supports_image_url ? (
                          <span className="badge badge-yellow" title={t("statusPanel.tileSanitizedHint")}>
                            {t("statusPanel.tileSanitized")}
                          </span>
                        ) : (
                          capBadge(supports_image_url)
                        )}
                      </td>
                      <td>
                        {sanitizedVision != null && supports_image_base64 != null && !supports_image_base64 ? (
                          <span className="badge badge-yellow" title={t("statusPanel.tileSanitizedHint")}>
                            {t("statusPanel.tileSanitized")}
                          </span>
                        ) : (
                          capBadge(supports_image_base64)
                        )}
                      </td>
                      <td>{capBadge(supports_video_url)}</td>
                      <td>{capBadge(supports_video_base64)}</td>
                      <td>
                        {thinking === "FORCE" ? (
                          <span className="badge badge-purple">
                            {t("statusPanel.thinkingOnly")}
                          </span>
                        ) : thinking === "MAX" ? (
                          <span className="badge badge-pink">MAX</span>
                        ) : thinking === "ON" ? (
                          <span className="badge badge-green">THINKING</span>
                        ) : thinking === "OFF" ? (
                          <span className="badge badge-blue">OFF</span>
                        ) : thinking === "DISABLED" ? (
                          <span className="badge badge-blue">{t("statusPanel.thinkingDisabled")}</span>
                        ) : (
                          <span className="badge badge-gray">{t("statusPanel.thinkingDefault")}</span>
                        )}
                      </td>
                      <td style={{ textAlign: "right", whiteSpace: "nowrap", fontVariantNumeric: "tabular-nums", fontSize: 11 }}>
                        {inputPrice != null ? `$${Math.floor(inputPrice * 1000) / 1000}` : "—"}
                      </td>
                      <td style={{ textAlign: "right", whiteSpace: "nowrap", fontVariantNumeric: "tabular-nums", fontSize: 11 }}>
                        {outputPrice != null ? `$${Math.floor(outputPrice * 1000) / 1000}` : "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
