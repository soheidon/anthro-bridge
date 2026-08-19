import { useTranslation } from "../i18n";
import ContextManagementWidget from "./ContextManagementWidget";
import type { EffectiveAutoCompact } from "../types";

interface HeaderProps {
  proxyStatus: "running" | "detected" | "unreachable" | "unknown";
  managedRunning: boolean;
  proxyLoading: boolean;
  proxyError: string | null;
  proxyDiag: string | null;
  successMessage: string | null;
  onStart: () => void;
  onStop: () => void;
  onClearDiag: () => void;
  switchMessage?: string | null;
  effectiveAutoCompact?: EffectiveAutoCompact | null;
  onToggleAutoCompact?: (enabled: boolean) => void;
  autoCompactSaving?: boolean;
  activeTab?: "gateway" | "mcp" | "settings";
}

export default function Header({
  proxyStatus,
  managedRunning,
  proxyLoading,
  proxyError,
  proxyDiag,
  successMessage,
  onStart,
  onStop,
  onClearDiag,
  switchMessage,
  effectiveAutoCompact,
  onToggleAutoCompact,
  autoCompactSaving,
  activeTab = "gateway",
}: HeaderProps) {
  const { t } = useTranslation();

  // Strict early return: Header is only rendered for the Gateway view
  if (activeTab !== "gateway") {
    return null;
  }

  const showGatewayControls = activeTab === "gateway";

  if (!showGatewayControls && !proxyDiag) {
    return null;
  }

  const statusKey =
    proxyStatus === "running" ? "header.gatewayRunning"
    : proxyStatus === "detected" ? "header.gatewayDetected"
    : proxyStatus === "unreachable" ? "header.gatewayUnreachable"
    : "status.unknown";

  return (
    <header className="app-header">
      <div className="header-proxy-section">
        {showGatewayControls && (
          <>
            {managedRunning ? (
              <button
                className="btn btn-large"
                onClick={onStop}
                disabled={proxyLoading}
              >
                {t("header.stopGateway")}
              </button>
            ) : (
              <button
                className="btn btn-primary btn-large"
                onClick={onStart}
                disabled={proxyLoading}
              >
                {t("header.startGateway")}
              </button>
            )}
            <span className={`status-badge status-${proxyStatus}`}>
              {t(statusKey)}
            </span>
            {onToggleAutoCompact && (
              <ContextManagementWidget
                effective={effectiveAutoCompact ?? null}
                onToggle={onToggleAutoCompact}
                disabled={autoCompactSaving}
              />
            )}
            {switchMessage && (
              <span className="header-switch-msg">
                <span className="loading header-loading-inline" />
                {switchMessage}
              </span>
            )}
            {proxyError && (
              <span className="proxy-error" title={proxyError}>
                {proxyError.length > 120 ? proxyError.slice(0, 120) + "…" : proxyError}
              </span>
            )}
          </>
        )}
      </div>
      {proxyDiag && proxyError && (
        <div className="proxy-diag">
          <div className="proxy-diag-header">
            <span>Diagnostics</span>
            <button className="btn btn-small" onClick={onClearDiag}>x</button>
          </div>
          <pre className="proxy-diag-pre">{proxyDiag}</pre>
        </div>
      )}
    </header>
  );
}
