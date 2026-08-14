import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import { MODEL_PRICING, PROVIDER_PRICE_ORDER } from "../config/modelPricing";
import { PROVIDER_MODELS } from "../modelCapabilities";
import {
  getLocalTimezone,
  formatDeepSeekPeakHoursLabel,
} from "../config/deepseekSchedule";

const TH_BASE: React.CSSProperties = {
  padding: "8px 10px",
  fontSize: 13,
  fontWeight: 700,
  color: "#374151",
  textAlign: "left",
  borderBottom: "1px solid #d1d5db",
  whiteSpace: "nowrap",
};

const TH_RIGHT: React.CSSProperties = {
  ...TH_BASE,
  textAlign: "right",
};

const TD_BASE: React.CSSProperties = {
  padding: "8px 10px",
  fontSize: 12,
  color: "#111827",
  borderBottom: "1px solid #e5e7eb",
};

const TD_RIGHT: React.CSSProperties = {
  ...TD_BASE,
  textAlign: "right",
  whiteSpace: "normal",
  fontVariantNumeric: "tabular-nums",
};

export function formatPrice(value: number | null | undefined, decimals: number): string {
  return value == null ? "—" : `$${value.toFixed(decimals)}`;
}

export function PriceCell({
  current,
  regular,
  decimals,
}: {
  current: number | null | undefined;
  regular?: number | null;
  decimals: number;
}) {
  const { t } = useTranslation();
  const currentText = formatPrice(current, decimals);

  if (regular == null) {
    return <span>{currentText}</span>;
  }

  const regularText = formatPrice(regular, decimals);
  const accessibleText = t("modelPricing.discountedPriceAria", {
    current: currentText,
    regular: regularText,
  });

  return (
    <span className="model-pricing-price-stack">
      <span className="sr-only">{accessibleText}</span>
      <span aria-hidden="true"><strong>{currentText}</strong></span>
      <s aria-hidden="true">{regularText}</s>
    </span>
  );
}

const TD_MONO: React.CSSProperties = {
  ...TD_BASE,
  fontFamily: "var(--font-mono)",
  fontSize: 12,
};

const TD_NOTES: React.CSSProperties = {
  ...TD_BASE,
  fontSize: 11,
  color: "#6b7280",
  lineHeight: 1.4,
  whiteSpace: "normal",
};

const EVEN_ROW_BG = "#fafafa";

const DEEPSEEK_PEAK_NOTE_KEY = "modelPricing.notes.deepseekPeakValley";

