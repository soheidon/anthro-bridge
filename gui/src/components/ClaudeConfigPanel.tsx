import { useState, useCallback, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ClaudeConfigCandidate } from "../types";

// ---------------------------------------------------------------------------
// Model definitions for the selection UI
// ---------------------------------------------------------------------------
interface ModelOption {
  id: string;          // gateway model name (e.g. "claude-deepseek-v4")
  labelOverride: string;
  provider: string;    // display name of the provider group
}

interface ProviderGroup {
  name: string;
  models: ModelOption[];
}

const PROVIDER_GROUPS: ProviderGroup[] = [
  {
    name: "DeepSeek",
    models: [
      { id: "claude-deepseek-v4", labelOverride: "DeepSeek V4 Pro via Gateway", provider: "DeepSeek" },
      { id: "claude-deepseek-flash", labelOverride: "DeepSeek V4 Flash via Gateway", provider: "DeepSeek" },
    ],
  },
  {
    name: "MiniMax",
    models: [
      { id: "claude-minimax-m3", labelOverride: "MiniMax M3 via Gateway", provider: "MiniMax" },
      { id: "claude-minimax-m3-thinking", labelOverride: "MiniMax M3 Thinking via Gateway", provider: "MiniMax" },
      { id: "claude-minimax-m2-7-highspeed", labelOverride: "MiniMax M2.7 Highspeed via Gateway", provider: "MiniMax" },
    ],
  },
  {
    name: "Kimi / Moonshot",
    models: [
      { id: "claude-kimi-k2-6", labelOverride: "Kimi K2.6 via Gateway", provider: "Kimi / Moonshot" },
      { id: "claude-kimi-k2-6-thinking", labelOverride: "Kimi K2.6 Thinking via Gateway", provider: "Kimi / Moonshot" },
    ],
  },
];

/** All available model IDs (flat list) */
const ALL_MODEL_IDS = PROVIDER_GROUPS.flatMap((g) => g.models.map((m) => m.id));

