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
  }, [providerId, state.envVarName, onRefresh]);

  const capabilityBadge = (label: string, supported: boolean) => (
    <span
      key={label}
      style={{
        fontSize: 10,
        padding: "1px 6px",
        borderRadius: 3,
        fontWeight: 600,
        background: supported ? "var(--accent-green)" : "var(--bg-input, #1a1a2e)",
        color: supported ? "#fff" : "var(--text-muted)",
        opacity: supported ? 1 : 0.5,
      }}
    >
      {label}
    </span>
  );

  return (
    <div
      style={{
        background: "var(--bg-panel, #13132b)",
        border: active ? "2px solid var(--accent, #7c5cfc)" : "1px solid var(--border, #333)",
        borderRadius: 8,
        padding: "12px 14px",
        display: "flex",
        flexDirection: "column",
        gap: 8,
        position: "relative",
      }}
    >
      {/* Header row */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 14, fontWeight: 700, color: "var(--text-primary)" }}>
            {provider.display_name}
          </span>
          {active && (
            <span
              style={{
                fontSize: 10,
                fontWeight: 700,
                padding: "1px 8px",
                borderRadius: 4,
                background: "var(--accent, #7c5cfc)",
                color: "#fff",
              }}
            >
              {t("apiKeyPanel.badgeActive")}
            </span>
          )}
        </div>
        {!active && (
          <button
            className="btn btn-small btn-primary"
            onClick={onActivate}
            disabled={switching}
          >
            {switching ? "..." : t("apiKeyPanel.setActive")}
          </button>
        )}
      </div>

      {/* Capabilities */}
      <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
        {capabilityBadge(t("apiKeyPanel.capVision"), provider.supports_vision)}
        {capabilityBadge(t("apiKeyPanel.capVideo"), provider.supports_video)}
        {capabilityBadge(t("apiKeyPanel.capThinking"), provider.supports_thinking)}
        {capabilityBadge(t("apiKeyPanel.capCountTokens"), provider.supports_count_tokens)}
      </div>

      {/* Separator */}
      <div style={{ borderTop: "1px solid var(--border, #333)" }} />

      {/* Env var name row */}
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <span style={{ fontSize: 10, fontWeight: 600, color: "var(--text-muted)", minWidth: 70 }}>
          {t("apiKeyPanel.envVarLabel")}
        </span>
        <input
          style={{
            flex: 1,
            maxWidth: 240,
            padding: "3px 6px",
            fontSize: 11,
            fontFamily: "monospace",
            background: "var(--bg-input, #1a1a2e)",
            color: "var(--text-primary)",
            border: state.envVarError
              ? "1px solid var(--error)"
              : "1px solid var(--border, #333)",
            borderRadius: 4,
          }}
          value={state.envVarName}
          onChange={(e) => {
            updateState({ envVarName: e.target.value.toUpperCase(), envVarError: null });
          }}
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
        <span style={{ fontSize: 10, color: "var(--error)", marginLeft: 76 }}>
          {state.envVarError}
        </span>
      )}

      {/* API key status row */}
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <span style={{ fontSize: 10, fontWeight: 600, color: "var(--text-muted)", minWidth: 70 }}>
          {t("apiKeyPanel.header")}
        </span>
        {keyStatus === null ? (
          <span style={{ fontSize: 10, color: "var(--text-muted)" }}>...</span>
        ) : keyStatus.set ? (
          <span style={{ fontSize: 10, color: "var(--accent-green)", fontWeight: 600 }}>
            {t("apiKeyPanel.set")} ({keyStatus.env_var})
          </span>
        ) : (
          <span style={{ fontSize: 10, color: "var(--error)", fontWeight: 600 }}>
            {t("apiKeyPanel.notSet")} ({keyStatus.env_var})
          </span>
        )}
      </div>

      {/* API key input row */}
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        <input
          className="api-key-input"
          type="password"
          value={state.keyText}
          onChange={(e) => updateState({ keyText: e.target.value })}
          placeholder="sk-..."
          style={{ flex: 1, maxWidth: 340, fontSize: 11 }}
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

  const providerEntries = Object.entries(config.providers);

  return (
    <>
      <div className="claude-config-help" style={{ marginBottom: 12 }}>
        <p>{t("apiKeyPanel.helpText")}</p>
      </div>

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
