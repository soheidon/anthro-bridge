import { useEffect, useLayoutEffect, useState, useCallback, useRef } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { TranslationKey } from "../i18n";
import type { GatewayStatus, GatewayConfig, ModelEntry } from "../types";
import { MODEL_CAPABILITIES } from "../modelCapabilities";

interface ProviderTilesProps {
  health: GatewayStatus | null;
  onConfigChanged?: () => void;
}

const PROVIDER_ORDER = ["deepseek", "mimo", "minimax", "kimi"];

const TILE_META: Record<string, { descKey: TranslationKey }> = {
  deepseek: { descKey: "statusPanel.tileDeepseekDesc" },
  mimo:     { descKey: "statusPanel.tileMimoDesc" },
  minimax:  { descKey: "statusPanel.tileMinimaxDesc" },
  kimi:     { descKey: "statusPanel.tileKimiDesc" },
};

interface ModelCaps {
  supports_vision: boolean;
  supports_video: boolean;
  supports_image_url: boolean;
  supports_image_base64: boolean;
  supports_video_url: boolean;
  supports_video_base64: boolean;
  force_thinking: boolean;
  thinking: string;
}

interface TileData {
  providerId: string;
  displayName: string;
  descKey: TranslationKey;
  opusUpstream: string;
  sonnetUpstream: string;
  haikuUpstream: string;
  opusCaps: ModelCaps;
  sonnetCaps: ModelCaps;
  haikuCaps: ModelCaps;
  opusThinkingMode: string | undefined;
  sonnetThinkingMode: string | undefined;
  haikuThinkingMode: string | undefined;
  opusReasoningEffort: string | undefined;
  sonnetReasoningEffort: string | undefined;
  haikuReasoningEffort: string | undefined;
  isActive: boolean;
}

function resolveTileCaps(upstreamModel: string): ModelCaps {
  const caps = MODEL_CAPABILITIES[upstreamModel];
  if (caps) {
    return { ...caps };
  }
  return {
    supports_vision: false,
    supports_video: false,
    supports_image_url: false,
    supports_image_base64: false,
    supports_video_url: false,
    supports_video_base64: false,
    force_thinking: false,
    thinking: "default",
  };
}

function modelSummary(
  tierLabel: string,
  upstream: string,
  thinkingMode: string | undefined,
  reasoningEffort: string | undefined,
  upstreamModel: string,
): string {
  let text = `${tierLabel} ${upstream}`;
  const caps = MODEL_CAPABILITIES[upstreamModel];
  const showThinking = thinkingMode === "thinking" || thinkingMode === "thinking_only";
  if (showThinking) text += " + thinking";
  const validEfforts = ["high", "medium", "low", "max"];
  if (reasoningEffort && validEfforts.includes(reasoningEffort) && caps?.supportsReasoningEffort) {
    text += ` + ${reasoningEffort} effort`;
  }
  return text;
}

function buildTiles(config: GatewayConfig | null): TileData[] {
  if (!config) return [];
  const activeId = config.active_provider ?? "deepseek";
  const tiles = Object.entries(config.providers).map(([pid, p]) => {
    const opus = p.models?.["claude-opus-4-8"];
    const sonnet = p.models?.["claude-sonnet-5"];
    const haiku = p.models?.["claude-haiku-4-5"];
    const opusUp = opus?.upstream_model ?? haiku?.upstream_model ?? "—";
    const sonnetUp = sonnet?.upstream_model ?? "—";
    const haikuUp = haiku?.upstream_model ?? "—";
    return {
      providerId: pid,
      displayName: p.display_name,
      descKey: TILE_META[pid]?.descKey ?? "",
      opusUpstream: opusUp,
      sonnetUpstream: sonnetUp,
      haikuUpstream: haikuUp,
      opusCaps: resolveTileCaps(opusUp),
      sonnetCaps: resolveTileCaps(sonnetUp),
      haikuCaps: resolveTileCaps(haikuUp),
      opusThinkingMode: opus?.thinking_mode,
      sonnetThinkingMode: sonnet?.thinking_mode,
      haikuThinkingMode: haiku?.thinking_mode,
      opusReasoningEffort: opus?.reasoning_effort,
      sonnetReasoningEffort: sonnet?.reasoning_effort,
      haikuReasoningEffort: haiku?.reasoning_effort,
      isActive: pid === activeId,
    };
  });
  tiles.sort((a, b) => {
    const ai = PROVIDER_ORDER.indexOf(a.providerId);
    const bi = PROVIDER_ORDER.indexOf(b.providerId);
    return (ai >= 0 ? ai : 99) - (bi >= 0 ? bi : 99);
  });
  return tiles;
}

