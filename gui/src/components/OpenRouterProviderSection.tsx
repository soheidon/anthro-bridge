import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ApiKeyStatus, ProviderConfig, OpenRouterProfile, CommandResponse } from "../types";
import OpenRouterModelSetCard from "./OpenRouterModelSetCard";

type SaveStatus = "idle" | "saving" | "saved" | "error";

const COL_STYLE: React.CSSProperties = {
  padding: "6px 10px",
  fontSize: 12,
  color: "#1f2937",
  whiteSpace: "nowrap",
};

// ---------------------------------------------------------------------------
// Model-set name helpers — must match Rust paths::parse_model_set_number
// ---------------------------------------------------------------------------

/** Parse a canonical "Model N" name. Must match the backend regex `^Model [1-9]\d*$`. */
export function parseAutoModelSetNumber(name: string): number | null {
  const match = /^Model ([1-9]\d*)$/.exec(name);
  if (!match) return null;
  const value = Number(match[1]);
  return Number.isSafeInteger(value) ? value : null;
}

/** Return the locale-appropriate display name for a model set. */
function displayModelSetName(profile: OpenRouterProfile, prefix: string): string {
  const number = parseAutoModelSetNumber(profile.display_name);
  return number === null ? profile.display_name : `OpenRouter ${prefix} ${number}`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

type OpenRouterProviderSectionProps = {
  providerId: "openrouter";
  provider: ProviderConfig;
  profiles: OpenRouterProfile[];
  activeProfileId: string | null;
  keyStatus: ApiKeyStatus | null;
  allKeyStatusLoading: boolean;
  gatewayRunning: boolean;
  refreshConfig: () => Promise<void>;
  restartGateway: () => Promise<void>;
  refreshKeyStatus: () => Promise<void>;
  onAddModelSet: () => Promise<void>;
  addError: string | null;
};

export default function OpenRouterProviderSection({
  providerId,
  provider,
  profiles,
  activeProfileId,
  keyStatus,
  allKeyStatusLoading: _allKeyStatusLoading,
  gatewayRunning,
  refreshConfig,
  restartGateway,
  refreshKeyStatus: _refreshKeyStatus,
  onAddModelSet,
  addError,
}: OpenRouterProviderSectionProps) {
  const { t } = useTranslation();

  // ── Accordion toggle ─────────────────────────────────────────────

  const [expanded, setExpanded] = useState(false);

  const toggleExpanded = () => setExpanded((prev) => !prev);

  const handleHeaderClick = () => toggleExpanded();

  const handleHeaderKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggleExpanded();
    }
  };

  // ── Auth state (moved from ProviderRow openrouter branch) ────────

  const [envVarName, setEnvVarName] = useState(provider.api_key_env);
  const [envVarError, setEnvVarError] = useState<string | null>(null);
  const [envVarStatus, setEnvVarStatus] = useState<SaveStatus>("idle");
  const [keyText, setKeyText] = useState("");
  const [keyStatusLocal, setKeyStatusLocal] = useState<SaveStatus>("idle");
  const [refreshingOpenRouterModels, setRefreshingOpenRouterModels] = useState(false);
  const refreshingOpenRouterModelsRef = useRef(false);
  const envTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const keyTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  // Sync env var name when provider config changes
  useEffect(() => {
    setEnvVarName(provider.api_key_env);
  }, [provider.api_key_env]);

  // Cleanup timers
  useEffect(() => {
    return () => {
      if (envTimerRef.current) clearTimeout(envTimerRef.current);
      if (keyTimerRef.current) clearTimeout(keyTimerRef.current);
    };
  }, []);

  // ── Auth: env var name save ──────────────────────────────────────

  const handleEnvVarSave = async () => {
    const trimmed = envVarName.trim();
    if (!trimmed || trimmed === provider.api_key_env) return;
    if (!/^[A-Z][A-Z0-9_]*$/.test(trimmed)) {
      setEnvVarError(t("apiKeyPanel.envVarErrorFormat"));
      return;
    }
    setEnvVarError(null);
    setEnvVarStatus("saving");
    if (envTimerRef.current) clearTimeout(envTimerRef.current);
    try {
      await invoke("update_provider_api_key_env", { providerId, apiKeyEnv: trimmed });
      setEnvVarStatus("saved");
      envTimerRef.current = setTimeout(() => setEnvVarStatus("idle"), 2000);
      refreshConfig();
    } catch (e) {
      setEnvVarStatus("error");
      setEnvVarError(String(e));
    }
  };

  // ── Auth: API key save ───────────────────────────────────────────

  const handleKeySave = async () => {
    const trimmed = keyText.trim();
    if (!trimmed || !keyStatus || keyStatusLocal === "saving") return;
    setKeyStatusLocal("saving");
    if (keyTimerRef.current) clearTimeout(keyTimerRef.current);
    try {
      await invoke("set_env_api_key", { key: trimmed, envVarName: keyStatus.env_var });
      setKeyStatusLocal("saved");
      setKeyText("");
      keyTimerRef.current = setTimeout(() => setKeyStatusLocal("idle"), 2000);
      refreshConfig();
    } catch {
      setKeyStatusLocal("error");
    }
  };

  // ── Auth: refresh OpenRouter model list ──────────────────────────

  const handleRefreshOpenRouterModels = useCallback(() => {
    if (refreshingOpenRouterModelsRef.current) return;
    refreshingOpenRouterModelsRef.current = true;
    setRefreshingOpenRouterModels(true);
    window.dispatchEvent(new CustomEvent("openrouter-models-refresh-requested"));
  }, []);

  useEffect(() => {
    const handleCompleted = () => {
      refreshingOpenRouterModelsRef.current = false;
      setRefreshingOpenRouterModels(false);
    };
    window.addEventListener("openrouter-models-refresh-completed", handleCompleted);
    return () => window.removeEventListener("openrouter-models-refresh-completed", handleCompleted);
  }, []);

  // ── Status helpers ───────────────────────────────────────────────

  const envStatusText =
    envVarStatus === "saving" ? t("apiKeyPanel.savingStatus") :
    envVarStatus === "saved" ? t("apiKeyPanel.savedStatus") :
    envVarStatus === "error" ? t("apiKeyPanel.errorStatus") : null;

  const keyStatusText =
    keyStatusLocal === "saving" ? t("apiKeyPanel.savingStatus") :
    keyStatusLocal === "saved" ? t("apiKeyPanel.savedStatus") :
    keyStatusLocal === "error" ? t("apiKeyPanel.errorStatus") : null;

  // ── Render ───────────────────────────────────────────────────────

  const modelPrefix = t("openRouterProfile.defaultNewName"); // "Model" in en

  return (
    <div>
      {/* ── Clickable accordion header ──────────────────────────────── */}
      <div
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={handleHeaderClick}
        onKeyDown={handleHeaderKeyDown}
        style={{
          display: "flex",
          alignItems: "center",
          background: "#ffffff",
          borderTop: "1px solid #e5e7eb",
          borderBottom: expanded ? "none" : "1px solid #e5e7eb",
          cursor: "pointer",
          transition: "background 0.1s",
        }}
        onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "#f8f9fa"; }}
        onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "#ffffff"; }}
      >
        <div style={{ ...COL_STYLE, fontSize: 14, color: "#6b7280", userSelect: "none", padding: "6px 4px 6px 8px", minWidth: 28 }}>
          {expanded ? "▾" : "▸"}
        </div>

        <div style={{ ...COL_STYLE, fontWeight: 600, minWidth: 130, fontSize: 13, padding: "6px 4px" }}>
          {provider.display_name}
        </div>

        <div style={{ ...COL_STYLE, fontFamily: "var(--font-mono)", fontSize: 11, minWidth: 150, color: "#374151" }}>
          {provider.api_key_env}
        </div>

        <div style={{ minWidth: 60, padding: "2px 8px" }}>
          {keyStatus === null ? (
            <span style={{ fontSize: 11, color: "#6b7280" }}>...</span>
          ) : keyStatus.set ? (
            <span style={{ fontSize: 11, color: "#107c10", fontWeight: 600 }}>
              {t("apiKeyPanel.set")}
            </span>
          ) : (
            <span style={{ fontSize: 11, color: "var(--error)", fontWeight: 600 }}>
              {t("apiKeyPanel.notSet")}
            </span>
          )}
        </div>
      </div>

      {/* ── Expanded content ──────────────────────────────────────── */}
      {expanded && (
        <>
          {/* Common auth area */}
          <div
            style={{
              background: "#fafafa",
              borderBottom: "1px solid #e5e7eb",
              padding: "10px 16px 10px 24px",
              display: "flex",
              flexDirection: "column",
              gap: 8,
            }}
          >
            {/* Env var name edit */}
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
                {t("apiKeyPanel.envVarLabel")}
              </span>
              <input
                style={{
                  width: 260,
                  padding: "4px 8px",
                  fontSize: 11,
                  fontFamily: "var(--font-mono)",
                  background: "#fff",
                  color: "#1f2937",
                  border: envVarError ? "1px solid var(--error)" : "1px solid #d0d7de",
                  borderRadius: 4,
                  outline: "none",
                }}
                value={envVarName}
                onChange={(e) => {
                  setEnvVarName(e.target.value.toUpperCase());
                  setEnvVarError(null);
                }}
                onBlur={handleEnvVarSave}
                onKeyDown={(e) => { if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur(); }}
                placeholder="OPENROUTER_API_KEY"
                spellCheck={false}
              />
              {envStatusText && (
                <span style={{ fontSize: 10, color: envVarStatus === "error" ? "var(--error)" : "#107c10" }}>
                  {envStatusText}
                </span>
              )}
              {envVarError && (
                <span style={{ fontSize: 10, color: "var(--error)" }}>{envVarError}</span>
              )}
            </div>

            {/* API key input */}
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <span style={{ fontSize: 11, fontWeight: 600, color: "#1f2937", minWidth: 90 }}>
                {t("apiKeyPanel.header")}
              </span>
              <input
                type="password"
                style={{
                  width: 300,
                  padding: "4px 8px",
                  fontSize: 11,
                  fontFamily: "var(--font-mono)",
                  background: "#fff",
                  color: "#1f2937",
                  border: "1px solid #d0d7de",
                  borderRadius: 4,
                  outline: "none",
                }}
                value={keyText}
                onChange={(e) => setKeyText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void handleKeySave();
                  }
                }}
                placeholder="sk-..."
                spellCheck={false}
              />
              <button
                className="btn btn-primary btn-small"
                onClick={handleKeySave}
                disabled={!keyText.trim() || keyStatusLocal === "saving"}
              >
                {keyStatusLocal === "saving" ? "..." : t("apiKeyPanel.saveKey")}
              </button>
              <button
                type="button"
                className="btn btn-secondary btn-small"
                onClick={handleRefreshOpenRouterModels}
                disabled={refreshingOpenRouterModels}
                style={{ whiteSpace: "nowrap" }}
              >
                {refreshingOpenRouterModels
                  ? t("openRouterModels.refreshing")
                  : t("openRouterModels.refresh")}
              </button>
              {keyStatusText && (
                <span style={{ fontSize: 10, color: keyStatusLocal === "error" ? "var(--error)" : "#107c10" }}>
                  {keyStatusText}
                </span>
              )}
            </div>
          </div>

          {/* ── Model set cards ──────────────────────────────────── */}
          {profiles.map((profile) => (
            <OpenRouterModelSetCard
              key={profile.id}
              provider={provider}
              profile={profile}
              displayName={displayModelSetName(profile, modelPrefix)}
              profilesCount={profiles.length}
              gatewayRunning={gatewayRunning}
              refreshConfig={refreshConfig}
              restartGateway={restartGateway}
            />
          ))}

          {/* ── Add Model Set button ─────────────────────────────── */}
          <div style={{
            borderTop: "1px solid #e5e7eb",
            borderBottom: "1px solid #e5e7eb",
            padding: "8px 16px",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}>
            <button
              type="button"
              className="btn btn-secondary btn-small"
              onClick={onAddModelSet}
            >
              + {t("openRouterProfile.addProfile")}
            </button>
            {addError && (
              <span style={{ fontSize: 10, color: "var(--error)" }}>{addError}</span>
            )}
          </div>
        </>
      )}
    </div>
  );
}
