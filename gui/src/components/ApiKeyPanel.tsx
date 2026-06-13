import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ApiKeyStatus, GatewayConfig, AllApiKeyStatus, ModelEntry } from "../types";

const COL_STYLE: React.CSSProperties = {
  padding: "6px 10px",
  fontSize: 12,
  color: "#1f2937",
  whiteSpace: "nowrap",
};

function ProviderRow({
  providerId,
  provider,
  keyStatus,
  models,
  onRefresh,
}: {
  providerId: string;
  provider: { display_name: string; api_key_env: string };
  keyStatus: ApiKeyStatus | null;
  models: Record<string, ModelEntry> | undefined;
  onRefresh: () => void;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [keyText, setKeyText] = useState("");
  const [keySaving, setKeySaving] = useState(false);
  const [keySaved, setKeySaved] = useState(false);
  const [envVarName, setEnvVarName] = useState(provider.api_key_env);
  const [envVarSaving, setEnvVarSaving] = useState(false);
  const [envVarSaved, setEnvVarSaved] = useState(false);
  const [envVarError, setEnvVarError] = useState<string | null>(null);

  // Model upstream edit state
  const proModel = "claude-sonnet-4-6";
  const flashModel = "claude-haiku-4-5";
  const currentPro = models?.[proModel]?.upstream_model ?? "";
  const currentFlash = models?.[flashModel]?.upstream_model ?? "";
  const [proText, setProText] = useState(currentPro);
  const [flashText, setFlashText] = useState(currentFlash);
  const [modelProSaving, setModelProSaving] = useState(false);
  const [modelFlashSaving, setModelFlashSaving] = useState(false);
  const [modelProSaved, setModelProSaved] = useState(false);
  const [modelFlashSaved, setModelFlashSaved] = useState(false);

  useEffect(() => {
    setProText(currentPro);
    setFlashText(currentFlash);
  }, [currentPro, currentFlash]);

  useEffect(() => {
    setEnvVarName(provider.api_key_env);
  }, [provider.api_key_env]);

  const handleSaveKey = async () => {
    if (!keyText.trim() || !keyStatus) return;
    setKeySaving(true);
    setKeySaved(false);
    try {
      await invoke("set_env_api_key", { key: keyText, envVarName: keyStatus.env_var });
      setKeySaving(false);
      setKeySaved(true);
      setKeyText("");
      setTimeout(() => setKeySaved(false), 2000);
      onRefresh();
    } catch (e) {
      setKeySaving(false);
      console.error(e);
    }
  };

  const handleSaveEnvVar = async () => {
    const trimmed = envVarName.trim();
    if (!trimmed) {
      setEnvVarError(t("apiKeyPanel.envVarErrorEmpty"));
      return;
    }
    if (!/^[A-Z][A-Z0-9_]*$/.test(trimmed)) {
      setEnvVarError(t("apiKeyPanel.envVarErrorFormat"));
      return;
    }
    setEnvVarError(null);
    setEnvVarSaving(true);
    setEnvVarSaved(false);
    try {
      await invoke("update_provider_api_key_env", { providerId, apiKeyEnv: trimmed });
      setEnvVarSaving(false);
      setEnvVarSaved(true);
      setTimeout(() => setEnvVarSaved(false), 2000);
      onRefresh();
    } catch (e) {
      setEnvVarSaving(false);
      setEnvVarError(String(e));
    }
  };

  const handleSaveModel = async (modelKey: string, value: string, setSaving: (v: boolean) => void, setSaved: (v: boolean) => void) => {
    if (!value.trim()) return;
    setSaving(true);
    setSaved(false);
    try {
      await invoke("set_model_upstream", { providerId, modelKey, upstreamModel: value.trim() });
      setSaving(false);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      onRefresh();
    } catch (e) {
      setSaving(false);
      console.error(e);
    }
  };

  return (
    <div>
      {/* Main row */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          background: "#ffffff",
          borderTop: "1px solid #e5e7eb",
          borderBottom: "1px solid #e5e7eb",
        }}
      >
        <div style={{ ...COL_STYLE, fontWeight: 600, minWidth: 130, fontSize: 13 }}>
          {provider.display_name}
        </div>

        <div style={{ ...COL_STYLE, fontFamily: "var(--font-mono)", fontSize: 11, minWidth: 170, color: "#374151" }}>
          {provider.api_key_env}
        </div>

        <div style={{ minWidth: 60, padding: "2px 8px" }}>
          {keyStatus === null ? (
            <span style={{ fontSize: 11, color: "#6b7280" }}>...</span>
          ) : keyStatus.set ? (
            <span style={{ fontSize: 11, color: "#107c10", fontWeight: 600 }}>
              {t("apiKeyPanel.set")}
            </span>
          ) : (
            <span style={{ fontSize: 11, color: "var(--error)", fontWeight: 600 }}>
              {t("apiKeyPanel.notSet")}
            </span>
          )}
        </div>

        <div style={{ flex: 1 }} />

        <div style={{ width: 80, padding: "2px 10px" }}>
          <button
            className="btn btn-small"
            onClick={() => setExpanded(!expanded)}
            style={{ fontSize: 11, padding: "2px 10px" }}
          >
            {expanded ? t("apiKeyPanel.collapse") : t("apiKeyPanel.edit")}
          </button>
        </div>
      </div>

      {/* Expandable edit area */}
      {expanded && (
        <div
          style={{
            background: "#fafafa",
            borderBottom: "1px solid #e5e7eb",
            padding: "10px 16px 10px 24px",
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          {/* Env var name edit */}
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
              {t("apiKeyPanel.envVarLabel")}
            </span>
            <input
              style={{
                width: 260,
                padding: "4px 8px",
                fontSize: 11,
                fontFamily: "var(--font-mono)",
                background: "#fff",
                color: "#1f2937",
                border: envVarError ? "1px solid var(--error)" : "1px solid #d0d7de",
                borderRadius: 4,
                outline: "none",
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
              onClick={handleSaveEnvVar}
              disabled={envVarSaving || !envVarName.trim() || envVarName === provider.api_key_env}
            >
              {envVarSaving ? "..." : t("apiKeyPanel.envVarSave")}
            </button>
            {envVarSaved && <span className="saved-toast">{t("apiKeyPanel.envVarSaved")}</span>}
            {envVarError && (
              <span style={{ fontSize: 10, color: "var(--error)" }}>{envVarError}</span>
            )}
          </div>

          {/* API key input */}
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
              {t("apiKeyPanel.header")}
            </span>
            <input
              type="password"
              style={{
                width: 340,
                padding: "4px 8px",
                fontSize: 11,
                fontFamily: "var(--font-mono)",
                background: "#fff",
                color: "#1f2937",
                border: "1px solid #d0d7de",
                borderRadius: 4,
                outline: "none",
              }}
              value={keyText}
              onChange={(e) => setKeyText(e.target.value)}
              placeholder="sk-..."
              spellCheck={false}
            />
            <button
              className="btn btn-primary btn-small"
              onClick={handleSaveKey}
              disabled={keySaving || !keyText.trim()}
            >
              {keySaving ? "..." : t("apiKeyPanel.saveKey")}
            </button>
            {keySaved && <span className="saved-toast">{t("apiKeyPanel.saved")}</span>}
          </div>

          {/* Sonnet 4.6 model edit */}
          {currentPro && (
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
                {t("apiKeyPanel.gatewayPro")}
              </span>
              <input
                style={{
                  width: 340,
                  padding: "4px 8px",
                  fontSize: 11,
                  fontFamily: "var(--font-mono)",
                  background: "#fff",
                  color: "#1f2937",
                  border: "1px solid #d0d7de",
                  borderRadius: 4,
                  outline: "none",
                }}
                value={proText}
                onChange={(e) => setProText(e.target.value)}
                placeholder={currentPro}
                spellCheck={false}
              />
              <button
                className="btn btn-primary btn-small"
                onClick={() => handleSaveModel(proModel, proText, setModelProSaving, setModelProSaved)}
                disabled={modelProSaving || !proText.trim() || proText.trim() === currentPro}
              >
                {modelProSaving ? "..." : t("apiKeyPanel.save")}
              </button>
              {modelProSaved && <span className="saved-toast">{t("apiKeyPanel.modelSaved")}</span>}
            </div>
          )}

          {/* Haiku 4.5 model edit */}
          {currentFlash && (
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
                {t("apiKeyPanel.gatewayFlash")}
              </span>
              <input
                style={{
                  width: 340,
                  padding: "4px 8px",
                  fontSize: 11,
                  fontFamily: "var(--font-mono)",
                  background: "#fff",
                  color: "#1f2937",
                  border: "1px solid #d0d7de",
                  borderRadius: 4,
                  outline: "none",
                }}
                value={flashText}
                onChange={(e) => setFlashText(e.target.value)}
                placeholder={currentFlash}
                spellCheck={false}
              />
              <button
                className="btn btn-primary btn-small"
                onClick={() => handleSaveModel(flashModel, flashText, setModelFlashSaving, setModelFlashSaved)}
                disabled={modelFlashSaving || !flashText.trim() || flashText.trim() === currentFlash}
              >
                {modelFlashSaving ? "..." : t("apiKeyPanel.save")}
              </button>
              {modelFlashSaved && <span className="saved-toast">{t("apiKeyPanel.modelSaved")}</span>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function ApiKeyPanel() {
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
  }, [refresh]);

  if (!config) {
    return <div className="loading" />;
  }

  const providerEntries = Object.entries(config.providers);

  return (
    <div className="settings-tile">
      <h3>{t("apiKeyPanel.header")}</h3>

      {/* Column headers */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          padding: "1px 0",
          marginBottom: 2,
        }}
      >
        <div style={{ ...COL_STYLE, fontWeight: 600, fontSize: 10, color: "#6b7280", minWidth: 130 }}>
          Provider
        </div>
        <div style={{ ...COL_STYLE, fontWeight: 600, fontSize: 10, color: "#6b7280", minWidth: 170 }}>
          Env Var
        </div>
        <div style={{ minWidth: 60, padding: "2px 8px", fontSize: 10, fontWeight: 600, color: "#6b7280" }}>
          Status
        </div>
        <div style={{ flex: 1 }} />
        <div style={{ width: 80, padding: "2px 10px", fontSize: 10, fontWeight: 600, color: "#6b7280" }}>
          Action
        </div>
      </div>

      {/* Provider rows */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          border: "1px solid #e5e7eb",
          borderRadius: 6,
          overflow: "hidden",
        }}
      >
        {providerEntries.map(([id, provider]) => (
          <ProviderRow
            key={id}
            providerId={id}
            provider={provider}
            keyStatus={allKeyStatus?.[id] ?? null}
            models={provider.models}
            onRefresh={refresh}
          />
        ))}
      </div>
    </div>
  );
}