type CapKey = "think" | "normal" | "image" | "imageUrl" | "imageB64" | "video" | "videoUrl" | "videoB64";

interface CapItem {
  key: CapKey;
  labelKey: TranslationKey;
  desc: string;
  supported: boolean;
}

function buildCapItems(caps: ModelCaps, t: (key: TranslationKey, params?: Record<string, string>) => string): CapItem[] {
  const thinkDesc = caps.force_thinking
    ? t("popup.desc.think.force")
    : caps.thinking === "disabled"
      ? t("popup.desc.think.no")
      : t("popup.desc.think.ok");
  const thinkSupported = !caps.force_thinking && caps.thinking !== "disabled";

  const normalSupported = !caps.force_thinking;

  return [
    { key: "think",     labelKey: "popup.label.think",     desc: thinkDesc,                          supported: thinkSupported },
    { key: "normal",    labelKey: "popup.label.normal",    desc: normalSupported ? t("popup.desc.normal.ok") : t("popup.desc.normal.no"), supported: normalSupported },
    { key: "image",     labelKey: "popup.label.image",     desc: caps.supports_vision ? t("popup.desc.image.ok") : t("popup.desc.image.no"), supported: caps.supports_vision },
    { key: "imageUrl",  labelKey: "popup.label.imageUrl",  desc: caps.supports_image_url ? t("popup.desc.imageUrl.ok") : t("popup.desc.imageUrl.no"), supported: caps.supports_image_url },
    { key: "imageB64",  labelKey: "popup.label.imageB64",  desc: caps.supports_image_base64 ? t("popup.desc.imageB64.ok") : t("popup.desc.imageB64.no"), supported: caps.supports_image_base64 },
    { key: "video",     labelKey: "popup.label.video",     desc: caps.supports_video ? t("popup.desc.video.ok") : t("popup.desc.video.no"), supported: caps.supports_video },
    { key: "videoUrl",  labelKey: "popup.label.videoUrl",  desc: caps.supports_video_url ? t("popup.desc.videoUrl.ok") : t("popup.desc.videoUrl.no"), supported: caps.supports_video_url },
    { key: "videoB64",  labelKey: "popup.label.videoB64",  desc: caps.supports_video_base64 ? t("popup.desc.videoB64.ok") : t("popup.desc.videoB64.no"), supported: caps.supports_video_base64 },
  ];
}

function modeDisplayText(
  thinkingMode: string | undefined,
  caps: ModelCaps,
  t: (key: TranslationKey, params?: Record<string, string>) => string,
): string | null {
  if (thinkingMode === "normal") return t("popup.mode.normal");
  if (thinkingMode === "thinking") return t("popup.mode.thinking");
  if (thinkingMode === "thinking_only") return t("popup.mode.thinkingOnly");
  if (caps.force_thinking) return t("popup.mode.thinkingOnly");
  if (caps.thinking === "disabled") return t("popup.mode.normal");
  return null;
}

