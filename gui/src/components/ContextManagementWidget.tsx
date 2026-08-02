import { useTranslation } from "../i18n";
import type { EffectiveAutoCompact } from "../types";

export interface ContextManagementWidgetProps {
  effective: EffectiveAutoCompact | null;
  onToggle: (enabled: boolean) => void;
  disabled?: boolean;
}

export default function ContextManagementWidget({
  effective,
  onToggle,
  disabled = false,
}: ContextManagementWidgetProps) {
  const { t } = useTranslation();

  const globallyEnabled = effective?.globallyEnabled ?? false;

  return (
    <button
      type="button"
      role="switch"
      aria-checked={globallyEnabled}
      aria-label={t("claudeCodeContext.enable")}
      className={`context-management-toggle${globallyEnabled ? " is-on" : ""}`}
      title={t("claudeCodeContext.widgetTooltip")}
      onClick={() => onToggle(!globallyEnabled)}
      disabled={disabled}
    >
      <span className="context-management-title">
        {t("claudeCodeContext.title")}
      </span>
      <span className="context-management-track" aria-hidden="true">
        <span className="context-management-thumb" />
      </span>
    </button>
  );
}