export default function ModelPricingAccordion() {
  const { t, lang } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [hoveredRow, setHoveredRow] = useState<number | null>(null);
  const [headerHovered, setHeaderHovered] = useState(false);
  const [tzId, setTzId] = useState<string>(getLocalTimezone);
  const [now] = useState(() => new Date());

  useEffect(() => {
    invoke<string | null>("get_pricing_display_timezone")
      .then((saved) => { if (saved) setTzId(saved); })
      .catch(() => {});
  }, []);

  const handleToggle = () => setExpanded((prev) => !prev);
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      handleToggle();
    }
  };

  // Build flat row list: provider × model
  const rows: Array<{
    provider: string;
    displayName: string;
    model: string;
    input: number;
    output: number;
    cached: number | null;
    regularInput?: number;
    regularOutput?: number;
    regularCached?: number;
    noteKey: string | undefined;
    noteKeys?: string[];
  }> = [];

  for (const providerId of PROVIDER_PRICE_ORDER) {
    const models = PROVIDER_MODELS[providerId] ?? [];
    const displayName =
      providerId === "deepseek" ? "DeepSeek" :
      providerId === "mimo" ? "MiMo" :
      providerId === "minimax" ? "MiniMax" :
      providerId === "kimi" ? "Kimi" :
      providerId === "openrouter" ? "OpenRouter" : providerId;

    for (const model of models) {
      const p = MODEL_PRICING[model];
      if (!p) continue;
      rows.push({
        provider: providerId,
        displayName,
        model,
        input: p.inputPerMillionUsd,
        output: p.outputPerMillionUsd,
        cached: p.cachedInputPerMillionUsd ?? null,
        regularInput: p.regularInputPerMillionUsd,
        regularOutput: p.regularOutputPerMillionUsd,
        regularCached: p.regularCachedInputPerMillionUsd,
        noteKey: p.pricingNoteKey,
        noteKeys: p.pricingNoteKeys,
      });
    }
  }

  return (
    <div className="settings-tile">
      <div
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        style={{
          display: "flex",
          alignItems: "center",
          cursor: "pointer",
          userSelect: "none",
          gap: 8,
          padding: "4px 0",
        }}
        onMouseEnter={() => setHeaderHovered(true)}
        onMouseLeave={() => setHeaderHovered(false)}
      >
        <span style={{ fontSize: 10, width: 14, display: "inline-block", flexShrink: 0, color: headerHovered ? "var(--accent)" : "#6b7280", userSelect: "none" }}>{expanded ? "▼" : "▶"}</span>
        <h3 style={{ margin: 0, fontSize: 14, fontWeight: 700, color: headerHovered ? "var(--accent)" : "var(--text-primary)" }}>{t("modelPricing.header")}</h3>
        <span style={{ fontSize: 12, color: "#6b7280" }}>{t("modelPricing.usdLabel")}</span>
        <span style={{ fontSize: 11, color: "#9ca3af" }}>{t("modelPricing.pricingDate")}</span>
        <span style={{ flex: 1 }} />
      </div>

      {expanded && (
        <>
          <div style={{ marginTop: 8 }}>
            <table style={{ width: "100%", borderCollapse: "collapse" }}>
              <thead>
                <tr>
                  <th style={{ ...TH_BASE, width: 90 }}>{t("modelPricing.colProvider")}</th>
                  <th style={{ ...TH_BASE, width: 220 }}>{t("modelPricing.colModel")}</th>
                  <th style={{ ...TH_RIGHT, width: 90 }}>{t("modelPricing.colInput")}</th>
                  <th style={{ ...TH_RIGHT, width: 90 }}>{t("modelPricing.colOutput")}</th>
                  <th style={{ ...TH_RIGHT, width: 100 }}>{t("modelPricing.colCachedInput")}</th>
                  <th style={TH_BASE}>{t("modelPricing.colNotes")}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r, i) => {
                  const isEven = i % 2 === 1;
                  const isHovered = hoveredRow === i;
                  const rowBg = isHovered ? "#f9fafb" : isEven ? EVEN_ROW_BG : "#fff";
                  return (
                    <tr
                      key={`${r.provider}-${r.model}`}
                      style={{ background: rowBg }}
                      onMouseEnter={() => setHoveredRow(i)}
                      onMouseLeave={() => setHoveredRow(null)}
                    >
                      <td style={TD_BASE}>{r.displayName}</td>
                      <td style={TD_MONO}>{r.model}</td>
                      <td style={TD_RIGHT}>
                        <PriceCell current={r.input} regular={r.regularInput} decimals={3} />
                      </td>
                      <td style={TD_RIGHT}>
                        <PriceCell current={r.output} regular={r.regularOutput} decimals={3} />
                      </td>
                      <td style={TD_RIGHT}>
                        <PriceCell current={r.cached} regular={r.regularCached} decimals={4} />
                      </td>
                      <td style={TD_NOTES}>
                        {(() => {
                          const noteKeys = r.noteKeys && r.noteKeys.length > 0
                            ? r.noteKeys
                            : r.noteKey
                              ? [r.noteKey]
                              : [];
                          return noteKeys.map((key) => {
                            if (key === DEEPSEEK_PEAK_NOTE_KEY) {
                              const peakHours = formatDeepSeekPeakHoursLabel(now, tzId, lang);
                              const prefix = t("modelPricing.notes.deepseekPeakValleyPrefix");
                              return <div key={key}>{prefix} {peakHours}。</div>;
                            }
                            return <div key={key}>{t(key as any)}</div>;
                          });
                        })()}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          <div style={{ fontSize: 11, color: "#9ca3af", marginTop: 6, lineHeight: 1.4 }}>
            {t("modelPricing.disclaimer")}
          </div>
        </>
      )}
    </div>
  );
}
