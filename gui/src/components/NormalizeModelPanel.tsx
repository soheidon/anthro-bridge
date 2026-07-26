import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import { ToggleSwitch } from "./ToggleSwitch";
import type { GatewayConfig } from "../types";

export default function NormalizeModelPanel() {
  const { t } = useTranslation();
  const [normalizeModelIdentity, setNormalizeModelIdentity] = useState(true);
  const [saved, setSaved] = useState(false);
  const initialized = useRef(false);

  useEffect(() => {
    invoke<GatewayConfig>("read_config")
      .then((cfg) => {
        if (cfg.normalize_response_model_identity !== undefined) {
          setNormalizeModelIdentity(cfg.normalize_response_model_identity);
        }
        initialized.current = true;
      })
      .catch(() => {});
  }, []);

  const handleChange = useCallback((value: boolean) => {
    setNormalizeModelIdentity(value);
    console.log("saving normalize model identity", { enabled: value });
    invoke("update_normalize_model_identity", { enabled: value })
      .then(() => {
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
      })
      .catch((e: unknown) => {
        console.error(e);
      });
  }, []);

  return (
    <div className="settings-tile">
      <div style={{ display: "flex", alignItems: "baseline", gap: 8, marginBottom: 16 }}>
        <h3 style={{ margin: 0 }}>{t("configPanel.normalizeModelIdentity")}</h3>
        <span style={{ fontSize: 11, color: "var(--text-muted)" }}>
          {t("configPanel.normalizeModelIdentityDesc")}
        </span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <ToggleSwitch
          checked={normalizeModelIdentity}
          onChange={handleChange}
          label=""
        />
        <span style={{
          fontSize: 13,
          fontWeight: 600,
          color: normalizeModelIdentity ? "#107c10" : "#6b7280",
        }}>
          {normalizeModelIdentity ? t("popup.mode.enabled") : t("popup.mode.disabled")}
        </span>
        {saved && <span className="saved-toast">{t("configPanel.serverSaved")}</span>}
      </div>
    </div>
  );
}
