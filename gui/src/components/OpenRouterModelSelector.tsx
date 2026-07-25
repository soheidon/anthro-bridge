import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { OpenRouterModel, OpenRouterModelsResult } from "../types/openrouter";

// ── Constants ──────────────────────────────────────────────────

/** Only these IDs are classified as "router" models */
const ROUTER_MODEL_IDS = new Set(["openrouter/auto", "openrouter/free"]);

/** Stable vendor display order — not count-based */
const PREFERRED_VENDOR_ORDER = [
  "anthropic",
  "openai",
  "google",
  "deepseek",
  "qwen",
  "moonshotai",
  "mistralai",
  "cohere",
  "poolside",
  "minimax",
  "x-ai",
];

const VENDOR_LABELS: Record<string, string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  google: "Google",
  deepseek: "DeepSeek",
  moonshotai: "Moonshot AI",
  qwen: "Qwen",
  cohere: "Cohere",
  poolside: "Poolside",
  minimax: "MiniMax",
  mistralai: "Mistral AI",
  "x-ai": "xAI",
  openrouter: "OpenRouter",
};

// ── Poolside classification ─────────────────────────────────────

type SelectorMode = "group" | "other";

const LAGUNA_S_2_1_MODEL_IDS = new Set([
  "poolside/laguna-s-2.1",
  "poolside/laguna-s-2.1:free",
]);

const LAGUNA_XS_2_1_MODEL_IDS = new Set([
  "poolside/laguna-xs-2.1",
  "poolside/laguna-xs-2.1:free",
]);

const PRIMARY_POOLSIDE_MODEL_IDS = new Set([
  ...LAGUNA_S_2_1_MODEL_IDS,
  ...LAGUNA_XS_2_1_MODEL_IDS,
]);

const PRIMARY_POOLSIDE_MODEL_ORDER = [
  "poolside/laguna-s-2.1",
  "poolside/laguna-s-2.1:free",
  "poolside/laguna-xs-2.1",
  "poolside/laguna-xs-2.1:free",
];

const OTHER_POOLSIDE_MODEL_IDS = new Set([
  "poolside/laguna-m-1",
  "poolside/laguna-m-1:free",
]);

const SENTINEL_OTHER = "__other";
const SENTINEL_BACK = "__back";

// ── Synthetic family aliases ───────────────────────────────────

const OPENROUTER_FAMILY_ALIAS_IDS = new Set([
  "~anthropic/claude-opus-latest",
  "~anthropic/claude-sonnet-latest",
  "~anthropic/claude-haiku-latest",
]);

const OPENROUTER_FAMILY_ALIASES: OpenRouterModel[] = [
  { id: "~anthropic/claude-opus-latest", displayName: "Anthropic: Claude Opus Latest",
    inputModalities: [], outputModalities: [], supportedParameters: [], pricing: {} },
  { id: "~anthropic/claude-sonnet-latest", displayName: "Anthropic: Claude Sonnet Latest",
    inputModalities: [], outputModalities: [], supportedParameters: [], pricing: {} },
  { id: "~anthropic/claude-haiku-latest", displayName: "Anthropic: Claude Haiku Latest",
    inputModalities: [], outputModalities: [], supportedParameters: [], pricing: {} },
];

const CUSTOM_VENDOR_ID = "__custom";

// ── Module-level shared fetch ──────────────────────────────────

let sharedModelsResult: OpenRouterModelsResult | null = null;
let sharedModelsPromise: Promise<OpenRouterModelsResult> | null = null;

function getSharedOpenRouterModels(
  forceRefresh: boolean,
): Promise<OpenRouterModelsResult> {
  if (sharedModelsPromise) return sharedModelsPromise;

  if (!forceRefresh && sharedModelsResult) {
    return Promise.resolve(sharedModelsResult);
  }

  sharedModelsPromise = invoke<OpenRouterModelsResult>(
    "openrouter_get_models",
    { forceRefresh },
  )
    .then((result) => {
      sharedModelsResult = result;
      return result;
    })
    .finally(() => {
      sharedModelsPromise = null;
    });

  return sharedModelsPromise;
}

