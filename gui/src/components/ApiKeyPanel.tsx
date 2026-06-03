import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ApiKeyStatus, GatewayConfig, ProviderConfig, AllApiKeyStatus } from "../types";

type ProviderCardState = {
  keyText: string;
  saving: boolean;
  saved: boolean;
  envVarName: string;
  envVarSaving: boolean;
  envVarSaved: boolean;
  envVarError: string | null;
};

function CapabilityBadge({ label, supported }: { label: string; supported: boolean }) {
  return (
    <span
      style={{
        fontSize: 11,
        padding: "2px 8px",
        borderRadius: 4,
        fontWeight: 500,
        background: supported ? "#e6f4ea" : "#f3f4f6",
        color: supported ? "#137333" : "#6b7280",
        border: supported ? "1px solid #b7dfc0" : "1px solid #d1d5db",
      }}
    >
      {label}
    </span>
  );
}

function ProviderCard({
  providerId,
  provider,
  active,
  keyStatus,
  onActivate,
  onRefresh,
  switching,
}: {
  providerId: string;
  provider: ProviderConfig;
  active: boolean;
  keyStatus: ApiKeyStatus | null;
  onActivate: () => void;
  onRefresh: () => void;
  switching: boolean;
}) {
  const { t } = useTranslation();

  const [state, setState] = useState<ProviderCardState>({
    keyText: "",
    saving: false,
    saved: false,
    envVarName: provider.api_key_env,
    envVarSaving: false,
    envVarSaved: false,
    envVarError: null,
  });

  useEffect(() => {
    setState((s) => ({ ...s, envVarName: provider.api_key_env }));
  }, [provider.api_key_env]);

  const updateState = (patch: Partial<ProviderCardState>) =>
    setState((s) => ({ ...s, ...patch }));

  const handleSaveKey = useCallback(async () => {
    if (!state.keyText.trim() || !keyStatus) return;
    updateState({ saving: true, saved: false });
    try {
      await invoke("set_env_api_key", { key: state.keyText, envVarName: keyStatus.env_var });
      updateState({ saving: false, saved: true, keyText: "" });
      setTimeout(() => updateState({ saved: false }), 2000);
      onRefresh();
    } catch (e) {
      updateState({ saving: false });
      console.error(e);
    }
  }, [state.keyText, keyStatus, onRefresh]);

  const handleSaveEnvVar = useCallback(async () => {
    const trimmed = state.envVarName.trim();
    if (!trimmed) {
      updateState({ envVarError: t("apiKeyPanel.envVarErrorEmpty") });
      return;
    }
    if (!/^[A-Z][A-Z0-9_]*$/.test(trimmed)) {
      updateState({ envVarError: t("apiKeyPanel.envVarErrorFormat") });
      return;
    }
    updateState({ envVarSaving: true, envVarSaved: false, envVarError: null });
    try {
      await invoke("update_provider_api_key_env", { providerId, apiKeyEnv: trimmed });
      updateState({ envVarSaving: false, envVarSaved: true });
      setTimeout(() => updateState({ envVarSaved: false }), 2000);
      onRefresh();
    } catch (e) {
      updateState({ envVarSaving: false, envVarError: String(e) });
    }
  }, [providerId, state.envVarName, onRefresh, t]);

  const cardStyle: React.CSSProperties = active
    ? {
        background: "#f0f7ff",
        border: "2px solid #0078d4",
        borderRadius: 8,
        padding: "14px 16px",
        display: "flex",
        flexDirection: "column",
        gap: 10,
      }
    : {
        background: "#ffffff",
        border: "1px solid #d0d7de",
        borderRadius: 8,
        padding: "14px 16px",
        display: "flex",
        flexDirection: "column",
        gap: 10,
      };

  const labelStyle: React.CSSProperties = {
    fontSize: 11,
    fontWeight: 600,
    color: "#1f2937",
    minWidth: 90,
  };

  const inputStyle: React.CSSProperties = {
    flex: 1,
    padding: "5px 8px",
    fontSize: 12,
    fontFamily: "var(--font-mono)",
    background: "#ffffff",
    color: "#1f2937",
    border: state.envVarError
      ? "1px solid var(--error)"
      : "1px solid #d0d7de",
    borderRadius: 4,
    outline: "none",
  };

  return (
    <div style={cardStyle}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ fontSize: 16, fontWeight: 700, color: "#1f2937" }}>
            {provider.display_name}
          </span>
          {active && (
            <span
              style={{
                fontSize: 10,
                fontWeight: 700,
                padding: "2px 10px",
                borderRadius: 4,
                background: "#0078d4",
                color: "#fff",
              }}
            >
              {t("apiKeyPanel.badgeActive")}
            </span>
          )}
        </div>
        {!active && (
          <button
            className="btn btn-primary btn-small"
            onClick={onActivate}
            disabled={switching}
          >
            {switching ? "..." : t("apiKeyPanel.setActive")}
          </button>
        )}
      </div>

      {/* Capabilities */}
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontSize: 11, color: "#6b7280", marginRight: 2 }}>Capabilities:</span>
        <CapabilityBadge label={t("apiKeyPanel.capVision")} supported={provider.supports_vision} />
        <CapabilityBadge label={t("apiKeyPanel.capVideo")} supported={provider.supports_video} />
        <CapabilityBadge label={t("apiKeyPanel.capThinking")} supported={provider.supports_thinking} />
        <CapabilityBadge label={t("apiKeyPanel.capCountTokens")} supported={provider.supports_count_tokens} />
      </div>

      {/* Divider */}
      <div style={{ borderTop: "1px solid #e5e7eb" }} />

      {/* Env var name */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={labelStyle}>{t("apiKeyPanel.envVarLabel")}</span>
        <input
          style={{ ...inputStyle, maxWidth: 280 }}
          value={state.envVarName}
          onChange={(e) => updateState({ envVarName: e.target.value.toUpperCase(), envVarError: null })}
          placeholder="MOONSHOT_API_KEY"
          spellCheck={false}
        />
        <button
          className="btn btn-primary btn-small"
          onClick={handleSaveEnvVar}
          disabled={
            state.envVarSaving ||
            !state.envVarName.trim() ||
            state.envVarName === provider.api_key_env
          }
        >
          {state.envVarSaving ? "..." : t("apiKeyPanel.envVarSave")}
        </button>
        {state.envVarSaved && <span className="saved-toast">{t("apiKeyPanel.envVarSaved")}</span>}
      </div>
      {state.envVarError && (
        <span style={{ fontSize: 10, color: "var(--error)", marginLeft: 100 }}>
          {state.envVarError}
        </span>
      )}

      {/* API key input */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={labelStyle}>{t("apiKeyPanel.header")}</span>
        <input
          className="api-key-input"
          type="password"
          value={state.keyText}
          onChange={(e) => updateState({ keyText: e.target.value })}
          placeholder="sk-..."
          style={{ flex: 1, maxWidth: 390, fontSize: 12 }}
        />
        <button
          className="btn btn-primary btn-small"
          onClick={handleSaveKey}
          disabled={state.saving || !state.keyText.trim()}
        >
          {state.saving ? "..." : t("apiKeyPanel.saveKey")}
        </button>
        {state.saved && <span className="saved-toast">{t("apiKeyPanel.saved")}</span>}
      </div>

      {/* API key status */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span style={labelStyle}>{t("apiKeyPanel.status")}</span>
        {keyStatus === null ? (
          <span style={{ fontSize: 11, color: "#6b7280" }}>...</span>
        ) : keyStatus.set ? (
          <span style={{ fontSize: 11, color: "#107c10", fontWeight: 600 }}>
            {t("apiKeyPanel.set")} ({keyStatus.env_var})
          </span>
        ) : (
          <span style={{ fontSize: 11, color: "var(--error)", fontWeight: 600 }}>
            {t("apiKeyPanel.notSet")} ({keyStatus.env_var})
          </span>
        )}
      </div>
    </div>
  );
}

export default function ApiKeyPanel() {
  const { t } = useTranslation();
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [allKeyStatus, setAllKeyStatus] = useState<AllApiKeyStatus | null>(null);
  const [switchingProvider, setSwitchingProvider] = useState(false);

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

  const handleActivate = useCallback(
    async (providerId: string) => {
      setSwitchingProvider(true);
      try {
        await invoke("update_active_provider", { providerId });
        await refresh();
      } catch (e) {
        console.error(e);
      } finally {
        setSwitchingProvider(false);
      }
    },
    [refresh]
  );

  if (!config) {
    return <div className="loading" />;
  }

  const activeProvider = config.providers[config.active_provider];
  const providerEntries = Object.entries(config.providers);

  return (
    <>
      <div className="claude-config-help" style={{ marginBottom: 12 }}>
        <p>{t("apiKeyPanel.helpText")}</p>
      </div>

      {/* Active provider info banner */}
      {activeProvider && (
        <div
          style={{
            padding: "10px 14px",
            background: activeProvider.supports_vision ? "#f0fdf4" : "#fefce8",
            border: activeProvider.supports_vision ? "1px solid #bbf7d0" : "1px solid #fde68a",
            borderRadius: 6,
            marginBottom: 12,
            fontSize: 12,
            color: "#1f2937",
            lineHeight: 1.5,
          }}
        >
          <strong>{t("apiKeyPanel.currentActive", { name: activeProvider.display_name })}</strong>
          <br />
          {activeProvider.supports_vision
            ? t("apiKeyPanel.visionSupported")
            : t("apiKeyPanel.visionNotSupported")}
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {providerEntries.map(([id, provider]) => (
          <ProviderCard
            key={id}
            providerId={id}
            provider={provider}
            active={id === config.active_provider}
            keyStatus={allKeyStatus?.[id] ?? null}
            onActivate={() => handleActivate(id)}
            onRefresh={refresh}
            switching={switchingProvider}
          />
        ))}
      </div>
    </>
  );
}
