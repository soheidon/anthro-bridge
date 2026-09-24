import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ClaudeConfigCandidate, ClaudeCodeLaunchCommand, CommandResponse, GatewayConfig } from "../types";
import { buildGatewayClientBaseUrl, GATEWAY_LOCAL_TOKEN } from "../config/gatewayConnection";

const CLAUDE_DESKTOP_MODELS = [
  { name: "claude-opus-5", labelOverride: "Opus 5" },
  { name: "claude-sonnet-5", labelOverride: "Sonnet 5" },
  { name: "claude-haiku-4-5",  labelOverride: "Haiku 4.5" },
];

function buildClaudeConfig(): object {
  return {
    inferenceProvider: "gateway",
    inferenceGatewayBaseUrl: buildGatewayClientBaseUrl(),
    inferenceGatewayApiKey: GATEWAY_LOCAL_TOKEN,
    inferenceGatewayAuthScheme: "bearer",
    inferenceModels: CLAUDE_DESKTOP_MODELS.map((m) => ({
      name: m.name,
      labelOverride: m.labelOverride,
    })),
  };
}

const CLAUDE_JSON = JSON.stringify(buildClaudeConfig(), null, 2);

export interface ClaudeCodeThirdPartySettingsProps {
  config?: GatewayConfig | null;
  refreshConfig?: () => Promise<void>;
  gatewayRunning?: boolean;
  restartGateway?: () => Promise<void>;
  showHeader?: boolean;
}

