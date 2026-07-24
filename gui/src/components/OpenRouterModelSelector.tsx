import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { TranslationKey } from "../i18n";
import type { OpenRouterModel, OpenRouterModelsResult } from "../types/openrouter";

// ── Recommended model IDs (Claude models via OpenRouter family aliases) ──
const RECOMMENDED_MODELS = new Set([
  "~anthropic/claude-opus-latest",
  "~anthropic/claude-sonnet-latest",
  "~anthropic/claude-haiku-latest",
]);

const ANTHROPIC_PREFIXES = [
  "anthropic/",
  "~anthropic/",
];

// ── Group classification ──
function classifyModel(model: OpenRouterModel): string {
  const id = model.id.toLowerCase();
  const slug = model.canonicalSlug?.toLowerCase() ?? "";
  const display = model.displayName.toLowerCase();

  if (RECOMMENDED_MODELS.has(model.id)) return "recommended";

  // Anthropic family aliases or real IDs
  if (id.startsWith("anthropic/") || id.startsWith("~anthropic/")
      || slug.startsWith("anthropic") || display.includes("claude")) {
    return "anthropic";
  }

  if (id.startsWith("openai/") || id.startsWith("~openai/")
      || slug.startsWith("openai") || display.includes("gpt")) {
    return "openai";
  }

  if (id.startsWith("google/") || id.startsWith("~google/")
      || slug.startsWith("google") || display.includes("gemini")) {
    return "google";
  }

  if (id.startsWith("deepseek/") || id.startsWith("~deepseek/")
      || slug.startsWith("deepseek")) {
    return "deepseek";
  }

  if (id.startsWith("moonshotai/") || id.startsWith("~moonshotai/")
      || slug.startsWith("moonshotai") || display.includes("kimi") || display.includes("moonshot")) {
    return "moonshot";
  }

  if (id.startsWith("qwen/") || id.startsWith("~qwen/")
      || slug.startsWith("qwen")) {
    return "qwen";
  }

  return "other";
}

// ── Group labels and order ──
const GROUP_ORDER = ["recommended", "anthropic", "openai", "google", "deepseek", "moonshot", "qwen", "other"];

const GROUP_LABELS: Record<string, TranslationKey> = {
  recommended: "openRouterModels.groupRecommended",
  anthropic: "openRouterModels.groupAnthropic",
  openai: "openRouterModels.groupOpenai",
  google: "openRouterModels.groupGoogle",
  deepseek: "openRouterModels.groupDeepseek",
  moonshot: "openRouterModels.groupMoonshot",
  qwen: "openRouterModels.groupQwen",
  other: "openRouterModels.groupOther",
};

// ── Props ──
interface OpenRouterModelSelectorProps {
  modelKey: string;
  gatewayModelLabel: string;
  currentUpstream: string;
  currentThinkingMode: string | undefined;
  onSaved: () => void;
}

