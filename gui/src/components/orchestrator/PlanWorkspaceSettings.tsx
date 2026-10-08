import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  OrchestratorConfig,
  PlanContext,
  PlanNewPreviewResponse,
  PlanNewConfirmResponse,
  PlanAppendResponse,
  PlanWorkspaceConfig,
} from "../../types/orchestrator";

interface Props {
  config: OrchestratorConfig;
  projectPath?: string;
  onChange: (config: OrchestratorConfig) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}

export function formatPlanWorkspaceError(
  err: unknown,
  t: (key: string, params?: Record<string, string | number>) => string
): string {
  let code: string | null = null;
  if (typeof err === "object" && err !== null && "code" in err && typeof (err as { code: unknown }).code === "string") {
    code = (err as { code: string }).code;
  } else if (typeof err === "string") {
    try {
      const parsed = JSON.parse(err);
      if (typeof parsed === "object" && parsed !== null && typeof parsed.code === "string") {
        code = parsed.code;
      }
    } catch {
      const match = err.match(/^([a-z_0-9]+):/);
      if (match) {
        code = match[1];
      }
    }
  }

  if (code) {
    const key = `orchestrator.planWorkspace.errors.${code}`;
    const translated = t(key);
    if (translated && translated !== key) {
      return translated;
    }
  }

  const genericKey = "orchestrator.planWorkspace.errors.generic";
  const genericTranslated = t(genericKey);
  return genericTranslated && genericTranslated !== genericKey
    ? genericTranslated
    : "An error occurred in Plan Workspace.";
}

export function formatPlanWorkspaceWarning(
  code: string | undefined,
  t: (key: string, params?: Record<string, string | number>) => string
): string {
  if (code) {
    const key = `orchestrator.planWorkspace.warnings.${code}`;
    const translated = t(key);
    if (translated && translated !== key) {
      return translated;
    }
  }

  const genericKey = "orchestrator.planWorkspace.warnings.generic";
  const genericTranslated = t(genericKey);
  return genericTranslated && genericTranslated !== genericKey
    ? genericTranslated
    : "Warning: Post-publication cleanup incomplete.";
}

