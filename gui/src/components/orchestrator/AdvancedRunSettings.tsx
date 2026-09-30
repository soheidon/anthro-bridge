import type { LoopIterationLimits, ValidationGateConfig } from "../../types/orchestrator";

interface Props {
  gates: ValidationGateConfig[];
  limits: LoopIterationLimits;
  onGateOverridesChange: (overrides: Record<string, boolean>) => void;
  onLimitOverridesChange: (overrides: Partial<LoopIterationLimits>) => void;
  t: (key: any) => string;
}

export function AdvancedRunSettings({ gates, limits, onGateOverridesChange, onLimitOverridesChange, t }: Props) {
  return (
    <details className="orchestrator-advanced-run-settings">
      <summary>{t("orchestrator.advancedRun.title")}</summary>
      <p>{t("orchestrator.advancedRun.transientNote")}</p>
      <fieldset>
        <legend>{t("orchestrator.advancedRun.validationGates")}</legend>
        {gates.map((gate) => (
          <label key={gate.id}>
            <input
              type="checkbox"
              checked={gate.enabled}
              onChange={(event) => onGateOverridesChange({ [gate.id]: event.target.checked })}
            />
            {gate.category ? (t(`orchestrator.validation.category.${gate.category}`) || gate.name) : gate.name}
          </label>
        ))}
      </fieldset>
      <fieldset>
        <legend>{t("orchestrator.advancedRun.iterationLimits")}</legend>
        {([
          ["maxPlanReviewIterations", "planReviews"],
          ["maxFixIterations", "fixes"],
          ["maxCodeReviewIterations", "codeReviews"],
        ] as const).map(([key, label]) => (
          <label key={key}>
            {t(`orchestrator.advancedRun.${label}`)}
            <input
              type="number"
              min={1}
              max={100}
              value={limits[key]}
              onChange={(event) => {
                const value = Number(event.target.value);
                if (Number.isInteger(value) && value >= 1 && value <= 100) {
                  onLimitOverridesChange({ [key]: value });
                }
              }}
            />
          </label>
        ))}
      </fieldset>
    </details>
  );
}
