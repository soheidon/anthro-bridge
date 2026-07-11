import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ApiKeyStatus, GatewayConfig, AllApiKeyStatus, ModelEntry } from "../types";
import {
  getProviderModels,
  CUSTOM_MODEL_SENTINEL,
  CUSTOM_MODEL_DEFAULTS,
  MODEL_CAPABILITIES,
  isKnownModel,
} from "../modelCapabilities";
import type { ThinkingModePolicy } from "../modelCapabilities";

const COL_STYLE: React.CSSProperties = {
  padding: "6px 10px",
  fontSize: 12,
  color: "#1f2937",
  whiteSpace: "nowrap",
};

function ModelSelector({
  providerId,
  modelKey,
  gatewayModelLabel,
  currentUpstream,
  thinkingModePolicy,
  currentThinkingMode,
  currentReasoningEffort,
  onSaved,
}: {
  providerId: string;
  modelKey: string;
  gatewayModelLabel: string;
  currentUpstream: string;
  thinkingModePolicy: ThinkingModePolicy;
  currentThinkingMode: string | undefined;
  currentReasoningEffort?: string;
  onSaved: () => void;
}) {
  const { t } = useTranslation();
  const providerModels = getProviderModels(providerId);
  const initialIsCustom = !!currentUpstream && !isKnownModel(currentUpstream) && currentUpstream !== "—";

  const [selected, setSelected] = useState(
    initialIsCustom
      ? CUSTOM_MODEL_SENTINEL
      : currentUpstream && providerModels.includes(currentUpstream)
        ? currentUpstream
        : providerModels[0] ?? CUSTOM_MODEL_SENTINEL,
  );
  const [customText, setCustomText] = useState(initialIsCustom ? currentUpstream : "");
  const [thinkingMode, setThinkingMode] = useState(
    currentThinkingMode === "normal" || currentThinkingMode === "thinking"
      ? currentThinkingMode
      : "normal",
  );
  const [reasoningEffort, setReasoningEffort] = useState(
    currentReasoningEffort === "high" || currentReasoningEffort === "medium" || currentReasoningEffort === "low"
      ? currentReasoningEffort
      : "",
  );
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // Sync when currentUpstream changes externally
  useEffect(() => {
    if (currentUpstream && providerModels.includes(currentUpstream)) {
      setSelected(currentUpstream);
      setCustomText("");
    } else if (currentUpstream && currentUpstream !== "—" && !isKnownModel(currentUpstream)) {
      setSelected(CUSTOM_MODEL_SENTINEL);
      setCustomText(currentUpstream);
    }
  }, [currentUpstream, providerModels]);

  // Sync thinking mode from config
  useEffect(() => {
    if (currentThinkingMode === "normal" || currentThinkingMode === "thinking") {
      setThinkingMode(currentThinkingMode);
    }
  }, [currentThinkingMode]);

  // Sync reasoning_effort from config
  useEffect(() => {
    if (currentReasoningEffort === "high" || currentReasoningEffort === "medium" || currentReasoningEffort === "low") {
      setReasoningEffort(currentReasoningEffort);
    } else {
      setReasoningEffort("");
    }
  }, [currentReasoningEffort]);

  const isCustom = selected === CUSTOM_MODEL_SENTINEL;
  const valueToSave = isCustom ? customText.trim() : selected;
  const selectedCaps = isCustom ? CUSTOM_MODEL_DEFAULTS : MODEL_CAPABILITIES[selected] ?? CUSTOM_MODEL_DEFAULTS;
  const supportsReasoningEffort = selectedCaps.supportsReasoningEffort;
  const isUnchanged = valueToSave === currentUpstream
    && (thinkingModePolicy !== "toggleable" || thinkingMode === (currentThinkingMode || "normal"))
    && (!supportsReasoningEffort || reasoningEffort === (currentReasoningEffort || ""));
  const canSave = !saving && valueToSave.length > 0 && !isUnchanged;

  const handleSave = async () => {
    if (!valueToSave) return;
    setSaving(true);
    setSaved(false);
    try {
      // Determine thinking_mode value from policy + selection
      let modeToSave: string | undefined;
      if (thinkingModePolicy === "thinking_only") {
        modeToSave = "thinking_only";
      } else if (thinkingModePolicy === "toggleable") {
        modeToSave = thinkingMode;
      }
      // For "unknown" (custom models), modeToSave stays undefined

      await invoke("set_model_upstream", {
        providerId,
        modelKey,
        upstreamModel: valueToSave,
        thinkingMode: modeToSave,
        reasoningEffort: supportsReasoningEffort && reasoningEffort ? reasoningEffort : undefined,
      });
      setSaving(false);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
      onSaved();
    } catch (e) {
      setSaving(false);
      console.error(e);
    }
  };

  // Refresh thinkingModePolicy when model changes
  const effectivePolicy: ThinkingModePolicy = isCustom ? "unknown" : thinkingModePolicy;

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
      <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
        {gatewayModelLabel}
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
        value={selected}
        onChange={(e) => {
          setSelected(e.target.value);
          if (e.target.value !== CUSTOM_MODEL_SENTINEL) setCustomText("");
        }}
      >
        {providerModels.map((m) => (
          <option key={m} value={m}>
            {m}
          </option>
        ))}
        <option value={CUSTOM_MODEL_SENTINEL}>{t("apiKeyPanel.customModel")}</option>
      </select>
      {isCustom && (
        <input
          style={{
            width: 220,
            padding: "4px 8px",
            fontSize: 11,
            fontFamily: "var(--font-mono)",
            background: "#fff",
            color: "#1f2937",
            border: "1px solid #d0d7de",
            borderRadius: 4,
            outline: "none",
          }}
          value={customText}
          onChange={(e) => setCustomText(e.target.value)}
          placeholder={t("apiKeyPanel.customPlaceholder")}
          spellCheck={false}
        />
      )}

      {/* Thinking mode selector */}
      {effectivePolicy === "toggleable" && (
        <>
          <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937" }}>
            {t("apiKeyPanel.thinkingMode")}:
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
              minWidth: 110,
            }}
            value={thinkingMode}
            onChange={(e) => setThinkingMode(e.target.value)}
          >
            <option value="normal">{t("apiKeyPanel.normalMode")}</option>
            <option value="thinking">{t("apiKeyPanel.thinkingModeOn")}</option>
          </select>
        </>
      )}
      {effectivePolicy === "thinking_only" && (
        <span style={{ fontSize: 11, color: "#6b7280", fontStyle: "italic" }}>
          {t("apiKeyPanel.thinkingOnly")}
        </span>
      )}

      {/* Reasoning effort selector (DeepSeek Pro models only) */}
      {providerId === "deepseek" && (
        <>
          <span style={{ fontSize: 11, fontWeight: 600, color: supportsReasoningEffort ? "#1f2937" : "#9ca3af" }}>
            {t("apiKeyPanel.reasoningEffort")}:
          </span>
          <select
            style={{
              padding: "4px 8px",
              fontSize: 11,
              fontFamily: "var(--font-mono)",
              background: supportsReasoningEffort ? "#fff" : "#f3f4f6",
              color: supportsReasoningEffort ? "#1f2937" : "#9ca3af",
              border: "1px solid #d0d7de",
              borderRadius: 4,
              outline: "none",
              minWidth: 90,
              cursor: supportsReasoningEffort ? "pointer" : "not-allowed",
            }}
            value={reasoningEffort}
            onChange={(e) => setReasoningEffort(e.target.value)}
            disabled={!supportsReasoningEffort}
          >
            <option value="">{t("apiKeyPanel.reasoningEffortUnset")}</option>
            <option value="high">{t("apiKeyPanel.reasoningEffortHigh")}</option>
            <option value="medium">{t("apiKeyPanel.reasoningEffortMedium")}</option>
            <option value="low">{t("apiKeyPanel.reasoningEffortLow")}</option>
          </select>
          {!supportsReasoningEffort && (
            <span style={{ fontSize: 10, color: "#9ca3af", fontStyle: "italic" }}>
              {t("apiKeyPanel.reasoningEffortFlashHint")}
            </span>
          )}
        </>
      )}

      <button
        className="btn btn-primary btn-small"
        onClick={handleSave}
        disabled={!canSave}
      >
        {saving ? "..." : t("apiKeyPanel.save")}
      </button>
      {saved && <span className="saved-toast">{t("apiKeyPanel.modelSaved")}</span>}
    </div>
  );
}

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

  const proModel = "claude-opus-4-8";
  const sonnetModel = "claude-sonnet-5";
  const haikuModel = "claude-haiku-4-5";
  const currentPro = models?.[proModel]?.upstream_model ?? "";
  const currentSonnet = models?.[sonnetModel]?.upstream_model ?? "";
  const currentHaiku = models?.[haikuModel]?.upstream_model ?? "";
  const currentProThinkingMode = models?.[proModel]?.thinking_mode;
  const currentSonnetThinkingMode = models?.[sonnetModel]?.thinking_mode;
  const currentHaikuThinkingMode = models?.[haikuModel]?.thinking_mode;

  // Determine thinkingModePolicy from current upstream model
  const proPolicy = isKnownModel(currentPro)
    ? MODEL_CAPABILITIES[currentPro].thinkingModePolicy
    : "unknown";
  const sonnetPolicy = isKnownModel(currentSonnet)
    ? MODEL_CAPABILITIES[currentSonnet].thinkingModePolicy
    : "unknown";
  const haikuPolicy = isKnownModel(currentHaiku)
    ? MODEL_CAPABILITIES[currentHaiku].thinkingModePolicy
    : "unknown";

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

          {/* Opus 4.8 model selector */}
          <ModelSelector
            providerId={providerId}
            modelKey={proModel}
            gatewayModelLabel={t("apiKeyPanel.gatewayPro")}
            currentUpstream={currentPro}
            thinkingModePolicy={proPolicy}
            currentThinkingMode={currentProThinkingMode}
            currentReasoningEffort={models?.[proModel]?.reasoning_effort}
            onSaved={onRefresh}
          />

          {/* Sonnet 5 model selector */}
          <ModelSelector
            providerId={providerId}
            modelKey={sonnetModel}
            gatewayModelLabel={t("apiKeyPanel.gatewayFlash")}
            currentUpstream={currentSonnet}
            thinkingModePolicy={sonnetPolicy}
            currentThinkingMode={currentSonnetThinkingMode}
            currentReasoningEffort={models?.[sonnetModel]?.reasoning_effort}
            onSaved={onRefresh}
          />

          {/* Haiku 4.5 model selector */}
          <ModelSelector
            providerId={providerId}
            modelKey={haikuModel}
            gatewayModelLabel={t("apiKeyPanel.gatewayHaiku")}
            currentUpstream={currentHaiku}
            thinkingModePolicy={haikuPolicy}
            currentThinkingMode={currentHaikuThinkingMode}
            currentReasoningEffort={models?.[haikuModel]?.reasoning_effort}
            onSaved={onRefresh}
          />
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
