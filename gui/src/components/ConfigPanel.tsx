import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRawConfig } from "../hooks/useRawConfig";
import { useTranslation } from "../i18n";
import type { WriteConfigResponse } from "../types";

type Encoding = "UTF-8" | "Shift-JIS";

// Module-level: survives component remounts (tab switches)
let lastSavedEncoding: Encoding | null = null;

export function ConfigPanelContent() {
  const { t } = useTranslation();
  const { data, error, loading, refresh } = useRawConfig();

  // Local editing state
  const [text, setText] = useState("");
  const [selectedEncoding, setSelectedEncoding] = useState<Encoding>(lastSavedEncoding ?? "UTF-8");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // currentEncoding: actual encoding on disk.
  // Priority: lastSavedEncoding (user's most recent save) > data.encoding_used (backend detection)
  // Fallback to "UTF-8" only when neither is available (initial load before data arrives).
  const currentEncoding: Encoding = lastSavedEncoding ?? (data?.encoding_used as Encoding) ?? "UTF-8";

  // Sync from server data to local state
  useEffect(() => {
    if (data) {
      setText(data.content);
      // Only auto-set selectedEncoding from backend detection if user hasn't explicitly saved
      if (!lastSavedEncoding) {
        setSelectedEncoding(data.encoding_used as Encoding);
      }
    }
  }, [data]);

  // Show warning only when selected encoding truly differs from the actual file encoding
  const encodingWillChange = selectedEncoding !== currentEncoding;

  const handleSave = useCallback(() => {
    setSaving(true);
    setSaveError(null);
    setSaved(false);
    invoke<WriteConfigResponse>("write_config", { content: text, encoding: selectedEncoding })
      .then((resp) => {
        setSaving(false);
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
        // Persist the encoding actually used for saving
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
    lastSavedEncoding = null;        // forget last save — trust backend re-detection
    refresh();
  }, [refresh]);

  const toolbar = (
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
        <span className="encoding-recommend">
          {t("configPanel.recommended")}
        </span>
      </div>
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        {saved && <span className="saved-toast">{t("configPanel.saved")}</span>}
        {saveError && <span className="error-text">{saveError}</span>}
        <button className="btn btn-small" onClick={handleReload}>
          {t("configPanel.reload")}
        </button>
        <button
          className="btn btn-primary btn-small"
          onClick={handleSave}
          disabled={saving}
        >
          {saving ? "..." : t("configPanel.save")}
        </button>
      </div>
    </div>
  );

  if (loading) {
    return <div className="loading" />;
  }

  if (error) {
    return <div className="error-text">{error}</div>;
  }

  return (
    <>
      {toolbar}
      <div className="advanced-warning">
        {t("configPanel.advancedWarning")}
      </div>
      {data?.config_path && (
        <div className="config-path-label">
          {data.config_path}
        </div>
      )}
      <textarea
        className="config-textarea"
        value={text}
        onChange={(e) => setText(e.target.value)}
        spellCheck={false}
      />
    </>
  );
}

// Legacy default export
export default function ConfigPanel() {
  return <ConfigPanelContent />;
}
