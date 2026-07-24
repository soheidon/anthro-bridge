import { useState, useEffect, useCallback, useRef } from "react";
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
import OpenRouterModelSelector from "./OpenRouterModelSelector";

const COL_STYLE: React.CSSProperties = {
  padding: "6px 10px",
  fontSize: 12,
  color: "#1f2937",
  whiteSpace: "nowrap",
};

type SaveStatus = "idle" | "saving" | "saved" | "error";

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
    currentReasoningEffort === "high" || currentReasoningEffort === "medium" || currentReasoningEffort === "low" || currentReasoningEffort === "max"
      ? currentReasoningEffort
      : "",
  );
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const requestIdRef = useRef(0);
  const statusTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

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

  useEffect(() => {
    if (currentThinkingMode === "normal" || currentThinkingMode === "thinking") {
      setThinkingMode(currentThinkingMode);
    }
  }, [currentThinkingMode]);

  useEffect(() => {
    if (currentReasoningEffort === "high" || currentReasoningEffort === "medium" || currentReasoningEffort === "low" || currentReasoningEffort === "max") {
      setReasoningEffort(currentReasoningEffort);
    } else {
      setReasoningEffort("");
    }
  }, [currentReasoningEffort]);

  // Cleanup status timer on unmount
  useEffect(() => {
    return () => {
      if (statusTimerRef.current) clearTimeout(statusTimerRef.current);
    };
  }, []);

  const isCustom = selected === CUSTOM_MODEL_SENTINEL;
  const valueToSave = isCustom ? customText.trim() : selected;
  const selectedCaps = isCustom ? CUSTOM_MODEL_DEFAULTS : MODEL_CAPABILITIES[selected] ?? CUSTOM_MODEL_DEFAULTS;
  const supportsReasoningEffort = selectedCaps.supportsReasoningEffort || !!selectedCaps.forcedReasoningEffort;
  const forcedEffort = selectedCaps.forcedReasoningEffort; // "max" for K3, undefined otherwise

  // Auto-save: invoke with the complete tier config
  const autoSave = useCallback(
    async (
      upstreamModel: string,
      nextThinkingMode: string | undefined,
      nextEffort: string | null,
      capsSupportsEffort: boolean,
    ) => {
      if (!upstreamModel) return;
      const reqId = ++requestIdRef.current;
      setSaveStatus("saving");
      if (statusTimerRef.current) clearTimeout(statusTimerRef.current);

      try {
        await invoke("set_model_upstream", {
          providerId,
          modelKey,
          upstreamModel,
          thinkingMode: nextThinkingMode,
          reasoningEffort: capsSupportsEffort && nextEffort ? nextEffort : null,
        });
        // Only update if this is still the latest request
        if (reqId === requestIdRef.current) {
          setSaveStatus("saved");
          statusTimerRef.current = setTimeout(() => setSaveStatus("idle"), 2000);
          onSaved();
        }
      } catch (e) {
        if (reqId === requestIdRef.current) {
          setSaveStatus("error");
        }
      }
    },
    [providerId, modelKey, onSaved],
  );

  const handleModelChange = (newModel: string) => {
    setSelected(newModel);
    const nextIsCustom = newModel === CUSTOM_MODEL_SENTINEL;
    const upstream = nextIsCustom ? customText.trim() : newModel;
    const nextCaps = nextIsCustom ? CUSTOM_MODEL_DEFAULTS : MODEL_CAPABILITIES[newModel] ?? CUSTOM_MODEL_DEFAULTS;
    const nextSupportsEffort = nextCaps.supportsReasoningEffort || !!nextCaps.forcedReasoningEffort;
    const nextForcedEffort = nextCaps.forcedReasoningEffort;
    // For K3 (forcedEffort): use "max"; for models without effort support: clear
    const nextEffort = nextForcedEffort ?? (nextCaps.supportsReasoningEffort ? reasoningEffort : "");
    if (!nextSupportsEffort) setReasoningEffort("");
    else if (nextForcedEffort) setReasoningEffort(nextForcedEffort);
    if (newModel !== CUSTOM_MODEL_SENTINEL) setCustomText("");

    let modeToSave: string | undefined;
    if (thinkingModePolicy === "thinking_only") {
      modeToSave = "thinking_only";
    } else if (thinkingModePolicy === "toggleable") {
      modeToSave = thinkingMode;
    }

    autoSave(upstream, modeToSave, nextEffort, nextSupportsEffort);
  };

  const handleThinkingModeChange = (newMode: string) => {
    setThinkingMode(newMode);
    autoSave(valueToSave, newMode, reasoningEffort, supportsReasoningEffort);
  };

  const handleReasoningEffortChange = (newEffort: string) => {
    setReasoningEffort(newEffort);

    let modeToSave: string | undefined;
    if (thinkingModePolicy === "thinking_only") {
      modeToSave = "thinking_only";
    } else if (thinkingModePolicy === "toggleable") {
      modeToSave = thinkingMode;
    }

    autoSave(valueToSave, modeToSave, newEffort, supportsReasoningEffort);
  };

  const handleCustomTextBlur = () => {
    const trimmed = customText.trim();
    if (!trimmed || trimmed === currentUpstream) return;

    let modeToSave: string | undefined;
    if (thinkingModePolicy === "thinking_only") {
      modeToSave = "thinking_only";
    } else if (thinkingModePolicy === "toggleable") {
      modeToSave = thinkingMode;
    }

    autoSave(trimmed, modeToSave, reasoningEffort, supportsReasoningEffort);
  };

  const effectivePolicy: ThinkingModePolicy = isCustom ? "unknown" : thinkingModePolicy;

  const statusText =
    saveStatus === "saving" ? t("apiKeyPanel.savingStatus") :
    saveStatus === "saved" ? t("apiKeyPanel.savedStatus") :
    saveStatus === "error" ? t("apiKeyPanel.errorStatus") : null;

  const statusColor =
    saveStatus === "saving" ? "#6b7280" :
    saveStatus === "saved" ? "#107c10" :
    saveStatus === "error" ? "var(--error)" : "#6b7280";

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
        onChange={(e) => handleModelChange(e.target.value)}
      >
        {providerModels.map((m) => (
          <option key={m} value={m}>{m}</option>
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
          onBlur={handleCustomTextBlur}
          onKeyDown={(e) => { if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur(); }}
          placeholder={t("apiKeyPanel.customPlaceholder")}
          spellCheck={false}
          onClick={(e) => e.stopPropagation()}
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
            onChange={(e) => handleThinkingModeChange(e.target.value)}
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

      {/* Reasoning effort — K3: fixed Max display only */}
      {forcedEffort && (
        <>
          <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937" }}>
            {t("apiKeyPanel.reasoningEffort")}:
          </span>
          <span style={{
            padding: "4px 8px",
            fontSize: 11,
            fontFamily: "var(--font-mono)",
            background: "#f3f4f6",
            color: "#1f2937",
            border: "1px solid #d0d7de",
            borderRadius: 4,
          }}>
            {t("apiKeyPanel.reasoningEffortMaxFixed")}
          </span>
          <span style={{ fontSize: 10, color: "#6b7280", fontStyle: "italic" }}>
            {t("apiKeyPanel.reasoningEffortMaxHint")}
          </span>
        </>
      )}

      {/* Reasoning effort — DeepSeek: normal selector */}
      {providerId === "deepseek" && supportsReasoningEffort && !forcedEffort && (
        <>
          <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937" }}>
            {t("apiKeyPanel.reasoningEffort")}:
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
              minWidth: 90,
              cursor: "pointer",
            }}
            value={reasoningEffort}
            onChange={(e) => handleReasoningEffortChange(e.target.value)}
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
      {providerId === "deepseek" && !supportsReasoningEffort && !forcedEffort && effectivePolicy !== "thinking_only" && (
        <span style={{ fontSize: 10, color: "#9ca3af", fontStyle: "italic" }}>
          {t("apiKeyPanel.reasoningEffortFlashHint")}
        </span>
      )}

      {/* Save status indicator */}
      {statusText && (
        <span style={{ fontSize: 10, color: statusColor, marginLeft: 4 }}>{statusText}</span>
      )}
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
  const [envVarName, setEnvVarName] = useState(provider.api_key_env);
  const [envVarError, setEnvVarError] = useState<string | null>(null);
  const [envVarStatus, setEnvVarStatus] = useState<SaveStatus>("idle");
  const [keyStatus_, setKeyStatusLocal] = useState<SaveStatus>("idle");
  const envTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const keyTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const proModel = "claude-opus-4-8";
  const sonnetModel = "claude-sonnet-5";
  const haikuModel = "claude-haiku-4-5";
  const currentPro = models?.[proModel]?.upstream_model ?? "";
  const currentSonnet = models?.[sonnetModel]?.upstream_model ?? "";
  const currentHaiku = models?.[haikuModel]?.upstream_model ?? "";

  const proPolicy = isKnownModel(currentPro) ? MODEL_CAPABILITIES[currentPro].thinkingModePolicy : "unknown";
  const sonnetPolicy = isKnownModel(currentSonnet) ? MODEL_CAPABILITIES[currentSonnet].thinkingModePolicy : "unknown";
  const haikuPolicy = isKnownModel(currentHaiku) ? MODEL_CAPABILITIES[currentHaiku].thinkingModePolicy : "unknown";

  useEffect(() => {
    setEnvVarName(provider.api_key_env);
  }, [provider.api_key_env]);

  useEffect(() => {
    return () => {
      if (envTimerRef.current) clearTimeout(envTimerRef.current);
      if (keyTimerRef.current) clearTimeout(keyTimerRef.current);
    };
  }, []);

  const toggleExpanded = () => setExpanded((prev) => !prev);

  const handleHeaderClick = () => {
    toggleExpanded();
  };

  const handleHeaderKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggleExpanded();
    }
  };

  // Save env var name on blur/Enter
  const handleEnvVarSave = async () => {
    const trimmed = envVarName.trim();
    if (!trimmed || trimmed === provider.api_key_env) return;
    if (!/^[A-Z][A-Z0-9_]*$/.test(trimmed)) {
      setEnvVarError(t("apiKeyPanel.envVarErrorFormat"));
      return;
    }
    setEnvVarError(null);
    setEnvVarStatus("saving");
    if (envTimerRef.current) clearTimeout(envTimerRef.current);
    try {
      await invoke("update_provider_api_key_env", { providerId, apiKeyEnv: trimmed });
      setEnvVarStatus("saved");
      envTimerRef.current = setTimeout(() => setEnvVarStatus("idle"), 2000);
      onRefresh();
    } catch (e) {
      setEnvVarStatus("error");
      setEnvVarError(String(e));
    }
  };

  // Save API key — explicit button or Enter
  const handleKeySave = async () => {
    const trimmed = keyText.trim();
    if (!trimmed || !keyStatus || keyStatus_ === "saving") return;
    setKeyStatusLocal("saving");
    if (keyTimerRef.current) clearTimeout(keyTimerRef.current);
    try {
      await invoke("set_env_api_key", { key: trimmed, envVarName: keyStatus.env_var });
      setKeyStatusLocal("saved");
      setKeyText("");
      keyTimerRef.current = setTimeout(() => setKeyStatusLocal("idle"), 2000);
      onRefresh();
    } catch {
      setKeyStatusLocal("error");
    }
  };

  const envStatusText =
    envVarStatus === "saving" ? t("apiKeyPanel.savingStatus") :
    envVarStatus === "saved" ? t("apiKeyPanel.savedStatus") :
    envVarStatus === "error" ? t("apiKeyPanel.errorStatus") : null;

  const keyStatusText =
    keyStatus_ === "saving" ? t("apiKeyPanel.savingStatus") :
    keyStatus_ === "saved" ? t("apiKeyPanel.savedStatus") :
    keyStatus_ === "error" ? t("apiKeyPanel.errorStatus") : null;

  return (
    <div>
      {/* Clickable header row */}
      <div
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={handleHeaderClick}
        onKeyDown={handleHeaderKeyDown}
        style={{
          display: "flex",
          alignItems: "center",
          background: "#ffffff",
          borderTop: "1px solid #e5e7eb",
          borderBottom: expanded ? "none" : "1px solid #e5e7eb",
          cursor: "pointer",
          transition: "background 0.1s",
        }}
        onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "#f8f9fa"; }}
        onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "#ffffff"; }}
      >
        <div style={{ ...COL_STYLE, fontSize: 14, color: "#6b7280", userSelect: "none", padding: "6px 4px 6px 8px", minWidth: 28 }}>
          {expanded ? "▾" : "▸"}
        </div>

        <div style={{ ...COL_STYLE, fontWeight: 600, minWidth: 130, fontSize: 13, padding: "6px 4px" }}>
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
          onClick={(e) => e.stopPropagation()}
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
              onBlur={handleEnvVarSave}
              onKeyDown={(e) => { if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur(); }}
              placeholder="MOONSHOT_API_KEY"
              spellCheck={false}
              onClick={(e) => e.stopPropagation()}
            />
            {envStatusText && (
              <span style={{ fontSize: 10, color: envVarStatus === "error" ? "var(--error)" : "#107c10" }}>
                {envStatusText}
              </span>
            )}
            {envVarError && (
              <span style={{ fontSize: 10, color: "var(--error)" }}>{envVarError}</span>
            )}
          </div>

          {/* API key input — explicit save button */}
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
              {t("apiKeyPanel.header")}
            </span>
            <input
              type="password"
              style={{
                width: 300,
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
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  void handleKeySave();
                }
              }}
              placeholder="sk-..."
              spellCheck={false}
              onClick={(e) => e.stopPropagation()}
            />
            <button
              className="btn btn-primary btn-small"
              onClick={handleKeySave}
              disabled={!keyText.trim() || keyStatus_ === "saving"}
            >
              {keyStatus_ === "saving" ? "..." : t("apiKeyPanel.saveKey")}
            </button>
            {keyStatusText && (
              <span style={{ fontSize: 10, color: keyStatus_ === "error" ? "var(--error)" : "#107c10" }}>
                {keyStatusText}
              </span>
            )}
          </div>

          {/* OpenRouter uses its own model selector; other providers use ModelSelector */}
          {providerId === "openrouter" ? (
            <>
              <OpenRouterModelSelector
                modelKey={proModel}
                gatewayModelLabel={t("apiKeyPanel.gatewayPro")}
                currentUpstream={currentPro}
                currentThinkingMode={models?.[proModel]?.thinking_mode}
                onSaved={onRefresh}
              />
              <OpenRouterModelSelector
                modelKey={sonnetModel}
                gatewayModelLabel={t("apiKeyPanel.gatewayFlash")}
                currentUpstream={currentSonnet}
                currentThinkingMode={models?.[sonnetModel]?.thinking_mode}
                onSaved={onRefresh}
              />
              <OpenRouterModelSelector
                modelKey={haikuModel}
                gatewayModelLabel={t("apiKeyPanel.gatewayHaiku")}
                currentUpstream={currentHaiku}
                currentThinkingMode={models?.[haikuModel]?.thinking_mode}
                onSaved={onRefresh}
              />
            </>
          ) : (
            <>
              {/* Opus 4.8 model selector */}
              <ModelSelector
                providerId={providerId}
                modelKey={proModel}
                gatewayModelLabel={t("apiKeyPanel.gatewayPro")}
                currentUpstream={currentPro}
                thinkingModePolicy={proPolicy}
                currentThinkingMode={models?.[proModel]?.thinking_mode}
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
                currentThinkingMode={models?.[sonnetModel]?.thinking_mode}
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
                currentThinkingMode={models?.[haikuModel]?.thinking_mode}
                currentReasoningEffort={models?.[haikuModel]?.reasoning_effort}
                onSaved={onRefresh}
              />
            </>
          )}
        </div>
      )}
    </div>
  );
}

export default function ApiKeyPanel({ onConfigChanged }: { onConfigChanged?: () => void }) {
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

  // Called after model/env-var saves to notify dashboard components
  const refreshAndNotify = useCallback(() => {
    refresh();
    onConfigChanged?.();
  }, [refresh, onConfigChanged]);

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
            onRefresh={refreshAndNotify}
          />
        ))}
      </div>
    </div>
  );
}