// ---------------------------------------------------------------------------
// Generate Claude Desktop config JSON from selected model IDs
// ---------------------------------------------------------------------------
function generateConfig(selectedIds: string[]): object {
  const models = PROVIDER_GROUPS.flatMap((g) =>
    g.models
      .filter((m) => selectedIds.includes(m.id))
      .map((m) => ({ name: m.id, labelOverride: m.labelOverride }))
  );
  return {
    inferenceProvider: "gateway",
    inferenceGatewayBaseUrl: "http://127.0.0.1:4000",
    inferenceGatewayApiKey: "sk-local-gateway",
    inferenceGatewayAuthScheme: "bearer",
    inferenceModels: models,
  };
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------
export function ClaudeConfigPanelContent() {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [foundConfigs, setFoundConfigs] = useState<ClaudeConfigCandidate[] | null>(null);
  const [searching, setSearching] = useState(true);
  const [showManual, setShowManual] = useState(false);

  // Model selection state: map of modelId → checked
  const [selected, setSelected] = useState<Record<string, boolean>>(() => {
    const init: Record<string, boolean> = {};
    for (const id of ALL_MODEL_IDS) init[id] = true;
    // Default MiniMax M2.7 Highspeed to unchecked (less commonly used)
    init["claude-minimax-m2-7-highspeed"] = false;
    return init;
  });

  // Search for Claude config files on mount
  useEffect(() => {
    invoke<ClaudeConfigCandidate[]>("find_claude_configs")
      .then((results) => { setFoundConfigs(results); setSearching(false); })
      .catch((e) => { console.error(e); setSearching(false); });
  }, []);

  // Derived: selected IDs and generated JSON
  const selectedIds = useMemo(
    () => ALL_MODEL_IDS.filter((id) => selected[id]),
    [selected]
  );
  const configJson = useMemo(
    () => JSON.stringify(generateConfig(selectedIds), null, 2),
    [selectedIds]
  );

  // Toggle individual model
  const toggleModel = useCallback((id: string) => {
    setSelected((prev) => ({ ...prev, [id]: !prev[id] }));
  }, []);

  // Toggle all models in a provider group
  const toggleProvider = useCallback((group: ProviderGroup) => {
    setSelected((prev) => {
      const allChecked = group.models.every((m) => prev[m.id]);
      const next = { ...prev };
      for (const m of group.models) next[m.id] = !allChecked;
      return next;
    });
  }, []);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(configJson).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [configJson]);

  const openAppDataClaude = () => {
    invoke("open_path", { path: "%APPDATA%\\Claude" }).catch(console.error);
  };

  const openUserProfileClaude = () => {
    invoke("open_path", { path: "%USERPROFILE%\\.claude" }).catch(console.error);
  };

  const openLocalAppDataClaude3p = () => {
    invoke("open_path", { path: "%LOCALAPPDATA%\\Claude-3p\\configLibrary" }).catch(console.error);
  };

  const hasConfigs = foundConfigs && foundConfigs.filter((f) => f.likely_config).length > 0;

  // Per-group checked state for the provider-level checkbox
  function groupChecked(group: ProviderGroup): boolean | "partial" {
    const states = group.models.map((m) => selected[m.id]);
    if (states.every(Boolean)) return true;
    if (states.some(Boolean)) return "partial";
    return false;
  }

  return (
    <>
      {/* ---- Discovery results ---- */}
      <div className="claude-config-discovery">
        <h4 className="discovery-title">{t("claudeConfig.discoveryTitle")}</h4>
        {searching ? (
          <div className="loading" />
        ) : hasConfigs ? (
          <ul className="discovery-list">
            {foundConfigs!.filter((f) => f.likely_config).map((f) => (
              <li key={f.path} className="discovery-item">
                <span className="discovery-likely" title={t("claudeConfig.likelyConfig")}>
                  ✓
                </span>
                <code className="discovery-path">{f.path}</code>
                <button
                  className="btn btn-small"
                  onClick={() => invoke("open_path", { path: f.path }).catch(console.error)}
                >
                  {t("claudeConfig.openFile")}
                </button>
                <button
                  className="btn btn-small"
                  onClick={() => {
                    const lastSep = Math.max(f.path.lastIndexOf("\\"), f.path.lastIndexOf("/"));
                    const dir = lastSep >= 0 ? f.path.substring(0, lastSep) : f.path;
                    invoke("open_path", { path: dir }).catch(console.error);
                  }}
                >
                  {t("claudeConfig.openFolder")}
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="empty-state">{t("claudeConfig.noFilesFound")}</p>
        )}
      </div>

      {/* Manual browse — collapsible, default closed */}
      <div>
        <button
          className="collapse-header"
          onClick={() => setShowManual(!showManual)}
        >
          <span>{showManual ? "▼" : "▶"}</span>
          {t("claudeConfig.browseManually")}
        </button>
        {showManual && (
          <div style={{ paddingLeft: 16 }}>
            <div className="claude-config-path-row">
              <code>%APPDATA%\Claude\claude_desktop_config.json</code>
              <button className="btn btn-small" onClick={openAppDataClaude}>
                {t("claudeConfig.openFolder")}
              </button>
            </div>
            <div className="claude-config-path-row">
              <code>%USERPROFILE%\.claude\settings.json</code>
              <button className="btn btn-small" onClick={openUserProfileClaude}>
                {t("claudeConfig.openFolder")}
              </button>
            </div>
            <div className="claude-config-path-row">
              <code>%LOCALAPPDATA%\Claude-3p\configLibrary\</code>
              <button className="btn btn-small" onClick={openLocalAppDataClaude3p}>
                {t("claudeConfig.openFolder")}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ---- Model selection ---- */}
      <div className="claude-model-select">
        <h4 className="discovery-title">{t("claudeConfig.selectModels")}</h4>
        {PROVIDER_GROUPS.map((group) => {
          const gc = groupChecked(group);
          return (
            <div key={group.name} className="claude-provider-group">
              <label className="claude-provider-label">
                <input
                  type="checkbox"
                  checked={gc === true}
                  ref={(el) => {
                    if (el) el.indeterminate = gc === "partial";
                  }}
                  onChange={() => toggleProvider(group)}
                />
                <span>{group.name}</span>
              </label>
              <div className="claude-model-list">
                {group.models.map((m) => (
                  <label key={m.id} className="claude-model-label">
                    <input
                      type="checkbox"
                      checked={selected[m.id]}
                      onChange={() => toggleModel(m.id)}
                    />
                    <span>{m.labelOverride}</span>
                  </label>
                ))}
              </div>
            </div>
          );
        })}
      </div>

      {/* ---- JSON preview + actions ---- */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginTop: 8 }}>
        <span style={{ fontSize: 12, fontWeight: 600, color: "var(--text-secondary)" }}>
          {t("claudeConfig.header")}
          {selectedIds.length === 0 && (
            <span style={{ color: "var(--error)", marginLeft: 8 }}>
              {t("claudeConfig.noModelsSelected")}
            </span>
          )}
        </span>
        <div className="copy-wrapper">
          <button
            className="btn btn-success btn-small"
            onClick={handleCopy}
            disabled={selectedIds.length === 0}
          >
            {copied ? t("claudeConfig.copied") : t("claudeConfig.copy")}
          </button>
          {copied && <span className="copied-toast">{t("claudeConfig.copied")}</span>}
        </div>
      </div>
      <pre className="json-block">{configJson}</pre>
    </>
  );
}

// Legacy default export
export default function ClaudeConfigPanel() {
  return <ClaudeConfigPanelContent />;
}
