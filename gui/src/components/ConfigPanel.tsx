import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRawConfig } from "../hooks/useRawConfig";
import { useTranslation } from "../i18n";
import type { WriteConfigResponse, GatewayConfig } from "../types";

type Encoding = "UTF-8" | "Shift-JIS";

// Module-level: survives component remounts
let lastSavedEncoding: Encoding | null = null;

export function ConfigPanelContent() {
  const { t } = useTranslation();
  const { data: rawData, error: rawError, loading: rawLoading, refresh: rawRefresh } = useRawConfig();

  // ── Server config state (from structured read_config) ──
  const [serverHost, setServerHost] = useState("127.0.0.1");
  const [serverPort, setServerPort] = useState(4000);
  const [serverCors, setServerCors] = useState(true);
  const [serverSaving, setServerSaving] = useState(false);
  const [serverSaved, setServerSaved] = useState(false);

  // ── Raw editor state ──
  const [text, setText] = useState("");
  const [selectedEncoding, setSelectedEncoding] = useState<Encoding>(lastSavedEncoding ?? "UTF-8");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [advancedExpanded, setAdvancedExpanded] = useState(false);
  const [sectionCollapsed, setSectionCollapsed] = useState(true);

  // ── Utilities state ──
  const [backupMsg, setBackupMsg] = useState<string | null>(null);
  const [utilityMsg, setUtilityMsg] = useState<string | null>(null);

  // ── Import state ──
  const [importExpanded, setImportExpanded] = useState(false);
  const [importText, setImportText] = useState("");
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [importErr, setImportErr] = useState<string | null>(null);

  const currentEncoding: Encoding = lastSavedEncoding ?? (rawData?.encoding_used as Encoding) ?? "UTF-8";

  // Load server config from structured read
  useEffect(() => {
    invoke<GatewayConfig>("read_config")
      .then((cfg) => {
        setServerHost(cfg.server.host);
        setServerPort(cfg.server.port);
        setServerCors(cfg.server.enable_cors);
      })
      .catch(() => {});
  }, []);

  // Load raw text when raw data arrives
  useEffect(() => {
    if (rawData) {
      setText(rawData.content);
      if (!lastSavedEncoding) {
        setSelectedEncoding(rawData.encoding_used as Encoding);
      }
    }
  }, [rawData]);

  const encodingWillChange = selectedEncoding !== currentEncoding;

  // ── Server config save ──
  const handleServerSave = useCallback(() => {
    setServerSaving(true);
    setServerSaved(false);
    invoke("update_server_config", { host: serverHost, port: serverPort, enableCors: serverCors })
      .then(() => {
        setServerSaving(false);
        setServerSaved(true);
        setTimeout(() => setServerSaved(false), 2000);
      })
      .catch((e: unknown) => {
        setServerSaving(false);
        console.error(e);
      });
  }, [serverHost, serverPort, serverCors]);

  // ── Raw editor save ──
  const handleSave = useCallback(() => {
    setSaving(true);
    setSaveError(null);
    setSaved(false);
    invoke<WriteConfigResponse>("write_config", { content: text, encoding: selectedEncoding })
      .then((resp) => {
        setSaving(false);
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
        lastSavedEncoding = resp.saved_encoding as Encoding;
        setSelectedEncoding(resp.saved_encoding as Encoding);
      })
      .catch((e: unknown) => {
        setSaving(false);
        setSaveError(String(e));
      });
  }, [text, selectedEncoding]);

  const handleReload = useCallback(() => {
    setSaveError(null);
    setSaved(false);
    lastSavedEncoding = null;
    rawRefresh();
  }, [rawRefresh]);

  // ── Utilities ──
  const configDir = rawData?.config_path ? rawData.config_path.replace(/[/\\][^/\\]*$/, "") : "";

  const openConfigFolder = () => {
    if (configDir) {
      invoke("open_path", { path: configDir }).catch(console.error);
    }
  };

  const openConfigFile = () => {
    if (rawData?.config_path) {
      invoke("open_path", { path: rawData.config_path }).catch(console.error);
    }
  };

  const handleBackup = () => {
    invoke<string>("backup_config")
      .then((name) => {
        setBackupMsg(name);
        setTimeout(() => setBackupMsg(null), 3000);
      })
      .catch((e: unknown) => {
        setUtilityMsg(String(e));
        setTimeout(() => setUtilityMsg(null), 3000);
      });
  };

  const handleRestore = () => {
    if (!window.confirm(t("configPanel.restoreConfirm"))) return;
    invoke("restore_config_from_backup")
      .then(() => {
        setUtilityMsg(t("configPanel.restoreDone"));
        setTimeout(() => setUtilityMsg(null), 3000);
        handleReload();
      })
      .catch((e: unknown) => {
        setUtilityMsg(String(e));
        setTimeout(() => setUtilityMsg(null), 3000);
      });
  };

  const handleReset = () => {
    if (!window.confirm(t("configPanel.resetConfirm"))) return;
    invoke("reset_config")
      .then(() => {
        setUtilityMsg(t("configPanel.resetDone"));
        setTimeout(() => setUtilityMsg(null), 3000);
        handleReload();
      })
      .catch((e: unknown) => {
        setUtilityMsg(String(e));
        setTimeout(() => setUtilityMsg(null), 3000);
      });
  };

  // ── Import ──
  const handleImport = () => {
    const trimmed = importText.trim();
    if (!trimmed) return;

    // Validate JSON
    let parsed: unknown;
    try {
      parsed = JSON.parse(trimmed);
    } catch {
      setImportErr(t("configPanel.importInvalidJson"));
      return;
    }
    setImportErr(null);

    // Write via write_config (which handles .bak + atomic write)
    invoke<WriteConfigResponse>("write_config", { content: trimmed, encoding: "UTF-8" })
      .then(() => {
        setImportMsg(t("configPanel.importDone"));
        setImportText("");
        setTimeout(() => setImportMsg(null), 3000);
        handleReload();
      })
      .catch((e: unknown) => {
        setImportErr(String(e));
      });
  };

  return (
    <div className="settings-tile">
      <div
        className="collapse-header config-collapse-main"
        onClick={() => setSectionCollapsed(!sectionCollapsed)}
      >
        <span className="collapse-chevron">{sectionCollapsed ? "▶" : "▼"}</span>
        <span>{t("configPanel.header")}</span>
      </div>
      <p className="tile-desc">{sectionCollapsed ? t("configPanel.collapsedDesc") : t("configPanel.desc")}</p>

      {!sectionCollapsed && (
        <>
          <p className="config-restart-note">{t("configPanel.gatewayRestartNote")}</p>
          {rawData?.config_path && (
            <div className="tile-path">
              {t("configPanel.configPath")}{" "}
              <span style={{ fontFamily: "var(--font-mono)", fontSize: 11 }}>{rawData.config_path}</span>
            </div>
          )}

      {/* ═══ Section A: Server Config ═══ */}
      <div className="config-section">
        <h4>{t("configPanel.serverConfig")}</h4>
        <div className="config-server-row">
          <label className="config-server-label">
            {t("configPanel.host")}
            <input
              className="config-server-input"
              value={serverHost}
              onChange={(e) => setServerHost(e.target.value)}
              placeholder="127.0.0.1"
            />
          </label>
          <label className="config-server-label">
            {t("configPanel.port")}
            <input
              className="config-server-input config-server-input-short"
              type="number"
              value={serverPort}
              onChange={(e) => setServerPort(Number(e.target.value))}
              min={1}
              max={65535}
            />
          </label>
          <label className="config-server-check">
            <input
              type="checkbox"
              checked={serverCors}
              onChange={(e) => setServerCors(e.target.checked)}
            />
            {" "}{t("configPanel.cors")}
          </label>
          <button
            className="btn btn-primary btn-small"
            onClick={handleServerSave}
            disabled={serverSaving}
          >
            {serverSaving ? "..." : t("configPanel.save")}
          </button>
          {serverSaved && <span className="saved-toast">{t("configPanel.serverSaved")}</span>}
        </div>
      </div>

      {/* ═══ Section B: Utilities ═══ */}
      <div className="config-section">
        <h4>{t("configPanel.utilities")}</h4>
        <div className="config-util-row">
          <button className="btn btn-small" onClick={openConfigFile}>
            {t("claudeConfig.openFile")}
          </button>
          <button className="btn btn-small" onClick={openConfigFolder}>
            {t("claudeConfig.openFolder")}
          </button>
          <button className="btn btn-small" onClick={handleBackup}>
            {t("configPanel.backup")}
          </button>
          <button className="btn btn-small" onClick={handleRestore}>
            {t("configPanel.restore")}
          </button>
          <button className="btn btn-small btn-danger" onClick={handleReset}>
            {t("configPanel.reset")}
          </button>
        </div>
        {backupMsg && <span className="saved-toast">{t("configPanel.backupDone", { name: backupMsg })}</span>}
        {utilityMsg && <span className={utilityMsg.includes("Restored") || utilityMsg.includes("Reset") ? "saved-toast" : "error-text"}>{utilityMsg}</span>}
      </div>

      {/* ═══ Section C: Import ═══ */}
      <div className="config-section">
        <button
          className="btn btn-small"
          onClick={() => setImportExpanded(!importExpanded)}
        >
          {importExpanded ? t("apiKeyPanel.collapse") : t("configPanel.import")}
        </button>
        {importExpanded && (
          <div style={{ marginTop: 8 }}>
            <p style={{ fontSize: 11, color: "#6b7280", margin: "0 0 4px" }}>
              {t("configPanel.importLabel")}
            </p>
            <textarea
              className="config-textarea"
              style={{ height: 160 }}
              value={importText}
              onChange={(e) => { setImportText(e.target.value); setImportErr(null); }}
              placeholder='Paste valid config.json content here...'
              spellCheck={false}
            />
            <div style={{ marginTop: 6, display: "flex", alignItems: "center", gap: 8 }}>
              <button
                className="btn btn-primary btn-small"
                onClick={handleImport}
                disabled={!importText.trim()}
              >
                {t("configPanel.importValidate")}
              </button>
              {importMsg && <span className="saved-toast">{importMsg}</span>}
              {importErr && <span className="error-text">{importErr}</span>}
            </div>
          </div>
        )}
      </div>

      {/* ═══ Section D: Advanced JSON Editor ═══ */}
      <div className="config-section">
        <button
          className="btn btn-small"
          onClick={() => setAdvancedExpanded(!advancedExpanded)}
        >
          {advancedExpanded ? t("apiKeyPanel.collapse") : t("configPanel.advancedToggle")}
        </button>
        {advancedExpanded && (
          <div style={{ marginTop: 8 }}>
            <p className="config-advanced-warning">{t("configPanel.advancedWarning")}</p>
            {rawLoading ? (
              <div className="loading" />
            ) : rawError ? (
              <div className="error-text">{rawError}</div>
            ) : (
              <>
                <div className="config-toolbar">
                  <div className="config-encoding-section">
                    <div className="encoding-toggle">
                      <button
                        className={`encoding-option ${selectedEncoding === "UTF-8" ? "encoding-active" : ""}`}
                        onClick={() => setSelectedEncoding("UTF-8")}
                      >
                        UTF-8
                      </button>
                      <button
                        className={`encoding-option ${selectedEncoding === "Shift-JIS" ? "encoding-active" : ""}`}
                        onClick={() => setSelectedEncoding("Shift-JIS")}
                      >
                        Shift-JIS
                      </button>
                    </div>
                    <span className="encoding-label">
                      {t("configPanel.currentEncoding", { enc: currentEncoding })}
                    </span>
                    {encodingWillChange && (
                      <span className="encoding-warning">
                        {t("configPanel.willChangeEncoding", { enc: selectedEncoding })}
                      </span>
                    )}
                  </div>
                  <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    {saved && <span className="saved-toast">{t("configPanel.saved")}</span>}
                    {saveError && <span className="error-text">{saveError}</span>}
                    <button className="btn btn-small" onClick={handleReload}>
                      {t("configPanel.reload")}
                    </button>
                    <button className="btn btn-primary btn-small" onClick={handleSave} disabled={saving}>
                      {saving ? "..." : t("configPanel.save")}
                    </button>
                  </div>
                </div>
                <textarea
                  className="config-textarea"
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                  spellCheck={false}
                />
              </>
            )}
          </div>
        )}
      </div>
        </>
      )}
    </div>
  );
}

export default function ConfigPanel() {
  return <ConfigPanelContent />;
}