export function ClaudeCodeThirdPartySettings({
  config,
  refreshConfig,
  gatewayRunning,
  restartGateway,
  showHeader = true,
}: ClaudeCodeThirdPartySettingsProps) {
  const { t } = useTranslation();
  const tpConfig = config?.claude_code?.third_party_provider;
  const [tpEnabled, setTpEnabled] = useState(tpConfig?.enabled ?? false);
  const [tpModel, setTpModel] = useState(tpConfig?.model ?? "mimo-v2.6-distill-qwen-9b");
  const [tpThinking, setTpThinking] = useState(tpConfig?.thinking_mode ?? "normal");
  const [tpVision, setTpVision] = useState(tpConfig?.supports_vision ?? false);
  const [tpContextWindow, setTpContextWindow] = useState<string>(
    tpConfig?.context_window != null ? String(tpConfig.context_window) : ""
  );

  // Sync state when config updates externally
  useEffect(() => {
    if (config?.claude_code?.third_party_provider) {
      const tp = config.claude_code.third_party_provider;
      setTpEnabled(tp.enabled);
      setTpModel(tp.model ?? "mimo-v2.6-distill-qwen-9b");
      setTpThinking(tp.thinking_mode ?? "normal");
      setTpVision(tp.supports_vision ?? false);
      setTpContextWindow(tp.context_window != null ? String(tp.context_window) : "");
    } else if (config) {
      setTpEnabled(false);
      setTpContextWindow("");
    }
  }, [config]);

  const saveThirdPartySettings = useCallback(
    async (
      enabled: boolean,
      model: string,
      thinking: string,
      vision: boolean,
      contextWindowStr: string
    ) => {
      const parsedWindow = parseInt(contextWindowStr.trim(), 10);
      const contextWindow =
        !isNaN(parsedWindow) && parsedWindow > 0 ? parsedWindow : null;

      try {
        const res = await invoke<CommandResponse<null>>(
          "update_claude_code_third_party_settings",
          {
            settings: {
              enabled,
              provider: "ollama",
              base_url: "http://127.0.0.1:11434",
              model: model.trim() || "mimo-v2.6-distill-qwen-9b",
              thinking_mode: thinking,
              supports_vision: vision,
              context_window: contextWindow,
            },
          }
        );
        if (refreshConfig) {
          await refreshConfig();
        }
        if (gatewayRunning && res?.restartGateway && restartGateway) {
          await restartGateway();
        }
      } catch (err) {
        console.error("Failed to update Claude Code 3P settings:", err);
      }
    },
    [refreshConfig, gatewayRunning, restartGateway]
  );

  const activeProviderDisplayName =
    config?.providers[config.active_provider ?? ""]?.display_name ??
    config?.active_provider ??
    "DeepSeek";

  return (
    <div style={{ padding: "10px 12px", background: "var(--bg-secondary, #f9fafb)", borderRadius: 6, border: "1px solid var(--border, #e5e7eb)" }}>
      {showHeader && (
        <div style={{ fontSize: 12, fontWeight: 700, marginBottom: 8, color: "var(--text-primary, #111827)" }}>
          {t("claudeConfig.thirdPartySectionTitle")}
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 8 }}>
        <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, cursor: "pointer" }}>
          <input
            type="radio"
            name={`claude_code_3p_mode_${showHeader ? "main" : "row"}`}
            checked={!tpEnabled}
            onChange={() => {
              setTpEnabled(false);
              saveThirdPartySettings(false, tpModel, tpThinking, tpVision, tpContextWindow);
            }}
          />
          <span>{t("claudeConfig.useDefaultProvider")} ({activeProviderDisplayName})</span>
        </label>
        <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, cursor: "pointer" }}>
          <input
            type="radio"
            name={`claude_code_3p_mode_${showHeader ? "main" : "row"}`}
            checked={tpEnabled}
            onChange={() => {
              setTpEnabled(true);
              saveThirdPartySettings(true, tpModel, tpThinking, tpVision, tpContextWindow);
            }}
          />
          <span style={{ fontWeight: 600 }}>{t("claudeConfig.useOllamaLocal")}</span>
        </label>
      </div>

      {tpEnabled && (
        <div style={{ marginTop: 8, paddingLeft: 18, display: "flex", flexDirection: "column", gap: 8 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 11, width: 80, color: "var(--text-secondary, #4b5563)", fontWeight: 600 }}>
              {t("claudeConfig.ollamaEndpoint")}:
            </span>
            <code style={{ fontSize: 11, background: "var(--bg-input, #fff)", padding: "2px 6px", borderRadius: 4, border: "1px solid var(--border, #d1d5db)" }}>
              http://127.0.0.1:11434
            </code>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 11, width: 80, color: "var(--text-secondary, #4b5563)", fontWeight: 600 }}>
              {t("claudeConfig.ollamaModel")}:
            </span>
            <input
              type="text"
              className="input-text"
              style={{ fontSize: 11, padding: "2px 6px", width: 220 }}
              value={tpModel}
              onChange={(e) => {
                const val = e.target.value;
                setTpModel(val);
              }}
              onBlur={() => {
                saveThirdPartySettings(tpEnabled, tpModel, tpThinking, tpVision, tpContextWindow);
              }}
              placeholder="mimo-v2.6-distill-qwen-9b"
            />
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 11, width: 80, color: "var(--text-secondary, #4b5563)", fontWeight: 600 }}>
              {t("claudeConfig.ollamaThinking")}:
            </span>
            <select
              className="input-select"
              style={{ fontSize: 11, padding: "2px 6px", width: 140 }}
              value={tpThinking}
              onChange={(e) => {
                const val = e.target.value as "normal" | "thinking";
                setTpThinking(val);
                saveThirdPartySettings(tpEnabled, tpModel, val, tpVision, tpContextWindow);
              }}
            >
              <option value="normal">{t("claudeConfig.thinkingNormal")}</option>
              <option value="thinking">{t("claudeConfig.thinkingEnabled")}</option>
            </select>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 11, width: 80, color: "var(--text-secondary, #4b5563)", fontWeight: 600 }}>
              {t("claudeConfig.ollamaVision")}:
            </span>
            <select
              className="input-select"
              style={{ fontSize: 11, padding: "2px 6px", width: 100 }}
              value={tpVision ? "true" : "false"}
              onChange={(e) => {
                const val = e.target.value === "true";
                setTpVision(val);
                saveThirdPartySettings(tpEnabled, tpModel, tpThinking, val, tpContextWindow);
              }}
            >
              <option value="false">{t("claudeConfig.visionOff")}</option>
              <option value="true">{t("claudeConfig.visionOn")}</option>
            </select>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 11, width: 80, color: "var(--text-secondary, #4b5563)", fontWeight: 600 }}>
              {t("claudeConfig.ollamaContextWindow")}:
            </span>
            <input
              type="number"
              min={1}
              max={10000000}
              className="input-text"
              style={{ fontSize: 11, padding: "2px 6px", width: 160 }}
              value={tpContextWindow}
              onChange={(e) => {
                setTpContextWindow(e.target.value);
              }}
              onBlur={() => {
                saveThirdPartySettings(tpEnabled, tpModel, tpThinking, tpVision, tpContextWindow);
              }}
              placeholder={t("claudeConfig.ollamaContextWindowPlaceholder")}
            />
          </div>

          <div style={{ fontSize: 11, color: "var(--text-muted, #6b7280)", fontStyle: "italic", marginTop: 2 }}>
            ℹ {t("claudeConfig.ollamaApiKeyNotice")}
          </div>
        </div>
      )}
    </div>
  );
}