export function PlanWorkspaceSettings({ config, projectPath, onChange, t }: Props) {
  const [planContext, setPlanContext] = useState<PlanContext | null>(null);
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);
  const [cleanupWarning, setCleanupWarning] = useState<{
    message: string;
    leftoverPath?: string;
  } | null>(null);

  // Preview / Confirm Modal State
  const [previewModalOpen, setPreviewModalOpen] = useState(false);
  const [previewData, setPreviewData] = useState<PlanNewPreviewResponse | null>(null);
  const [planTitle, setPlanTitle] = useState("");
  const [initialContent, setInitialContent] = useState("");

  // Append Modal State
  const [appendModalOpen, setAppendModalOpen] = useState(false);
  const [targetPlanId, setTargetPlanId] = useState("");
  const [sectionType, setSectionType] = useState("append_section");
  const [sectionTitle, setSectionTitle] = useState("");
  const [sectionContent, setSectionContent] = useState("");

  const planWorkspaceConfig: PlanWorkspaceConfig = config.planWorkspace ?? {};

  const fetchContext = useCallback(async () => {
    if (!projectPath) {
      setPlanContext(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const ctx = await invoke<PlanContext>("get_plan_context", {
        projectPath,
        config: config.planWorkspace,
      });
      setPlanContext(ctx);
    } catch (err) {
      setError(formatPlanWorkspaceError(err, t));
    } finally {
      setLoading(false);
    }
  }, [projectPath, config.planWorkspace, t]);

  useEffect(() => {
    fetchContext();
  }, [fetchContext]);

  const updatePlanConfig = (patch: Partial<PlanWorkspaceConfig>) => {
    const updated: PlanWorkspaceConfig = {
      ...planWorkspaceConfig,
      ...patch,
    };
    onChange({
      ...config,
      planWorkspace: updated,
    });
  };

  const handlePreviewNewPlan = async () => {
    if (!projectPath) return;
    setActionLoading(true);
    setError(null);
    setCleanupWarning(null);
    try {
      const preview = await invoke<PlanNewPreviewResponse>("preview_new_plan", {
        projectPath,
        config: config.planWorkspace,
      });
      setPreviewData(preview);
      setPlanTitle("");
      setInitialContent("");
      setPreviewModalOpen(true);
    } catch (err) {
      setError(formatPlanWorkspaceError(err, t));
    } finally {
      setActionLoading(false);
    }
  };

  const handleConfirmCreatePlan = async () => {
    if (!projectPath || !previewData) return;
    setActionLoading(true);
    setError(null);
    setCleanupWarning(null);
    try {
      const res = await invoke<PlanNewConfirmResponse>("confirm_new_plan", {
        projectPath,
        request: {
          token: previewData.token,
          title: planTitle.trim() || "Implementation Plan",
          initialContent: initialContent.trim() || "# Implementation Plan\n\nInitial draft.",
        },
        config: config.planWorkspace,
      });
      setSuccessMsg(t("orchestrator.planWorkspace.successCreated", { id: res.createdPlanId }));
      if (res.cleanupWarningCode) {
        setCleanupWarning({
          message: formatPlanWorkspaceWarning(res.cleanupWarningCode, t),
          leftoverPath: res.leftoverTempPath,
        });
      }
      setPreviewModalOpen(false);
      setPreviewData(null);
      await fetchContext();
    } catch (err) {
      setError(formatPlanWorkspaceError(err, t));
    } finally {
      setActionLoading(false);
    }
  };

  const handleCancelPreview = () => {
    setPreviewModalOpen(false);
    setPreviewData(null);
  };

  const handleOpenAppend = () => {
    const defaultTarget =
      planContext?.currentLeafPlanId ??
      planContext?.currentPrimaryPlan?.id ??
      "";
    setTargetPlanId(defaultTarget);
    setSectionType("append_section");
    setSectionTitle("");
    setSectionContent("");
    setAppendModalOpen(true);
  };

  const handleAppendSection = async () => {
    if (!projectPath || !targetPlanId) return;
    setActionLoading(true);
    setError(null);
    setCleanupWarning(null);

    // Find expected file digest
    let expectedDigest = "";
    if (planContext?.currentPrimaryPlan && planContext.currentPrimaryPlan.id === targetPlanId) {
      expectedDigest = planContext.currentPrimaryPlan.digest;
    } else if (planContext?.activeSupplementalPlans) {
      const supp = planContext.activeSupplementalPlans.find((s) => s.id === targetPlanId);
      if (supp) expectedDigest = supp.digest;
    }

    try {
      const idempotencyToken = crypto.randomUUID();
      const res = await invoke<PlanAppendResponse>("append_plan_section", {
        projectPath,
        request: {
          targetPlanId,
          expectedFileDigest: expectedDigest,
          sectionType,
          sectionTitle: sectionTitle.trim(),
          sectionContent: sectionContent.trim(),
          idempotencyToken,
        },
        config: config.planWorkspace,
      });
      setSuccessMsg(t("orchestrator.planWorkspace.successAppended", { id: res.planId }));
      setAppendModalOpen(false);
      await fetchContext();
    } catch (err) {
      setError(formatPlanWorkspaceError(err, t));
    } finally {
      setActionLoading(false);
    }
  };

  const handleCancelAppend = () => {
    setAppendModalOpen(false);
  };

  const isResolved = planContext?.resolverStatus === "resolved";
  const unresolvedReason = planContext?.unresolvedReasonCode
    ? t(`orchestrator.planWorkspace.reason.${planContext.unresolvedReasonCode}`) ||
      planContext.unresolvedReasonCode
    : null;

  return (
    <section className="orchestrator-settings-section orchestrator-plan-workspace-settings">
      <div className="orchestrator-plan-workspace-header">
        <h3>{t("orchestrator.planWorkspace.title")}</h3>
        <button
          type="button"
          className="orchestrator-btn orchestrator-btn-sm"
          onClick={fetchContext}
          disabled={loading || !projectPath}
          title={t("orchestrator.planWorkspace.refresh")}
        >
          {loading ? "…" : t("orchestrator.planWorkspace.refresh")}
        </button>
      </div>

      {error && <div className="orchestrator-alert orchestrator-alert-danger" role="alert">{error}</div>}
      {successMsg && (
        <div className="orchestrator-alert orchestrator-alert-success" role="status">
          {successMsg}
        </div>
      )}
      {cleanupWarning && (
        <div className="orchestrator-alert orchestrator-alert-warning" role="status" data-testid="pw-cleanup-warning">
          <div className="orchestrator-plan-warning-message">{cleanupWarning.message}</div>
          {cleanupWarning.leftoverPath && (
            <div className="orchestrator-plan-leftover-path" style={{ marginTop: "4px", fontSize: "0.85em", opacity: 0.9 }}>
              <code>{cleanupWarning.leftoverPath}</code>
            </div>
          )}
        </div>
      )}

      {/* Plan Workspace Configuration Fields */}
      <div className="orchestrator-plan-config-grid">
        <div className="orchestrator-control-item">
          <label htmlFor="pw-plan-dir" className="orchestrator-control-label">
            {t("orchestrator.planWorkspace.planDir")}
          </label>
          <input
            id="pw-plan-dir"
            type="text"
            className="orchestrator-input"
            placeholder=".plan"
            value={planWorkspaceConfig.planDir ?? ""}
            onChange={(e) => updatePlanConfig({ planDir: e.target.value || undefined })}
          />
          <span className="orchestrator-control-help">{t("orchestrator.planWorkspace.planDirDesc")}</span>
        </div>

        <div className="orchestrator-control-item">
          <label htmlFor="pw-filename-template" className="orchestrator-control-label">
            {t("orchestrator.planWorkspace.filenameTemplate")}
          </label>
          <input
            id="pw-filename-template"
            type="text"
            className="orchestrator-input"
            placeholder="V{version}-r{revision}{suffix}.md"
            value={planWorkspaceConfig.filenameTemplate ?? ""}
            onChange={(e) => updatePlanConfig({ filenameTemplate: e.target.value || undefined })}
          />
          <span className="orchestrator-control-help">
            {t("orchestrator.planWorkspace.filenameTemplateDesc")}
          </span>
        </div>

        <div className="orchestrator-control-item">
          <label htmlFor="pw-version-sources" className="orchestrator-control-label">
            {t("orchestrator.planWorkspace.versionSources")}
          </label>
          <input
            id="pw-version-sources"
            type="text"
            className="orchestrator-input"
            placeholder="package.json, gui/src-tauri/Cargo.toml, Cargo.toml"
            value={planWorkspaceConfig.versionSources?.join(", ") ?? ""}
            onChange={(e) => {
              const val = e.target.value;
              const list = val
                .split(",")
                .map((s) => s.trim())
                .filter(Boolean);
              updatePlanConfig({ versionSources: list.length > 0 ? list : undefined });
            }}
          />
          <span className="orchestrator-control-help">
            {t("orchestrator.planWorkspace.versionSourcesDesc")}
          </span>
        </div>

        <div className="orchestrator-control-item">
          <label htmlFor="pw-series-override" className="orchestrator-control-label">
            {t("orchestrator.planWorkspace.planSeriesOverride")}
          </label>
          <input
            id="pw-series-override"
            type="text"
            className="orchestrator-input"
            placeholder="0.24.0"
            value={planWorkspaceConfig.planSeriesVersionOverride ?? ""}
            onChange={(e) =>
              updatePlanConfig({ planSeriesVersionOverride: e.target.value || undefined })
            }
          />
          <span className="orchestrator-control-help">
            {t("orchestrator.planWorkspace.planSeriesOverrideDesc")}
          </span>
        </div>
      </div>

      {/* Plan Resolver Status Card */}
      <div className="orchestrator-plan-status-card">
        <div className="orchestrator-plan-status-header">
          <span className="orchestrator-plan-status-title">
            {t("orchestrator.planWorkspace.statusCardTitle")}
          </span>
          <span
            className={`orchestrator-badge ${
              isResolved ? "orchestrator-badge-success" : "orchestrator-badge-warning"
            }`}
          >
            {isResolved
              ? t("orchestrator.planWorkspace.resolved")
              : t("orchestrator.planWorkspace.unresolved")}
          </span>
        </div>

        {planContext && (
          <div className="orchestrator-plan-status-details">
            {!isResolved && unresolvedReason && (
              <div className="orchestrator-plan-unresolved-alert" role="alert">
                <strong>{t("orchestrator.planWorkspace.unresolvedReason")}:</strong> {unresolvedReason}
              </div>
            )}

            <div className="orchestrator-plan-meta-row">
              <span className="meta-label">{t("orchestrator.planWorkspace.appVersion")}:</span>
              <span className="meta-value">{planContext.applicationVersion || "—"}</span>
              <span className="meta-label">{t("orchestrator.planWorkspace.seriesVersion")}:</span>
              <span className="meta-value">{planContext.planSeriesVersion || "—"}</span>
            </div>

            <div className="orchestrator-plan-meta-row">
              <span className="meta-label">{t("orchestrator.planWorkspace.currentPlan")}:</span>
              <span className="meta-value">
                {planContext.currentPrimaryPlan
                  ? `${planContext.currentPrimaryPlan.id} (${planContext.currentPrimaryPlan.path})`
                  : t("orchestrator.planWorkspace.noPrimaryPlan")}
              </span>
            </div>

            <div className="orchestrator-plan-meta-row">
              <span className="meta-label">{t("orchestrator.planWorkspace.leafPlan")}:</span>
              <span className="meta-value">{planContext.currentLeafPlanId || "—"}</span>
            </div>

            {planContext.effectivePlanDigest && (
              <div className="orchestrator-plan-meta-row">
                <span className="meta-label">{t("orchestrator.planWorkspace.effectiveDigest")}:</span>
                <span className="meta-value digest-code">
                  <code>{planContext.effectivePlanDigest}</code>
                </span>
              </div>
            )}

            {planContext.activeSupplementalPlans && planContext.activeSupplementalPlans.length > 0 && (
              <div className="orchestrator-plan-supplemental-list">
                <span className="meta-label">{t("orchestrator.planWorkspace.activePlans")}:</span>
                <ul>
                  {planContext.activeSupplementalPlans.map((supp) => (
                    <li key={supp.id}>
                      <strong>{supp.id}</strong> ({supp.path}) — <code>{supp.digest.slice(0, 8)}</code>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}

        <div className="orchestrator-plan-actions-row">
          <button
            type="button"
            className="orchestrator-btn orchestrator-btn-primary"
            onClick={handlePreviewNewPlan}
            disabled={actionLoading || !projectPath}
          >
            {t("orchestrator.planWorkspace.previewNew")}
          </button>
          <button
            type="button"
            className="orchestrator-btn"
            onClick={handleOpenAppend}
            disabled={actionLoading || !projectPath || !isResolved}
          >
            {t("orchestrator.planWorkspace.appendSection")}
          </button>
        </div>
      </div>

      {/* Preview & Confirm New Plan Modal */}
      {previewModalOpen && previewData && (
        <div className="orchestrator-modal-overlay" role="dialog" aria-modal="true">
          <div className="orchestrator-modal-content">
            <h4>{t("orchestrator.planWorkspace.previewModal.title")}</h4>
            <div className="orchestrator-modal-body">
              <div className="orchestrator-control-item">
                <span className="meta-label">
                  {t("orchestrator.planWorkspace.previewModal.candidateRevision")}:
                </span>
                <span className="meta-value">r{previewData.candidateRevision}</span>
              </div>
              <div className="orchestrator-control-item">
                <span className="meta-label">
                  {t("orchestrator.planWorkspace.previewModal.candidateFilename")}:
                </span>
                <span className="meta-value">{previewData.candidateFilename}</span>
              </div>
              <div className="orchestrator-control-item">
                <span className="meta-label">
                  {t("orchestrator.planWorkspace.previewModal.candidatePath")}:
                </span>
                <span className="meta-value">{previewData.candidatePath}</span>
              </div>
              <div className="orchestrator-control-item">
                <label htmlFor="pw-modal-title" className="orchestrator-control-label">
                  {t("orchestrator.planWorkspace.previewModal.planTitle")}
                </label>
                <input
                  id="pw-modal-title"
                  type="text"
                  className="orchestrator-input"
                  placeholder="Feature Implementation"
                  value={planTitle}
                  onChange={(e) => setPlanTitle(e.target.value)}
                />
              </div>
              <div className="orchestrator-control-item">
                <label htmlFor="pw-modal-content" className="orchestrator-control-label">
                  {t("orchestrator.planWorkspace.previewModal.initialContent")}
                </label>
                <textarea
                  id="pw-modal-content"
                  className="orchestrator-textarea"
                  rows={6}
                  placeholder="# Implementation Plan&#10;&#10;Details..."
                  value={initialContent}
                  onChange={(e) => setInitialContent(e.target.value)}
                />
              </div>
            </div>
            <div className="orchestrator-modal-actions">
              <button
                type="button"
                className="orchestrator-btn"
                onClick={handleCancelPreview}
                disabled={actionLoading}
              >
                {t("orchestrator.planWorkspace.previewModal.cancelBtn")}
              </button>
              <button
                type="button"
                className="orchestrator-btn orchestrator-btn-primary"
                onClick={handleConfirmCreatePlan}
                disabled={actionLoading}
              >
                {actionLoading ? "…" : t("orchestrator.planWorkspace.previewModal.confirmBtn")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Append Plan Section Modal */}
      {appendModalOpen && (
        <div className="orchestrator-modal-overlay" role="dialog" aria-modal="true">
          <div className="orchestrator-modal-content">
            <h4>{t("orchestrator.planWorkspace.appendModal.title")}</h4>
            <div className="orchestrator-modal-body">
              <div className="orchestrator-control-item">
                <label htmlFor="pw-append-target" className="orchestrator-control-label">
                  {t("orchestrator.planWorkspace.appendModal.targetPlan")}
                </label>
                <select
                  id="pw-append-target"
                  className="orchestrator-select"
                  value={targetPlanId}
                  onChange={(e) => setTargetPlanId(e.target.value)}
                >
                  {planContext?.currentPrimaryPlan && (
                    <option value={planContext.currentPrimaryPlan.id}>
                      {planContext.currentPrimaryPlan.id} (Primary)
                    </option>
                  )}
                  {planContext?.activeSupplementalPlans?.map((supp) => (
                    <option key={supp.id} value={supp.id}>
                      {supp.id} (Supplemental)
                    </option>
                  ))}
                </select>
              </div>
              <div className="orchestrator-control-item">
                <label htmlFor="pw-append-type" className="orchestrator-control-label">
                  {t("orchestrator.planWorkspace.appendModal.sectionType")}
                </label>
                <select
                  id="pw-append-type"
                  className="orchestrator-select"
                  value={sectionType}
                  onChange={(e) => setSectionType(e.target.value)}
                >
                  <option value="append_section">Section (append_section)</option>
                  <option value="append_task_notes">Task Notes (append_task_notes)</option>
                  <option value="append_findings">Findings (append_findings)</option>
                </select>
              </div>
              <div className="orchestrator-control-item">
                <label htmlFor="pw-append-title" className="orchestrator-control-label">
                  {t("orchestrator.planWorkspace.appendModal.sectionTitle")}
                </label>
                <input
                  id="pw-append-title"
                  type="text"
                  className="orchestrator-input"
                  placeholder="Section Title"
                  value={sectionTitle}
                  onChange={(e) => setSectionTitle(e.target.value)}
                />
              </div>
              <div className="orchestrator-control-item">
                <label htmlFor="pw-append-content" className="orchestrator-control-label">
                  {t("orchestrator.planWorkspace.appendModal.sectionContent")}
                </label>
                <textarea
                  id="pw-append-content"
                  className="orchestrator-textarea"
                  rows={6}
                  placeholder="Section markdown content..."
                  value={sectionContent}
                  onChange={(e) => setSectionContent(e.target.value)}
                />
              </div>
            </div>
            <div className="orchestrator-modal-actions">
              <button
                type="button"
                className="orchestrator-btn"
                onClick={handleCancelAppend}
                disabled={actionLoading}
              >
                {t("orchestrator.planWorkspace.appendModal.cancelBtn")}
              </button>
              <button
                type="button"
                className="orchestrator-btn orchestrator-btn-primary"
                onClick={handleAppendSection}
                disabled={actionLoading || !sectionTitle.trim() || !sectionContent.trim()}
              >
                {actionLoading ? "…" : t("orchestrator.planWorkspace.appendModal.appendBtn")}
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
