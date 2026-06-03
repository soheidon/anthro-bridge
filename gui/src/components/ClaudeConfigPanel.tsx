import { useState, useCallback, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ClaudeConfigCandidate, GatewayConfig } from "../types";

// ---------------------------------------------------------------------------
// Claude Desktop always sees these 2 Anthropic official model names.
// The gateway routes them based on the active provider selected in the GUI.
// ---------------------------------------------------------------------------
const CLAUDE_DESKTOP_MODELS = [
  { name: "claude-sonnet-4-6", labelOverride: "Gateway Pro" },
  { name: "claude-haiku-4-5",  labelOverride: "Gateway Flash" },
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

// ---------------------------------------------------------------------------
// Helper: resolve upstream model for a gateway model on a specific provider
// ---------------------------------------------------------------------------
function getUpstream(
  provider: GatewayConfig["providers"][string] | undefined,
  gatewayModel: string,
): string {
  if (!provider) return "—";
  if (provider.models?.[gatewayModel]) {
    return provider.models[gatewayModel].upstream_model;
  }
  return provider.model_map?.[gatewayModel] ?? "—";
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

  // Gateway config: provider list + active_provider
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [activeProvider, setActiveProvider] = useState<string>("deepseek");
  const [saving, setSaving] = useState(false);

  // Load config + discover files on mount
  useEffect(() => {
    invoke<GatewayConfig>("read_config")
      .then((cfg) => {
        setConfig(cfg);
        setActiveProvider(cfg.active_provider ?? "deepseek");
      })
      .catch(console.error);
    invoke<ClaudeConfigCandidate[]>("find_claude_configs")
      .then((results) => { setFoundConfigs(results); setSearching(false); })
      .catch((e) => { console.error(e); setSearching(false); });
  }, []);

  // Switch active provider
  const handleProviderChange = useCallback((providerId: string) => {
    setActiveProvider(providerId);
    setSaving(true);
    invoke("update_active_provider", { providerId })
      .then(() => setSaving(false))
      .catch((e) => { console.error(e); setSaving(false); });
  }, []);

  // Build routing table rows for the selected provider
  const routingRows = useMemo(() => {
    const provider = config?.providers[activeProvider];
    return CLAUDE_DESKTOP_MODELS.map((m) => ({
      gateway: m.name,
      label: m.labelOverride,
      upstream: getUpstream(provider, m.name),
    }));
  }, [config, activeProvider]);

  // Build provider list for radio buttons
  const providerEntries = useMemo(() => {
    if (!config) return [];
    return Object.entries(config.providers).map(([id, p]) => ({
      id,
      displayName: p.display_name,
    }));
  }, [config]);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(CLAUDE_JSON).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, []);

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
        <button className="collapse-header" onClick={() => setShowManual(!showManual)}>
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

      {/* ---- Provider selector ---- */}
      <div className="claude-model-select">
        <h4 className="discovery-title">{t("claudeConfig.selectProvider")}</h4>
        <p className="claude-provider-hint">{t("claudeConfig.providerHint")}</p>
        <div className="claude-provider-radio-group">
          {providerEntries.map(({ id, displayName }) => (
            <label key={id} className="claude-provider-radio">
              <input
                type="radio"
                name="claude-provider"
                value={id}
                checked={activeProvider === id}
                onChange={() => handleProviderChange(id)}
              />
              <span>{displayName}</span>
            </label>
          ))}
        </div>
        {saving && <span style={{ fontSize: 11, color: "var(--text-muted)", marginLeft: 4 }}>...</span>}

        {/* Routing display */}
        {config && (
          <div style={{ marginTop: 10 }}>
            <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text-secondary)", marginBottom: 4 }}>
              {t("claudeConfig.currentRouting")}
            </div>
            <table className="model-routing-table" style={{ marginTop: 0 }}>
              <thead>
                <tr>
                  <th>{t("claudeConfig.colClaudeModel")}</th>
                  <th>{t("claudeConfig.colLabel")}</th>
                  <th>{t("statusPanel.colUpstream")}</th>
                </tr>
              </thead>
              <tbody>
                {routingRows.map((r) => (
                  <tr key={r.gateway}>
                    <td className="mono">{r.gateway}</td>
                    <td>{r.label}</td>
                    <td className="mono" style={{ color: r.upstream === "—" ? "var(--error)" : "var(--text-muted)" }}>
                      {r.upstream}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* ---- JSON preview + copy ---- */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginTop: 8 }}>
        <span style={{ fontSize: 12, fontWeight: 600, color: "var(--text-secondary)" }}>
          {t("claudeConfig.header")}
        </span>
        <div className="copy-wrapper">
          <button className="btn btn-success btn-small" onClick={handleCopy}>
            {copied ? t("claudeConfig.copied") : t("claudeConfig.copy")}
          </button>
          {copied && <span className="copied-toast">{t("claudeConfig.copied")}</span>}
        </div>
      </div>
      <pre className="json-block">{CLAUDE_JSON}</pre>
    </>
  );
}

// Legacy default export
export default function ClaudeConfigPanel() {
  return <ClaudeConfigPanelContent />;
}
