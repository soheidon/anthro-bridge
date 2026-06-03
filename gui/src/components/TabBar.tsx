import { useTranslation } from "../i18n";
import type { TranslationKey } from "../i18n/translations";

export type TabId = "dashboard" | "gateway" | "claude" | "apikey";

interface TabBarProps {
  activeTab: TabId;
  onTabChange: (tab: TabId) => void;
}

const TABS: { id: TabId; labelKey: TranslationKey }[] = [
  { id: "dashboard", labelKey: "tab.dashboard" },
  { id: "gateway", labelKey: "tab.advanced" },
  { id: "claude", labelKey: "tab.claudeSetup" },
  { id: "apikey", labelKey: "tab.apiKey" },
];

export default function TabBar({ activeTab, onTabChange }: TabBarProps) {
  const { t } = useTranslation();

  return (
    <div className="tab-bar">
      {TABS.map((tab) => (
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
