import { useState, useMemo, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import Header from "./components/Header";
import ProviderTiles from "./components/ProviderTiles";
import StatusPanel from "./components/StatusPanel";
import LogPanel from "./components/LogPanel";
import { ConfigPanelContent } from "./components/ConfigPanel";
import { ClaudeConfigPanelContent } from "./components/ClaudeConfigPanel";
import ApiKeyPanel from "./components/ApiKeyPanel";
import ModelPricingAccordion from "./components/ModelPricingAccordion";
import LanguageSelector from "./components/LanguageSelector";
import FirstRunLanguagePicker from "./components/FirstRunLanguagePicker";
import { useHealthCheck } from "./hooks/useHealthCheck";
import { useProxyToggle } from "./hooks/useProxyToggle";
import { LanguageProvider, useTranslation } from "./i18n";

function AppContent() {
  const { t } = useTranslation();
  const [inSettings, setInSettings] = useState(false);
  const { managedRunning, loading: proxyLoading, error: proxyError, diag: proxyDiag, successMessage, start, stop, clearDiag } = useProxyToggle();
  const { data: health, error: healthError, loading: healthLoading, refresh: healthRefresh } = useHealthCheck(managedRunning);

  // Incremented when provider changes, triggers StatusPanel to reload
  const [configVersion, setConfigVersion] = useState(0);

  // First-run language selection
  const [firstRun, setFirstRun] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>("is_first_run")
      .then(setFirstRun)
      .catch(() => setFirstRun(false));
  }, []);

  // Force window to 1100x720 after OS-level state restoration
  useEffect(() => {
    getCurrentWindow().setSize(new LogicalSize(1100, 720)).catch(() => {});
  }, []);

  const proxyStatus = useMemo(() => {
    if (health?.managed_child_running) return "running";
    if (!health) return "unknown";
    if (health.reachable) return "detected";
    return "unreachable";
  }, [health]);

  const handleStop = useCallback(() => {
    stop();
    setTimeout(() => {
      healthRefresh?.();
    }, 500);
  }, [stop, healthRefresh]);

  const handleConfigChanged = useCallback(() => {
    setConfigVersion((v) => v + 1);
  }, []);

  const handleToggleSettings = useCallback(() => {
    setInSettings((prev) => !prev);
  }, []);

  const handleBack = useCallback(() => {
    setInSettings(false);
  }, []);

  // Show full-screen language picker on first run
  if (firstRun === null) {
    // Loading — wait for is_first_run check
    return null;
  }

  if (firstRun) {
    return <FirstRunLanguagePicker onDone={() => setFirstRun(false)} />;
  }

  return (
    <div className="app">
      <Header
        proxyStatus={proxyStatus}
        managedRunning={health?.managed_child_running ?? false}
        proxyLoading={proxyLoading}
        proxyError={proxyError}
        proxyDiag={proxyDiag}
        successMessage={successMessage}
        onStart={start}
        onStop={handleStop}
        onClearDiag={clearDiag}
        inSettings={inSettings}
        onToggleSettings={handleToggleSettings}
        onBack={handleBack}
      />
      {inSettings ? (
        <div className="settings-page">
          <LanguageSelector />
          <ApiKeyPanel onConfigChanged={handleConfigChanged} />
          <ModelPricingAccordion />
          <ClaudeConfigPanelContent />
          <ConfigPanelContent />
        </div>
      ) : (
        <div className="dashboard-page">
          <ProviderTiles health={health} onConfigChanged={handleConfigChanged} refreshKey={configVersion} />
          <StatusPanel health={health} healthError={healthError} healthLoading={healthLoading} refreshKey={configVersion} />
          <LogPanel />
        </div>
      )}
    </div>
  );
}

export default function App() {
  return (
    <LanguageProvider>
      <AppContent />
    </LanguageProvider>
  );
}
