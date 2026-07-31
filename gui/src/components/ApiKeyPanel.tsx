import { useState, useEffect, useCallback, useRef, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ApiKeyStatus, GatewayConfig, AllApiKeyStatus, ModelEntry, CommandResponse } from "../types";
import {
  getProviderModels,
  CUSTOM_MODEL_SENTINEL,
  CUSTOM_MODEL_DEFAULTS,
  MODEL_CAPABILITIES,
  isKnownModel,
} from "../modelCapabilities";
import type { ThinkingModePolicy, ThinkingOption } from "../modelCapabilities";
import OpenRouterProviderSection, { parseAutoModelSetNumber } from "./OpenRouterProviderSection";
import type { OpenRouterProfile } from "../types";

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
  gatewayRunning,
  restartGateway,
}: {
  providerId: string;
  modelKey: string;
  gatewayModelLabel: string;
  currentUpstream: string;
  thinkingModePolicy: ThinkingModePolicy;
  currentThinkingMode: string | undefined;
  currentReasoningEffort?: string;
  onSaved: () => Promise<void>;
  gatewayRunning: boolean;
  restartGateway: () => Promise<void>;
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
  // For "forced" policy (OpenRouter Laguna): derive selected option from current thinking_mode + effort
  const [forcedOption, setForcedOption] = useState<ThinkingOption>(() => {
    if (currentThinkingMode === "normal") return "off";
    if (currentThinkingMode === "thinking" && currentReasoningEffort === "max") return "max";
    if (currentThinkingMode === "thinking") return "on";
    // No config yet: use model default (first in forcedThinkingOptions)
    return "off";
  });
  // Sync forcedOption when policy is forced but thinking mode changes externally
  useEffect(() => {
    if (thinkingModePolicy !== "forced") return;
    if (currentThinkingMode === "normal") setForcedOption("off");
    else if (currentThinkingMode === "thinking" && currentReasoningEffort === "max") setForcedOption("max");
    else if (currentThinkingMode === "thinking") setForcedOption("on");
  }, [thinkingModePolicy, currentThinkingMode, currentReasoningEffort]);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");
  const [saveError, setSaveError] = useState<string | null>(null);
  const pendingSaveRef = useRef<{
    upstreamModel: string;
    nextThinkingMode: string | undefined;
    nextEffort: string | null;
    capsSupportsEffort: boolean;
  } | null>(null);
  const savingRef = useRef(false);
  const statusTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const mountedRef = useRef(true);

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

  // Cleanup on unmount: prevent post-unmount setState, side-effects,
  // and tail-kick autoSave.  Also clears any pending status timer.
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      pendingSaveRef.current = null;
      if (statusTimerRef.current) {
        clearTimeout(statusTimerRef.current);
      }
    };
  }, []);

  const isCustom = selected === CUSTOM_MODEL_SENTINEL;
  const valueToSave = isCustom ? customText.trim() : selected;
  const selectedCaps = isCustom ? CUSTOM_MODEL_DEFAULTS : MODEL_CAPABILITIES[selected] ?? CUSTOM_MODEL_DEFAULTS;
  const supportsReasoningEffort = selectedCaps.supportsReasoningEffort || !!selectedCaps.forcedReasoningEffort;
  const forcedEffort = selectedCaps.forcedReasoningEffort; // "max" for K3, undefined otherwise

  // Auto-save: enqueue the latest request; drain saves one at a time.
  // After all pending saves drain, run refreshConfig and (if needed)
  // restartGateway once for the batch.  Restart is OR-aggregated across
  // every successful save in the batch — a trailing no-op must not
  // suppress a restart required by an earlier change.
  //
  // Tail kick lives INSIDE `finally` so it fires even when post-save
  // returns early (onSaved or restartGateway throws).
  //
  // Guarantees:
  //  - Save order never reverses; the most recent user selection
  //    ultimately lands in config.json.
  //  - refreshConfig fires once per batch (after all saves drain).
  //  - restartGateway fires once per batch if ANY successful save
  //    required it (not just the last).
  //  - New requests queued during post-save start a fresh batch
  //    (even when post-save itself fails).
  const autoSave = useCallback(
    async (
      upstreamModel: string,
      nextThinkingMode: string | undefined,
      nextEffort: string | null,
      capsSupportsEffort: boolean,
    ) => {
      if (!upstreamModel) return;
      pendingSaveRef.current = { upstreamModel, nextThinkingMode, nextEffort, capsSupportsEffort };
      if (savingRef.current) return; // already draining, latest will be picked up
      savingRef.current = true;
      if (mountedRef.current) {
        setSaveStatus("saving");
        setSaveError(null);
      }
      if (statusTimerRef.current) clearTimeout(statusTimerRef.current);

      try {
        // ── Phase 1: drain all pending saves in order ──────────
        //
        // Three distinct flags:
        //   anySaveSucceeded     — did ANY request write to disk?
        //   batchNeedsRestart    — did ANY successful save need restart? (OR)
        //   lastAttemptSucceeded — did the USER'S MOST-RECENT request succeed?
        let batchNeedsRestart = false;
        let anySaveSucceeded = false;
        let lastAttemptSucceeded = false;
        while (pendingSaveRef.current) {
          const current = pendingSaveRef.current;
          pendingSaveRef.current = null;
          lastAttemptSucceeded = false;

          try {
            const response = await invoke<CommandResponse<void>>("set_model_upstream", {
              providerId,
              modelKey,
              upstreamModel: current.upstreamModel,
              thinkingMode: current.nextThinkingMode,
              reasoningEffort: current.capsSupportsEffort && current.nextEffort ? current.nextEffort : null,
            });
            batchNeedsRestart = batchNeedsRestart || response.restartGateway;
            anySaveSucceeded = true;
            lastAttemptSucceeded = true;
          } catch (e) {
            // Suppress error when a newer request already superseded this one
            if (mountedRef.current && !pendingSaveRef.current) {
              setSaveStatus("error");
              setSaveError(String(e));
            }
          }
        }

        // ── Phase 2: post-save once for the batch ──────────────
        //
        // Run refresh/restart for any successful save regardless of
        // whether the last attempt failed — prior successes must be
        // reflected even when the last request errored.

        if (!mountedRef.current || !anySaveSucceeded) {
          return;
        }

        // Refresh
        try {
          await onSaved();
        } catch (e) {
          if (mountedRef.current) {
            setSaveStatus("error");
            setSaveError(t("openRouterModels.saveOkRefreshFailed", { error: String(e) }));
          }
          return;
        }

        // Re-check mount after onSaved — do not start restartGateway
        // if the component unmounted during onSaved.
        if (!mountedRef.current) return;

        // Restart (OR-aggregated across the batch)
        if (gatewayRunning && batchNeedsRestart) {
          try {
            await restartGateway();
          } catch (e) {
            if (mountedRef.current) {
              setSaveStatus("error");
              setSaveError(t("openRouterModels.saveOkRestartFailed", { error: String(e) }));
            }
            return;
          }
        }

        // Final display: reflect the last attempt's outcome.
        // If the last save failed, the error display from Phase 1
        // is preserved — never overwrite with "saved".
        if (mountedRef.current && lastAttemptSucceeded) {
          setSaveStatus("saved");
          setSaveError(null);
          statusTimerRef.current = setTimeout(() => {
            if (mountedRef.current) {
              setSaveStatus("idle");
            }
          }, 2000);
        }
      } finally {
        savingRef.current = false;

        // ── Tail kick (INSIDE finally): requests queued during
        //     post-save start a fresh batch.  This fires even when
        //     onSaved or restartGateway threw. ──────────────────
        const tail = (pendingSaveRef as MutableRefObject<{
          upstreamModel: string;
          nextThinkingMode: string | undefined;
          nextEffort: string | null;
          capsSupportsEffort: boolean;
        } | null>).current;
        if (mountedRef.current && tail) {
          void autoSave(tail.upstreamModel, tail.nextThinkingMode, tail.nextEffort, tail.capsSupportsEffort);
        }
      }
    },
    [providerId, modelKey, onSaved, gatewayRunning, restartGateway, t],
  );

  const handleModelChange = (newModel: string) => {
    setSelected(newModel);
    const nextIsCustom = newModel === CUSTOM_MODEL_SENTINEL;
    const upstream = nextIsCustom ? customText.trim() : newModel;
    const nextCaps = nextIsCustom ? CUSTOM_MODEL_DEFAULTS : MODEL_CAPABILITIES[newModel] ?? CUSTOM_MODEL_DEFAULTS;
    const nextSupportsEffort = nextCaps.supportsReasoningEffort || !!nextCaps.forcedReasoningEffort;
    const nextForcedEffort = nextCaps.forcedReasoningEffort;
    const nextForcedOptions = nextCaps.forcedThinkingOptions;
    // For K3 (forcedThinkingOptions, no forcedEffort): default to first option if current doesn't match
    const nextEffort = nextForcedEffort
      ?? (nextCaps.supportsReasoningEffort
        ? (nextForcedOptions && !nextForcedOptions.includes(reasoningEffort as ThinkingOption)
          ? (nextForcedOptions[0] ?? "max")
          : reasoningEffort)
        : "");
    if (!nextSupportsEffort) setReasoningEffort("");
    else if (nextForcedEffort) setReasoningEffort(nextForcedEffort);
    else if (nextForcedOptions && !nextForcedOptions.includes(reasoningEffort as ThinkingOption)) {
      setReasoningEffort(nextForcedOptions[0] ?? "max");
    }
    if (newModel !== CUSTOM_MODEL_SENTINEL) setCustomText("");

    let modeToSave: string | undefined;
    let nextEffortVal = nextEffort;
    if (thinkingModePolicy === "thinking_only") {
      modeToSave = "thinking_only";
    } else if (thinkingModePolicy === "toggleable") {
      modeToSave = thinkingMode;
    } else if (thinkingModePolicy === "forced") {
      // For forced models, reset to model default
      const defaultOpt = nextCaps.forcedThinkingOptions?.[0] ?? "off";
      setForcedOption(defaultOpt);
      setReasoningEffort("");
      if (defaultOpt === "max") {
        modeToSave = "thinking";
        nextEffortVal = "max";
      } else if (defaultOpt === "on") {
        modeToSave = "thinking";
        nextEffortVal = "";
      } else {
        modeToSave = "normal";
        nextEffortVal = "";
      }
      autoSave(upstream, modeToSave, nextEffortVal, nextSupportsEffort);
      return;
    }

    autoSave(upstream, modeToSave, nextEffortVal, nextSupportsEffort);
  };

  const handleThinkingModeChange = (newMode: string) => {
    setThinkingMode(newMode);
    autoSave(valueToSave, newMode, reasoningEffort, supportsReasoningEffort);
  };

  const handleForcedOptionChange = (opt: ThinkingOption) => {
    setForcedOption(opt);
    let modeToSave: string;
    let effToSave: string;
    if (opt === "max") {
      modeToSave = "thinking";
      effToSave = "max";
      setReasoningEffort("max");
    } else if (opt === "on") {
      modeToSave = "thinking";
      effToSave = "";
      setReasoningEffort("");
    } else {
      modeToSave = "normal";
      effToSave = "";
      setReasoningEffort("");
    }
    autoSave(valueToSave, modeToSave, effToSave, supportsReasoningEffort);
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

      {/* Thinking mode selector: forced (OpenRouter Laguna) */}
      {effectivePolicy === "forced" && (
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
              minWidth: 80,
            }}
            value={forcedOption}
            onChange={(e) => handleForcedOptionChange(e.target.value as ThinkingOption)}
          >
            {(selectedCaps.forcedThinkingOptions ?? ["max", "on", "off"]).map((opt) => (
              <option key={opt} value={opt}>
                {opt === "max" ? "Max" : opt === "on" ? "On" : "Off"}
              </option>
            ))}
          </select>
        </>
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

      {/* Reasoning effort — K3: low/high/max selector */}
      {providerId === "kimi" && supportsReasoningEffort && !forcedEffort && selectedCaps.forcedThinkingOptions && (
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
            value={reasoningEffort || "max"}
            onChange={(e) => handleReasoningEffortChange(e.target.value)}
          >
            {selectedCaps.forcedThinkingOptions.map((opt) => (
              <option key={opt} value={opt}>
                {opt === "max" ? t("apiKeyPanel.reasoningEffortMaxFixed") : opt === "high" ? t("apiKeyPanel.reasoningEffortHigh") : t("apiKeyPanel.reasoningEffortLow")}
              </option>
            ))}
          </select>
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
      {saveError && (
        <span style={{ fontSize: 10, color: "var(--error)", marginLeft: 4 }} title={saveError}>
          {saveError}
        </span>
      )}
    </div>
  );
}

function ProviderRow({
  providerId,
  provider,
  keyStatus,
  models,
  refreshConfig,
  gatewayRunning,
  restartGateway,
}: {
  providerId: string;
  provider: { display_name: string; api_key_env: string };
  keyStatus: ApiKeyStatus | null;
  models: Record<string, ModelEntry> | undefined;
  refreshConfig: () => Promise<void>;
  gatewayRunning: boolean;
  restartGateway: () => Promise<void>;
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

  const proModel = "claude-opus-5";
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
      refreshConfig();
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
      refreshConfig();
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

        <div style={{ ...COL_STYLE, fontFamily: "var(--font-mono)", fontSize: 11, minWidth: 150, color: "#374151" }}>
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

        {/* no actions for non-OpenRouter providers */}
        <div style={{ display: "flex", alignItems: "center", gap: 4, paddingRight: 4, flex: 1, justifyContent: "flex-end" }} />
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

          {/* Model selectors */}
          {/* Opus 5 model selector */}
          <ModelSelector
            providerId={providerId}
            modelKey={proModel}
            gatewayModelLabel={t("apiKeyPanel.gatewayPro")}
            currentUpstream={currentPro}
            thinkingModePolicy={proPolicy}
            currentThinkingMode={models?.[proModel]?.thinking_mode}
            currentReasoningEffort={models?.[proModel]?.reasoning_effort}
            onSaved={refreshConfig}
            gatewayRunning={gatewayRunning}
            restartGateway={restartGateway}
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
            onSaved={refreshConfig}
            gatewayRunning={gatewayRunning}
            restartGateway={restartGateway}
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
            onSaved={refreshConfig}
            gatewayRunning={gatewayRunning}
            restartGateway={restartGateway}
          />
        </div>
      )}
    </div>
  );
}