// ── Utility Functions ──────────────────────────────────────────

function normalizeModelId(modelId: string): string {
  return modelId.startsWith("~") ? modelId.slice(1) : modelId;
}

function getVendorId(modelId: string): string {
  return normalizeModelId(modelId).split("/")[0] || "other";
}

function formatVendorName(id: string): string {
  return (
    VENDOR_LABELS[id] ??
    id
      .split(/[-_]/)
      .map((p) => p.charAt(0).toUpperCase() + p.slice(1))
      .join(" ")
  );
}

function isRouterModel(modelId: string): boolean {
  return ROUTER_MODEL_IDS.has(normalizeModelId(modelId));
}

function vendorSortIndex(id: string): number {
  const index = PREFERRED_VENDOR_ORDER.indexOf(id);
  return index === -1 ? Number.MAX_SAFE_INTEGER : index;
}

// ── Price Formatting ───────────────────────────────────────────

function formatPrice(model: OpenRouterModel): string | null {
  const raw = model.pricing.prompt;
  if (!raw) return null;
  const perToken = Number(raw);
  if (!Number.isFinite(perToken) || perToken < 0) return null;
  const perMillion = perToken * 1_000_000;
  return `$${perMillion.toLocaleString(undefined, { maximumFractionDigits: 6 })}/M`;
}

function cleanModelDisplayName(model: OpenRouterModel): string {
  const vendorId = getVendorId(model.id);
  const vendorColon = formatVendorName(vendorId) + ": ";
  let name = model.displayName;
  if (name.startsWith(vendorColon)) name = name.slice(vendorColon.length);
  else if (name.startsWith(vendorId + "/")) name = name.slice(vendorId.length + 1);
  return name;
}

type ThinkingSelection = "default" | "max" | "on" | "off";

type ThinkingOption = { value: ThinkingSelection; label: string };

function normalizeThinkingSelection(
  thinkingMode: string | undefined,
  reasoningEffort: string | undefined,
): ThinkingSelection {
  if (thinkingMode === "normal") return "off";
  if (thinkingMode === "thinking" && reasoningEffort === "max") return "max";
  if (thinkingMode === "thinking") return "on";
  return "default";
}

function isThinkingValueSupported(
  modelId: string,
  value: ThinkingSelection,
): boolean {
  if (LAGUNA_S_2_1_MODEL_IDS.has(modelId)) return value === "default" || value === "max" || value === "off";
  if (LAGUNA_XS_2_1_MODEL_IDS.has(modelId)) return value === "default" || value === "on" || value === "off";
  return value === "default";
}

function toStoredThinking(selection: ThinkingSelection): {
  thinkingMode: string | null;
  reasoningEffort: string | null;
} {
  switch (selection) {
    case "max": return { thinkingMode: "thinking", reasoningEffort: "max" };
    case "on":  return { thinkingMode: "thinking", reasoningEffort: null };
    case "off": return { thinkingMode: "normal", reasoningEffort: null };
    default:    return { thinkingMode: null, reasoningEffort: null };
  }
}

function thinkingOptionsForModel(
  modelId: string,
  t: ReturnType<typeof useTranslation>["t"],
): ThinkingOption[] {
  if (LAGUNA_S_2_1_MODEL_IDS.has(modelId)) {
    return [
      { value: "default", label: t("openRouterModels.thinkingDefault") },
      { value: "max", label: t("openRouterModels.thinkingMax") },
      { value: "off", label: t("openRouterModels.thinkingOff") },
    ];
  }
  if (LAGUNA_XS_2_1_MODEL_IDS.has(modelId)) {
    return [
      { value: "default", label: t("openRouterModels.thinkingDefault") },
      { value: "on", label: t("openRouterModels.thinkingOn") },
      { value: "off", label: t("openRouterModels.thinkingOff") },
    ];
  }
  return [{ value: "default", label: t("openRouterModels.thinkingDefault") }];
}

