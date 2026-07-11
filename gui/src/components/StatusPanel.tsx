import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { GatewayStatus, AllApiKeyStatus, GatewayConfig } from "../types";

interface StatusPanelProps {
  health: GatewayStatus | null;
  healthError: string | null;
  healthLoading: boolean;
  refreshKey?: number;
}

const SHELL_MODELS = [
  { name: "claude-opus-4-8", role: "Opus 4.8" },
  { name: "claude-sonnet-5", role: "Sonnet 5" },
  { name: "claude-haiku-4-5", role: "Haiku 4.5" },
];

export default function StatusPanel({ health, healthError, healthLoading, refreshKey }: StatusPanelProps) {
  const { t } = useTranslation();
  const [allKeyStatus, setAllKeyStatus] = useState<AllApiKeyStatus | null>(null);
  const [config, setConfig] = useState<GatewayConfig | null>(null);

  const refresh = useCallback(() => {
    invoke<AllApiKeyStatus>("check_all_api_keys")
      .then(setAllKeyStatus)
      .catch(() => setAllKeyStatus(null));
    invoke<GatewayConfig>("read_config")
      .then(setConfig)
      .catch(() => {});
  }, []);

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
    supports_image_url: boolean;
    supports_image_base64: boolean;
    supports_video_url: boolean;
    supports_video_base64: boolean;
    sanitizedVision: boolean;
  }
  const routedModels: RoutedModelRow[] = [];
  if (activeProvider?.models) {
    for (const shell of SHELL_MODELS) {
      const entry = activeProvider.models[shell.name];
      if (entry) {
        const vis = entry.supports_vision ?? activeProvider.supports_vision;
        const vid = entry.supports_video ?? activeProvider.supports_video;
        const thinking = entry.thinking === "disabled" ? "disabled"
          : entry.force_thinking ? "force"
          : "default";
        routedModels.push({
          gateway: shell.name,
          upstream: entry.upstream_model,
          role: shell.role,
          thinking,
          supports_image_url: entry.supports_image_url ?? vis,
          supports_image_base64: entry.supports_image_base64 ?? vis,
          supports_video_url: entry.supports_video_url ?? vid,
          supports_video_base64: entry.supports_video_base64 ?? vid,
          sanitizedVision: !vis,
        });
      }
    }
  }
  const capYesNo = (val: boolean) => val ? t("statusPanel.yes") : t("statusPanel.no");

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
                  </tr>
                </thead>
                <tbody>
                  {routedModels.map(({ gateway, upstream, role, thinking, supports_image_url, supports_image_base64, supports_video_url, supports_video_base64, sanitizedVision }) => (
                    <tr key={gateway}>
                      <td className="mono">{gateway}</td>
                      <td className="mono" style={{ color: "var(--text-muted)" }}>{upstream}</td>
                      <td style={{ fontWeight: 600 }}>{role}</td>
                      <td>
                        {sanitizedVision && !supports_image_url ? (
                          <span className="badge badge-yellow" title={t("statusPanel.tileSanitizedHint")}>
                            {t("statusPanel.tileSanitized")}
                          </span>
                        ) : (
                          <span className={`badge ${supports_image_url ? "badge-green" : "badge-gray"}`}>
                            {capYesNo(supports_image_url)}
                          </span>
                        )}
                      </td>
                      <td>
                        {sanitizedVision && !supports_image_base64 ? (
                          <span className="badge badge-yellow" title={t("statusPanel.tileSanitizedHint")}>
                            {t("statusPanel.tileSanitized")}
                          </span>
                        ) : (
                          <span className={`badge ${supports_image_base64 ? "badge-green" : "badge-gray"}`}>
                            {capYesNo(supports_image_base64)}
                          </span>
                        )}
                      </td>
                      <td>
                        <span className={`badge ${supports_video_url ? "badge-green" : "badge-gray"}`}>
                          {capYesNo(supports_video_url)}
                        </span>
                      </td>
                      <td>
                        <span className={`badge ${supports_video_base64 ? "badge-green" : "badge-gray"}`}>
                          {capYesNo(supports_video_base64)}
                        </span>
                      </td>
                      <td>
                        {thinking === "force" ? (
                          <span className="badge badge-purple">
                            {t("statusPanel.thinkingOnly")}
                          </span>
                        ) : (
                          <span className={`badge ${thinking === "disabled" ? "badge-blue" : "badge-gray"}`}>
                            {thinking === "disabled" ? t("statusPanel.thinkingDisabled") : t("statusPanel.thinkingDefault")}
                          </span>
                        )}
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
