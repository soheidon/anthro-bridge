import React, { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "../../i18n";
import type {
  AgentRole,
  LoopIterationLimits,
  OrchestratorConfig,
  OrchestratorProfile,
  OrchestratorQuickSlot,
  ProjectMetadataResponse,
  RoleAssignment,
  ValidationGateConfig,
  OrchestratorStep,
  RunConfigurationSnapshot,
  StartRunResponse,
  PlanWorkspaceConfig,
  McpServerConfig,
  StepProgressEvent,
  RunLogEvent,
  PlanArchiveOptions,
  PlanArchivePreview,
  HumanGateDecision,
  RunRecoverySummary,
  RecoveryPreflight,
  PlanConvergenceRecoveryPreview,
  ConvergenceProgress,
  PlanConvergenceWaitingCandidate,
  PlanConvergenceCommandResult,
  PlanConvergenceCommandError,
} from "../../types/orchestrator";
import {
  parseConvergenceProgress,
  parseConvergenceWaitingCandidate,
} from "./convergencePayload";
import {
  shouldAcceptRunEvent,
  validateActiveRoleCapabilities,
  getActiveRolesForWorkflow,
} from "../../types/orchestrator";
import {
  DEFAULT_ORCHESTRATOR_PROFILES,
  DEFAULT_VALIDATION_GATES,
  DEFAULT_ITERATION_LIMITS,
  DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
  BUILTIN_ORCHESTRATOR_PRESETS,
  getDefaultPresetIdForWorkflow,
  getDefaultRoleAssignments,
  resolvePresetRoleAssignments,
  hasGateConfigChanged,
  mergeSuggestedValidationGates,
} from "../../config/orchestratorPresets";
import "./Orchestrator.css";

import { ProjectSelector } from "./ProjectSelector";
import { WorkflowTabs } from "./WorkflowTabs";
import { QuickProfileRoleCard } from "./QuickProfileRoleCard";
import { ExecutionView, type ExecutionState } from "./ExecutionView";
import { ToggleSwitch } from "../ToggleSwitch";

function defaultPlanArchiveDirectory(projectPath: string): string {
  const trimmed = projectPath.trim();
  if (!trimmed) return "";
  const separator = trimmed.includes("\\") ? "\\" : "/";
  return `${trimmed.replace(/[\\/]+$/, "")}${separator}.plan`;
}

export default function OrchestratorPanel() {
  const { t } = useTranslation();

  // State
  const [projectPath, setProjectPath] = useState<string>("");
  const projectPathRef = useRef<string>("");
  projectPathRef.current = projectPath;
  const setProjectPathSync = useCallback((newPath: string) => {
    projectPathRef.current = newPath;
    setProjectPath(newPath);
  }, []);
  const [customArchiveDirectory, setCustomArchiveDirectory] = useState<string | null>(null);
  const archiveDirectory = customArchiveDirectory ?? defaultPlanArchiveDirectory(projectPath);
  const [archivePreview, setArchivePreview] = useState<PlanArchivePreview | null>(null);
  const [archivePreviewError, setArchivePreviewError] = useState<string | null>(null);
  const [archiveRefreshNonce, setArchiveRefreshNonce] = useState(0);
  const [metadata, setMetadata] = useState<ProjectMetadataResponse | null>(null);
  const [detecting, setDetecting] = useState<boolean>(false);

  // Keep persisted workflow IDs verbatim, including values this UI cannot run.
  const [activeWorkflowId, setActiveWorkflowId] = useState<string>("full_loop");
  const [activePresetId, setActivePresetId] = useState<string>("balanced");
  const planArchiveRequired = activeWorkflowId === "full_loop" || activeWorkflowId === "human_gated_loop" || activeWorkflowId === "plan_only";

  useEffect(() => {
    if (!planArchiveRequired || !projectPath.trim() || !archiveDirectory.trim()) {
      setArchivePreview(null);
      setArchivePreviewError(null);
      return;
    }

    let current = true;
    invoke<PlanArchivePreview>("preview_plan_archive", {
      projectPath,
      archiveDirectory,
    }).then((preview) => {
      if (current) {
        setArchivePreview(preview);
        setArchivePreviewError(null);
      }
    }).catch((error) => {
      if (current) {
        setArchivePreview(null);
        setArchivePreviewError(String(error));
      }
    });

    return () => { current = false; };
  }, [planArchiveRequired, projectPath, archiveDirectory, archiveRefreshNonce]);

  const [profiles, setProfiles] = useState<OrchestratorProfile[]>(DEFAULT_ORCHESTRATOR_PROFILES);
  const [roleAssignments, setRoleAssignments] = useState<Record<AgentRole, RoleAssignment>>(
    () => getDefaultRoleAssignments("balanced")
  );
  const [validationGates, setValidationGates] = useState<ValidationGateConfig[]>(DEFAULT_VALIDATION_GATES);
  const [limits, setLimits] = useState<LoopIterationLimits>(DEFAULT_ITERATION_LIMITS);
  const [quickSlots, setQuickSlots] = useState<OrchestratorQuickSlot[]>(DEFAULT_ORCHESTRATOR_QUICK_SLOTS);
  const [autoValidationEnabled, setAutoValidationEnabled] = useState(false);
  const autoValidationEnabledRef = useRef(false);
  autoValidationEnabledRef.current = autoValidationEnabled;
  const [leanAntigravityMode, setLeanAntigravityMode] = useState<boolean>(false);
  const [planConvergenceOptIn, setPlanConvergenceOptIn] = useState(false);
  const convergenceRunRef = useRef(false);
  const [planWorkspaceConfig, setPlanWorkspaceConfig] = useState<PlanWorkspaceConfig | null>(null);
  const [mcpServers, setMcpServers] = useState<Record<string, McpServerConfig>>({});

  // Execution state
  const [currentRunId, setCurrentRunId] = useState<string | null>(null);
  const currentRunIdRef = useRef<string | null>(null);
  currentRunIdRef.current = currentRunId;
  const [startPending, setStartPending] = useState(false);
  const startPendingRef = useRef(false);

  const [taskPrompt, setTaskPrompt] = useState<string>("");
  const [executionState, setExecutionState] = useState<ExecutionState>("idle");
  const [currentStep, setCurrentStep] = useState<OrchestratorStep | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [verdict, setVerdict] = useState<"READY" | "NOT_READY" | null>(null);
  const [validationIssues, setValidationIssues] = useState<string[]>([]);
  const [planText, setPlanText] = useState<string | null>(null);
  const [runningWorkflowId, setRunningWorkflowId] = useState<string | null>(null);
  const [presetError, setPresetError] = useState<string | null>(null);
  const [antigravityDispatches, setAntigravityDispatches] = useState<number | null>(null);
  const [antigravityDispatchLimit, setAntigravityDispatchLimit] = useState<number | null>(null);
  const [budgetScope, setBudgetScope] = useState<"task" | "run" | null>(null);
  const [waitingReason, setWaitingReason] = useState<string | null>(null);
  const [convergenceProgress, setConvergenceProgress] = useState<ConvergenceProgress | null>(null);
  const [convergenceWaitingCandidate, setConvergenceWaitingCandidate] =
    useState<PlanConvergenceWaitingCandidate | null>(null);
  const [convergenceActionPending, setConvergenceActionPending] = useState(false);
  const [convergenceActionError, setConvergenceActionError] = useState<string | null>(null);
  const [interruptedRuns, setInterruptedRuns] = useState<RunRecoverySummary[]>([]);
  const [recoverySelection, setRecoverySelection] = useState<RecoveryPreflight | null>(null);
  const [convergenceRecoveryPreview, setConvergenceRecoveryPreview] = useState<PlanConvergenceRecoveryPreview | null>(null);
  const [recoveryBusy, setRecoveryBusy] = useState(false);
  const [recoveryError, setRecoveryError] = useState<string | null>(null);
  const [recoveryConfirmation, setRecoveryConfirmation] = useState<"restore" | "adopt" | "resume" | "convergence" | null>(null);

  useEffect(() => {
    let mounted = true;
    // Listing is intentionally summary-only; fingerprinting begins only after
    // the user opens a specific interrupted run.
    void invoke<RunRecoverySummary[]>("list_interrupted_runs")
      .then((runs) => { if (mounted) setInterruptedRuns(runs.filter((run) => run.status === "interrupted" || run.status === "active" || (run.workflowType === "plan_convergence" && run.status === "failed"))); })
      .catch((error) => { if (mounted) setRecoveryError(String(error)); });
    return () => { mounted = false; };
  }, []);

  const isRunActive =
    executionState === "running" ||
    executionState === "paused" ||
    executionState === "waiting_for_user" ||
    executionState === "waiting_for_blocking_resolution" ||
    startPending;

  const displayWorkflowId = runningWorkflowId ?? activeWorkflowId;

  const clearRunPresentation = useCallback(() => {
    convergenceRunRef.current = false;
    setRunningWorkflowId(null);
    setCurrentRunId(null);
    setCurrentStep(null);
    setExecutionState("idle");
    setVerdict(null);
    setValidationIssues([]);
    setPlanText(null);
    setAntigravityDispatches(null);
    setAntigravityDispatchLimit(null);
    setBudgetScope(null);
    setWaitingReason(null);
    setConvergenceProgress(null);
    setConvergenceWaitingCandidate(null);
    setConvergenceActionError(null);
  }, []);

  const detectionGenerationRef = useRef(0);
  const latestDetectionPathRef = useRef<string>("");

  const detectProjectMetadata = useCallback(async (targetPath: string) => {
    const trimmedPath = targetPath.trim();
    if (!trimmedPath) return;

    const generation = ++detectionGenerationRef.current;
    latestDetectionPathRef.current = trimmedPath;
    setDetecting(true);

    try {
      const res = await invoke<ProjectMetadataResponse>("detect_project_metadata", {
        projectPath: trimmedPath,
      });

      // Reject stale response if a newer scan was started or path changed
      if (
        generation !== detectionGenerationRef.current ||
        latestDetectionPathRef.current !== trimmedPath ||
        projectPathRef.current.trim() !== trimmedPath
      ) {
        return;
      }

      setMetadata(res);
      const suggested = res.suggestedGates ?? (res as any).suggested_gates ?? [];
      if (suggested.length > 0 && autoValidationEnabledRef.current) {
        setValidationGates((currentGates) => {
          if (
            generation !== detectionGenerationRef.current ||
            latestDetectionPathRef.current !== trimmedPath ||
            projectPathRef.current.trim() !== trimmedPath
          ) {
            return currentGates;
          }
          const merged = mergeSuggestedValidationGates(currentGates, suggested);
          if (hasGateConfigChanged(currentGates, merged)) {
            void invoke("update_orchestrator_config", { config: { validationGates: merged } });
            return merged;
          }
          return currentGates;
        });
      }
    } catch (e) {
      if (
        generation === detectionGenerationRef.current &&
        latestDetectionPathRef.current === trimmedPath &&
        projectPathRef.current.trim() === trimmedPath
      ) {
        console.error("Failed to detect project metadata:", e);
      }
    } finally {
      if (generation === detectionGenerationRef.current) {
        setDetecting(false);
      }
    }
  }, []);

  // Handle Project Detection triggered by user
  const handleDetect = useCallback(
    async (pathToCheck?: string) => {
      const target = pathToCheck !== undefined ? pathToCheck : projectPath;
      await detectProjectMetadata(target);
    },
    [projectPath, detectProjectMetadata]
  );

  // Load config on mount
  useEffect(() => {
    async function loadConfig() {
      try {
        const cfg = await invoke<OrchestratorConfig>("get_orchestrator_config");
        if (cfg) {
          if (cfg.profiles !== undefined) setProfiles(cfg.profiles);
          const path = cfg.projectPath;
          if (path) {
            setProjectPathSync(path);
            void detectProjectMetadata(path);
          }
          const wf = cfg.activeWorkflowId;
          if (typeof wf === "string" && wf.length > 0) setActiveWorkflowId(wf);
          const pr = cfg.activePresetId;
          if (pr) setActivePresetId(pr);
          const assigns = cfg.assignments;
          if (assigns) setRoleAssignments(assigns);
          const gates = cfg.validationGates;
          if (gates) setValidationGates(gates);
          const lms = cfg.iterationLimits;
          if (lms) setLimits(lms);
          if (cfg.quickSlots !== undefined) setQuickSlots(cfg.quickSlots);
          if (cfg.autoValidationEnabled !== undefined) {
            setAutoValidationEnabled(cfg.autoValidationEnabled);
            autoValidationEnabledRef.current = cfg.autoValidationEnabled;
          }
          if (cfg.leanAntigravityMode !== undefined) {
            setLeanAntigravityMode(cfg.leanAntigravityMode);
          }
          setPlanWorkspaceConfig(cfg.planWorkspace ?? null);
          setMcpServers(cfg.mcpServers ?? {});
        }
      } catch (e) {
        console.error("Failed to load orchestrator config:", e);
      }
    }
    void loadConfig();
  }, [detectProjectMetadata]);

  // Setup Tauri event listeners for runtime progress with stale event rejection
  useEffect(() => {
    let unlistenStep: (() => void) | undefined;
    let unlistenLog: (() => void) | undefined;

    async function setupListeners() {
      try {
        unlistenStep = await listen<StepProgressEvent>("orchestrator:step_update", (event) => {
          const data = event.payload;
          if (!data) return;

          // Reject stale events from previous or mismatched runs
          if (!shouldAcceptRunEvent(data.runId, currentRunIdRef.current, startPendingRef.current)) {
            return;
          }

          const stepStr = data.step;
          if (stepStr === "plan_draft" || stepStr === "plan_generation") setCurrentStep("planning");
          else if (stepStr === "plan_integration") setCurrentStep("plan_integration");
          else if (stepStr === "plan_review" || stepStr === "plan_revision") setCurrentStep("plan_review");
          else if (stepStr === "implementation") setCurrentStep("implementation");
          else if (stepStr === "validation") setCurrentStep("validation");
          else if (stepStr === "code_review") setCurrentStep("code_review");
          else if (stepStr === "fix") setCurrentStep("fixing");
          else if (stepStr === "human_gate") {
            setCurrentStep("human_gate");
            setExecutionState("waiting_for_user");
          } else if (stepStr === "waiting_for_user") {
            setExecutionState("waiting_for_user");
          } else if (stepStr === "waiting_for_blocking_resolution") {
            setExecutionState("waiting_for_blocking_resolution");
          } else if (stepStr === "complete") {
            setCurrentStep("completed");
            setExecutionState("completed");
            setArchiveRefreshNonce((value) => value + 1);
          } else if (stepStr === "failed") {
            setExecutionState("failed");
          } else if (stepStr === "paused") {
            setExecutionState("paused");
          }

          if (data.planText !== undefined && data.planText !== null) {
            setPlanText(data.planText);
          }

          if (convergenceRunRef.current) {
            // Typed live progress and the candidate-bound human gate payload are
            // decoded from their documented encodings; anything malformed is
            // ignored rather than rendered as free-form backend text.
            const progress = parseConvergenceProgress(data.iterationInfo);
            if (progress) setConvergenceProgress(progress);

            const candidate = parseConvergenceWaitingCandidate(data.planText);
            if (candidate) setConvergenceWaitingCandidate(candidate);
          }

          if (data.antigravityDispatches !== undefined && data.antigravityDispatches !== null) {
            setAntigravityDispatches(data.antigravityDispatches);
          }
          if (data.antigravityDispatchLimit !== undefined && data.antigravityDispatchLimit !== null) {
            setAntigravityDispatchLimit(data.antigravityDispatchLimit);
          }
          if (data.budgetScope !== undefined) {
            setBudgetScope(data.budgetScope);
          }
          if (data.waitingReason !== undefined) {
            setWaitingReason(data.waitingReason ?? null);
          }

          // Budget exhaustion is already shown in the localized budget panel;
          // never surface a backend-generated English sentence in the event log.
          if (data.message && !data.budgetScope && !convergenceRunRef.current) {
            setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${data.message}`]);
          }
          if (convergenceRunRef.current) {
            const key = stepStr === "failed" ? "orchestrator.planConvergence.failed"
              : stepStr === "complete" ? "orchestrator.planConvergence.complete"
              : stepStr === "waiting_for_user" ? "orchestrator.planConvergence.waiting"
              : stepStr === "cancelled" ? "orchestrator.exec.cancelBtn" : null;
            if (key) setLogs((prev) => [...prev, t(key)]);
            if (stepStr === "cancelled") setExecutionState("cancelled");
          }

          if (data.reviewResult) {
            if (data.reviewResult.verdict === "approved") {
              setVerdict("READY");
            } else if (data.reviewResult.verdict === "changes_required") {
              setVerdict("NOT_READY");
            }
          }

          if (data.validationSummary) {
            if (!data.validationSummary.passed) {
              setValidationIssues(data.validationSummary.failedGateNames || []);
            } else {
              setValidationIssues([]);
            }
          }
        });

        unlistenLog = await listen<RunLogEvent>("orchestrator:log", (event) => {
          const data = event.payload;
          if (data && !convergenceRunRef.current && shouldAcceptRunEvent(data.runId, currentRunIdRef.current, startPendingRef.current)) {
            setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${data.message}`]);
          }
        });
      } catch (e) {
        // Mock fallback in non-tauri or test environments
      }
    }

    void setupListeners();

    return () => {
      if (unlistenStep) unlistenStep();
      if (unlistenLog) unlistenLog();
    };
  }, []);

  // Save config helper
  const saveConfig = useCallback(
    async (
      updatedPath: string,
      updatedWorkflow: string,
      updatedPreset: string,
      updatedAssignments: Record<AgentRole, RoleAssignment>,
      updatedGates: ValidationGateConfig[],
      updatedLimits: LoopIterationLimits,
      updatedLeanMode?: boolean
    ) => {
      try {
        const payload: OrchestratorConfig = {
          projectPath: updatedPath,
          activeWorkflowId: updatedWorkflow,
          activePresetId: updatedPreset,
          assignments: updatedAssignments,
          validationGates: updatedGates,
          iterationLimits: updatedLimits,
          leanAntigravityMode: updatedLeanMode !== undefined ? updatedLeanMode : leanAntigravityMode,
        };
        await invoke("update_orchestrator_config", { config: payload });
      } catch (e) {
        console.error("Failed to save orchestrator config:", e);
      }
    },
    [leanAntigravityMode]
  );

  const handleToggleLeanMode = useCallback(
    (checked: boolean) => {
      setLeanAntigravityMode(checked);
      void saveConfig(projectPath, activeWorkflowId, activePresetId, roleAssignments, validationGates, limits, checked);
    },
    [projectPath, activeWorkflowId, activePresetId, roleAssignments, validationGates, limits, saveConfig]
  );

  // Handle Role Assignment change
  const handleChangeRoleProfile = useCallback(
    (role: AgentRole, profileId: string) => {
      setPresetError(null);
      const next = {
        ...roleAssignments,
        [role]: {
          ...roleAssignments[role],
          profileId,
        },
      };
      setRoleAssignments(next);
      setActivePresetId("custom");
      void saveConfig(projectPath, activeWorkflowId, "custom", next, validationGates, limits);
    },
    [roleAssignments, projectPath, activeWorkflowId, validationGates, limits, saveConfig]
  );

  const handleApplySuggestedGates = useCallback(
    (gates: ValidationGateConfig[]) => {
      const merged = mergeSuggestedValidationGates(validationGates, gates);
      setValidationGates(merged);
      void saveConfig(projectPath, activeWorkflowId, activePresetId, roleAssignments, merged, limits);
    },
    [projectPath, activeWorkflowId, activePresetId, roleAssignments, validationGates, limits, saveConfig]
  );

  const handleApplyDefaultPreset = useCallback(() => {
    const presetId = getDefaultPresetIdForWorkflow(activeWorkflowId);
    const result = resolvePresetRoleAssignments(presetId, profiles, activeWorkflowId);
    if (!result.success || !result.assignments) {
      setPresetError(t("orchestrator.presets.presetApplyError"));
      return;
    }
    setPresetError(null);
    const newAssignments = result.assignments;
    const newLimits = result.iterationLimits ?? DEFAULT_ITERATION_LIMITS;
    setRoleAssignments(newAssignments);
    setActivePresetId(presetId);
    setLimits(newLimits);
    void saveConfig(projectPath, activeWorkflowId, presetId, newAssignments, validationGates, newLimits);
  }, [activeWorkflowId, profiles, projectPath, validationGates, saveConfig, t]);

  const activeRoles = useMemo(() => planConvergenceOptIn
    ? ["planner", "plan_reviewer"] as AgentRole[]
    : getActiveRolesForWorkflow(activeWorkflowId), [activeWorkflowId, planConvergenceOptIn]);

  // Capability Validation scoped to active roles
  const validationErrors = useMemo(() => {
    const roleProfileMap: Record<string, string> = {};
    for (const [r, assign] of Object.entries(roleAssignments)) {
      roleProfileMap[r] = assign.profileId;
    }
    return validateActiveRoleCapabilities(planConvergenceOptIn ? "plan_only" : activeWorkflowId, roleProfileMap, profiles);
  }, [activeWorkflowId, roleAssignments, profiles, planConvergenceOptIn]);

  const isKnownWorkflow = useMemo(
    () => (["full_loop", "human_gated_loop", "plan_only", "implement_only", "review_only"] as string[]).includes(activeWorkflowId),
    [activeWorkflowId]
  );

  const canStart = useMemo(() => {
    if (!isKnownWorkflow || !projectPath.trim() || (metadata && !metadata.exists)) return false;
    if (validationErrors.length > 0) return false;
    return true;
  }, [isKnownWorkflow, projectPath, metadata, validationErrors]);

  const disabledReason = useMemo(() => {
    if (!isKnownWorkflow) return t("orchestrator.validation.workflowUnavailable");
    if (!projectPath.trim()) return t("orchestrator.validation.noProjectPath") || "Please select a valid project directory.";
    if (metadata && !metadata.exists) return t("orchestrator.validation.pathNotFound") || "Project directory does not exist.";
    if (validationErrors.length > 0) return validationErrors[0].message;
    return undefined;
  }, [isKnownWorkflow, projectPath, metadata, validationErrors, t]);

  // Execution Handlers with Tauri backend integration
  const handleStart = async () => {
    if (!canStart || startPendingRef.current) return;
    const assignmentsMap = {} as Record<AgentRole, OrchestratorProfile>;
    for (const r of (planConvergenceOptIn ? ["planner", "plan_reviewer"] as AgentRole[] : activeRoles)) {
      const assign = roleAssignments[r];
      const prof = profiles.find((p) => p.id === assign?.profileId);
      if (!prof) return;
      assignmentsMap[r] = prof;
    }

    const workflowForRun = activeWorkflowId;
    convergenceRunRef.current = planConvergenceOptIn;
    startPendingRef.current = true;
    setStartPending(true);
    try {
      const planArchiveOptions: PlanArchiveOptions | null =
        workflowForRun === "full_loop" || workflowForRun === "human_gated_loop" || workflowForRun === "plan_only"
          ? { directory: archiveDirectory }
          : null;

      setRunningWorkflowId(workflowForRun);
      currentRunIdRef.current = null;
      setCurrentRunId(null);
      setExecutionState("running");
      const initialStep: OrchestratorStep =
        workflowForRun === "review_only"
          ? "code_review"
          : workflowForRun === "implement_only"
          ? "implementation"
          : "planning";
      setCurrentStep(initialStep);
      setVerdict(null);
      setValidationIssues([]);
      setLogs([planConvergenceOptIn ? t("orchestrator.planConvergence.optIn")
        : `[${new Date().toLocaleTimeString()}] Initializing Orchestration run for project: ${projectPath}`]);

      const snapshot: RunConfigurationSnapshot = {
        projectPath,
        assignments: assignmentsMap,
        iterationLimits: limits,
        validationGates,
        budgetLimits: {},
        createdAtUnix: Math.floor(Date.now() / 1000),
        leanAntigravityMode,
        ...(planWorkspaceConfig ? { planWorkspace: planWorkspaceConfig } : {}),
        mcpServers,
      };

      const res = planConvergenceOptIn
        ? await invoke<StartRunResponse>("start_plan_convergence", {
            snapshot, taskPrompt,
            convergenceConfig: { optIn: true, totalTimeoutSecs: 900 },
            workspaceConfig: planWorkspaceConfig,
          })
        : await invoke<StartRunResponse>("start_orchestrator_run", {
            snapshot, taskPrompt, workflowType: workflowForRun, planArchiveOptions,
          });
      currentRunIdRef.current = res.runId;
      setCurrentRunId(res.runId);
    } catch (e: any) {
      convergenceRunRef.current = false;
      currentRunIdRef.current = null;
      setCurrentRunId(null);
      clearRunPresentation();
      setLogs((prev) => [...prev, planConvergenceOptIn
        ? t("orchestrator.planConvergence.failed")
        : `[${new Date().toLocaleTimeString()}] Error: ${e?.message || e}`]);
    } finally {
      startPendingRef.current = false;
      setStartPending(false);
    }
  };

  const handlePause = async () => {
    if (!currentRunId) return;
    try {
      await invoke("pause_orchestrator_run", { runId: currentRunId });
      setExecutionState("paused");
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Pause signal sent.`]);
    } catch (e) {
      console.error("Failed to pause:", e);
    }
  };

  const handleResume = async () => {
    if (!currentRunId) return;
    try {
      await invoke("resume_orchestrator_run", { runId: currentRunId });
      setExecutionState("running");
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Resume signal sent.`]);
    } catch (e) {
      console.error("Failed to resume:", e);
    }
  };

  const handleCancel = async () => {
    if (!currentRunId) return;
    // Budget exhaustion ends the worker and releases ActiveRun in the supervisor.
    // Dismiss this terminal waiting presentation locally rather than cancelling
    // a run that is no longer active in the backend.
    if (executionState === "waiting_for_user" && budgetScope !== null) {
      setExecutionState("cancelled");
      setCurrentStep(null);
      return;
    }
    try {
      await invoke("cancel_orchestrator_run", { runId: currentRunId });
      setExecutionState("cancelled");
      setCurrentStep(null);
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Cancel signal sent.`]);
    } catch (e) {
      console.error("Failed to cancel:", e);
    }
  };

  const handleSubmitClarification = async (response: string) => {
    if (!currentRunId) return;
    try {
      await invoke("submit_clarification_response", { runId: currentRunId, response });
      setExecutionState("running");
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Clarification submitted.`]);
    } catch (e: any) {
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Failed to submit clarification: ${e?.message || e}`]);
    }
  };

  const handleResolveBlocking = async (action: "retry" | "abort", guidance?: string) => {
    if (!currentRunId) return;
    try {
      await invoke("resolve_blocking_finding", {
        runId: currentRunId,
        action,
        guidance: guidance || null,
      });
      if (action === "retry") {
        setExecutionState("running");
        setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Retrying with user guidance.`]);
      } else {
        setExecutionState("failed");
        setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Run aborted by user.`]);
      }
    } catch (e: any) {
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Failed to resolve blocking finding: ${e?.message || e}`]);
    }
  };

  const handleResolveHumanGate = async (decision: HumanGateDecision) => {
    if (!currentRunId) return;
    try {
      await invoke("resolve_human_gate", { runId: currentRunId, decision });
      if (decision.type === "approve") {
        setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Human operator approved changes.`]);
      } else if (decision.type === "request_changes") {
        setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Human operator requested changes: ${decision.feedback}`]);
        setExecutionState("running");
      } else {
        setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Human operator aborted run.`]);
        setExecutionState("cancelled");
      }
    } catch (e: any) {
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Failed to resolve human gate: ${e?.message || e}`]);
    }
  };

  const handleConfirmWorkerStopped = async () => {
    if (!currentRunId) return;
    try {
      await invoke("confirm_worker_stopped_and_reclaim", { runId: currentRunId });
      setExecutionState("running");
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Worker stopped confirmed. Claim re-opened.`]);
    } catch (e: any) {
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Failed to confirm worker stopped: ${e?.message || e}`]);
    }
  };

  const handleConfirmConvergedNewPlan = async (action: "confirm" | "reject") => {
    const candidate = convergenceWaitingCandidate;
    // The action is bound to the run and candidate the operator actually saw.
    // A candidate from a different run, or one that no longer belongs to the
    // current run, is refused instead of being applied to the wrong state.
    if (!candidate || !currentRunId || candidate.runId !== currentRunId) {
      setConvergenceActionError(t("orchestrator.planConvergence.waiting.staleError"));
      return;
    }
    setConvergenceActionPending(true);
    setConvergenceActionError(null);
    try {
      const result = await invoke<PlanConvergenceCommandResult>("confirm_converged_new_plan", {
        runId: candidate.runId,
        expectedRevision: candidate.revision,
        candidateId: candidate.candidateId,
        action,
      });
      if (result.kind === "created") {
        setLogs((prev) => [...prev, t("orchestrator.planConvergence.newPlanCreated")]);
        setExecutionState("completed");
      } else {
        setLogs((prev) => [...prev, t("orchestrator.planConvergence.newPlanRejected")]);
        setExecutionState("cancelled");
      }
      setConvergenceWaitingCandidate(null);
    } catch (error) {
      const typed = error as PlanConvergenceCommandError;
      // A confirmation refused because another run owns the active-run boundary
      // is a stale action, not a plan-workspace failure.
      setConvergenceActionError(
        typed?.code === "PC_STALE_REVISION" ||
          typed?.code === "PC_CANDIDATE_MISMATCH" ||
          typed?.code === "PC_RUN_ALREADY_RESOLVED" ||
          typed?.code === "PC_CONFLICT_RUN_ACTIVE"
          ? t("orchestrator.planConvergence.waiting.staleError")
          : t("orchestrator.planConvergence.waiting.confirmError"),
      );
    } finally {
      setConvergenceActionPending(false);
    }
  };

  const handleReset = () => {
    clearRunPresentation();
    setLogs([]);
  };

  const inspectRecovery = async (runId: string) => {
    setRecoveryBusy(true);
    setRecoveryError(null);
    setConvergenceRecoveryPreview(null);
    setRecoverySelection(null);
    try {
      const run = interruptedRuns.find((item) => item.runId === runId);
      if (run?.workflowType === "plan_convergence") {
        const preview = await invoke<PlanConvergenceRecoveryPreview>("preflight_plan_convergence_recovery", { runId });
        setConvergenceRecoveryPreview(preview);
        setRecoverySelection(null);
        return;
      }
      const preflight = await invoke<RecoveryPreflight>("preflight_run_recovery", { runId });
      setRecoverySelection(preflight);
      setConvergenceRecoveryPreview(null);
    } catch (error) {
      setRecoveryError(String(error));
      setRecoverySelection(null);
    } finally { setRecoveryBusy(false); }
  };

  const resolveRecovery = async (choice: "restore" | "adopt" | "resume" | "cancel" | "convergence") => {
    if (choice === "convergence") {
      if (!convergenceRecoveryPreview) return;
      setRecoveryBusy(true);
      setRecoveryError(null);
      try {
        const result = await invoke<{ runId: string }>("reconcile_plan_convergence_acceptance", {
          runId: convergenceRecoveryPreview.runId,
          expectedRevision: convergenceRecoveryPreview.journalRevision,
        });
        setCurrentRunId(result.runId);
        setExecutionState("completed");
        const runs = await invoke<RunRecoverySummary[]>("list_interrupted_runs");
        setInterruptedRuns(runs.filter((run) => run.status === "interrupted" || run.status === "active" || (run.workflowType === "plan_convergence" && run.status === "failed")));
        setConvergenceRecoveryPreview(null);
        setRecoveryConfirmation(null);
      } catch (error) { setRecoveryError(String(error)); }
      finally { setRecoveryBusy(false); }
      return;
    }
    if (!recoverySelection) return;
    setRecoveryBusy(true);
    setRecoveryError(null);
    try {
      if (choice === "resume") {
        const started = await invoke<{ runId: string }>("resume_interrupted_run", {
          runId: recoverySelection.runId,
          expectedRevision: recoverySelection.journalRevision,
          expectedFingerprint: recoverySelection.currentFingerprint,
          confirmedWorkerStopped: true,
        });
        setCurrentRunId(started.runId);
        setExecutionState("running");
        setRecoverySelection(null);
        setRecoveryConfirmation(null);
        return;
      }
      await invoke("resolve_run_recovery", {
        runId: recoverySelection.runId, choice,
        expectedRevision: recoverySelection.journalRevision,
        expectedFingerprint: recoverySelection.currentFingerprint,
        expectedBackupId: recoverySelection.backupId,
        confirmed: choice === "restore" || choice === "adopt",
      });
      if (choice === "cancel") { setRecoverySelection(null); setRecoveryConfirmation(null); }
      if (choice === "adopt") {
        const runs = await invoke<RunRecoverySummary[]>("list_interrupted_runs");
        setInterruptedRuns(runs.filter((run) => run.status === "interrupted" || run.status === "active" || (run.workflowType === "plan_convergence" && run.status === "failed")));
        setRecoverySelection(null);
        setRecoveryConfirmation(null);
      }
      if (choice === "restore") { setRecoveryConfirmation(null); await inspectRecovery(recoverySelection.runId); }
    } catch (error) { setRecoveryError(String(error)); }
    finally { setRecoveryBusy(false); }
  };

  const rolesList: AgentRole[] = ["planner", "plan_integrator", "plan_reviewer", "implementer", "fixer", "code_reviewer"];

  return (
    <div className="orchestrator-panel-container">
      <div className="orchestrator-workspace-flow">
          <ProjectSelector
            projectPath={projectPath}
            onProjectPathChange={(p) => {
              const trimmed = p.trim();
              detectionGenerationRef.current += 1;
              latestDetectionPathRef.current = trimmed;
              setDetecting(false);
              setMetadata(null);
              setCustomArchiveDirectory(null);
              setProjectPathSync(p);
              setArchivePreview(null);
              void saveConfig(p, activeWorkflowId, activePresetId, roleAssignments, validationGates, limits);
            }}
            metadata={metadata}
            onDetect={handleDetect}
            detecting={detecting}
            archiveDirectory={archiveDirectory}
            nextArchiveFileName={archivePreview?.nextFileName ?? null}
            archivePreviewError={archivePreviewError}
            showPlanArchive={planArchiveRequired}
            onArchiveDirectoryChange={(path) => {
              setCustomArchiveDirectory(path);
              setArchivePreview(null);
            }}
            onApplySuggestedGates={handleApplySuggestedGates}
            autoValidationEnabled={autoValidationEnabled}
            t={t}
          />

          {interruptedRuns.length > 0 && <section className="orchestrator-card" aria-labelledby="orchestrator-recovery-title">
            <h3 id="orchestrator-recovery-title" className="orchestrator-card-title">{t("orchestrator.recovery.title")}</h3>
            <p className="orchestrator-desc">{t("orchestrator.recovery.description")}</p>
            {interruptedRuns.map((run) => <div className="orchestrator-recovery-row" key={run.runId}>
              <span>{run.workflowType} · {run.currentState} · {run.runId}</span>
              <button type="button" className="orchestrator-button-secondary" disabled={recoveryBusy || isRunActive} onClick={() => void inspectRecovery(run.runId)}>
                {recoveryBusy ? t("orchestrator.recovery.loading") : t("orchestrator.recovery.inspect")}
              </button>
            </div>)}
            {recoveryError && <p role="alert" className="orchestrator-desc">{t("orchestrator.recovery.error")}</p>}
            {convergenceRecoveryPreview && <div className="orchestrator-recovery-detail" role="region" aria-label={t("orchestrator.planConvergence.recovery.title")}>
              <h4>{t("orchestrator.planConvergence.recovery.title")}</h4>
              <p>{t("orchestrator.planConvergence.recovery.description")}</p>
              <p>{t("orchestrator.planConvergence.recovery.candidate")}: <code>{convergenceRecoveryPreview.candidateId}</code> · {t("orchestrator.planConvergence.recovery.sequence")}: {convergenceRecoveryPreview.sequence}</p>
              <p>{t("orchestrator.planConvergence.recovery.target")}: <code>{convergenceRecoveryPreview.targetPlanId}</code></p>
              <p>{t("orchestrator.planConvergence.recovery.section")}: {convergenceRecoveryPreview.sectionTitle}</p>
              <pre className="orchestrator-recovery-plan-content">{convergenceRecoveryPreview.sectionContent}</pre>
              <p>{t("orchestrator.planConvergence.recovery.contextDigest")}: <code>{convergenceRecoveryPreview.planContextDigest}</code></p>
              <div className="orchestrator-recovery-actions">
                <button type="button" disabled={recoveryBusy} onClick={() => { setConvergenceRecoveryPreview(null); setRecoveryConfirmation(null); }}>{t("orchestrator.recovery.cancel")}</button>
                {convergenceRecoveryPreview.mayApplyUnpublishedAppend && <button type="button" disabled={recoveryBusy || isRunActive} onClick={() => setRecoveryConfirmation("convergence")}>{t("orchestrator.planConvergence.recovery.action")}</button>}
              </div>
            </div>}
            {recoverySelection && <div className="orchestrator-recovery-detail" role="region" aria-label={t("orchestrator.recovery.details")}>
              <p>{t("orchestrator.recovery.stage")}: {recoverySelection.checkpointStage} ({recoverySelection.checkpointKind})</p>
              <p>{recoverySelection.workspaceMatches ? t("orchestrator.recovery.match") : t("orchestrator.recovery.drift")}</p>
              <p>{t("orchestrator.recovery.affectedPaths")}: {recoverySelection.affectedPaths.length}</p>
              {recoverySelection.affectedPaths.length > 0 && <ul aria-label={t("orchestrator.recovery.affectedPathsList")}>
                {recoverySelection.affectedPaths.map((path) => <li key={path}><code>{path}</code></li>)}
              </ul>}
              {recoverySelection.canRestore && <>
                <p>{t("orchestrator.recovery.backupDestination")}: <code>{recoverySelection.backupDestination}</code></p>
                <p>{t("orchestrator.recovery.restoreScope")}</p>
              </>}
              {recoverySelection.reasonCode && <p>{t("orchestrator.recovery.blocked")}: {recoverySelection.reasonCode}</p>}
              <div className="orchestrator-recovery-actions">
                {recoverySelection.canResume && <button type="button" disabled={recoveryBusy || isRunActive} onClick={() => setRecoveryConfirmation("resume")}>{t("orchestrator.recovery.resume")}</button>}
                <button type="button" disabled={!recoverySelection.canRestore || recoveryBusy} onClick={() => setRecoveryConfirmation("restore")}>{t("orchestrator.recovery.restore")}</button>
                <button type="button" disabled={!recoverySelection.canAdopt || recoveryBusy} onClick={() => setRecoveryConfirmation("adopt")}>{t("orchestrator.recovery.adopt")}</button>
                <button type="button" disabled={recoveryBusy} onClick={() => void resolveRecovery("cancel")}>{t("orchestrator.recovery.cancel")}</button>
              </div>
              {!recoverySelection.canResume && <p className="orchestrator-desc">{t("orchestrator.recovery.resumeUnavailable")}</p>}
            </div>}
            {recoveryConfirmation && (recoverySelection || convergenceRecoveryPreview) && <div className="orchestrator-recovery-confirm-backdrop">
              <section role="alertdialog" aria-modal="true" aria-labelledby="orchestrator-recovery-confirm-title" className="orchestrator-card orchestrator-recovery-confirm">
                <h4 id="orchestrator-recovery-confirm-title">{t(recoveryConfirmation === "convergence" ? "orchestrator.planConvergence.recovery.confirmTitle" : "orchestrator.recovery.confirmTitle")}</h4>
                <p>{recoveryConfirmation === "convergence" ? t("orchestrator.planConvergence.recovery.confirm") : t(recoveryConfirmation === "restore" ? "orchestrator.recovery.confirmRestore" : recoveryConfirmation === "adopt" ? "orchestrator.recovery.confirmAdopt" : "orchestrator.recovery.confirmResume")}</p>
                {recoveryConfirmation === "convergence" && convergenceRecoveryPreview && <>
                  <p>{t("orchestrator.planConvergence.recovery.target")}: <code>{convergenceRecoveryPreview.targetPlanId}</code></p>
                  <p>{convergenceRecoveryPreview.sectionTitle}</p>
                  <pre className="orchestrator-recovery-plan-content">{convergenceRecoveryPreview.sectionContent}</pre>
                </>}
                {recoveryConfirmation === "restore" && <>
                  <p>{t("orchestrator.recovery.backupDestination")}: <code>{recoverySelection?.backupDestination}</code></p>
                  <p>{t("orchestrator.recovery.affectedPaths")}: {recoverySelection?.affectedPaths.length}</p>
                  {(recoverySelection?.affectedPaths.length ?? 0) > 0 && <ul aria-label={t("orchestrator.recovery.affectedPathsList")}>
                    {recoverySelection?.affectedPaths.map((path) => <li key={path}><code>{path}</code></li>)}
                  </ul>}
                  <p>{t("orchestrator.recovery.restoreScope")}</p>
                </>}
                <div className="orchestrator-recovery-actions">
                  <button type="button" disabled={recoveryBusy} onClick={() => setRecoveryConfirmation(null)}>{t("orchestrator.recovery.cancelConfirm")}</button>
                  <button type="button" disabled={recoveryBusy} onClick={() => void resolveRecovery(recoveryConfirmation)}>{recoveryBusy ? t("orchestrator.recovery.loading") : t("orchestrator.recovery.confirmProceed")}</button>
                </div>
              </section>
            </div>}
          </section>}

          <WorkflowTabs
            activeWorkflowId={activeWorkflowId}
            disabled={isRunActive}
            onSelect={(wfId) => {
              if (isRunActive) return;
              if (wfId !== displayWorkflowId) {
                clearRunPresentation();
              }
              setPresetError(null);
              setActiveWorkflowId(wfId);
              void saveConfig(projectPath, wfId, activePresetId, roleAssignments, validationGates, limits);
            }}
            t={t}
          />

          <div className="orchestrator-card orchestrator-lean-mode-section">
            <ToggleSwitch
              checked={planConvergenceOptIn}
              disabled={isRunActive}
              onChange={setPlanConvergenceOptIn}
              label={t("orchestrator.planConvergence.optIn")}
              description={t("orchestrator.planConvergence.description")}
            />
            <ToggleSwitch
              checked={leanAntigravityMode}
              onChange={handleToggleLeanMode}
              label={t("orchestrator.leanMode.title") || "Lean Antigravity Mode"}
              description={t("orchestrator.leanMode.desc") || "Guides Antigravity workers toward bounded exploration, fewer redundant file re-reads, and batched fixes. Takes effect on the next run."}
            />
            {displayWorkflowId === "human_gated_loop" && (
              <div
                className="orchestrator-budget-info"
                style={{
                  marginTop: "10px",
                  paddingTop: "8px",
                  borderTop: "1px solid var(--color-border, rgba(255,255,255,0.08))",
                  fontSize: "0.85rem",
                  color: "var(--color-text-secondary, #888)",
                }}
              >
                <div style={{ fontWeight: 600, color: "var(--color-text-primary, #ddd)" }}>
                  {budgetScope === "task"
                    ? t("orchestrator.budget.exhaustedTask", {
                        current: String(antigravityDispatches ?? 0),
                        limit: String(antigravityDispatchLimit ?? 2),
                      })
                    : budgetScope === "run"
                    ? t("orchestrator.budget.exhaustedRun", {
                        current: String(antigravityDispatches ?? 0),
                        limit: String(antigravityDispatchLimit ?? 6),
                      })
                    : t("orchestrator.budget.dispatchCount", {
                        current: String(antigravityDispatches ?? 0),
                        limit: String(antigravityDispatchLimit ?? 6),
                      })}
                </div>
                <div style={{ marginTop: "4px", fontSize: "0.8rem", opacity: 0.85 }}>
                  {t("orchestrator.budget.notice")}
                </div>
              </div>
            )}
          </div>

          <div className="orchestrator-card orchestrator-roles-section">
            <div className="orchestrator-card-header">
              <span className="orchestrator-label">
                {t("orchestrator.roles.sectionTitle") || "Role Assignments & Capabilities"}
              </span>
              <button
                type="button"
                className="orchestrator-btn-secondary"
                disabled={isRunActive}
                onClick={handleApplyDefaultPreset}
              >
                {t("orchestrator.presets.applyDefault") || "既定構成を適用"}
              </button>
            </div>
            {presetError && (
              <div role="alert" className="orchestrator-preset-error-banner" style={{ color: "var(--color-error, #e53935)", padding: "8px 12px", fontSize: "0.9rem" }}>
                {presetError}
              </div>
            )}
            <div className="orchestrator-roles-grid">
              {activeRoles.map((role) => {
                const roleErr = validationErrors.find((e) => e.role === role);
                return (
                  <QuickProfileRoleCard
                    key={role}
                    role={role}
                    profiles={profiles}
                    quickSlots={quickSlots}
                    selectedProfileId={roleAssignments[role]?.profileId ?? ""}
                    workflowId={activeWorkflowId}
                    onSelect={(profileId) => handleChangeRoleProfile(role, profileId)}
                    invalidMessage={roleErr?.message}
                    t={t}
                  />
                );
              })}
            </div>
          </div>

          <ExecutionView
            runId={currentRunId}
            workflowId={displayWorkflowId}
            taskPrompt={taskPrompt}
            onTaskPromptChange={setTaskPrompt}
            state={executionState}
            currentStep={currentStep}
            stepProgress={0}
            logs={logs}
            verdict={verdict}
            validationIssues={validationIssues}
            planText={planText}
            onStart={handleStart}
            onPause={handlePause}
            onResume={handleResume}
            onCancel={handleCancel}
            onReset={handleReset}
            onSubmitClarification={handleSubmitClarification}
            onResolveBlocking={handleResolveBlocking}
            onResolveHumanGate={handleResolveHumanGate}
            onConfirmWorkerStopped={handleConfirmWorkerStopped}
            canStart={canStart && !startPending}
            disabledReason={disabledReason}
            waitingReason={waitingReason}
            budgetScope={budgetScope}
            antigravityDispatches={antigravityDispatches}
            antigravityDispatchLimit={antigravityDispatchLimit}
            planConvergence={planConvergenceOptIn}
            convergenceProgress={convergenceProgress}
            convergenceWaitingCandidate={convergenceWaitingCandidate}
            convergenceActionPending={convergenceActionPending}
            convergenceActionError={convergenceActionError}
            onConfirmConvergedNewPlan={handleConfirmConvergedNewPlan}
          />
      </div>
    </div>
  );
}
