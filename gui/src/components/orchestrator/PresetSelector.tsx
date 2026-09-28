import React from "react";
import { useTranslation } from "../../i18n";
import { BUILTIN_ORCHESTRATOR_PRESETS } from "../../config/orchestratorPresets";

interface PresetSelectorProps {
  activePresetId: string;
  onSelectPreset: (presetId: string) => void;
  isCustom: boolean;
}

export const PresetSelector: React.FC<PresetSelectorProps> = ({
  activePresetId,
  onSelectPreset,
  isCustom,
}) => {
  const { t } = useTranslation();

  return (
    <div className="orchestrator-card orchestrator-preset-selector">
      <div className="orchestrator-card-header">
        <h3 className="orchestrator-card-title">{t("orchestrator.presets.title") || "Multi-Model Presets"}</h3>
        {isCustom && <span className="orchestrator-badge-custom">{t("orchestrator.presets.customBadge") || "Customized"}</span>}
      </div>

      <div className="orchestrator-preset-grid">
        {BUILTIN_ORCHESTRATOR_PRESETS.map((preset) => {
          const isSelected = !isCustom && activePresetId === preset.id;
          return (
            <button
              key={preset.id}
              type="button"
              className={`orchestrator-preset-card ${isSelected ? "active" : ""}`}
              onClick={() => onSelectPreset(preset.id)}
            >
              <div className="orchestrator-preset-header">
                <span className="orchestrator-preset-name">{preset.name}</span>
                {isSelected && <span className="orchestrator-preset-check">✓</span>}
              </div>
              <p className="orchestrator-preset-desc">{preset.description}</p>
            </button>
          );
        })}
      </div>
    </div>
  );
};
