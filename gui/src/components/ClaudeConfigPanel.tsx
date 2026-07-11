import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ClaudeConfigCandidate } from "../types";

const CLAUDE_DESKTOP_MODELS = [
  { name: "claude-opus-4-8", labelOverride: "Opus 4.8" },
  { name: "claude-sonnet-5", labelOverride: "Sonnet 5" },
  { name: "claude-haiku-4-5",  labelOverride: "Haiku 4.5" },
];

function buildClaudeConfig(): object {
  return {
    inferenceProvider: "gateway",
    inferenceGatewayBaseUrl: "http://127.0.0.1:4000",
    inferenceGatewayApiKey: "sk-local-gateway",
    inferenceGatewayAuthScheme: "bearer",
    inferenceModels: CLAUDE_DESKTOP_MODELS.map((m) => ({
      name: m.name,
      labelOverride: m.labelOverride,
    })),
  };
}

const CLAUDE_JSON = JSON.stringify(buildClaudeConfig(), null, 2);

export function ClaudeConfigPanelContent() {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [jsonCopied, setJsonCopied] = useState(false);
  const [foundConfigs, setFoundConfigs] = useState<ClaudeConfigCandidate[] | null>(null);
  const [searching, setSearching] = useState(true);
  const [showJson, setShowJson] = useState(false);
  const [showBrowse, setShowBrowse] = useState(false);

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

  const configCandidates = foundConfigs?.filter((f) => f.likely_config) ?? [];
  const hasConfigs = configCandidates.length > 0;

  const handleOpenFolder = (cfg: ClaudeConfigCandidate) => {
    const lastSep = Math.max(cfg.path.lastIndexOf("\\"), cfg.path.lastIndexOf("/"));
    const dir = lastSep >= 0 ? cfg.path.substring(0, lastSep) : cfg.path;
    invoke("open_path", { path: dir }).catch(console.error);
  };

  return (
    <div className="settings-tile">
      <h3>{t("claudeConfig.header")}</h3>
      <p className="tile-desc">{t("claudeConfig.dashboardNote")}</p>

      {/* Detected config files */}
      <div style={{ marginTop: 8 }}>
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
    </div>
  );
}

export default function ClaudeConfigPanel() {
  return <ClaudeConfigPanelContent />;
}
