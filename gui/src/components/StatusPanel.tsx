import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { GatewayStatus, ApiKeyStatus, GatewayConfig } from "../types";

interface StatusPanelProps {
  health: GatewayStatus | null;
  healthError: string | null;
  healthLoading: boolean;
}

export default function StatusPanel({ health, healthError, healthLoading }: StatusPanelProps) {
  const { t } = useTranslation();
  const [apiKeyStatus, setApiKeyStatus] = useState<ApiKeyStatus | null>(null);
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [switching, setSwitching] = useState(false);
  const [switchMsg, setSwitchMsg] = useState<string | null>(null);

  const refreshConfig = useCallback(() => {
    invoke<GatewayConfig>("read_config")
      .then(setConfig)
      .catch(() => {});
  }, []);

  useEffect(() => {
    invoke<ApiKeyStatus>("check_api_key")
      .then(setApiKeyStatus)
      .catch(() => setApiKeyStatus(null));
    refreshConfig();
  }, [refreshConfig]);

  const handleProviderChange = async (providerId: string) => {
    setSwitching(true);
    setSwitchMsg(null);
    try {
      await invoke("update_active_provider", { providerId });
      await refreshConfig();
      if (health?.managed_child_running) {
        setSwitchMsg(t("statusPanel.restartRequired"));
      } else {
        setSwitchMsg(t("statusPanel.providerChanged"));
        setTimeout(() => setSwitchMsg(null), 3000);
      }
    } catch (e) {
      setSwitchMsg(String(e));
    } finally {
      setSwitching(false);
    }
  };

  const activeProvider = config?.providers[config?.active_provider ?? ""];
  const gatewayRunning = health?.managed_child_running ?? false;

  return (
    <div className="panel status-panel">
      <div className="panel-header">
        <span>{t("statusPanel.header")}</span>
      </div>
      <div className="panel-content">
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

          {/* API key card */}
          <div className="status-card">
            <div className="status-card-label">
              {apiKeyStatus ? apiKeyStatus.env_var : t("statusPanel.apiKey")}
            </div>
            {apiKeyStatus === null ? (
              <div className="loading" />
            ) : apiKeyStatus.set ? (
              <div className="status-card-value green">
                {t("statusPanel.set")}
              </div>
            ) : (
              <div className="status-card-value red">
                {t("statusPanel.notSet")}
              </div>
            )}
          </div>

          {/* Gateway URL card */}
          <div className="status-card">
            <div className="status-card-label">{t("statusPanel.gatewayUrl")}</div>
            <div className="status-card-value" style={{ fontSize: 12 }}>
              {t("statusPanel.gatewayUrlValue")}
            </div>
          </div>

          {/* Active provider card */}
          <div className="status-card">
            <div className="status-card-label">{t("statusPanel.activeProvider")}</div>
            {config ? (
              <select
                style={{
                  marginTop: 4,
                  padding: "4px 8px",
                  fontSize: 12,
                  fontFamily: "var(--font-sans)",
                  background: "var(--bg-input)",
                  color: "var(--text-primary)",
                  border: "1px solid var(--border)",
                  borderRadius: 4,
                  width: "100%",
                  cursor: "pointer",
                }}
                value={config.active_provider}
                onChange={(e) => handleProviderChange(e.target.value)}
                disabled={switching}
              >
                {Object.entries(config.providers).map(([id, p]) => (
                  <option key={id} value={id}>
                    {p.display_name}
                  </option>
                ))}
              </select>
            ) : (
              <div className="loading" />
            )}
            {switchMsg && (
              <div
                style={{
                  fontSize: 10,
                  marginTop: 4,
                  color: gatewayRunning ? "var(--warning)" : "var(--accent-green)",
                  fontWeight: 600,
                }}
              >
                {switchMsg}
              </div>
            )}
            {activeProvider && (
              <div style={{ fontSize: 10, color: "var(--text-muted)", marginTop: 4 }}>
                {activeProvider.supports_vision
                  ? t("statusPanel.visionOk")
                  : t("statusPanel.visionNotSupported")}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
