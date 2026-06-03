import { useState, useMemo, useCallback } from "react";
import Header from "./components/Header";
import ProviderTiles from "./components/ProviderTiles";
import StatusPanel from "./components/StatusPanel";
import LogPanel from "./components/LogPanel";
import { ConfigPanelContent } from "./components/ConfigPanel";
import { ClaudeConfigPanelContent } from "./components/ClaudeConfigPanel";
import ApiKeyPanel from "./components/ApiKeyPanel";
import { useHealthCheck } from "./hooks/useHealthCheck";
import { useProxyToggle } from "./hooks/useProxyToggle";
import { LanguageProvider, useTranslation } from "./i18n";

export default function App() {
  const { t } = useTranslation();
  const [inSettings, setInSettings] = useState(false);
  const { managedRunning, loading: proxyLoading, error: proxyError, diag: proxyDiag, successMessage, start, stop, clearDiag } = useProxyToggle();
  const { data: health, error: healthError, loading: healthLoading, refresh: healthRefresh } = useHealthCheck(managedRunning);

  // Incremented when provider changes, triggers StatusPanel to reload
  const [configVersion, setConfigVersion] = useState(0);

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

  return (
    <LanguageProvider>
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
        />
        {inSettings ? (
          <div className="settings-page">
            <div className="settings-header">
              <button className="tab-back" onClick={handleBack}>
                ← {t("settings.back")}
              </button>
            </div>
            <ApiKeyPanel />
            <ClaudeConfigPanelContent />
            <ConfigPanelContent />
          </div>
        ) : (
          <div className="dashboard-page">
            <ProviderTiles health={health} onConfigChanged={handleConfigChanged} />
            <StatusPanel health={health} healthError={healthError} healthLoading={healthLoading} refreshKey={configVersion} />
            <LogPanel />
          </div>
        )}
      </div>
    </LanguageProvider>
  );
}
