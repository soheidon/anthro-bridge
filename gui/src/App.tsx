import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize, currentMonitor } from "@tauri-apps/api/window";
import Header from "./components/Header";
import ProviderTiles from "./components/ProviderTiles";
import StatusPanel from "./components/StatusPanel";
import LogPanel from "./components/LogPanel";
import { ConfigPanelContent } from "./components/ConfigPanel";
import { ClaudeConfigPanelContent } from "./components/ClaudeConfigPanel";
import ApiKeyPanel from "./components/ApiKeyPanel";
import ModelPricingAccordion from "./components/ModelPricingAccordion";
import TimezoneSettingPanel from "./components/TimezoneSettingPanel";
import NormalizeModelPanel from "./components/NormalizeModelPanel";
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
  const [switchMessage, setSwitchMessage] = useState<string | null>(null);

  // First-run language selection
  const [firstRun, setFirstRun] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>("is_first_run")
      .then(setFirstRun)
      .catch(() => setFirstRun(false));
  }, []);

  // Force window to 1150x670 after OS-level state restoration
  useEffect(() => {
    getCurrentWindow().setSize(new LogicalSize(1150, 670)).catch(() => {});
  }, []);

  // Log panel collapse state (lifted from LogPanel for window resize control)
  const [logCollapsed, setLogCollapsed] = useState(true);
  const collapsedSizeRef = useRef<LogicalSize | null>(null);
  const resizingRef = useRef(false);
  const LOG_PANEL_EXTRA_HEIGHT = 260;

  const handleLogToggle = useCallback(async () => {
    if (resizingRef.current) return;
    resizingRef.current = true;

    const willExpand = logCollapsed;

    try {
      if (willExpand) {
        await expandWindowForLog();
        setLogCollapsed(false);
      } else {
        setLogCollapsed(true);
        await restoreWindowAfterLog();
      }
    } catch (error) {
      console.error("Failed to resize log panel window", error);
      setLogCollapsed(!willExpand);
    } finally {
      resizingRef.current = false;
    }
  }, [logCollapsed]);

  async function expandWindowForLog() {
    const appWindow = getCurrentWindow();
    const scaleFactor = await appWindow.scaleFactor();
    const innerPhysical = await appWindow.innerSize();
    const outerPhysical = await appWindow.outerSize();
    const outerPosition = await appWindow.outerPosition();

    const innerLogical = innerPhysical.toLogical(scaleFactor);
    collapsedSizeRef.current = new LogicalSize(innerLogical.width, innerLogical.height);

    let requestedLogicalHeight = innerLogical.height + LOG_PANEL_EXTRA_HEIGHT;

    const monitor = await currentMonitor();
    if (monitor) {
      const workAreaBottom = monitor.workArea.position.y + monitor.workArea.size.height;
      const decorationHeight = outerPhysical.height - innerPhysical.height;
      const maxInnerPhysicalHeight = Math.max(0, workAreaBottom - outerPosition.y - decorationHeight);
      const maxInnerLogicalHeight = maxInnerPhysicalHeight / scaleFactor;

      requestedLogicalHeight = Math.max(
        innerLogical.height,
        Math.min(requestedLogicalHeight, maxInnerLogicalHeight)
      );
    }

    await appWindow.setSize(new LogicalSize(innerLogical.width, requestedLogicalHeight));
  }

  async function restoreWindowAfterLog() {
    if (collapsedSizeRef.current) {
      const appWindow = getCurrentWindow();
      await appWindow.setSize(collapsedSizeRef.current);
      collapsedSizeRef.current = null;
    }
  }

  // Restore window size if component unmounts while log is expanded
  useEffect(() => {
    return () => {
      const saved = collapsedSizeRef.current;
      if (!saved) return;
      void getCurrentWindow()
        .setSize(saved)
        .catch((error) => {
          console.error("Failed to restore window size", error);
        });
    };
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
        switchMessage={switchMessage}
      />
      {inSettings ? (
        <div className="settings-page">
          <LanguageSelector />
          <ApiKeyPanel onConfigChanged={handleConfigChanged} />
          <NormalizeModelPanel />
          <TimezoneSettingPanel />
          <ModelPricingAccordion />
          <ClaudeConfigPanelContent />
          <ConfigPanelContent />
        </div>
      ) : (
        <div className="dashboard-page">
          <ProviderTiles health={health} onConfigChanged={handleConfigChanged} refreshKey={configVersion} onSwitchMessage={setSwitchMessage} />
          <StatusPanel health={health} healthError={healthError} healthLoading={healthLoading} refreshKey={configVersion} />
          <LogPanel collapsed={logCollapsed} onToggleCollapse={handleLogToggle} />
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
