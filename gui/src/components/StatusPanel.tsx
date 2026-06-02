import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { GatewayStatus, ApiKeyStatus } from "../types";

interface StatusPanelProps {
  health: GatewayStatus | null;
  healthError: string | null;
  healthLoading: boolean;
}

export default function StatusPanel({ health, healthError, healthLoading }: StatusPanelProps) {
  const { t } = useTranslation();
  const [apiKeyStatus, setApiKeyStatus] = useState<ApiKeyStatus | null>(null);

  useEffect(() => {
    invoke<ApiKeyStatus>("check_api_key")
      .then(setApiKeyStatus)
      .catch(() => setApiKeyStatus(null));
  }, []);

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
        </div>
      </div>
    </div>
  );
}
