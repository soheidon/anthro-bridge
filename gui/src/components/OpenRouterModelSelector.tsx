import { useState, useEffect, useMemo, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { TranslationKey } from "../i18n";
import type { OpenRouterModel, OpenRouterModelsResult } from "../types/openrouter";
import type { CommandResponse } from "../types";
import { BUILTIN_OPENROUTER_MODELS as BUILTIN_REGISTRY } from "../config/builtinOpenRouter";

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

// ── Built-in vendor registry (single source of truth) ──────────

type SelectorMode = "group" | "other";

type BuiltinModelDefinition = {
  id: string;          // upstream model ID, e.g. "tencent/hy3"
  displayName: string; // shown after vendor prefix stripping
};

type BuiltinVendor = {
  id: string;          // "poolside" | "tencent"
  labelKey: TranslationKey; // i18n key
  models: BuiltinModelDefinition[];
};

const BUILTIN_OPENROUTER_VENDORS: BuiltinVendor[] = [
  {
    id: "poolside",
    labelKey: "openRouterModels.groupPoolside",
    models: [
      { id: "poolside/laguna-s-2.1",      displayName: "Laguna S 2.1" },
      { id: "poolside/laguna-s-2.1:free", displayName: "Laguna S 2.1 (Free)" },
      { id: "poolside/laguna-xs-2.1",     displayName: "Laguna XS 2.1" },
      { id: "poolside/laguna-xs-2.1:free",displayName: "Laguna XS 2.1 (Free)" },
    ],
  },
  {
    id: "tencent",
    labelKey: "openRouterModels.groupTencent",
    models: [
      { id: "tencent/hy3",      displayName: "Hy3" },
      { id: "tencent/hy3:free", displayName: "Hy3 (Free)" },
    ],
  },
  {
    id: "inclusionai",
    labelKey: "openRouterModels.groupInclusionAI",
    models: [], // filled below from BUILTIN_OPENROUTER_MODELS
  },
  {
    id: "stepfun",
    labelKey: "openRouterModels.groupStepFun",
    models: [], // filled below from BUILTIN_OPENROUTER_MODELS
  },
  {
    id: "openai",
    labelKey: "openRouterModels.groupOpenAI",
    models: [], // filled below from BUILTIN_OPENROUTER_MODELS
  },
  {
    id: "google",
    labelKey: "openRouterModels.groupGoogle",
    models: [], // filled below from BUILTIN_OPENROUTER_MODELS
  },
];

// Populate vendor models from the single registry (avoids double management)
// Import is at top of file; reference via the module-level registry import.
for (const v of BUILTIN_OPENROUTER_VENDORS) {
  if (v.models.length > 0) continue; // already populated for poolside/tencent
  const entries = Object.entries(BUILTIN_REGISTRY)
    .filter(([, entry]) => entry.vendor.toLowerCase() === v.id)
    .map(([id, entry]) => ({ id, displayName: entry.displayName }));
  v.models.push(...entries);
}

// Derived sets / indices — never edit by hand.
const BUILTIN_MODEL_BY_ID: Map<string, { vendor: BuiltinVendor; model: BuiltinModelDefinition }> =
  new Map(
    BUILTIN_OPENROUTER_VENDORS.flatMap((v) =>
      v.models.map((m) => [m.id, { vendor: v, model: m }] as const),
    ),
  );

const BUILTIN_MODEL_IDS: Set<string> = new Set(BUILTIN_MODEL_BY_ID.keys());

const POOLSIDE_VENDOR = BUILTIN_OPENROUTER_VENDORS.find((v) => v.id === "poolside")!;
const TENCENT_VENDOR = BUILTIN_OPENROUTER_VENDORS.find((v) => v.id === "tencent")!;

const LAGUNA_S_2_1_MODEL_IDS = new Set(
  POOLSIDE_VENDOR.models.filter((m) => m.id.includes("laguna-s-2.1")).map((m) => m.id),
);

const LAGUNA_XS_2_1_MODEL_IDS = new Set(
  POOLSIDE_VENDOR.models.filter((m) => m.id.includes("laguna-xs-2.1")).map((m) => m.id),
);

const PRIMARY_POOLSIDE_MODEL_IDS = new Set(POOLSIDE_VENDOR.models.map((m) => m.id));

const PRIMARY_POOLSIDE_MODEL_ORDER = POOLSIDE_VENDOR.models.map((m) => m.id);

const TENCENT_HY3_MODEL_IDS = new Set(TENCENT_VENDOR.models.map((m) => m.id));

const INCLUSIONAI_VENDOR = BUILTIN_OPENROUTER_VENDORS.find((v) => v.id === "inclusionai")!;
const STEPFUN_VENDOR = BUILTIN_OPENROUTER_VENDORS.find((v) => v.id === "stepfun")!;

const INCLUSIONAI_MODEL_IDS = new Set(INCLUSIONAI_VENDOR.models.map((m) => m.id));
const STEPFUN_MODEL_IDS = new Set(STEPFUN_VENDOR.models.map((m) => m.id));

const RING_MODEL_IDS = new Set(["inclusionai/ring-2.6-1t"]);
const LING_NON_THINKING_IDS = new Set(["inclusionai/ling-2.6-1t", "inclusionai/ling-2.6-flash"]);
const LING_3_FREE_IDS = new Set(["inclusionai/ling-3.0-flash:free"]);
const STEP_3_7_IDS = new Set(["stepfun/step-3.7-flash"]);
const STEP_3_5_IDS = new Set(["stepfun/step-3.5-flash"]);

const GOOGLE_VENDOR = BUILTIN_OPENROUTER_VENDORS.find((v) => v.id === "google")!;
const GOOGLE_MODEL_IDS = new Set(GOOGLE_VENDOR.models.map((m) => m.id));
const GEMINI_MODEL_IDS = GOOGLE_MODEL_IDS;
const GEMINI_3_1_7_IDS = new Set(["google/gemini-3.1-pro-preview", "google/gemini-3.7-flash"]);
const GEMINI_SUPPORTED_THINKING = new Map<string, Set<ThinkingSelection>>();
for (const id of GEMINI_3_1_7_IDS) GEMINI_SUPPORTED_THINKING.set(id, new Set<ThinkingSelection>(["low", "medium", "high"]));
const OPENAI_VENDOR = BUILTIN_OPENROUTER_VENDORS.find((v) => v.id === "openai")!;
const OPENAI_MODEL_IDS = new Set(OPENAI_VENDOR.models.map((m) => m.id));

type OpenAITier = "sol" | "terra" | "luna";
type OpenAIMode = "standard" | "pro";

function buildOpenAIModelId(tier: OpenAITier, mode: OpenAIMode): string {
  const suffix = mode === "pro" ? "-pro" : "";
  return `openai/gpt-5.6-${tier}${suffix}`;
}

function parseOpenAIModelId(modelId: string): { tier: OpenAITier; mode: OpenAIMode } | null {
  const match = normalizeModelId(modelId).match(/^openai\/gpt-5\.6-(sol|terra|luna)(-pro)?$/);
  if (!match) return null;
  return { tier: match[1] as OpenAITier, mode: match[2] ? "pro" : "standard" };
}

interface OpenAIModelChoice {
  id: string;
  displayName: string;
  tier: OpenAITier;
}

const OPENAI_MODEL_CHOICES: OpenAIModelChoice[] = (() => {
  const seen = new Set<string>();
  const choices: OpenAIModelChoice[] = [];
  for (const m of OPENAI_VENDOR.models) {
    const parsed = parseOpenAIModelId(m.id);
    if (!parsed || seen.has(parsed.tier)) continue;
    seen.add(parsed.tier);
    choices.push({
      id: buildOpenAIModelId(parsed.tier, "standard"),
      displayName: `GPT-5.6 ${parsed.tier.charAt(0).toUpperCase() + parsed.tier.slice(1)}`,
      tier: parsed.tier,
    });
  }
  return choices;
})();

function findBuiltinVendorByModelId(modelId: string): BuiltinVendor | null {
  return BUILTIN_MODEL_BY_ID.get(modelId)?.vendor ?? null;
}

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

/** Synthetic built-in OpenRouter models — always available, even when the
 *  OpenRouter API cache is empty or the network fetch fails. API-fetched
 *  entries overwrite these on collision so live metadata (pricing, modalities)
 *  is still preferred when it exists. */
const BUILTIN_OPENROUTER_MODELS: OpenRouterModel[] = BUILTIN_OPENROUTER_VENDORS.flatMap(
  (vendor) =>
    vendor.models.map((model) => ({
      id: model.id,
      displayName: `${formatVendorName(vendor.id)}: ${model.displayName}`,
      inputModalities: [],
      outputModalities: [],
      supportedParameters: [],
      pricing: {},
    })),
);

const CUSTOM_VENDOR_ID = "__custom";

// ── Module-level shared fetch ──────────────────────────────────

let sharedModelsResult: OpenRouterModelsResult | null = null;
let sharedModelsPromise: Promise<OpenRouterModelsResult> | null = null;

export function getOpenRouterModelsCached(
  forceRefresh: boolean = false,
): Promise<OpenRouterModelsResult> {
  return getSharedOpenRouterModels(forceRefresh);
}

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

/** Clears the module-level model cache. Exposed for tests so each test
 *  can inject its own `openrouter_get_models` result without a stale
 *  cached result leaking from a previous test. */
export function __resetOpenRouterModelsCacheForTests(): void {
  sharedModelsResult = null;
  sharedModelsPromise = null;
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

/** Convert a per-token price string to per-million-USD number (or null).
 *  Used by StatusPanel to resolve OpenRouter model pricing. */
export function parsePerMillionUsd(raw: string | undefined): number | null {
  if (!raw) return null;
  const perToken = Number(raw);
  if (!Number.isFinite(perToken) || perToken < 0) return null;
  return perToken * 1_000_000;
}

function cleanModelDisplayName(model: OpenRouterModel): string {
  const vendorId = getVendorId(model.id);
  const vendorColon = formatVendorName(vendorId) + ": ";
  let name = model.displayName;
  if (name.startsWith(vendorColon)) name = name.slice(vendorColon.length);
  else if (name.startsWith(vendorId + "/")) name = name.slice(vendorId.length + 1);
  return name;
}

type ThinkingSelection = "max" | "on" | "off" | "minimal" | "low" | "medium" | "high" | "xhigh";

type ThinkingOption = { value: ThinkingSelection; label: string };

function normalizeThinkingSelection(
  modelId: string,
  thinkingMode: string | undefined,
  reasoningEffort: string | undefined,
): ThinkingSelection {
  if (GEMINI_MODEL_IDS.has(modelId)) {
    if (thinkingMode === "normal") return "low";
    if (thinkingMode === "thinking") {
      if (reasoningEffort === "minimal") return "minimal";
      if (reasoningEffort === "low") return "low";
      if (reasoningEffort === "medium") return "medium";
      if (reasoningEffort === "high") return "high";
    }
    return "high";
  }
  if (OPENAI_MODEL_IDS.has(modelId)) {
    if (thinkingMode === "normal") return "off";
    if (thinkingMode === "thinking") {
      if (reasoningEffort === "low") return "low";
      if (reasoningEffort === "medium") return "medium";
      if (reasoningEffort === "high") return "high";
      if (reasoningEffort === "xhigh") return "xhigh";
      if (reasoningEffort === "max") return "max";
      return "medium";
    }
    return "medium";
  }
  if (thinkingMode === "normal") return "off";
  if (thinkingMode === "thinking") {
    if (reasoningEffort === "max") return "max";
    if (reasoningEffort === "xhigh") return "xhigh";
    if (reasoningEffort === "high") return "high";
    if (reasoningEffort === "medium") return "medium";
    if (reasoningEffort === "low") return "low";
    if (TENCENT_HY3_MODEL_IDS.has(modelId)) return "off";
    return "on";
  }
  // No config: use model default
  if (LAGUNA_S_2_1_MODEL_IDS.has(modelId)) return "max";
  if (LAGUNA_XS_2_1_MODEL_IDS.has(modelId)) return "on";
  if (RING_MODEL_IDS.has(modelId)) return "xhigh";
  if (STEP_3_7_IDS.has(modelId)) return "medium";
  if (STEP_3_5_IDS.has(modelId)) return "on";
  if (LING_NON_THINKING_IDS.has(modelId)) return "off";
  return "off";
}

/** When switching models, find the closest supported Thinking value
 *  using index distance on the priority chain.  Ties go toward "off"
 *  (weaker) — safer default when equally close. */
function findClosestThinking(
  modelId: string,
  current: ThinkingSelection,
): ThinkingSelection {
  if (isThinkingValueSupported(modelId, current)) return current;

  const CHAIN: ThinkingSelection[] = [
    "max", "xhigh", "high", "medium", "low", "on", "off",
  ];
  const currentIdx = CHAIN.indexOf(current);
  if (currentIdx === -1) return normalizeThinkingSelection(modelId, undefined, undefined);

  const candidates = CHAIN
    .map((value, idx) => ({ value, distance: Math.abs(idx - currentIdx), idx }))
    .filter(({ value }) => isThinkingValueSupported(modelId, value));
  if (candidates.length === 0) return normalizeThinkingSelection(modelId, undefined, undefined);

  // Sort: shortest distance first; tie → higher index (toward "off"/weaker)
  candidates.sort((a, b) => a.distance - b.distance || b.idx - a.idx);
  return candidates[0].value;
}

function isThinkingValueSupported(
  modelId: string,
  value: ThinkingSelection,
): boolean {
  if (GEMINI_MODEL_IDS.has(modelId)) return GEMINI_SUPPORTED_THINKING.get(modelId)?.has(value) ?? false;
  if (OPENAI_MODEL_IDS.has(modelId)) {
    return value === "off" || value === "low" || value === "medium"
        || value === "high" || value === "xhigh" || value === "max";
  }
  if (LAGUNA_S_2_1_MODEL_IDS.has(modelId)) return value === "max" || value === "off";
  if (LAGUNA_XS_2_1_MODEL_IDS.has(modelId)) return value === "on" || value === "off";
  if (TENCENT_HY3_MODEL_IDS.has(modelId)) {
    return value === "off" || value === "low" || value === "high";
  }
  if (RING_MODEL_IDS.has(modelId)) return value === "high" || value === "xhigh";
  if (STEP_3_7_IDS.has(modelId)) return value === "low" || value === "medium" || value === "high";
  if (STEP_3_5_IDS.has(modelId)) return value === "on";
  if (LING_NON_THINKING_IDS.has(modelId)) return value === "off";
  if (LING_3_FREE_IDS.has(modelId)) return value === "off" || value === "on";
  return false;
}

function toStoredThinking(selection: ThinkingSelection): {
  thinkingMode: string | null;
  reasoningEffort: string | null;
} {
  switch (selection) {
    case "max":    return { thinkingMode: "thinking", reasoningEffort: "max" };
    case "xhigh":  return { thinkingMode: "thinking", reasoningEffort: "xhigh" };
    case "high":   return { thinkingMode: "thinking", reasoningEffort: "high" };
    case "medium": return { thinkingMode: "thinking", reasoningEffort: "medium" };
    case "low":    return { thinkingMode: "thinking", reasoningEffort: "low" };
    case "minimal": return { thinkingMode: "thinking", reasoningEffort: "minimal" };
    case "on":     return { thinkingMode: "thinking", reasoningEffort: null };
    case "off":    return { thinkingMode: "normal",  reasoningEffort: null };
  }
}

function thinkingOptionsForModel(
  modelId: string,
  t: ReturnType<typeof useTranslation>["t"],
): ThinkingOption[] {
  if (GEMINI_MODEL_IDS.has(modelId)) {
    return Array.from(GEMINI_SUPPORTED_THINKING.get(modelId) ?? []).map((value) => ({
      value,
      label: value === "minimal" ? "Thinking: Minimal" : `Thinking: ${value.charAt(0).toUpperCase() + value.slice(1)}`,
    }));
  }
  if (OPENAI_MODEL_IDS.has(modelId)) {
    return [
      { value: "off",    label: t("apiKeyPanel.normalMode") },
      { value: "low",    label: t("openRouterModels.reasoningLow") },
      { value: "medium", label: t("openRouterModels.reasoningMedium") },
      { value: "high",   label: t("openRouterModels.reasoningHigh") },
      { value: "xhigh",  label: t("openRouterModels.reasoningExtraHigh") },
      { value: "max",    label: t("openRouterModels.reasoningMax") },
    ];
  }
  if (LAGUNA_S_2_1_MODEL_IDS.has(modelId)) {
    return [
      { value: "max", label: "Thinking: Max" },
      { value: "off", label: t("apiKeyPanel.normalMode") },
    ];
  }
  if (LAGUNA_XS_2_1_MODEL_IDS.has(modelId)) {
    return [
      { value: "on", label: "Thinking" },
      { value: "off", label: t("apiKeyPanel.normalMode") },
    ];
  }
  if (TENCENT_HY3_MODEL_IDS.has(modelId)) {
    return [
      { value: "off",  label: t("apiKeyPanel.normalMode") },
      { value: "low",  label: "Thinking: Low" },
      { value: "high", label: "Thinking: High" },
    ];
  }
  if (RING_MODEL_IDS.has(modelId)) {
    return [
      { value: "high",  label: "Thinking: High" },
      { value: "xhigh", label: "Thinking: XHigh" },
    ];
  }
  if (STEP_3_7_IDS.has(modelId)) {
    return [
      { value: "low",    label: "Thinking: Low" },
      { value: "medium", label: "Thinking: Medium" },
      { value: "high",   label: "Thinking: High" },
    ];
  }
  if (STEP_3_5_IDS.has(modelId)) {
    return [{ value: "on", label: "Thinking" }];
  }
  if (LING_NON_THINKING_IDS.has(modelId)) {
    return [{ value: "off", label: t("apiKeyPanel.normalMode") }];
  }
  if (LING_3_FREE_IDS.has(modelId)) {
    return [
      { value: "off", label: t("apiKeyPanel.normalMode") },
      { value: "on",  label: "Thinking" },
    ];
  }
  return [{ value: "off", label: t("apiKeyPanel.normalMode") }];
}

// ── Save queue types ───────────────────────────────────────────

type SaveResult =
  | { status: "saved" }
  | { status: "saved_restart_failed" }
  | { status: "failed" }
  | { status: "superseded" };

type PendingSave = {
  request: {
    routeId: string;
    profileId?: string;
    modelKey: string;
    upstreamModel: string;
    thinkingMode?: string | null;
    reasoningEffort?: string | null;
  };
  /** One-shot settle — resolves the Promise exactly once.  Subsequent
   *  calls are silently ignored so we never double-resolve. */
  settle: (result: SaveResult) => void;
};

// ── Route-save generation hook — captures route identity and
//    generation counter before an async save so the handler can
//    bail out if a newer save superseded it. ──────────────────────

type SaveRequest = {
  /** The generation number when this save was initiated.  Compare
   *  against `saveGenerationRef.current` after await. */
  generation: number;
  routeId: string;
  profileId?: string;
  modelKey: string;
};

export function useRouteSaveGeneration(
  routeId: string,
  profileId: string | undefined,
  modelKey: string,
  currentRouteIdRef: React.MutableRefObject<string>,
  saveGenerationRef: React.MutableRefObject<number>,
) {
  /** Capture the current route identity and bump the generation counter.
   *  Call BEFORE the async save — the returned `SaveRequest` is what
   *  you check against `isCurrent` after await. */
  const begin = useCallback((): SaveRequest => {
    const generation = ++saveGenerationRef.current;
    return {
      generation,
      routeId,
      profileId,
      modelKey,
    };
  }, [routeId, profileId, modelKey, saveGenerationRef]);

  /** Returns true when no newer save has started (generation is still
   *  the latest) AND the route hasn't changed since the save began.
   *  Both must hold for the handler to update local UI. */
  const isCurrent = useCallback(
    (req: SaveRequest): boolean => {
      if (req.generation !== saveGenerationRef.current) return false;
      if (req.routeId !== currentRouteIdRef.current) return false;
      return true;
    },
    [saveGenerationRef, currentRouteIdRef],
  );

  return useMemo(
    () => ({ begin, isCurrent }),
    [begin, isCurrent],
  );
}

// ── Save queue hook — extracted so tests can exercise the same save
//    serialization and drain logic as the production component. ─────

export function useOpenRouterSaveQueue(options: {
  onSaved: () => Promise<void>;
  gatewayRunning: boolean;
  restartGateway: () => Promise<void>;
  currentRouteIdRef: React.MutableRefObject<string>;
  syncUiFromSavedRouteRef: React.MutableRefObject<() => void>;
  lastSubmittedRef: React.MutableRefObject<{
    routeId: string;
    upstreamModel: string;
    thinkingMode?: string;
    reasoningEffort?: string;
  } | null>;
  setSaveError: (error: string | null) => void;
  formatSaveFailed: (error: unknown) => string;
  formatRefreshFailed: (error: unknown) => string;
  formatRestartFailed: (error: unknown) => string;
}) {
  const {
    onSaved,
    gatewayRunning,
    restartGateway,
    currentRouteIdRef,
    syncUiFromSavedRouteRef,
    lastSubmittedRef,
    setSaveError,
    formatSaveFailed,
    formatRefreshFailed,
    formatRestartFailed,
  } = options;

  const pendingSaveRef = useRef<PendingSave | null>(null);
  const inFlightSaveRef = useRef<PendingSave | null>(null);
  const savingRef = useRef(false);
  const mountedRef = useRef(true);
  const [saving, setSaving] = useState(false);

  // ── Unmount cleanup ──────────────────────────────────────────

  useEffect(() => {
    return () => {
      mountedRef.current = false;
      pendingSaveRef.current?.settle({ status: "superseded" });
      pendingSaveRef.current = null;
      inFlightSaveRef.current?.settle({ status: "superseded" });
      inFlightSaveRef.current = null;
    };
  }, []);

  // ── Drain loop ───────────────────────────────────────────────

  const drainSaveQueue = useCallback(async () => {
    if (savingRef.current) return;
    savingRef.current = true;
    if (mountedRef.current) {
      setSaving(true);
    }

    try {
      let batchNeedsRestart = false;
      let anySaveSucceeded = false;
      let lastAttempt: PendingSave | null = null;
      let lastAttemptSucceeded = false;
      let rollbackRouteId: string | null = null;

      while (pendingSaveRef.current) {
        const current = pendingSaveRef.current;
        pendingSaveRef.current = null;
        inFlightSaveRef.current = current;
        lastAttempt = current;
        lastAttemptSucceeded = false;
        rollbackRouteId = current.request.routeId;

        lastSubmittedRef.current = {
          routeId: current.request.routeId,
          upstreamModel: current.request.upstreamModel,
          thinkingMode: current.request.thinkingMode ?? undefined,
          reasoningEffort: current.request.reasoningEffort ?? undefined,
        };

        try {
          const response = await invoke<CommandResponse<void>>("set_model_upstream", {
            providerId: "openrouter",
            modelKey: current.request.modelKey,
            upstreamModel: current.request.upstreamModel,
            thinkingMode: current.request.thinkingMode ?? null,
            reasoningEffort: current.request.reasoningEffort ?? null,
            profileId: current.request.profileId ?? null,
          });

          if (!mountedRef.current) {
            current.settle({ status: "superseded" });
            inFlightSaveRef.current = null;
            return;
          }

          batchNeedsRestart = batchNeedsRestart || response.restartGateway;
          anySaveSucceeded = true;
          lastAttemptSucceeded = true;
          if (pendingSaveRef.current) {
            current.settle({ status: "superseded" });
          }
        } catch (error) {
          if (
            mountedRef.current &&
            !pendingSaveRef.current &&
            current.request.routeId === currentRouteIdRef.current
          ) {
            setSaveError(formatSaveFailed(error));
          }
          current.settle({ status: "failed" });
        }
      }
      inFlightSaveRef.current = null;

      // ── Phase 2: post-save once for the batch ──────────────────

      if (!mountedRef.current || !anySaveSucceeded) {
        if (mountedRef.current && !anySaveSucceeded) {
          lastSubmittedRef.current = null;
          if (rollbackRouteId === currentRouteIdRef.current) {
            syncUiFromSavedRouteRef.current();
          }
        }
        return;
      }

      // Refresh
      try {
        await onSaved();
      } catch (firstError) {
        if (!mountedRef.current) {
          if (lastAttemptSucceeded) {
            lastAttempt?.settle({ status: "superseded" });
          }
          return;
        }

        if (mountedRef.current) {
          setSaveError(formatRefreshFailed(firstError));
        }

        // Retry once
        try {
          await onSaved();
          if (mountedRef.current) {
            setSaveError(null);
          }
        } catch (retryError) {
          if (mountedRef.current) {
            setSaveError(formatRefreshFailed(retryError));
          }
        }
      }

      if (!mountedRef.current) {
        if (lastAttemptSucceeded) {
          lastAttempt?.settle({ status: "superseded" });
        }
        return;
      }

      // Restart (OR-aggregated)
      if (gatewayRunning && batchNeedsRestart) {
        try {
          await restartGateway();
        } catch (error) {
          if (mountedRef.current) {
            setSaveError(formatRestartFailed(error));
          }
          if (lastAttemptSucceeded) {
            lastAttempt?.settle({ status: "saved_restart_failed" });
          }
          return;
        }
      }

      if (lastAttemptSucceeded) {
        lastAttempt?.settle({ status: "saved" });
      }
    } finally {
      savingRef.current = false;
      if (mountedRef.current) {
        setSaving(false);
      }
      // Tail kick: requests queued during post-save start a fresh batch
      if (mountedRef.current && pendingSaveRef.current) {
        void drainSaveQueue();
      }
    }
  }, [onSaved, gatewayRunning, restartGateway, currentRouteIdRef, syncUiFromSavedRouteRef, lastSubmittedRef, setSaveError, formatSaveFailed, formatRefreshFailed, formatRestartFailed]);

  // ── Enqueue ──────────────────────────────────────────────────

  const saveModelRoute = useCallback(
    (args: {
      routeId: string;
      profileId?: string;
      modelKey: string;
      upstreamModel: string;
      thinkingMode?: string | null;
      reasoningEffort?: string | null;
    }): Promise<SaveResult> => {
      pendingSaveRef.current?.settle({ status: "superseded" });

      let settled = false;
      const pending: PendingSave = {
        request: {
          routeId: args.routeId,
          profileId: args.profileId,
          modelKey: args.modelKey,
          upstreamModel: args.upstreamModel,
          thinkingMode: args.thinkingMode ?? null,
          reasoningEffort: args.reasoningEffort ?? null,
        },
        settle: (_result: SaveResult) => {},
      };

      const promise = new Promise<SaveResult>((resolve) => {
        pending.settle = (result: SaveResult) => {
          if (settled) return;
          settled = true;
          resolve(result);
        };
      });
      pendingSaveRef.current = pending;

      void drainSaveQueue();
      return promise;
    },
    [drainSaveQueue],
  );

  const saveModel = useCallback(
    async (args: {
      routeId: string;
      profileId?: string;
      modelKey: string;
      upstreamModel: string;
    }): Promise<SaveResult> => {
      const normalized = args.upstreamModel.trim();
      if (!normalized) return { status: "failed" };
      return saveModelRoute({
        routeId: args.routeId,
        profileId: args.profileId,
        modelKey: args.modelKey,
        upstreamModel: normalized,
      });
    },
    [saveModelRoute],
  );

  return useMemo(
    () => ({ saving, savingRef, saveModelRoute, saveModel }),
    [saving, saveModelRoute, saveModel],
  );
}

// ── Component ──────────────────────────────────────────────────

interface OpenRouterModelSelectorProps {
  modelKey: string;
  gatewayModelLabel: string;
  currentUpstream: string;
  currentThinkingMode: string | undefined;
  currentReasoningEffort: string | undefined;
  onSaved: () => Promise<void>;
  refreshController?: boolean;
  profileId?: string;
  gatewayRunning: boolean;
  restartGateway: () => Promise<void>;
}

export default function OpenRouterModelSelector(
  props: OpenRouterModelSelectorProps,
) {
  const { modelKey, gatewayModelLabel, currentUpstream, currentThinkingMode, currentReasoningEffort, onSaved, refreshController, profileId, gatewayRunning, restartGateway } = props;
  const { t } = useTranslation();

  const [hasLoadedModels, setHasLoadedModels] = useState(false);
  const [selectorMode, setSelectorMode] = useState<SelectorMode>("group");
  const [vendorSelection, setVendorSelection] = useState<string>("");
  const [modelSelection, setModelSelection] = useState<string>("");
  const [showCustom, setShowCustom] = useState(false);
  const [customText, setCustomText] = useState("");
  const [thinkingSelection, setThinkingSelection] = useState<ThinkingSelection>(() =>
    normalizeThinkingSelection(currentUpstream, currentThinkingMode, currentReasoningEffort),
  );

  const [models, setModels] = useState<OpenRouterModel[]>([]);
  const [loading, setLoading] = useState(false);
  const [stale, setStale] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadWarning, setLoadWarning] = useState<string | null>(null);

  const [saveError, setSaveError] = useState<string | null>(null);

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
    // Built-in synthetic aliases first (Claude "latest" slugs).
    for (const m of OPENROUTER_FAMILY_ALIASES) byId.set(m.id, m);
    // Built-in vendor models (Poolside, Tencent) — always present, even if
    // the OpenRouter API cache is empty or the network fetch failed.
    for (const m of BUILTIN_OPENROUTER_MODELS) byId.set(m.id, m);
    // Live API results overwrite the synthetic entries on collision so live
    // pricing / modalities / descriptions are preferred when available.
    for (const m of models) byId.set(m.id, m);
    return [...byId.values()];
  }, [models]);

  const primaryPoolsideModels = useMemo(() => {
    const byId = new Map(selectableModels.map((m) => [m.id, m]));
    return PRIMARY_POOLSIDE_MODEL_ORDER
      .map((id) => byId.get(id))
      .filter((m): m is OpenRouterModel => m !== undefined);
  }, [selectableModels]);

  const primaryBuiltinModels = useMemo(() => {
    // Poolside + Tencent (all built-in vendors), in registry order.
    const byId = new Map(selectableModels.map((m) => [m.id, m]));
    const ordered: OpenRouterModel[] = [];
    for (const v of BUILTIN_OPENROUTER_VENDORS) {
      for (const m of v.models) {
        const found = byId.get(m.id);
        if (found) ordered.push(found);
      }
    }
    return ordered;
  }, [selectableModels]);

  const otherSelectableModels = useMemo(
    () => selectableModels.filter((m) => !BUILTIN_MODEL_IDS.has(m.id)),
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
      // Model list is narrowed by the selected built-in vendor.
      if (vendorSelection === "tencent") {
        return primaryBuiltinModels.filter((m) =>
          TENCENT_HY3_MODEL_IDS.has(m.id),
        );
      }
      if (vendorSelection === "inclusionai") {
        return primaryBuiltinModels.filter((m) =>
          INCLUSIONAI_MODEL_IDS.has(m.id),
        );
      }
      if (vendorSelection === "stepfun") {
        return primaryBuiltinModels.filter((m) =>
          STEPFUN_MODEL_IDS.has(m.id),
        );
      }
      if (vendorSelection === "google") {
        return primaryBuiltinModels.filter((m) =>
          GEMINI_MODEL_IDS.has(m.id),
        );
      }
      // Default (Poolside) or unset
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
  }, [selectorMode, vendorSelection, primaryPoolsideModels, primaryBuiltinModels, selectableModels]);

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

  // ── OpenAI derived state (from optimistic modelSelection) ──────

  const selectedOpenAIModel = useMemo(
    () => parseOpenAIModelId(modelSelection),
    [modelSelection],
  );
  const isOpenaiModel = selectedOpenAIModel !== null;
  const openaiMode: OpenAIMode = selectedOpenAIModel?.mode ?? "standard";
  const openaiTierSelection = selectedOpenAIModel
    ? buildOpenAIModelId(selectedOpenAIModel.tier, "standard")
    : "";

  // ── Route identity tracking + edit guards ────────────────────
  // Only reconstitute UI from saved config when profile or model
  // key actually changes — NOT on every selectableModels repopulation
  // or parent re-render.  During an active save, stale external props
  // are ignored to prevent local edits from being overwritten.

  const routeId = useMemo(
    () => `${profileId ?? ""}:${modelKey}`,
    [profileId, modelKey],
  );

  // Latest-route ref — synced every render so async handlers always
  // see the current route, never a stale one from a previous render.
  const currentRouteIdRef = useRef(routeId);
  currentRouteIdRef.current = routeId;

  const saveGenerationRef = useRef(0);

  const { begin: beginSave, isCurrent: isCurrentSave } = useRouteSaveGeneration(
    routeId,
    profileId,
    modelKey,
    currentRouteIdRef,
    saveGenerationRef,
  );

  const lastSyncedRouteId = useRef<string | null>(null);

  // Track external value changes so we can detect rollback
  const prevCurrentUpstream = useRef(currentUpstream);
  const prevCurrentThinkingMode = useRef(currentThinkingMode);
  const prevCurrentReasoningEffort = useRef(currentReasoningEffort);

  /** The last (upstream, thinking, effort) submitted to `saveModelRoute`.
   *  Stores the *expected persisted* values (not the raw command args)
   *  so they match what the parent reads back from config. */
  const lastSubmittedRef = useRef<{
    routeId: string;
    upstreamModel: string;
    thinkingMode?: string;
    reasoningEffort?: string;
  } | null>(null);

  /** The upstream model that Effect A last accepted from saved config.
   *  Effect B reads this as its classification target.
   *  A state (not a ref) so that changes trigger Effect B re-execution
   *  even when selectableModels has already loaded. */
  const [classificationTarget, setClassificationTarget] =
    useState(currentUpstream ?? "");

  // Rollback local UI to the saved route.  Used when invoke fails or
  // when a stale config sync must be rejected.
  const syncUiFromSavedRoute = useCallback(() => {
    setSelectedModelId(currentUpstream);
    setThinkingSelection(
      normalizeThinkingSelection(currentUpstream, currentThinkingMode, currentReasoningEffort),
    );
    setClassificationTarget(currentUpstream ?? "");
    if (!currentUpstream) {
      setSelectorMode("group");
      setVendorSelection("");
      setModelSelection("");
      setShowCustom(false);
      setCustomText("");
      return;
    }
    const builtinVendor = findBuiltinVendorByModelId(currentUpstream);
    if (builtinVendor) {
      setSelectorMode("group");
      setVendorSelection(builtinVendor.id);
      setModelSelection(currentUpstream);
      setShowCustom(false);
      setCustomText("");
      return;
    }
    // Non-builtin: neutral state.  Effect B will classify.
    setSelectorMode("other");
    setVendorSelection("");
    setModelSelection(currentUpstream);
    setShowCustom(false);
    setCustomText("");
  }, [currentUpstream, currentThinkingMode, currentReasoningEffort]);

  // Latest-callback ref — async drain always calls the current
  // syncUiFromSavedRoute, never a stale closure from an old route.
  const syncUiFromSavedRouteRef = useRef(syncUiFromSavedRoute);
  syncUiFromSavedRouteRef.current = syncUiFromSavedRoute;

  // ── Save queue hook ──────────────────────────────────────────

  const formatSaveFailed = useCallback(
    (error: unknown) => t("openRouterModels.saveFailed", { error: String(error) }),
    [t],
  );
  const formatRefreshFailed = useCallback(
    (error: unknown) => t("openRouterModels.saveOkRefreshFailed", { error: String(error) }),
    [t],
  );
  const formatRestartFailed = useCallback(
    (error: unknown) => t("openRouterModels.saveOkRestartFailed", { error: String(error) }),
    [t],
  );

  const { saving, savingRef, saveModelRoute, saveModel } = useOpenRouterSaveQueue({
    onSaved,
    gatewayRunning,
    restartGateway,
    currentRouteIdRef,
    syncUiFromSavedRouteRef,
    lastSubmittedRef,
    setSaveError,
    formatSaveFailed,
    formatRefreshFailed,
    formatRestartFailed,
  });

  // ── Effect A: sync selectedModelId + thinking from saved config ──
  // Only fires when the saved route values actually change externally
  // (profile/model switch, config reload after save, or explicit
  // rollback after a save failure).  selectableModels and
  // hasLoadedModels are NOT dependencies — classification of
  // non-builtin models is delegated entirely to Effect B.
  useEffect(() => {
    const routeChanged = lastSyncedRouteId.current !== routeId;
    const upstreamChanged = prevCurrentUpstream.current !== currentUpstream;
    const thinkingChanged =
      prevCurrentThinkingMode.current !== currentThinkingMode;
    const effortChanged =
      prevCurrentReasoningEffort.current !== currentReasoningEffort;

    if (!routeChanged && !upstreamChanged && !thinkingChanged && !effortChanged) {
      return;
    }

    // ── Save guard: while a drain is in-flight, ignore stale external
    //    props that don't match our last submitted values to the backend.
    //    They're intermediate states from a parent re-render, not
    //    authoritative config reloads. ───────────────────────────────
    if (savingRef.current && lastSubmittedRef.current && lastSubmittedRef.current.routeId === routeId) {
      const last = lastSubmittedRef.current;
      const upstreamMatches =
        !upstreamChanged || currentUpstream === last.upstreamModel;
      const thinkingMatches =
        !thinkingChanged || currentThinkingMode === last.thinkingMode;
      const effortMatches =
        !effortChanged || currentReasoningEffort === last.reasoningEffort;
      if (!upstreamMatches || !thinkingMatches || !effortMatches) {
        // External values differ from our last submit — stale intermediate
        // render.  Wait for the authoritative config reload after save
        // completes.
        return;
      }
    }

    lastSyncedRouteId.current = routeId;
    prevCurrentUpstream.current = currentUpstream;
    prevCurrentThinkingMode.current = currentThinkingMode;
    prevCurrentReasoningEffort.current = currentReasoningEffort;

    syncUiFromSavedRoute();
  }, [currentUpstream, currentThinkingMode, currentReasoningEffort, routeId, syncUiFromSavedRoute]);

  // ── Effect B: classify non-builtin model once selectableModels ──
  // loads.  The classification target is `classificationTarget` — a state
  // set by Effect A only when a saved config sync is accepted.
  // Guards skip this effect while a save is in-flight OR when the user
  // has edited away from the last saved model, so it never overwrites a
  // local edit with an old saved value.
  useEffect(() => {
    if (!hasLoadedModels || !classificationTarget) return;

    // Active save: don't overwrite optimistic local state.
    if (saving) return;

    // User has a local edit in progress — don't reclassify to the old
    // saved model.
    if (selectedModelId !== classificationTarget) return;

    const upstream = classificationTarget;

    // Builtin models are fully classified by Effect A.
    if (findBuiltinVendorByModelId(upstream)) return;

    if (selectableModels.some((m) => m.id === upstream)) {
      const vendorId = isRouterModel(upstream)
        ? "openrouter"
        : getVendorId(upstream);
      setSelectorMode("other");
      setVendorSelection(vendorId);
      setModelSelection(upstream);
      setShowCustom(false);
      setCustomText("");
      return;
    }

    if (OPENROUTER_FAMILY_ALIAS_IDS.has(upstream)) {
      // Alias not yet in selectableModels — keep trying on next list update.
      // State was set to neutral by Effect A.
      return;
    }

    // Unknown model — show as custom.
    setSelectorMode("other");
    setVendorSelection(CUSTOM_VENDOR_ID);
    setModelSelection("");
    setShowCustom(true);
    setCustomText(upstream);
  }, [hasLoadedModels, selectableModels, classificationTarget, selectedModelId, saving]);

  // ── Event Handlers ───────────────────────────────────────────

  const handleVendorChange = useCallback(
    (value: string) => {
      setSaveError(null);

      if (selectorMode === "group") {
        const builtinVendor = BUILTIN_OPENROUTER_VENDORS.find((v) => v.id === value);
        if (builtinVendor) {
          setVendorSelection(builtinVendor.id);
          setModelSelection(
            BUILTIN_MODEL_IDS.has(selectedModelId) &&
              findBuiltinVendorByModelId(selectedModelId)?.id === builtinVendor.id
              ? selectedModelId
              : "",
          );
          return;
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
        const fallbackVendor = findBuiltinVendorByModelId(selectedModelId)?.id
          ?? POOLSIDE_VENDOR.id;
        setSelectorMode("group");
        setVendorSelection(fallbackVendor);
        setModelSelection(
          BUILTIN_MODEL_IDS.has(selectedModelId) &&
            findBuiltinVendorByModelId(selectedModelId)?.id === fallbackVendor
            ? selectedModelId
            : "",
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
    async (modelId: string, openaiModeOverride?: OpenAIMode) => {
      if (!modelId) return;
      let resolvedId = modelId;

      // OpenAI tier/mode resolution: on tier change the dropdown passes
      // openaiMode as an explicit override; on mode change the mode
      // dropdown passes the newly requested mode. Without explicit override,
      // fall back to the mode already encoded in the model ID.
      const parsedOpenAI = parseOpenAIModelId(resolvedId);
      if (parsedOpenAI) {
        resolvedId = buildOpenAIModelId(
          parsedOpenAI.tier,
          openaiModeOverride ?? parsedOpenAI.mode,
        );
      }

      const model = selectableModels.find((m) => m.id === resolvedId);
      if (!model) return;

      const saveReq = beginSave();
      setSaveError(null);

      const nextThinking: ThinkingSelection = findClosestThinking(model.id, thinkingSelection);
      const { thinkingMode, reasoningEffort } = toStoredThinking(nextThinking);

      // Optimistic update: show the new model immediately while saving.
      const previous = {
        selectedModelId,
        modelSelection,
        thinkingSelection,
      };
      setSelectedModelId(model.id);
      setModelSelection(model.id);
      setThinkingSelection(nextThinking);

      const result = await saveModelRoute({
        routeId: saveReq.routeId,
        profileId: saveReq.profileId,
        modelKey: saveReq.modelKey,
        upstreamModel: model.id,
        thinkingMode,
        reasoningEffort,
      });

      if (!isCurrentSave(saveReq)) return;
      if (result.status === "superseded") return;
      if (result.status === "failed") {
        setSelectedModelId(previous.selectedModelId);
        setModelSelection(previous.modelSelection);
        setThinkingSelection(previous.thinkingSelection);
        return;
      }
    },
    [selectableModels, selectedModelId, modelSelection, thinkingSelection, saveModelRoute,
      beginSave, isCurrentSave],
  );

  const handleCustomConfirm = useCallback(async () => {
    const normalized = customText.trim();
    if (!normalized) return;

    const saveReq = beginSave();
    setSaveError(null);
    const result = await saveModel({
      routeId: saveReq.routeId,
      profileId: saveReq.profileId,
      modelKey: saveReq.modelKey,
      upstreamModel: normalized,
    });
    if (!isCurrentSave(saveReq)) return;
    if (result.status === "superseded") return;
    if (result.status === "failed") return;
    setSelectedModelId(normalized);
    setVendorSelection(CUSTOM_VENDOR_ID);
    setModelSelection("");
    setShowCustom(true);
    setCustomText(normalized);
  }, [customText, saveModel, beginSave, isCurrentSave]);

  const handleThinkingChange = useCallback(
    async (mode: ThinkingSelection) => {
      if (!selectedModelId) return;
      const { thinkingMode, reasoningEffort } = toStoredThinking(mode);

      const saveReq = beginSave();

      const previous = thinkingSelection;
      setThinkingSelection(mode);

      const result = await saveModelRoute({
        routeId: saveReq.routeId,
        profileId: saveReq.profileId,
        modelKey: saveReq.modelKey,
        upstreamModel: selectedModelId,
        thinkingMode: thinkingMode ?? null,
        reasoningEffort: reasoningEffort ?? null,
      });

      if (!isCurrentSave(saveReq)) return;
      if (result.status === "superseded") return;
      if (result.status === "failed") {
        setThinkingSelection(previous);
        return;
      }
    },
    [selectedModelId, thinkingSelection, saveModelRoute, beginSave, isCurrentSave],
  );

  const handleOpenaiModeChange = useCallback(
    async (mode: OpenAIMode) => {
      const parsed = parseOpenAIModelId(modelSelection);
      if (!parsed) return;
      const nextModelId = buildOpenAIModelId(parsed.tier, mode);
      // Pass mode explicitly so the tier-resolution block in handleModelChange
      // doesn't overwrite it with a stale closure value.
      await handleModelChange(nextModelId, mode);
    },
    [modelSelection, handleModelChange],
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

  const groupSelectValue =
    selectorMode === "group" &&
    BUILTIN_OPENROUTER_VENDORS.some((v) => v.id === vendorSelection)
      ? vendorSelection
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
            data-testid="openrouter-vendor-select"
          >
            {BUILTIN_OPENROUTER_VENDORS.map((v) => (
              <option key={v.id} value={v.id}>{t(v.labelKey)}</option>
            ))}
            <option value={SENTINEL_OTHER}>{t("openRouterModels.groupOtherModels")}</option>
          </select>
        ) : (
          <select
            className="openrouter-vendor-select"
            value={vendorSelection}
            onChange={(e) => handleVendorChange(e.target.value)}
            data-testid="openrouter-vendor-select"
          >
            <option value="">{t("openRouterModels.selectVendor")}</option>
            <option value={SENTINEL_BACK}>
              ← {BUILTIN_OPENROUTER_VENDORS.map((v) => t(v.labelKey)).join("/")}/{t("openRouterModels.groupOtherModels")}
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
            {isOpenaiModel ? (
              <>
                {/* Tier dropdown */}
                <select
                  className="openrouter-model-select"
                  value={openaiTierSelection}
                  onChange={(e) => void handleModelChange(e.target.value, openaiMode)}
                  data-testid="openrouter-model-select"
                  disabled={saving}
                  aria-label={t("openRouterModels.selectModel")}
                >
                  {OPENAI_MODEL_CHOICES.map((choice) => (
                    <option key={choice.id} value={choice.id}>
                      {choice.displayName}
                    </option>
                  ))}
                </select>

                {/* Mode dropdown */}
                <span className="openrouter-mode-label">{t("openRouterModels.modeLabel")}</span>
                <select
                  className="openrouter-mode-select"
                  value={openaiMode}
                  onChange={(e) => void handleOpenaiModeChange(e.target.value as OpenAIMode)}
                  disabled={saving}
                  data-testid="openrouter-openai-mode-select"
                  aria-label={t("openRouterModels.modeLabel")}
                >
                  <option value="standard">{t("openRouterModels.modeStandard")}</option>
                  <option value="pro">{t("openRouterModels.modePro")}</option>
                </select>

                {/* Effort dropdown */}
                <span className="openrouter-mode-label">{t("apiKeyPanel.thinkingMode")}</span>
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
              </>
            ) : (
              <>
                <select
                  className="openrouter-model-select"
                  value={modelSelection}
                  onChange={(e) => handleModelChange(e.target.value)}
                  data-testid="openrouter-model-select"
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
                <span className="openrouter-mode-label">{t("apiKeyPanel.thinkingMode")}</span>
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
              </>
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
