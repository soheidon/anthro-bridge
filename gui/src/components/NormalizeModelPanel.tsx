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
      <h3>{t("configPanel.normalizeModelIdentity")}</h3>
      <p className="tile-desc">{t("configPanel.normalizeModelIdentityDesc")}</p>
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginTop: 8 }}>
        <ToggleSwitch
          checked={normalizeModelIdentity}
          onChange={handleChange}
          label={t("configPanel.normalizeModelIdentity")}
        />
        {saved && <span className="saved-toast">{t("configPanel.serverSaved")}</span>}
      </div>
    </div>
  );
}
