import { useState, useEffect, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "../i18n";

interface TitleBarProps {
  activeTab?: "gateway" | "mcp";
  onTabChange?: (tab: "gateway" | "mcp") => void;
}

export default function TitleBar({ activeTab = "gateway", onTabChange }: TitleBarProps) {
  const { t } = useTranslation();
  const [title, setTitle] = useState<string>("Anthro Bridge");
  const [isMaximized, setIsMaximized] = useState<boolean>(false);

  useEffect(() => {
    let unlistenResize: (() => void) | undefined;
    let isMounted = true;

    async function initTitleAndState() {
      try {
        const appWindow = getCurrentWindow();
        const currentTitle = await appWindow.title();
        if (isMounted && currentTitle) {
          setTitle(currentTitle);
        }

        const max = await appWindow.isMaximized();
        if (isMounted) {
          setIsMaximized(max);
        }

        const unlisten = await appWindow.onResized(async () => {
          try {
            const currentMax = await appWindow.isMaximized();
            if (isMounted) {
              setIsMaximized(currentMax);
            }
          } catch (e) {
            console.error("Failed to check isMaximized on resize:", e);
          }
        });

        if (isMounted) {
          unlistenResize = unlisten;
        } else {
          unlisten();
        }
      } catch (e) {
        console.error("Failed to initialize TitleBar window state:", e);
      }
    }

    void initTitleAndState();

    return () => {
      isMounted = false;
      if (unlistenResize) {
        unlistenResize();
      }
    };
  }, []);

  const handleMinimize = useCallback(async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.minimize();
    } catch (e) {
      console.error("Failed to minimize window:", e);
    }
  }, []);

  const handleToggleMaximize = useCallback(async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.toggleMaximize();
      const max = await appWindow.isMaximized();
      setIsMaximized(max);
    } catch (e) {
      console.error("Failed to toggle maximize window:", e);
    }
  }, []);

  const handleClose = useCallback(async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.close();
    } catch (e) {
      console.error("Failed to close window:", e);
    }
  }, []);

  const handleDoubleClick = useCallback(() => {
    void handleToggleMaximize();
  }, [handleToggleMaximize]);

  return (
    <div className="custom-titlebar">
      {/* 1. Workspace Index Tabs (Strictly NO drag-region) */}
      {onTabChange && (
        <div className="titlebar-tabs">
          <button
            type="button"
            className={`titlebar-tab titlebar-tab-app ${activeTab === "gateway" ? "titlebar-tab-active" : ""}`}
            onClick={() => onTabChange("gateway")}
          >
            <img
              src="/app-icon.png"
              alt="App Icon"
              className="titlebar-icon"
            />
            <span className="titlebar-title">
              {title}
            </span>
          </button>

          <button
            type="button"
            className={`titlebar-tab titlebar-tab-mcp ${activeTab === "mcp" ? "titlebar-tab-active" : ""}`}
            onClick={() => onTabChange("mcp")}
          >
            {t("tab.mcp")}
          </button>
        </div>
      )}

      {/* 2. Central Spacer (Drag Region) */}
      <div
        className="titlebar-drag-spacer"
        data-tauri-drag-region
        onDoubleClick={handleDoubleClick}
      />

      {/* 3. Window Controls Region (Strictly NO drag-region) */}
      <div className="titlebar-controls">
        <button
          type="button"
          className="titlebar-btn titlebar-btn-minimize"
          title="最小化"
          aria-label="最小化"
          onClick={handleMinimize}
        >
          <svg width="10" height="1" viewBox="0 0 10 1">
            <line x1="0" y1="0.5" x2="10" y2="0.5" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>

        <button
          type="button"
          className="titlebar-btn titlebar-btn-maximize"
          title={isMaximized ? "元に戻す" : "最大化"}
          aria-label={isMaximized ? "元に戻す" : "最大化"}
          onClick={handleToggleMaximize}
        >
          {isMaximized ? (
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <path
                d="M2.5 2.5V0.5H9.5V7.5H7.5M0.5 2.5H7.5V9.5H0.5V2.5Z"
                stroke="currentColor"
                strokeWidth="1"
              />
            </svg>
          ) : (
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <rect x="0.5" y="0.5" width="9" height="9" stroke="currentColor" strokeWidth="1" />
            </svg>
          )}
        </button>

        <button
          type="button"
          className="titlebar-btn titlebar-btn-close"
          title="閉じる"
          aria-label="閉じる"
          onClick={handleClose}
        >
          <svg width="10" height="10" viewBox="0 0 10 10">
            <line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" strokeWidth="1.1" />
            <line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" strokeWidth="1.1" />
          </svg>
        </button>
      </div>
    </div>
  );
}
