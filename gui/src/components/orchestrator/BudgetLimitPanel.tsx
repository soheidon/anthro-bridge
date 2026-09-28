import React from "react";
import { useTranslation } from "../../i18n";
import type { LoopIterationLimits } from "../../types/orchestrator";

interface BudgetLimitPanelProps {
  limits: LoopIterationLimits;
  onChangeLimits: (limits: LoopIterationLimits) => void;
}

export const BudgetLimitPanel: React.FC<BudgetLimitPanelProps> = ({
  limits,
  onChangeLimits,
}) => {
  const { t } = useTranslation();

  const handleChange = (field: keyof LoopIterationLimits, value: number) => {
    onChangeLimits({
      ...limits,
      [field]: Math.max(1, value),
    });
  };

  return (
    <div className="orchestrator-card orchestrator-budget-limits">
      <div className="orchestrator-card-header">
        <h3 className="orchestrator-card-title">
          {t("orchestrator.limits.title") || "Loop Safety & Iteration Limits"}
        </h3>
      </div>
      <p className="orchestrator-desc">
        {t("orchestrator.limits.desc") ||
          "Limits prevent runaway token spend and infinite loops between Reviewers, Implementers, and Fixers."}
      </p>

      <div className="orchestrator-limits-grid">
        <div className="limit-item">
          <label className="orchestrator-label">
            {t("orchestrator.limits.planReview") || "Max Plan Review Loops"}:
          </label>
          <div className="limit-input-wrap">
            <input
              type="number"
              min={1}
              max={5}
              className="orchestrator-input orchestrator-number-input"
              value={limits.maxPlanReviewIterations}
              onChange={(e) =>
                handleChange("maxPlanReviewIterations", parseInt(e.target.value, 10) || 1)
              }
            />
            <span className="limit-unit">iterations</span>
          </div>
        </div>

        <div className="limit-item">
          <label className="orchestrator-label">
            {t("orchestrator.limits.fixLoops") || "Max Fix Iteration Loops"}:
          </label>
          <div className="limit-input-wrap">
            <input
              type="number"
              min={1}
              max={10}
              className="orchestrator-input orchestrator-number-input"
              value={limits.maxFixIterations}
              onChange={(e) =>
                handleChange("maxFixIterations", parseInt(e.target.value, 10) || 1)
              }
            />
            <span className="limit-unit">iterations</span>
          </div>
        </div>

        <div className="limit-item">
          <label className="orchestrator-label">
            {t("orchestrator.limits.codeReview") || "Max Code Review Loops"}:
          </label>
          <div className="limit-input-wrap">
            <input
              type="number"
              min={1}
              max={5}
              className="orchestrator-input orchestrator-number-input"
              value={limits.maxCodeReviewIterations}
              onChange={(e) =>
                handleChange("maxCodeReviewIterations", parseInt(e.target.value, 10) || 1)
              }
            />
            <span className="limit-unit">iterations</span>
          </div>
        </div>
      </div>

      {/* Planned Advanced Budget Controls */}
      <div className="orchestrator-planned-section" style={{ marginTop: "1rem", opacity: 0.65 }}>
        <h4 style={{ fontSize: "0.85rem", color: "var(--text-muted)", marginBottom: "0.5rem" }}>
          Advanced Rate & Budget Controls (Planned)
        </h4>
        <div className="orchestrator-limits-grid" style={{ pointerEvents: "none" }}>
          <div className="limit-item">
            <label className="orchestrator-label">Max Calls / Run (Planned):</label>
            <input type="number" disabled className="orchestrator-input orchestrator-number-input" value={50} readOnly />
          </div>
          <div className="limit-item">
            <label className="orchestrator-label">Timeout (Planned):</label>
            <input type="number" disabled className="orchestrator-input orchestrator-number-input" value={180} readOnly />
          </div>
          <div className="limit-item">
            <label className="orchestrator-label">On Rate Limit (Planned):</label>
            <input type="text" disabled className="orchestrator-input" value="fallback" readOnly />
          </div>
        </div>
      </div>
    </div>
  );
};
