import { useState } from "react";
import { useTranslation } from "../i18n";
import { MODEL_PRICING, PROVIDER_PRICE_ORDER } from "../config/modelPricing";
import { PROVIDER_MODELS } from "../modelCapabilities";

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
  whiteSpace: "nowrap",
  fontVariantNumeric: "tabular-nums",
};

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

export default function ModelPricingAccordion() {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [hoveredRow, setHoveredRow] = useState<number | null>(null);

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
    note: string | undefined;
  }> = [];

  for (const providerId of PROVIDER_PRICE_ORDER) {
    const models = PROVIDER_MODELS[providerId] ?? [];
    const displayName =
      providerId === "deepseek" ? "DeepSeek" :
      providerId === "mimo" ? "MiMo" :
      providerId === "minimax" ? "MiniMax" :
      providerId === "kimi" ? "Kimi" : providerId;

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
        note: p.pricingNote,
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
      >
        <h3 style={{ margin: 0, fontSize: 14, fontWeight: 700 }}>{t("modelPricing.header")}</h3>
        <span style={{ fontSize: 12, color: "#6b7280" }}>{t("modelPricing.usdLabel")}</span>
        <span style={{ flex: 1 }} />
        <span style={{ fontSize: 14, color: "#6b7280" }}>{expanded ? "▾" : "▸"}</span>
      </div>

      {expanded && (
        <>
          <div style={{ maxHeight: 280, overflowY: "auto", marginTop: 8 }}>
            <table style={{ width: "100%", borderCollapse: "collapse" }}>
              <thead style={{ position: "sticky", top: 0, background: "#f3f4f6", zIndex: 1 }}>
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
                      <td style={TD_RIGHT}>${r.input.toFixed(3)}</td>
                      <td style={TD_RIGHT}>${r.output.toFixed(2)}</td>
                      <td style={TD_RIGHT}>{r.cached != null ? `$${r.cached.toFixed(4)}` : "—"}</td>
                      <td style={TD_NOTES}>{r.note ?? ""}</td>
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
