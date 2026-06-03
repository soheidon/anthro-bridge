import { useTranslation } from "../i18n";
import type { TranslationKey } from "../i18n";

export type SettingsTabId = "gateway" | "claude" | "apikey";

interface TabBarProps {
  activeTab: SettingsTabId;
  onTabChange: (tab: SettingsTabId) => void;
  onBack: () => void;
  visible: boolean;
}

const SETTINGS_TABS: { id: SettingsTabId; labelKey: TranslationKey }[] = [
  { id: "gateway", labelKey: "tab.advanced" },
  { id: "claude", labelKey: "tab.claudeSetup" },
  { id: "apikey", labelKey: "tab.apiKey" },
];

export default function TabBar({ activeTab, onTabChange, onBack, visible }: TabBarProps) {
  const { t } = useTranslation();

  if (!visible) return null;

  return (
    <div className="tab-bar">
      <button className="tab-item tab-back" onClick={onBack}>
        ← {t("settings.back")}
      </button>
      {SETTINGS_TABS.map((tab) => (
        <button
          key={tab.id}
          className={`tab-item ${activeTab === tab.id ? "tab-active" : ""}`}
          onClick={() => onTabChange(tab.id)}
        >
          {t(tab.labelKey)}
        </button>
      ))}
    </div>
  );
}