export default function ProviderTiles({ health, onConfigChanged }: ProviderTilesProps) {
  const { t } = useTranslation();
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [switching, setSwitching] = useState(false);
  const [switchMessage, setSwitchMessage] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [popoverPos, setPopoverPos] = useState<{ top: number; left: number; maxHeight: number } | null>(null);
  const [popoverHeight, setPopoverHeight] = useState(0);
  const closeTimerRef = useRef<number | null>(null);
  const tileRefs = useRef<Map<string, HTMLDivElement>>(new Map());
  const popoverRef = useRef<HTMLDivElement>(null);

  const POPOVER_MARGIN = 16;

  const refresh = useCallback(() => {
    invoke<GatewayConfig>("read_config")
      .then(setConfig)
      .catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    return () => {
      if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
    };
  }, []);

  const cancelClose = useCallback(() => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  const scheduleClose = useCallback(() => {
    closeTimerRef.current = window.setTimeout(() => {
      setHoveredId(null);
      setPopoverPos(null);
      setPopoverHeight(0);
    }, 150);
  }, []);

  const handleTileEnter = useCallback((providerId: string) => {
    cancelClose();
    setPopoverPos(null);
    setPopoverHeight(0);
    setHoveredId(providerId);
  }, [cancelClose]);

  useLayoutEffect(() => {
    if (!hoveredId || popoverPos || !popoverRef.current) return;
    const height = popoverRef.current.offsetHeight;
    const width = popoverRef.current.offsetWidth;
    if (height === 0 || width === 0) return;
    setPopoverHeight(height);

    const el = tileRefs.current.get(hoveredId);
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const spaceBelow = window.innerHeight - rect.bottom - POPOVER_MARGIN;
    const spaceAbove = rect.top - POPOVER_MARGIN;
    const placeBelow = height <= spaceBelow || spaceBelow >= spaceAbove;
    const top = placeBelow
      ? rect.bottom + 6
      : Math.max(POPOVER_MARGIN, rect.top - height - 6);
    const maxHeight = placeBelow
      ? Math.max(100, window.innerHeight - top - POPOVER_MARGIN)
      : Math.max(100, spaceAbove);
    const left = Math.max(
      POPOVER_MARGIN,
      Math.min(rect.right - width, window.innerWidth - width - POPOVER_MARGIN),
    );
    setPopoverPos({ top, left, maxHeight });
  }, [hoveredId, popoverPos, popoverHeight]);

  const handlePopoverEnter = useCallback(() => {
    cancelClose();
  }, [cancelClose]);

  const handlePopoverLeave = useCallback(() => {
    scheduleClose();
  }, [scheduleClose]);

  const tiles = buildTiles(config);
  const activeProviderId = config?.active_provider ?? "deepseek";
  const gatewayRunning = health?.port_listening ?? false;

  const handleTileClick = useCallback(async (providerId: string) => {
    if (switching) return;
    if (providerId === activeProviderId) return;
    setSwitching(true);
    setSwitchMessage(null);
    try {
      if (gatewayRunning) {
        setSwitchMessage(t("statusPanel.restarting"));
        await invoke("stop_proxy");
        await invoke("update_active_provider", { providerId });
        await invoke("start_proxy");
        setSwitchMessage(t("statusPanel.restarted"));
      } else {
        await invoke("update_active_provider", { providerId });
        setSwitchMessage(t("statusPanel.savedNextStart"));
      }
      refresh();
      onConfigChanged?.();
    } catch (e) {
      console.error(e);
      setSwitchMessage(t("statusPanel.restartFailed"));
    } finally {
      setSwitching(false);
      setTimeout(() => setSwitchMessage(null), 5000);
    }
  }, [switching, activeProviderId, gatewayRunning, refresh, onConfigChanged, t]);

  const hoveredTile = tiles.find(t => t.providerId === hoveredId);

  return (
    <div className="dashboard-section">
      <h3>{t("statusPanel.tileSelectProvider")}</h3>

      <div className="provider-tile-grid">
        {tiles.map((tile) => (
          <div
            key={tile.providerId}
            ref={(el) => {
              if (el) tileRefs.current.set(tile.providerId, el);
            }}
            className={`provider-tile${tile.isActive ? " selected" : ""}`}
            style={switching ? { opacity: 0.6, pointerEvents: "none" } : undefined}
            onMouseEnter={() => handleTileEnter(tile.providerId)}
            onMouseLeave={scheduleClose}
            onClick={() => handleTileClick(tile.providerId)}
          >
            <div className="provider-tile-name">{tile.displayName}</div>
            <div className="provider-tile-desc">{t(tile.descKey)}</div>
            <div className="provider-tile-routes-simple">
              <div><span className="up-mono">{modelSummary(t("statusPanel.tilePro"), tile.opusUpstream, tile.opusThinkingMode, tile.opusReasoningEffort, tile.opusUpstream)}</span></div>
              <div><span className="up-mono">{modelSummary(t("statusPanel.tileFlash"), tile.sonnetUpstream, tile.sonnetThinkingMode, tile.sonnetReasoningEffort, tile.sonnetUpstream)}</span></div>
              <div><span className="up-mono">{modelSummary(t("statusPanel.tileHaiku"), tile.haikuUpstream, tile.haikuThinkingMode, tile.haikuReasoningEffort, tile.haikuUpstream)}</span></div>
            </div>
            <div className="provider-tile-badge">{t("statusPanel.tileActive")}</div>
          </div>
        ))}
      </div>

      {hoveredTile &&
        createPortal(
          <div
            ref={popoverRef}
            className="popover-card"
            style={{
              top: popoverPos?.top ?? -9999,
              left: popoverPos?.left ?? -9999,
              maxHeight: popoverPos?.maxHeight ?? "100vh",
              visibility: popoverPos ? "visible" : "hidden",
            }}
            onMouseEnter={handlePopoverEnter}
            onMouseLeave={handlePopoverLeave}
          >
            <div className="popover-header">
              <span>{t("popup.title", { provider: hoveredTile.displayName })}</span>
            </div>

            <div className="popover-body">
              {/* Opus 4.8 */}
              <div className="popover-model-section">
                <div className="popover-model-name">
                  <span className="up-mono">{modelSummary(t("statusPanel.tilePro"), hoveredTile.opusUpstream, hoveredTile.opusThinkingMode, hoveredTile.opusReasoningEffort, hoveredTile.opusUpstream)}</span>
                </div>
                {(() => {
                  const modeText = modeDisplayText(hoveredTile.opusThinkingMode, hoveredTile.opusCaps, t);
                  if (modeText) {
                    return (
                      <div className="popover-cap-item cap-supported">
                        <div className="popover-cap-label">{t("popup.mode.label")}</div>
                        <div className="popover-cap-desc">{modeText}</div>
                      </div>
                    );
                  }
                  return null;
                })()}
                {buildCapItems(hoveredTile.opusCaps, t).map((item) => (
                  <div key={item.key} className={`popover-cap-item ${item.supported ? "cap-supported" : "cap-unsupported"}`}>
                    <div className="popover-cap-label">{t(item.labelKey)}</div>
                    <div className="popover-cap-desc">{item.desc}</div>
                  </div>
                ))}
              </div>

              {/* Sonnet 5 */}
              <div className="popover-model-section">
                <div className="popover-model-name">
                  <span className="up-mono">{modelSummary(t("statusPanel.tileFlash"), hoveredTile.sonnetUpstream, hoveredTile.sonnetThinkingMode, hoveredTile.sonnetReasoningEffort, hoveredTile.sonnetUpstream)}</span>
                </div>
                {(() => {
                  const modeText = modeDisplayText(hoveredTile.sonnetThinkingMode, hoveredTile.sonnetCaps, t);
                  if (modeText) {
                    return (
                      <div className="popover-cap-item cap-supported">
                        <div className="popover-cap-label">{t("popup.mode.label")}</div>
                        <div className="popover-cap-desc">{modeText}</div>
                      </div>
                    );
                  }
                  return null;
                })()}
                {buildCapItems(hoveredTile.sonnetCaps, t).map((item) => (
                  <div key={item.key} className={`popover-cap-item ${item.supported ? "cap-supported" : "cap-unsupported"}`}>
                    <div className="popover-cap-label">{t(item.labelKey)}</div>
                    <div className="popover-cap-desc">{item.desc}</div>
                  </div>
                ))}
              </div>

              {/* Haiku 4.5 */}
              <div className="popover-model-section">
                <div className="popover-model-name">
                  <span className="up-mono">{modelSummary(t("statusPanel.tileHaiku"), hoveredTile.haikuUpstream, hoveredTile.haikuThinkingMode, hoveredTile.haikuReasoningEffort, hoveredTile.haikuUpstream)}</span>
                </div>
                {(() => {
                  const modeText = modeDisplayText(hoveredTile.haikuThinkingMode, hoveredTile.haikuCaps, t);
                  if (modeText) {
                    return (
                      <div className="popover-cap-item cap-supported">
                        <div className="popover-cap-label">{t("popup.mode.label")}</div>
                        <div className="popover-cap-desc">{modeText}</div>
                      </div>
                    );
                  }
                  return null;
                })()}
                {buildCapItems(hoveredTile.haikuCaps, t).map((item) => (
                  <div key={item.key} className={`popover-cap-item ${item.supported ? "cap-supported" : "cap-unsupported"}`}>
                    <div className="popover-cap-label">{t(item.labelKey)}</div>
                    <div className="popover-cap-desc">{item.desc}</div>
                  </div>
                ))}
              </div>
            </div>
          </div>,
          document.body
        )
      }

      {(switching || switchMessage) && (
        <div className="provider-switch-msg">
          {switching && <div className="loading" />}
          {switchMessage && !switching && <span>{switchMessage}</span>}
        </div>
      )}
    </div>
  );
}