export default function ApiKeyPanel({
  config,
  refreshConfig,
  gatewayRunning,
  restartGateway,
}: {
  config: GatewayConfig | null;
  refreshConfig: () => Promise<void>;
  gatewayRunning: boolean;
  restartGateway: () => Promise<void>;
}) {
  const { t } = useTranslation();
  const [allKeyStatus, setAllKeyStatus] = useState<AllApiKeyStatus | null>(null);

  const [addError, setAddError] = useState<string | null>(null);

  // Load API key statuses on mount and when config changes
  useEffect(() => {
    invoke<AllApiKeyStatus>("check_all_api_keys")
      .then(setAllKeyStatus)
      .catch(() => setAllKeyStatus(null));
  }, [config]);

  // ── OpenRouter model set auto-numbering ──────────────────────────
  // Must match Rust paths::parse_model_set_number — canonical "Model N" only.

  function nextModelSetNumber(profiles: OpenRouterProfile[]): number {
    const used = new Set<number>();
    for (const profile of profiles) {
      const number = parseAutoModelSetNumber(profile.display_name);
      if (number !== null) used.add(number);
    }
    let n = 1;
    while (used.has(n)) n++;
    return n;
  }

  const handleAddProfile = useCallback(async () => {
    setAddError(null);

    const openRouterProvider = config?.providers["openrouter"];
    const profiles = openRouterProvider?.profiles ?? [];
    const number = nextModelSetNumber(profiles);
    const name = `Model ${number}`;

    let res: CommandResponse;
    try {
      res = await invoke<CommandResponse>("add_openrouter_profile", { name });
    } catch (e) {
      setAddError(String(e));
      return;
    }
    try {
      await refreshConfig();
    } catch (e) {
      setAddError(`Saved, but screen reload failed: ${String(e)}`);
      return;
    }
    if (gatewayRunning && res.restartGateway) {
      try {
        await restartGateway();
      } catch (e) {
        setAddError(`Saved, but gateway restart failed: ${String(e)}`);
      }
    }
  }, [config, refreshConfig, gatewayRunning, restartGateway]);

  if (!config) {
    return <div className="loading" />;
  }

  const providerEntries = Object.entries(config.providers);
  const activeOpenRouterProfileId = config.active_openrouter_profile_id;

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
        <div style={{ ...COL_STYLE, fontWeight: 600, fontSize: 10, color: "#6b7280", minWidth: 150 }}>
          Env Var
        </div>
        <div style={{ minWidth: 60, padding: "2px 8px", fontSize: 10, fontWeight: 600, color: "#6b7280" }}>
          Status
        </div>
        <div style={{ flex: 1 }} />
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
        {providerEntries.flatMap(([id, provider]) => {
          if (id === "openrouter") {
            const profiles = provider.profiles ?? [];
            return (
              <OpenRouterProviderSection
                key="openrouter"
                providerId="openrouter"
                provider={provider}
                profiles={profiles}
                activeProfileId={activeOpenRouterProfileId ?? null}
                keyStatus={allKeyStatus?.[id] ?? null}
                allKeyStatusLoading={!allKeyStatus}
                gatewayRunning={gatewayRunning}
                refreshConfig={refreshConfig}
                restartGateway={restartGateway}
                refreshKeyStatus={() => invoke<AllApiKeyStatus>("check_all_api_keys").then(setAllKeyStatus).catch(() => setAllKeyStatus(null))}
                onAddModelSet={handleAddProfile}
                addError={addError}
              />
            );
          }
          return (
            <ProviderRow
              key={id}
              providerId={id}
              provider={provider}
              keyStatus={allKeyStatus?.[id] ?? null}
              models={provider.models}
              refreshConfig={refreshConfig}
              gatewayRunning={gatewayRunning}
              restartGateway={restartGateway}
            />
          );
        })}
      </div>
    </div>
  );
}
