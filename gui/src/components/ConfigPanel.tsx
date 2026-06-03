import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useRawConfig } from "../hooks/useRawConfig";
import { useTranslation } from "../i18n";

type Encoding = "UTF-8" | "Shift-JIS";

export function ConfigPanelContent() {
  const { t } = useTranslation();
  const { data, error, loading, refresh } = useRawConfig();

  // Local editing state
  const [text, setText] = useState("");
  const [encoding, setEncoding] = useState<Encoding>("UTF-8");
  const [savedEncoding, setSavedEncoding] = useState<Encoding | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // Sync from server data to local state
  useEffect(() => {
    if (data) {
      setText(data.content);
      setEncoding(data.encoding_used as Encoding);
    }
  }, [data]);

  const detectedEncoding = savedEncoding ?? (data?.encoding_used as Encoding ?? "UTF-8");
  const encodingWillChange = encoding !== detectedEncoding;

  const handleSave = useCallback(() => {
    setSaving(true);
    setSaveError(null);
    setSaved(false);
    invoke("write_config", { content: text, encoding })
      .then(() => {
        setSaving(false);
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
        setSavedEncoding(encoding);
        // Don't refresh — encoding re-detection misidentifies ASCII-only Shift-JIS as UTF-8
      })
      .catch((e: unknown) => {
        setSaving(false);
        setSaveError(String(e));
      });
  }, [text, encoding]);

  const handleReload = useCallback(() => {
    setSaveError(null);
    setSaved(false);
    setSavedEncoding(null);
    refresh();
  }, [refresh]);

  const toolbar = (
    <div className="config-toolbar">
      <div className="config-encoding-section">
        <div className="encoding-toggle">
          <button
            className={`encoding-option ${encoding === "UTF-8" ? "encoding-active" : ""}`}
            onClick={() => setEncoding("UTF-8")}
          >
            UTF-8
          </button>
          <button
            className={`encoding-option ${encoding === "Shift-JIS" ? "encoding-active" : ""}`}
            onClick={() => setEncoding("Shift-JIS")}
          >
            Shift-JIS
          </button>
        </div>
        <span className="encoding-label">
          {t("configPanel.currentEncoding", { enc: detectedEncoding })}
        </span>
        {encodingWillChange && (
          <span className="encoding-warning">
            {t("configPanel.willChangeEncoding", { enc: encoding })}
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
