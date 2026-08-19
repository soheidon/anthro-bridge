import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize, currentMonitor } from "@tauri-apps/api/window";
import TitleBar from "./components/TitleBar";
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
import McpPanel from "./components/McpPanel";
import McpSettingPanel from "./components/McpSettingPanel";
import { useHealthCheck } from "./hooks/useHealthCheck";
import { useProxyToggle } from "./hooks/useProxyToggle";
import { LanguageProvider, useTranslation } from "./i18n";
import type { EffectiveAutoCompact, GatewayConfig } from "./types";
import { calculateDashboardCardCount } from "./dashboardTiles";
import {
  DASHBOARD_GRID_COLUMNS,
  DASHBOARD_GRID_ROW_GAP,
  DASHBOARD_TILE_MIN_HEIGHT,
  calculateAvailableLogicalHeight,
  calculateDashboardGridRows,
  calculateInitialWindowHeight,
} from "./windowSizing";

function AppContent() {
  const { t } = useTranslation();
  const [inSettings, setInSettings] = useState(false);
  const [mainTab, setMainTab] = useState<"gateway" | "mcp">("gateway");
  const { managedRunning, loading: proxyLoading, error: proxyError, diag: proxyDiag, successMessage, start, stop, clearDiag } = useProxyToggle();
  const { data: health, error: healthError, loading: healthLoading, refresh: healthRefresh } = useHealthCheck(managedRunning);

  // Incremented when provider changes, triggers StatusPanel to reload
  const [configVersion, setConfigVersion] = useState(0);
  const [config, setConfig] = useState<GatewayConfig | null>(null);
  const [switchMessage, setSwitchMessage] = useState<string | null>(null);
  const [effectiveAutoCompact, setEffectiveAutoCompact] = useState<EffectiveAutoCompact | null>(null);
  const [autoCompactSaving, setAutoCompactSaving] = useState(false);

  const refreshConfig = useCallback(async () => {
    const next = await invoke<GatewayConfig>("read_config");
    setConfig(next);
    setConfigVersion((v) => v + 1);
  }, []);

  // Initial config load
  useEffect(() => {
    void refreshConfig();
  }, [refreshConfig]);

  // Re-resolve the effective auto-compact settings whenever the config changes.
  useEffect(() => {
    invoke<EffectiveAutoCompact>("resolve_claude_code_auto_compact")
      .then(setEffectiveAutoCompact)
      .catch((e) => {
        console.error("resolve_claude_code_auto_compact", e);
        setEffectiveAutoCompact(null);
      });
  }, [configVersion]);

  // First-run language selection
  const [firstRun, setFirstRun] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<boolean>("is_first_run")
      .then(setFirstRun)
      .catch(() => setFirstRun(false));
  }, []);

  // Size the window to fit the configured dashboard card rows.
  const sizedDashboardRowCountRef = useRef<number | null>(null);
  useEffect(() => {
    if (!config) return;

    const cardCount = calculateDashboardCardCount(config);
    const rowCount = calculateDashboardGridRows(
      cardCount,
      DASHBOARD_GRID_COLUMNS,
    );

    if (sizedDashboardRowCountRef.current === rowCount) return;
    sizedDashboardRowCountRef.current = rowCount;

    void (async () => {
      try {
        const appWindow = getCurrentWindow();
        const scaleFactor = await appWindow.scaleFactor();
        const innerPhysical = await appWindow.innerSize();
        const outerPhysical = await appWindow.outerSize();
        const outerPosition = await appWindow.outerPosition();
        const monitor = await currentMonitor();
        const innerLogical = innerPhysical.toLogical(scaleFactor);

        const maxHeight = monitor
          ? calculateAvailableLogicalHeight({
              scaleFactor,
              outerY: outerPosition.y,
              outerHeight: outerPhysical.height,
              innerHeight: innerPhysical.height,
              workAreaY: monitor.workArea.position.y,
              workAreaHeight: monitor.workArea.size.height,
            })
          : undefined;

        const height = calculateInitialWindowHeight(rowCount, {
          baseHeight: 720,
          baseRows: 2,
          rowHeight: DASHBOARD_TILE_MIN_HEIGHT + DASHBOARD_GRID_ROW_GAP + 10,
          minHeight: 700,
          maxHeight,
        });

        await appWindow.setSize(new LogicalSize(innerLogical.width, height));
      } catch (error) {
        console.error("Failed to apply dashboard window size", error);
      }
    })();
  }, [config]);

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

  const handleToggleAutoCompact = useCallback(
    async (enabled: boolean) => {
      setAutoCompactSaving(true);
      try {
        await invoke("update_claude_code_auto_compact_global", { enabled });
        await refreshConfig();
      } catch (e) {
        console.error("update_claude_code_auto_compact_global", e);
      } finally {
        setAutoCompactSaving(false);
      }
    },
    [refreshConfig],
  );

  const handleMainTabChange = useCallback((tab: "gateway" | "mcp") => {
    setMainTab(tab);
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
      <TitleBar activeTab={mainTab} onTabChange={handleMainTabChange} />
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
        effectiveAutoCompact={effectiveAutoCompact}
        onToggleAutoCompact={handleToggleAutoCompact}
        autoCompactSaving={autoCompactSaving}
      />
      {inSettings ? (
        <div className="settings-page">
          <LanguageSelector />
          <ApiKeyPanel
            config={config}
            refreshConfig={refreshConfig}
            gatewayRunning={health?.port_listening ?? false}
            restartGateway={async () => {
              await invoke("stop_proxy");
              await invoke("start_proxy");
            }}
          />
          <NormalizeModelPanel />
          <McpSettingPanel config={config} refreshConfig={refreshConfig} />
          <TimezoneSettingPanel />
          <ModelPricingAccordion />
          <ClaudeConfigPanelContent />
          <ConfigPanelContent />
        </div>
      ) : mainTab === "gateway" ? (
        <div className="dashboard-page">
          <ProviderTiles health={health} onConfigChanged={handleConfigChanged} refreshKey={configVersion} onSwitchMessage={setSwitchMessage} />
          <StatusPanel health={health} healthError={healthError} healthLoading={healthLoading} refreshKey={configVersion} />
          <LogPanel collapsed={logCollapsed} onToggleCollapse={handleLogToggle} />
        </div>
      ) : (
        <McpPanel config={config} refreshConfig={refreshConfig} />
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
