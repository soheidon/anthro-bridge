import { useTranslation } from "../i18n";

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
  inSettings: boolean;
  onToggleSettings: () => void;
  onBack: () => void;
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
  inSettings,
  onToggleSettings,
  onBack,
}: HeaderProps) {
  const { t } = useTranslation();

  const statusKey =
    proxyStatus === "running" ? "header.gatewayRunning"
    : proxyStatus === "detected" ? "header.gatewayDetected"
    : proxyStatus === "unreachable" ? "header.gatewayUnreachable"
    : "status.unknown";

  return (
    <header className="app-header">
      <div className="header-proxy-section">
        {!inSettings && (
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
      <div className="header-right">
        <span className="version-info">v0.8.0</span>
        <button
          className={`btn btn-small ${inSettings ? "btn-active" : ""}`}
          onClick={onToggleSettings}
        >
          {inSettings ? "✕" : "⚙"} {t("header.settings")}
        </button>
      </div>
    </header>
  );
}