// ── Component ──────────────────────────────────────────────────

interface OpenRouterModelSelectorProps {
  modelKey: string;
  gatewayModelLabel: string;
  currentUpstream: string;
  currentThinkingMode: string | undefined;
  currentReasoningEffort: string | undefined;
  onSaved: () => void;
  refreshController?: boolean;
}

export default function OpenRouterModelSelector(
  props: OpenRouterModelSelectorProps,
) {
  const { modelKey, gatewayModelLabel, currentUpstream, currentThinkingMode, currentReasoningEffort, onSaved, refreshController } = props;
  const { t } = useTranslation();

  const [hasLoadedModels, setHasLoadedModels] = useState(false);
  const [selectorMode, setSelectorMode] = useState<SelectorMode>("group");
  const [vendorSelection, setVendorSelection] = useState<string>("");
  const [modelSelection, setModelSelection] = useState<string>("");
  const [showCustom, setShowCustom] = useState(false);
  const [customText, setCustomText] = useState("");
  const [thinkingSelection, setThinkingSelection] = useState<ThinkingSelection>(() =>
    normalizeThinkingSelection(currentThinkingMode, currentReasoningEffort),
  );

  const [models, setModels] = useState<OpenRouterModel[]>([]);
  const [loading, setLoading] = useState(false);
  const [stale, setStale] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadWarning, setLoadWarning] = useState<string | null>(null);

  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const savingRef = useRef(false);

  const [selectedModelId, setSelectedModelId] = useState(currentUpstream);

  // ── Fetch Models ─────────────────────────────────────────────

  const fetchModels = useCallback(
    async (forceRefresh = false): Promise<boolean> => {
      setLoading(true);
      setLoadError(null);
      setLoadWarning(null);

      try {
        const result = await getSharedOpenRouterModels(forceRefresh);
        setModels(result.models);
        setStale(result.stale);
        setLoadWarning(result.warning ?? null);
        return true;
      } catch (error) {
        setLoadError(String(error));
        setStale(false);
        return false;
      } finally {
        setHasLoadedModels(true);
        setLoading(false);
      }
    },
    [],
  );

  useEffect(() => {
    fetchModels();
  }, [fetchModels]);

  // ── Listen for refresh events from sibling instances ────────

  useEffect(() => {
    const handler = () => {
      fetchModels(false);
    };
    window.addEventListener("openrouter-models-updated", handler);
    return () =>
      window.removeEventListener("openrouter-models-updated", handler);
  }, [fetchModels]);

  // ── Derived ──────────────────────────────────────────────────

  const selectableModels = useMemo(() => {
    const byId = new Map<string, OpenRouterModel>();
    for (const m of OPENROUTER_FAMILY_ALIASES) byId.set(m.id, m);
    for (const m of models) byId.set(m.id, m);
    return [...byId.values()];
  }, [models]);

  const primaryPoolsideModels = useMemo(() => {
    const byId = new Map(selectableModels.map((m) => [m.id, m]));
    return PRIMARY_POOLSIDE_MODEL_ORDER
      .map((id) => byId.get(id))
      .filter((m): m is OpenRouterModel => m !== undefined);
  }, [selectableModels]);

  const otherSelectableModels = useMemo(
    () => selectableModels.filter((m) => !PRIMARY_POOLSIDE_MODEL_IDS.has(m.id)),
    [selectableModels],
  );

  const otherVendorOptions = useMemo((): [string, string][] => {
    const vendorIds = new Set<string>();
    for (const m of otherSelectableModels) {
      if (isRouterModel(m.id)) vendorIds.add("openrouter");
      else vendorIds.add(getVendorId(m.id));
    }
    const sorted = [...vendorIds].sort((a, b) => {
      const orderDiff = vendorSortIndex(a) - vendorSortIndex(b);
      if (orderDiff !== 0) return orderDiff;
      return formatVendorName(a).localeCompare(formatVendorName(b));
    });
    return sorted.map((id) => {
      if (id === "openrouter") return [id, t("openRouterModels.groupRouter")];
      return [id, formatVendorName(id)];
    });
  }, [otherSelectableModels, t]);

  const visibleModelOptions = useMemo(() => {
    if (selectorMode === "group") {
      return primaryPoolsideModels;
    }

    if (!vendorSelection || vendorSelection === CUSTOM_VENDOR_ID) return [];

    if (vendorSelection === "poolside") {
      return selectableModels.filter((m) =>
        OTHER_POOLSIDE_MODEL_IDS.has(m.id),
      );
    }

    if (vendorSelection === "openrouter") {
      return selectableModels.filter((m) => isRouterModel(m.id));
    }

    return selectableModels.filter(
      (m) => getVendorId(m.id) === vendorSelection,
    );
  }, [selectorMode, vendorSelection, primaryPoolsideModels, selectableModels]);

  const selectedUiModel = useMemo(
    () => selectableModels.find((m) => m.id === modelSelection),
    [selectableModels, modelSelection],
  );

  const selectedUiPrice = useMemo(
    () => (selectedUiModel ? formatPrice(selectedUiModel) : null),
    [selectedUiModel],
  );

  const thinkingOptions = useMemo<ThinkingOption[]>(
    () => thinkingOptionsForModel(modelSelection, t),
    [modelSelection, t],
  );

  // ── Auto-detect from currentUpstream ────────────────────────

  useEffect(() => {
    setSelectedModelId(currentUpstream);

    if (!currentUpstream) {
      setSelectorMode("group");
      setVendorSelection("");
      setModelSelection("");
      setShowCustom(false);
      setCustomText("");
      return;
    }

    const isKnownAlias = OPENROUTER_FAMILY_ALIAS_IDS.has(currentUpstream);
    if (!hasLoadedModels && !isKnownAlias) return;

    const exists = selectableModels.some(
      (m) => m.id === currentUpstream,
    );

    if (PRIMARY_POOLSIDE_MODEL_IDS.has(currentUpstream)) {
      setSelectorMode("group");
      setVendorSelection("poolside");
      setModelSelection(currentUpstream);
      setShowCustom(false);
      setCustomText("");
      return;
    }

    setSelectorMode("other");

    if (!exists) {
      setVendorSelection(CUSTOM_VENDOR_ID);
      setModelSelection("");
      setShowCustom(true);
      setCustomText(currentUpstream);
      return;
    }

    const vendorId = isRouterModel(currentUpstream)
      ? "openrouter"
      : getVendorId(currentUpstream);
    setVendorSelection(vendorId);
    setModelSelection(currentUpstream);
    setShowCustom(false);
    setCustomText("");
  }, [currentUpstream, selectableModels, hasLoadedModels]);

  useEffect(() => {
    setThinkingSelection(normalizeThinkingSelection(currentThinkingMode, currentReasoningEffort));
  }, [currentThinkingMode, currentReasoningEffort]);

  // ── Save ─────────────────────────────────────────────────────

  const saveModel = useCallback(
    async (upstreamModel: string): Promise<boolean> => {
      const normalized = upstreamModel.trim();
      if (!normalized || savingRef.current) return false;

      savingRef.current = true;
      setSaving(true);
      setSaveError(null);

      try {
        await invoke("set_model_upstream", {
          providerId: "openrouter",
          modelKey,
          upstreamModel: normalized,
        });

        setSelectedModelId(normalized);

        try {
          onSaved();
        } catch (error) {
          console.error(
            "OpenRouter model was saved, but refresh failed:",
            error,
          );
        }

        return true;
      } catch (error) {
        setSaveError(String(error));
        throw error;
      } finally {
        savingRef.current = false;
        setSaving(false);
      }
    },
    [modelKey, onSaved],
  );

  // ── Event Handlers ───────────────────────────────────────────

  const handleVendorChange = useCallback(
    (value: string) => {
      setSaveError(null);

      if (selectorMode === "group") {
        if (value === "poolside") {
          setVendorSelection("poolside");
          setModelSelection(
            PRIMARY_POOLSIDE_MODEL_IDS.has(selectedModelId) ? selectedModelId : "",
          );
        }
        if (value === SENTINEL_OTHER) {
          setSelectorMode("other");
          setVendorSelection("");
          setModelSelection("");
          setShowCustom(false);
          setCustomText("");
        }
        return;
      }

      // selectorMode === "other"
      if (value === SENTINEL_BACK) {
        setSelectorMode("group");
        setVendorSelection("poolside");
        setModelSelection(
          PRIMARY_POOLSIDE_MODEL_IDS.has(selectedModelId) ? selectedModelId : "",
        );
        setShowCustom(false);
        setCustomText("");
        return;
      }

      if (value === CUSTOM_VENDOR_ID) {
        setVendorSelection(CUSTOM_VENDOR_ID);
        setModelSelection("");
        setShowCustom(true);
        setCustomText("");
        return;
      }

      setVendorSelection(value);
      setModelSelection("");
      setShowCustom(false);
    },
    [selectorMode, selectedModelId],
  );

  const handleModelChange = useCallback(
    async (modelId: string) => {
      if (!modelId || savingRef.current) return;
      const model = selectableModels.find((m) => m.id === modelId);
      if (!model) return;

      setSaveError(null);

      const nextThinking: ThinkingSelection = isThinkingValueSupported(model.id, thinkingSelection)
        ? thinkingSelection
        : "default";
      const { thinkingMode, reasoningEffort } = toStoredThinking(nextThinking);

      savingRef.current = true;
      setSaving(true);

      try {
        await invoke("set_model_upstream", {
          providerId: "openrouter",
          modelKey,
          upstreamModel: model.id,
          thinkingMode,
          reasoningEffort,
        });

        setSelectedModelId(model.id);
        setModelSelection(model.id);
        setThinkingSelection(nextThinking);

        try { onSaved(); } catch {
          console.error("OpenRouter model change: refresh failed");
        }
      } catch (error) {
        setSaveError(String(error));
      } finally {
        savingRef.current = false;
        setSaving(false);
      }
    },
    [selectableModels, modelKey, onSaved, thinkingSelection],
  );

  const handleCustomConfirm = useCallback(async () => {
    const normalized = customText.trim();
    if (!normalized) return;
    setSaveError(null);
    try {
      const saved = await saveModel(normalized);
      if (!saved) return;
      setVendorSelection(CUSTOM_VENDOR_ID);
      setModelSelection("");
      setShowCustom(true);
      setCustomText(normalized);
    } catch (error) {
      console.error("Failed to save custom OpenRouter model:", error);
    }
  }, [customText, saveModel]);

  const handleThinkingChange = useCallback(
    async (mode: ThinkingSelection) => {
      if (savingRef.current) return;
      if (!selectedModelId) return;

      savingRef.current = true;
      setSaving(true);
      setSaveError(null);

      try {
        const thinkingMode = mode === "off" ? "normal" : mode === "default" ? undefined : "thinking";
        const reasoningEffort = mode === "max" ? "max" : undefined;

        await invoke("set_model_upstream", {
          providerId: "openrouter",
          modelKey,
          upstreamModel: selectedModelId,
          thinkingMode: thinkingMode ?? null,
          reasoningEffort: reasoningEffort ?? null,
        });

        setThinkingSelection(mode);

        try {
          onSaved();
        } catch (error) {
          console.error("Thinking mode was saved, but refresh failed:", error);
        }
      } catch (error) {
        setSaveError(String(error));
      } finally {
        savingRef.current = false;
        setSaving(false);
      }
    },
    [modelKey, selectedModelId, onSaved],
  );

  // ── Refresh controller (only on the designated instance) ────

  useEffect(() => {
    if (!refreshController) return;

    const handler = () => {
      void (async () => {
        try {
          const succeeded = await fetchModels(true);
          if (succeeded) {
            window.dispatchEvent(new CustomEvent("openrouter-models-updated"));
          }
        } finally {
          window.dispatchEvent(new CustomEvent("openrouter-models-refresh-completed"));
        }
      })();
    };

    window.addEventListener("openrouter-models-refresh-requested", handler);
    return () => window.removeEventListener("openrouter-models-refresh-requested", handler);
  }, [fetchModels, refreshController]);

  // ── Render ───────────────────────────────────────────────────

  const groupSelectValue = selectorMode === "group" && vendorSelection === "poolside"
    ? "poolside"
    : "";

  return (
    <div className="openrouter-model-selector">
      <div className="openrouter-tier-row">
        <span className="openrouter-tier-label">{gatewayModelLabel}</span>

        {/* Vendor select (dual-mode) */}
        {selectorMode === "group" ? (
          <select
            className="openrouter-vendor-select"
            value={groupSelectValue}
            onChange={(e) => handleVendorChange(e.target.value)}
          >
            <option value="poolside">{t("openRouterModels.groupPoolside")}</option>
            <option value={SENTINEL_OTHER}>{t("openRouterModels.groupOtherModels")}</option>
          </select>
        ) : (
          <select
            className="openrouter-vendor-select"
            value={vendorSelection}
            onChange={(e) => handleVendorChange(e.target.value)}
          >
            <option value="">{t("openRouterModels.selectVendor")}</option>
            <option value={SENTINEL_BACK}>
              ← {t("openRouterModels.groupPoolside")}/{t("openRouterModels.groupOtherModels")}
            </option>
            {otherVendorOptions.map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
            <option value={CUSTOM_VENDOR_ID}>
              {t("openRouterModels.customModelShort")}
            </option>
          </select>
        )}

        {/* Model select OR custom input */}
        {showCustom && vendorSelection === CUSTOM_VENDOR_ID ? (
          <div className="openrouter-custom-inline">
            <input
              className="openrouter-custom-input-inline"
              value={customText}
              onChange={(e) => setCustomText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleCustomConfirm();
              }}
              placeholder={t("openRouterModels.customModelPlaceholder")}
              autoFocus
            />
            <button
              type="button"
              className="openrouter-custom-confirm-inline"
              onClick={() => void handleCustomConfirm()}
              disabled={saving || !customText.trim()}
            >
              {t("openRouterModels.confirm")}
            </button>
          </div>
        ) : (
          <>
            <select
              className="openrouter-model-select"
              value={modelSelection}
              onChange={(e) => handleModelChange(e.target.value)}
              disabled={
                !vendorSelection ||
                vendorSelection === CUSTOM_VENDOR_ID ||
                saving
              }
              aria-label={t("openRouterModels.selectModel")}
            >
              <option value="">{t("openRouterModels.selectModel")}</option>
              {visibleModelOptions.map((m) => (
                <option key={m.id} value={m.id}>
                  {cleanModelDisplayName(m)}
                </option>
              ))}
            </select>

            {/* Thinking mode */}
            <select
              className="openrouter-thinking-select"
              value={thinkingSelection}
              onChange={(e) => void handleThinkingChange(e.target.value as ThinkingSelection)}
              disabled={saving || !selectedUiModel}
              aria-label={t("openRouterModels.thinkingMode")}
            >
              {thinkingOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>

            {selectedUiPrice && (
              <span className="openrouter-model-price-label">
                IN {selectedUiPrice}
              </span>
            )}
          </>
        )}

      </div>

      {saveError && (
        <div className="openrouter-save-error" role="alert">
          {saveError}
        </div>
      )}

      {loading && !hasLoadedModels && (
        <div className="openrouter-loading-inline">
          {t("openRouterModels.loading")}
        </div>
      )}

      {loadWarning && (
        <div
          className={
            stale
              ? "openrouter-cache-warning-inline"
              : "openrouter-warning-inline"
          }
        >
          {loadWarning}
        </div>
      )}

      {loadError && (
        <div className="openrouter-error-inline" role="alert">
          {t("openRouterModels.error")} {loadError}
        </div>
      )}
    </div>
  );
}
