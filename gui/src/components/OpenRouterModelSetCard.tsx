import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "../i18n";
import type { ProviderConfig, OpenRouterProfile, CommandResponse, ModelEntry } from "../types";
import OpenRouterModelSelector from "./OpenRouterModelSelector";

const COL_STYLE: React.CSSProperties = {
  padding: "6px 10px",
  fontSize: 12,
  color: "#1f2937",
  whiteSpace: "nowrap",
};

const MODEL_TIERS = [
  { modelKey: "claude-opus-5", labelKey: "apiKeyPanel.gatewayPro" },
  { modelKey: "claude-sonnet-5", labelKey: "apiKeyPanel.gatewayFlash" },
  { modelKey: "claude-haiku-4-5", labelKey: "apiKeyPanel.gatewayHaiku" },
] as const;

type OpenRouterModelSetCardProps = {
  provider: ProviderConfig;
  profile: OpenRouterProfile;
  displayName: string;
  profilesCount: number;
  gatewayRunning: boolean;
  refreshConfig: () => Promise<void>;
  restartGateway: () => Promise<void>;
};

export default function OpenRouterModelSetCard({
  provider,
  profile,
  displayName,
  profilesCount,
  gatewayRunning,
  refreshConfig,
  restartGateway,
}: OpenRouterModelSetCardProps) {
  const { t } = useTranslation();

  // ── Rename state ──────────────────────────────────────────────────
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameText, setRenameText] = useState("");
  const renameSubmitRef = useRef(false);
  const renameCancelRef = useRef(false);
  const [profileActionError, setProfileActionError] = useState<string | null>(null);

  // ── Profile mutations ────────────────────────────────────────────

  const doProfileMutation = useCallback(async <T,>(
    command: string,
    args: Record<string, unknown>,
  ): Promise<CommandResponse<T> | null> => {
    setProfileActionError(null);
    let res: CommandResponse<T>;
    try {
      res = await invoke<CommandResponse<T>>(command, args);
    } catch (e) {
      setProfileActionError(`Settings change failed: ${String(e)}`);
      return null;
    }
    try {
      await refreshConfig();
    } catch (e) {
      setProfileActionError(`Saved, but screen reload failed: ${String(e)}`);
      return null;
    }
    if (gatewayRunning && res.restartGateway) {
      try {
        await restartGateway();
      } catch (e) {
        setProfileActionError(`Saved, but gateway restart failed: ${String(e)}`);
      }
    }
    return res;
  }, [refreshConfig, gatewayRunning, restartGateway]);

  const handleDeleteProfile = useCallback(async () => {
    if (profilesCount <= 1) return;
    const msg = t("openRouterProfile.deleteConfirmInactive");
    if (!confirm(msg)) return;
    await doProfileMutation("delete_openrouter_profile", { profileId: profile.id });
  }, [profile.id, profilesCount, t, doProfileMutation]);

  const startRename = useCallback(() => {
    setRenamingId(profile.id);
    setRenameText(displayName);
    renameSubmitRef.current = false;
    renameCancelRef.current = false;
  }, [profile.id, displayName]);

  const submitRename = useCallback(async () => {
    if (renameSubmitRef.current) return;
    const trimmed = renameText.trim();
    if (!trimmed || trimmed === profile.display_name) {
      setRenamingId(null);
      return;
    }
    renameSubmitRef.current = true;
    await doProfileMutation("rename_openrouter_profile", { profileId: profile.id, newName: trimmed });
    setRenamingId(null);
  }, [profile.id, profile.display_name, renameText, doProfileMutation]);

  const cancelRename = useCallback(() => {
    renameCancelRef.current = true;
    setRenamingId(null);
  }, []);

  const handleRenameKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      submitRename();
    } else if (e.key === "Escape") {
      cancelRename();
    }
  }, [submitRename, cancelRename]);

  const handleRenameBlur = useCallback(() => {
    if (renameCancelRef.current) return;
    submitRename();
  }, [submitRename]);

  // ── Visibility checkbox ──────────────────────────────────────────

  const [isUpdatingVisibility, setUpdatingVisibility] = useState(false);

  const handleVisibilityToggle = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    setUpdatingVisibility(true);
    const hidden = !e.target.checked;
    try {
      await invoke("set_openrouter_profile_hidden", { profileId: profile.id, hidden });
      await refreshConfig();
    } catch (err) {
      // Refresh reverts the checkbox on failure
      await refreshConfig();
    } finally {
      setUpdatingVisibility(false);
    }
  }, [profile.id, refreshConfig]);

  // ── Model entry helpers ───────────────────────────────────────────

  const models: Record<string, ModelEntry> | undefined = profile.models;

  return (
    <div>
      {/* ── Header row ──────────────────────────────────────────── */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          background: "#ffffff",
          borderTop: "1px solid #e5e7eb",
          borderBottom: "none",
        }}
      >
        <label
          style={{ ...COL_STYLE, display: "flex", alignItems: "center", gap: 6, cursor: "pointer", userSelect: "none", padding: "6px 4px 6px 8px" }}
          onClick={(e) => e.stopPropagation()}
        >
          <input
            type="checkbox"
            checked={!profile.hidden}
            disabled={isUpdatingVisibility}
            onChange={handleVisibilityToggle}
            onClick={(e) => e.stopPropagation()}
            style={{ cursor: "pointer" }}
          />
          <span style={{ fontSize: 11, color: "#4b5563" }}>{t("openRouterProfile.showOnDashboard")}</span>
        </label>

        <div style={{ ...COL_STYLE, fontWeight: 600, minWidth: 130, fontSize: 13, padding: "6px 4px" }}>
          {renamingId === profile.id ? (
            <input
              style={{
                width: 120,
                padding: "2px 6px",
                fontSize: 12,
                fontFamily: "var(--font-mono)",
                background: "#fff",
                color: "#1f2937",
                border: "1px solid #d0d7de",
                borderRadius: 4,
                outline: "none",
              }}
              value={renameText}
              onChange={(e) => setRenameText(e.target.value)}
              onBlur={handleRenameBlur}
              onKeyDown={handleRenameKeyDown}
              autoFocus
              onClick={(e) => e.stopPropagation()}
              placeholder={t("openRouterProfile.profileNamePlaceholder")}
              spellCheck={false}
            />
          ) : (
            <span
              onDoubleClick={startRename}
              style={{ cursor: "default", userSelect: "none" }}
              title={t("openRouterProfile.renameProfile")}
            >
              {displayName}
            </span>
          )}
        </div>

        <div style={{ ...COL_STYLE, fontFamily: "var(--font-mono)", fontSize: 11, minWidth: 150, color: "#374151" }}>
          {provider.api_key_env}
        </div>

        <div style={{ minWidth: 60, padding: "2px 8px" }} />

        {/* Profile actions */}
        <div style={{ display: "flex", alignItems: "center", gap: 4, paddingRight: 4, flex: 1, justifyContent: "flex-end" }}>
          <button
            type="button"
            className="btn btn-secondary btn-small"
            onClick={(e) => { e.stopPropagation(); startRename(); }}
            style={{ fontSize: 10, padding: "2px 8px" }}
            title={t("openRouterProfile.renameProfile")}
          >
            {t("apiKeyPanel.edit")}
          </button>
        </div>
      </div>

      {/* ── Edit area (always visible) ─────────────────────────────── */}
      <div
        style={{
          background: "#fafafa",
          borderBottom: "1px solid #e5e7eb",
          padding: "10px 16px 10px 24px",
          display: "flex",
          flexDirection: "column",
          gap: 8,
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Profile action error */}
        {profileActionError && (
          <div style={{ fontSize: 10, color: "var(--error)", marginTop: 4 }}>{profileActionError}</div>
        )}

        {/* 3 × OpenRouterModelSelector */}
        {MODEL_TIERS.map(({ modelKey, labelKey }, idx) => (
          <OpenRouterModelSelector
            key={modelKey}
            modelKey={modelKey}
            gatewayModelLabel={t(labelKey)}
            currentUpstream={models?.[modelKey]?.upstream_model ?? ""}
            currentThinkingMode={models?.[modelKey]?.thinking_mode}
            currentReasoningEffort={models?.[modelKey]?.reasoning_effort}
            onSaved={refreshConfig}
            refreshController={idx === 0}
            profileId={profile.id}
            gatewayRunning={gatewayRunning}
            restartGateway={restartGateway}
          />
        ))}

        {/* Delete — shown only while renaming */}
        {renamingId === profile.id && profilesCount > 1 && (
          <div style={{
            marginTop: 8,
            paddingTop: 8,
            borderTop: "1px solid #e5e7eb",
            display: "flex",
            justifyContent: "flex-end",
          }}>
            <button
              type="button"
              className="btn btn-small"
              onClick={(e) => { e.stopPropagation(); handleDeleteProfile(); }}
              style={{
                fontSize: 10,
                padding: "2px 8px",
                background: "#fff",
                color: "#dc2626",
                borderColor: "#fca5a5",
                cursor: "pointer",
              }}
              title={t("openRouterProfile.deleteProfile")}
            >
              {t("openRouterProfile.deleteProfile")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
