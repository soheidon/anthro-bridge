import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import {
  TIMEZONE_OPTIONS,
  getLocalTimezone,
} from "../config/deepseekSchedule";
import { getTimezoneOffsetMinutes, formatUtcOffset } from "../utils/timezone";

// Group options by their "group" field
function groupOptions(options: typeof TIMEZONE_OPTIONS) {
  const groups: Record<string, typeof TIMEZONE_OPTIONS> = {};
  const order: string[] = [];
  for (const opt of options) {
    if (!groups[opt.group]) {
      groups[opt.group] = [];
      order.push(opt.group);
    }
    groups[opt.group].push(opt);
  }
  return { groups, order };
}

export default function TimezoneSettingPanel() {
  const { t } = useTranslation();
  const [tzId, setTzId] = useState<string>(getLocalTimezone);
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    invoke<string | null>("get_pricing_display_timezone")
      .then((saved) => { if (saved) setTzId(saved); })
      .catch(() => {});
  }, []);

  // Update now every minute for DST-aware offset display
  useEffect(() => {
    const intervalId = window.setInterval(() => {
      setNow(new Date());
    }, 60_000);
    return () => { window.clearInterval(intervalId); };
  }, []);

  const handleChange = useCallback((newTz: string) => {
    setTzId(newTz);
    invoke("set_pricing_display_timezone", { timezoneId: newTz }).catch(() => {});
  }, []);

  const { groups, order } = groupOptions(TIMEZONE_OPTIONS);

  return (
    <div className="settings-tile">
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <span style={{ fontSize: 13, fontWeight: 700, color: "var(--text-primary)" }}>
          {t("peakValley.pricingDisplayTimezone")}
        </span>
        <select
          style={{
            fontSize: 12,
            fontFamily: "var(--font-mono)",
            border: "1px solid #d0d7de",
            borderRadius: 4,
            padding: "4px 8px",
            background: "#fff",
            color: "#1f2937",
            outline: "none",
            maxWidth: 500,
          }}
          value={tzId}
          onChange={(e) => handleChange(e.target.value)}
        >
          {order.map((group) => (
            <optgroup key={group} label={group}>
              {groups[group].map((opt) => {
                const offset = formatUtcOffset(getTimezoneOffsetMinutes(now, opt.id));
                return (
                  <option key={opt.id} value={opt.id}>
                    {opt.label}  {offset}
                  </option>
                );
              })}
            </optgroup>
          ))}
        </select>
        <span style={{ fontSize: 11, color: "#6b7280" }}>
          {t("peakValley.pricingDisplayTimezoneHint")}
        </span>
      </div>
    </div>
  );
}