export function ClaudeConfigPanelContent({
  config,
  refreshConfig,
  gatewayRunning,
  restartGateway,
}: {
  config?: GatewayConfig | null;
  refreshConfig?: () => Promise<void>;
  gatewayRunning?: boolean;
  restartGateway?: () => Promise<void>;
} = {}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [headerHovered, setHeaderHovered] = useState(false);
  const [copied, setCopied] = useState(false);
  const [jsonCopied, setJsonCopied] = useState(false);
  const [launchCommandCopied, setLaunchCommandCopied] = useState(false);
  const [foundConfigs, setFoundConfigs] = useState<ClaudeConfigCandidate[] | null>(null);
  const [searching, setSearching] = useState(true);
  const [showJson, setShowJson] = useState(false);
  const [showBrowse, setShowBrowse] = useState(false);

  const handleToggle = useCallback(() => setExpanded((prev) => !prev), []);
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); handleToggle(); }
  }, [handleToggle]);

  useEffect(() => {
    invoke<ClaudeConfigCandidate[]>("find_claude_configs")
      .then((results) => { setFoundConfigs(results); setSearching(false); })
      .catch((e) => { console.error(e); setSearching(false); });
  }, []);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(CLAUDE_JSON).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, []);

  const handleJsonCopy = useCallback(() => {
    navigator.clipboard.writeText(CLAUDE_JSON).then(() => {
      setJsonCopied(true);
      setTimeout(() => setJsonCopied(false), 2000);
    });
  }, []);

  const handleLaunchCommandCopy = useCallback(() => {
    invoke<ClaudeCodeLaunchCommand>("build_claude_code_launch_command")
      .then((result) =>
        navigator.clipboard.writeText(result.command).then(() => {
          setLaunchCommandCopied(true);
          setTimeout(() => setLaunchCommandCopied(false), 2000);
        })
      )
      .catch((e) => console.error("build_claude_code_launch_command failed", e));
  }, []);

  const configCandidates = foundConfigs?.filter((f) => f.likely_config) ?? [];
  const hasConfigs = configCandidates.length > 0;

  const handleOpenFolder = (cfg: ClaudeConfigCandidate) => {
    const lastSep = Math.max(cfg.path.lastIndexOf("\\"), cfg.path.lastIndexOf("/"));
    const dir = lastSep >= 0 ? cfg.path.substring(0, lastSep) : cfg.path;
    invoke("open_path", { path: dir }).catch(console.error);
  };

  return (
    <div id="claude-desktop-config-panel" className="settings-tile">
      <div
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        onMouseEnter={() => setHeaderHovered(true)}
        onMouseLeave={() => setHeaderHovered(false)}
        style={{
          display: "flex",
          alignItems: "center",
          cursor: "pointer",
          userSelect: "none",
          gap: 8,
          padding: "4px 0",
        }}
      >
        <span style={{ fontSize: 10, width: 14, display: "inline-block", flexShrink: 0, color: headerHovered ? "var(--accent)" : "#6b7280", userSelect: "none" }}>{expanded ? "▼" : "▶"}</span>
        <h3 style={{ margin: 0, fontSize: 14, fontWeight: 700, color: headerHovered ? "var(--accent)" : "var(--text-primary)" }}>{t("claudeConfig.header")}</h3>
        <span style={{ flex: 1 }} />
      </div>

      {expanded && (
        <>
      <p className="tile-desc">{t("claudeConfig.dashboardNote")}</p>

      {/* Claude Code 3P Provider Configuration */}
      <div style={{ marginTop: 12 }}>
        <ClaudeCodeThirdPartySettings
          config={config}
          refreshConfig={refreshConfig}
          gatewayRunning={gatewayRunning}
          restartGateway={restartGateway}
          showHeader={true}
        />
      </div>

      {/* Detected config files */}
      <div style={{ marginTop: 12 }}>
        <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 4, color: "#374151" }}>
          {t("claudeConfig.discoveryTitle")}
        </div>
        {searching ? (
          <div className="loading" />
        ) : hasConfigs ? (
          configCandidates.map((cfg, i) => (
            <div key={i} className="tile-path" style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
              <span style={{ fontFamily: "var(--font-mono)", fontSize: 11, flex: 1, wordBreak: "break-all" }}>
                ✓ {cfg.path}
              </span>
              <div style={{ display: "flex", gap: 4, flexShrink: 0 }}>
                <button className="btn btn-small" onClick={() => invoke("open_path", { path: cfg.path }).catch(console.error)}>
                  {t("claudeConfig.openFile")}
                </button>
                <button className="btn btn-small" onClick={() => handleOpenFolder(cfg)}>
                  {t("claudeConfig.openFolder")}
                </button>
              </div>
            </div>
          ))
        ) : (
          <p className="empty-state" style={{ fontSize: 11 }}>{t("claudeConfig.noFilesFound")}</p>
        )}
      </div>

      {/* Browse manually */}
      <div style={{ marginTop: 8 }}>
        <button
          className="btn btn-small"
          onClick={() => setShowBrowse(!showBrowse)}
          style={{ fontSize: 11 }}
        >
          {showBrowse ? "▾" : "▸"} {t("claudeConfig.browseManually")}
        </button>
      </div>

      {/* Action buttons */}
      <div className="tile-actions" style={{ marginTop: 10 }}>
        <button className="btn btn-success btn-small" onClick={handleCopy}>
          {copied ? t("claudeConfig.copied") : t("claudeConfig.copy")}
        </button>
        <button className="btn btn-small" onClick={handleLaunchCommandCopy}>
          {launchCommandCopied ? t("claudeConfig.copied") : t("claudeConfig.copyLaunchCommand")}
        </button>
        <button
          className="btn btn-small"
          onClick={() => setShowJson(!showJson)}
        >
          {showJson ? t("apiKeyPanel.collapse") : t("claudeConfig.showJson")}
        </button>
      </div>

      {/* JSON preview */}
      {showJson && (
        <div style={{ marginTop: 10 }}>
          <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 4, color: "#374151" }}>
            {t("claudeConfig.jsonHeading")}
          </div>
          <pre className="json-block" style={{ maxHeight: 200 }}>
            {CLAUDE_JSON}
          </pre>
          <button className="btn btn-success btn-small" onClick={handleJsonCopy} style={{ marginTop: 6 }}>
            {jsonCopied ? t("claudeConfig.copied") : t("claudeConfig.copyFromJson")}
          </button>
        </div>
      )}
        </>
      )}
    </div>
  );
}

export default function ClaudeConfigPanel() {
  return <ClaudeConfigPanelContent />;
}