export default function OpenRouterModelSelector({
  modelKey,
  gatewayModelLabel,
  currentUpstream,
  currentThinkingMode,
  onSaved,
}: OpenRouterModelSelectorProps) {
  const { t } = useTranslation();
  const [models, setModels] = useState<OpenRouterModel[]>([]);
  const [fetchedAt, setFetchedAt] = useState<string>("");
  const [source, setSource] = useState<string>("");
  const [stale, setStale] = useState(false);
  const [warning, setWarning] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState(false);
  const [selectedModelId, setSelectedModelId] = useState(currentUpstream);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [customText, setCustomText] = useState("");
  const [showCustom, setShowCustom] = useState(false);

  const containerRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const statusTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const requestIdRef = useRef(0);

  // ── Determine if current model is in cache ──
  const isInCache = useMemo(
    () => models.some((m) => m.id === currentUpstream),
    [models, currentUpstream],
  );

  // ── Load models ──
  const loadModels = useCallback(async (forceRefresh: boolean) => {
    setLoading(true);
    setWarning(null);
    const reqId = ++requestIdRef.current;
    try {
      const result = await invoke<OpenRouterModelsResult>("openrouter_get_models", { forceRefresh });
      if (reqId !== requestIdRef.current) return; // stale
      setModels(result.models);
      setFetchedAt(result.fetchedAt);
      setSource(result.source);
      setStale(result.stale);
      setWarning(result.warning ?? null);
    } catch (e) {
      console.error("Failed to fetch OpenRouter models:", e);
      setWarning(String(e));
    } finally {
      if (reqId === requestIdRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadModels(false);
  }, [loadModels]);

  // ── Group models ──
  const grouped = useMemo(() => {
    const groups: Map<string, OpenRouterModel[]> = new Map();
    for (const m of models) {
      const group = classifyModel(m);
      if (!groups.has(group)) groups.set(group, []);
      groups.get(group)!.push(m);
    }
    // Sort within groups by display name
    for (const [_, items] of groups) {
      items.sort((a, b) => a.displayName.localeCompare(b.displayName));
    }
    // Order groups
    const result: { key: string; label: string; items: OpenRouterModel[] }[] = [];
    for (const gk of GROUP_ORDER) {
      const items = groups.get(gk);
      if (items && items.length > 0) {
        result.push({ key: gk, label: t(GROUP_LABELS[gk]), items });
      }
    }
    return result;
  }, [models, t]);

  // ── Filtered models based on search ──
  const filteredGroups = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return grouped;
    return grouped
      .map((g) => ({
        ...g,
        items: g.items.filter(
          (m) =>
            m.displayName.toLowerCase().includes(q) ||
            m.id.toLowerCase().includes(q) ||
            (m.description ?? "").toLowerCase().includes(q),
        ),
      }))
      .filter((g) => g.items.length > 0);
  }, [grouped, search]);

  // ── Selected model display name ──
  const selectedDisplayName = useMemo(() => {
    if (showCustom && customText) return customText;
    const found = models.find((m) => m.id === selectedModelId);
    return found?.displayName ?? selectedModelId;
  }, [models, selectedModelId, showCustom, customText]);

  // ── Anthropic compatibility check ──
  const isAnthropicCompatible = useMemo(() => {
    if (showCustom && customText) return false; // custom = unknown
    const model = models.find((m) => m.id === selectedModelId);
    if (!model) return false;
    return ANTHROPIC_PREFIXES.some(
      (p) => model.id.startsWith(p) || (model.canonicalSlug ?? "").startsWith(p),
    );
  }, [models, selectedModelId, showCustom, customText]);

  // ── Save upstream model ──
  const saveModel = useCallback(
    async (upstream: string) => {
      setSaveStatus("saving");
      const reqId = ++requestIdRef.current;
      try {
        await invoke("set_model_upstream", {
          providerId: "openrouter",
          modelKey,
          upstreamModel: upstream,
        });
        if (reqId !== requestIdRef.current) return;
        setSaveStatus("saved");
        if (statusTimerRef.current) clearTimeout(statusTimerRef.current);
        statusTimerRef.current = setTimeout(() => setSaveStatus("idle"), 2000);
        onSaved();
      } catch (e) {
        console.error("Failed to update OpenRouter upstream:", e);
        if (reqId !== requestIdRef.current) return;
        setSaveStatus("error");
        if (statusTimerRef.current) clearTimeout(statusTimerRef.current);
        statusTimerRef.current = setTimeout(() => setSaveStatus("idle"), 3000);
      }
    },
    [modelKey, onSaved],
  );

  // ── Handle model selection ──
  const handleSelect = useCallback(
    (modelId: string) => {
      setSelectedModelId(modelId);
      setShowCustom(false);
      setCustomText("");
      setOpen(false);
      saveModel(modelId);
    },
    [saveModel],
  );

  const handleCustomConfirm = useCallback(() => {
    const text = customText.trim();
    if (!text) return;
    setSelectedModelId(text);
    setOpen(false);
    saveModel(text);
  }, [customText, saveModel]);

  const handleRefresh = useCallback(() => {
    loadModels(true);
  }, [loadModels]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && showCustom) {
        e.preventDefault();
        handleCustomConfirm();
      }
    },
    [showCustom, handleCustomConfirm],
  );

  // ── Close on outside click ──
  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    if (open) {
      document.addEventListener("mousedown", handleClick);
      return () => document.removeEventListener("mousedown", handleClick);
    }
  }, [open]);

  // ── Focus search on open ──
  useEffect(() => {
    if (open && searchRef.current) {
      searchRef.current.focus();
    }
  }, [open]);

  return (
    <div
      ref={containerRef}
      className="openrouter-model-selector"
      style={{ position: "relative", minWidth: 200 }}
    >
      {/* Selected value trigger */}
      <div
        className={`model-select-trigger${open ? " open" : ""}`}
        onClick={() => setOpen((v) => !v)}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            setOpen((v) => !v);
          }
        }}
      >
        <span className="model-select-value" style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
          {selectedDisplayName}
        </span>
        {stale && (
          <span className="stale-badge" title={warning ?? undefined} style={{ color: "#c0392b", marginLeft: 4 }}>
            &#9888;
          </span>
        )}
        <span className="model-select-arrow">&#9662;</span>
      </div>

      {/* Dropdown */}
      {open && (
        <div className="model-select-dropdown openrouter-dropdown" style={{ width: 360 }}>
          {/* Search */}
          <div className="openrouter-search-bar">
            <input
              ref={searchRef}
              type="text"
              className="openrouter-search-input"
              placeholder={t("openRouterModels.searchModels")}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={handleKeyDown}
            />
            <button
              className="openrouter-refresh-btn"
              onClick={handleRefresh}
              disabled={loading}
              title={t("openRouterModels.refresh")}
            >
              {loading ? "⟳" : "↻"}
            </button>
          </div>

          {/* Status */}
          {(source === "network" || source === "cache") && (
            <div className="openrouter-status-line" style={{ fontSize: 10, color: "#6b7280", padding: "2px 10px" }}>
              {source === "network"
                ? t("openRouterModels.fetchedLive", { time: fetchedAt })
                : t("openRouterModels.fetchedCached", { time: fetchedAt })}
            </div>
          )}

          {/* Warning */}
          {warning && (
            <div className="openrouter-warning" style={{ fontSize: 11, color: "#c0392b", padding: "2px 10px" }}>
              {warning}
            </div>
          )}

          {/* Compatibility warning */}
          {!isAnthropicCompatible && selectedModelId && (
            <div className="openrouter-compat-warning" style={{
              fontSize: 11, color: "#8d5f00", padding: "4px 10px",
              background: "#fef9e7", borderBottom: "1px solid #f9e79f",
            }}>
              {t("openRouterModels.compatWarning")}
            </div>
          )}

          {/* Grouped model list */}
          <div className="openrouter-model-list" style={{ maxHeight: 320, overflowY: "auto" }}>
            {filteredGroups.map((group) => (
              <div key={group.key} className="openrouter-group">
                <div className="openrouter-group-label" style={{
                  fontSize: 10, fontWeight: 600, color: "#9ca3af",
                  padding: "4px 10px 2px", textTransform: "uppercase", letterSpacing: "0.05em",
                }}>
                  {group.label}
                </div>
                {group.items.map((model) => (
                  <div
                    key={model.id}
                    className={`openrouter-model-item${model.id === selectedModelId && !showCustom ? " selected" : ""}`}
                    style={{
                      padding: "4px 10px",
                      cursor: "pointer",
                      background: model.id === selectedModelId && !showCustom ? "var(--accent-bg, #e8f0fe)" : undefined,
                    }}
                    onClick={() => handleSelect(model.id)}
                    onMouseEnter={(e) => {
                      (e.currentTarget as HTMLElement).style.background =
                        model.id === selectedModelId && !showCustom
                          ? "var(--accent-bg, #e8f0fe)"
                          : "#f3f4f6";
                    }}
                    onMouseLeave={(e) => {
                      (e.currentTarget as HTMLElement).style.background =
                        model.id === selectedModelId && !showCustom
                          ? "var(--accent-bg, #e8f0fe)"
                          : "";
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                      <div>
                        <span style={{ fontWeight: 500, fontSize: 13 }}>{model.displayName}</span>
                        <span style={{ fontSize: 10, color: "#9ca3af", marginLeft: 6 }}>
                          {model.canonicalSlug ?? model.id}
                        </span>
                      </div>
                      {model.pricing.prompt && (
                        <span style={{ fontSize: 10, color: "#6b7280" }}>
                          {model.pricing.prompt}/1M
                        </span>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            ))}

            {filteredGroups.length === 0 && !loading && (
              <div style={{ padding: 12, textAlign: "center", color: "#9ca3af", fontSize: 12 }}>
                {t("openRouterModels.noResults")}
              </div>
            )}
          </div>

          {/* Custom model entry */}
          <div className="openrouter-custom" style={{ borderTop: "1px solid #e5e7eb", padding: "6px 10px" }}>
            {showCustom ? (
              <div style={{ display: "flex", gap: 4 }}>
                <input
                  type="text"
                  className="openrouter-custom-input"
                  style={{ flex: 1, fontSize: 12, padding: "2px 6px" }}
                  placeholder={t("openRouterModels.customModelPlaceholder")}
                  value={customText}
                  onChange={(e) => setCustomText(e.target.value)}
                  onKeyDown={handleKeyDown}
                  autoFocus
                />
                <button
                  style={{ fontSize: 12, padding: "2px 8px" }}
                  onClick={handleCustomConfirm}
                  disabled={!customText.trim()}
                >
                  {t("openRouterModels.confirm")}
                </button>
              </div>
            ) : (
              <div
                style={{
                  fontSize: 12, color: "var(--accent-color, #2563eb)",
                  cursor: "pointer", padding: "2px 0",
                }}
                onClick={() => {
                  setShowCustom(true);
                  setSearch("");
                }}
              >
                {t("openRouterModels.customModel")}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Save status indicator */}
      {saveStatus !== "idle" && (
        <span style={{ fontSize: 11, marginLeft: 6, color: saveStatus === "saved" ? "#16a34a" : saveStatus === "error" ? "#dc2626" : "#6b7280" }}>
          {saveStatus === "saving" ? "..." : saveStatus === "saved" ? "✓" : "✗"}
        </span>
      )}
    </div>
  );
}
