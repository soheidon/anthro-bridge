import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ApiKeyStatus, GatewayConfig } from "../types";

export default function ApiKeyPanel() {
  const { t } = useTranslation();
  const [keyText, setKeyText] = useState("");
  const [keyStatus, setKeyStatus] = useState<ApiKeyStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [envVarName, setEnvVarName] = useState("");
  const [envVarSaving, setEnvVarSaving] = useState(false);
  const [envVarSaved, setEnvVarSaved] = useState(false);
  const [envVarError, setEnvVarError] = useState<string | null>(null);

  useEffect(() => {
    invoke<ApiKeyStatus>("check_api_key").then(setKeyStatus).catch(() => setKeyStatus(null));
    invoke<GatewayConfig>("read_config")
      .then((cfg) => {
        setConfig(cfg);
        const provider = cfg.providers[cfg.active_provider];
        if (provider) {
          setEnvVarName(provider.api_key_env);
        }
      })
      .catch(() => {});
  }, []);

  const handleSave = async () => {
    if (!keyStatus?.env_var) return;
    setSaving(true);
    setSaved(false);
    try {
      await invoke("set_env_api_key", { key: keyText, envVarName: keyStatus.env_var });
      const status = await invoke<ApiKeyStatus>("check_api_key");
      setKeyStatus(status);
      setSaving(false);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setSaving(false);
      console.error(e);
    }
  };

  const handleSaveEnvVarName = async () => {
    if (!config) return;
    const trimmed = envVarName.trim();
    if (!trimmed) {
      setEnvVarError("Environment variable name cannot be empty");
      return;
    }
    // Validate format: uppercase, digits, underscores
    if (!/^[A-Z][A-Z0-9_]*$/.test(trimmed)) {
      setEnvVarError(
        "Must start with uppercase letter, contain only uppercase letters, digits, and underscores"
      );
      return;
    }
    setEnvVarError(null);
    setEnvVarSaving(true);
    setEnvVarSaved(false);
    try {
      await invoke("update_provider_api_key_env", {
        providerId: config.active_provider,
        apiKeyEnv: trimmed,
      });
      // Refresh config and key status
      const [newCfg, status] = await Promise.all([
        invoke<GatewayConfig>("read_config"),
        invoke<ApiKeyStatus>("check_api_key"),
      ]);
      setConfig(newCfg);
      setKeyStatus(status);
      setEnvVarSaving(false);
      setEnvVarSaved(true);
      setTimeout(() => setEnvVarSaved(false), 2000);
    } catch (e) {
      setEnvVarSaving(false);
      setEnvVarError(String(e));
    }
  };

  const activeProvider = config?.providers[config.active_provider];

  return (
    <>
      <div className="claude-config-help">
        <p>{t("apiKeyPanel.helpText")}</p>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {/* Active provider indicator */}
        {activeProvider && (
          <div style={{ fontSize: 12, fontWeight: 600, color: "var(--text-secondary)" }}>
            {t("apiKeyPanel.activeProvider")}: {activeProvider.display_name}
          </div>
        )}

        {/* Env var name row */}
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: "var(--text-muted)", minWidth: 80 }}>
            {t("apiKeyPanel.envVarLabel")}
          </span>
          <input
            style={{
              flex: 1,
              maxWidth: 280,
              padding: "4px 8px",
              fontSize: 12,
              fontFamily: "monospace",
              background: "var(--bg-input, #1a1a2e)",
              color: "var(--text-primary)",
              border: envVarError
                ? "1px solid var(--error)"
                : "1px solid var(--border, #333)",
              borderRadius: 4,
            }}
            value={envVarName}
            onChange={(e) => {
              setEnvVarName(e.target.value.toUpperCase());
              setEnvVarError(null);
            }}
            placeholder="MOONSHOT_API_KEY"
            spellCheck={false}
          />
          <button
            className="btn btn-primary btn-small"
            onClick={handleSaveEnvVarName}
            disabled={envVarSaving || !envVarName.trim() || envVarName === activeProvider?.api_key_env}
          >
            {envVarSaving ? "..." : t("apiKeyPanel.envVarSave")}
          </button>
          {envVarSaved && <span className="saved-toast">{t("apiKeyPanel.envVarSaved")}</span>}
        </div>

        {envVarError && (
          <span style={{ fontSize: 11, color: "var(--error)", marginLeft: 88 }}>
            {envVarError}
          </span>
        )}

        {/* Help text for env var */}
        <div style={{ marginLeft: 88 }}>
          <span style={{ fontSize: 10, color: "var(--text-muted)" }}>
            {t("apiKeyPanel.envVarHelpText")}
          </span>
        </div>

        {/* Separator */}
        <div style={{ borderTop: "1px solid var(--border, #333)", margin: "4px 0" }} />

        {/* API key status row */}
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: "var(--text-muted)", minWidth: 80 }}>
            {keyStatus?.env_var ?? t("apiKeyPanel.header")}
          </span>
          {keyStatus === null ? (
            <span style={{ fontSize: 11, color: "var(--text-muted)" }}>...</span>
          ) : keyStatus.set ? (
            <span style={{ fontSize: 11, color: "var(--accent-green)", fontWeight: 600 }}>
              {t("apiKeyPanel.set")}
            </span>
          ) : (
            <span style={{ fontSize: 11, color: "var(--error)", fontWeight: 600 }}>
              {t("apiKeyPanel.notSet")}
            </span>
          )}
        </div>

        {/* API key input row */}
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            className="api-key-input"
            type="password"
            value={keyText}
            onChange={(e) => setKeyText(e.target.value)}
            placeholder={t("apiKeyPanel.placeholder")}
            style={{ flex: 1, maxWidth: 420 }}
          />
          <button
            className="btn btn-primary btn-small"
            onClick={handleSave}
            disabled={saving || !keyText.trim()}
          >
            {saving ? "..." : t("apiKeyPanel.save")}
          </button>
          {saved && <span className="saved-toast">{t("apiKeyPanel.saved")}</span>}
        </div>
      </div>
    </>
  );
}
