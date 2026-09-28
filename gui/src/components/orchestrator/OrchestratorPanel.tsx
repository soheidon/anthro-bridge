import React, { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "../../i18n";
import type {
  AgentRole,
  LoopIterationLimits,
  OrchestratorConfig,
  OrchestratorProfile,
  ProjectMetadataResponse,
  RoleAssignment,
  ValidationGateConfig,
  OrchestratorStep,
  RunConfigurationSnapshot,
  StartRunResponse,
  StepProgressEvent,
  RunLogEvent,
} from "../../types/orchestrator";
import { shouldAcceptRunEvent, validateAllRoleCapabilities } from "../../types/orchestrator";
import {
  DEFAULT_ORCHESTRATOR_PROFILES,
  DEFAULT_VALIDATION_GATES,
  DEFAULT_ITERATION_LIMITS,
  BUILTIN_ORCHESTRATOR_PRESETS,
  getDefaultRoleAssignments,
} from "../../config/orchestratorPresets";
import "./Orchestrator.css";

import { ProjectSelector } from "./ProjectSelector";
import { WorkflowSelector } from "./WorkflowSelector";
import { PresetSelector } from "./PresetSelector";
import { RoleAssignmentCard } from "./RoleAssignmentCard";
import { ValidationGatePanel } from "./ValidationGatePanel";
import { BudgetLimitPanel } from "./BudgetLimitPanel";
import { ExecutionView, type ExecutionState } from "./ExecutionView";

export default function OrchestratorPanel() {
  const { t } = useTranslation();

  // State
  const [projectPath, setProjectPath] = useState<string>("");
  const [metadata, setMetadata] = useState<ProjectMetadataResponse | null>(null);
  const [detecting, setDetecting] = useState<boolean>(false);

  const [activeWorkflowId, setActiveWorkflowId] = useState<string>("full_loop");
  const [activePresetId, setActivePresetId] = useState<string>("balanced");
  const [isCustom, setIsCustom] = useState<boolean>(false);

  const [profiles, setProfiles] = useState<OrchestratorProfile[]>(DEFAULT_ORCHESTRATOR_PROFILES);
  const [roleAssignments, setRoleAssignments] = useState<Record<AgentRole, RoleAssignment>>(
    () => getDefaultRoleAssignments("balanced")
  );
  const [validationGates, setValidationGates] = useState<ValidationGateConfig[]>(DEFAULT_VALIDATION_GATES);
  const [limits, setLimits] = useState<LoopIterationLimits>(DEFAULT_ITERATION_LIMITS);

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

  const detectProjectMetadata = useCallback(async (targetPath: string) => {
    if (!targetPath.trim()) return;
    setDetecting(true);
    try {
      const res = await invoke<ProjectMetadataResponse>("detect_project_metadata", {
        projectPath: targetPath.trim(),
      });
      setMetadata(res);
    } catch (e) {
      console.error("Failed to detect project metadata:", e);
    } finally {
      setDetecting(false);
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
            setProjectPath(path);
            void detectProjectMetadata(path);
          }
          const wf = cfg.activeWorkflowId;
          if (wf) setActiveWorkflowId(wf);
          const pr = cfg.activePresetId;
          if (pr) setActivePresetId(pr);
          const assigns = cfg.assignments;
          if (assigns) setRoleAssignments(assigns);
          const gates = cfg.validationGates;
          if (gates) setValidationGates(gates);
          const lms = cfg.iterationLimits;
          if (lms) setLimits(lms);
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
          if (stepStr === "plan_generation") setCurrentStep("planning");
          else if (stepStr === "plan_review" || stepStr === "plan_revision") setCurrentStep("plan_review");
          else if (stepStr === "implementation") setCurrentStep("implementation");
          else if (stepStr === "validation") setCurrentStep("validation");
          else if (stepStr === "code_review") setCurrentStep("code_review");
          else if (stepStr === "fix") setCurrentStep("fixing");
          else if (stepStr === "waiting_for_user") {
            setExecutionState("waiting_for_user");
          } else if (stepStr === "waiting_for_blocking_resolution") {
            setExecutionState("waiting_for_blocking_resolution");
          } else if (stepStr === "complete") {
            setCurrentStep("completed");
            setExecutionState("completed");
          } else if (stepStr === "failed") {
            setExecutionState("failed");
          } else if (stepStr === "paused") {
            setExecutionState("paused");
          }

          if (data.message) {
            setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${data.message}`]);
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
          if (data && shouldAcceptRunEvent(data.runId, currentRunIdRef.current, startPendingRef.current)) {
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
      updatedLimits: LoopIterationLimits
    ) => {
      try {
        const payload: OrchestratorConfig = {
          projectPath: updatedPath,
          activeWorkflowId: updatedWorkflow,
          activePresetId: updatedPreset,
          assignments: updatedAssignments,
          validationGates: updatedGates,
          iterationLimits: updatedLimits,
        };
        await invoke("update_orchestrator_config", { config: payload });
      } catch (e) {
        console.error("Failed to save orchestrator config:", e);
      }
    },
    []
  );

  // Handle Preset selection
  const handleSelectPreset = useCallback(
    (presetId: string) => {
      const preset = BUILTIN_ORCHESTRATOR_PRESETS.find((p) => p.id === presetId);
      if (!preset) return;

      setActivePresetId(presetId);
      setIsCustom(false);

      const newAssignments = getDefaultRoleAssignments(presetId);
      setRoleAssignments(newAssignments);
      const newLimits = preset.iterationLimits ?? DEFAULT_ITERATION_LIMITS;
      const newGates = preset.validationGates ?? DEFAULT_VALIDATION_GATES;
      setLimits(newLimits);
      setValidationGates(newGates);

      void saveConfig(
        projectPath,
        activeWorkflowId,
        presetId,
        newAssignments,
        newGates,
        newLimits
      );
    },
    [projectPath, activeWorkflowId, saveConfig]
  );

  // Handle Role Assignment change
  const handleChangeRoleProfile = useCallback(
    (role: AgentRole, profileId: string) => {
      setIsCustom(true);
      const next = {
        ...roleAssignments,
        [role]: {
          ...roleAssignments[role],
          profileId,
        },
      };
      setRoleAssignments(next);
      void saveConfig(projectPath, activeWorkflowId, "custom", next, validationGates, limits);
    },
    [roleAssignments, projectPath, activeWorkflowId, validationGates, limits, saveConfig]
  );

  // Handle Validation Gates change
  const handleChangeGates = useCallback(
    (gates: ValidationGateConfig[]) => {
      setIsCustom(true);
      setValidationGates(gates);
      void saveConfig(projectPath, activeWorkflowId, "custom", roleAssignments, gates, limits);
    },
    [projectPath, activeWorkflowId, roleAssignments, limits, saveConfig]
  );

  // Handle Limits change
  const handleChangeLimits = useCallback(
    (newLimits: LoopIterationLimits) => {
      setIsCustom(true);
      setLimits(newLimits);
      void saveConfig(projectPath, activeWorkflowId, "custom", roleAssignments, validationGates, newLimits);
    },
    [projectPath, activeWorkflowId, roleAssignments, validationGates, saveConfig]
  );

  // Capability Validation
  const validationErrors = useMemo(() => {
    const roleProfileMap: Record<string, string> = {};
    for (const [r, assign] of Object.entries(roleAssignments)) {
      roleProfileMap[r] = assign.profileId;
    }
    return validateAllRoleCapabilities(roleProfileMap, profiles);
  }, [roleAssignments, profiles]);

  const canStart = useMemo(() => {
    if (!projectPath.trim() || (metadata && !metadata.exists)) return false;
    if (validationErrors.length > 0) return false;
    return true;
  }, [projectPath, metadata, validationErrors]);

  const disabledReason = useMemo(() => {
    if (!projectPath.trim()) return t("orchestrator.validation.noProjectPath") || "Please select a valid project directory.";
    if (metadata && !metadata.exists) return t("orchestrator.validation.pathNotFound") || "Project directory does not exist.";
    if (validationErrors.length > 0) return validationErrors[0].message;
    return undefined;
  }, [projectPath, metadata, validationErrors, t]);

  // Execution Handlers with Tauri backend integration
  const handleStart = async () => {
    if (!canStart || startPendingRef.current) return;
    const assignmentsMap = {} as Record<AgentRole, OrchestratorProfile>;
    for (const [r, assign] of Object.entries(roleAssignments)) {
      const prof = profiles.find((p) => p.id === assign.profileId);
      if (!prof) return;
      assignmentsMap[r as AgentRole] = prof;
    }

    currentRunIdRef.current = null;
    setCurrentRunId(null);
    startPendingRef.current = true;
    setStartPending(true);
    setExecutionState("running");
    setCurrentStep("planning");
    setVerdict(null);
    setValidationIssues([]);
    setLogs([`[${new Date().toLocaleTimeString()}] Initializing Orchestration run for project: ${projectPath}`]);

    const snapshot: RunConfigurationSnapshot = {
      projectPath,
      assignments: assignmentsMap,
      iterationLimits: limits,
      validationGates,
      budgetLimits: {},
      createdAtUnix: Math.floor(Date.now() / 1000),
    };

    try {
      const res = await invoke<StartRunResponse>("start_orchestrator_run", {
        snapshot,
        taskPrompt,
        workflowType: activeWorkflowId,
      });
      currentRunIdRef.current = res.runId;
      setCurrentRunId(res.runId);
    } catch (e: any) {
      currentRunIdRef.current = null;
      setCurrentRunId(null);
      setExecutionState("failed");
      setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] Error: ${e?.message || e}`]);
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

  const handleReset = () => {
    setCurrentRunId(null);
    setExecutionState("idle");
    setCurrentStep(null);
    setLogs([]);
    setVerdict(null);
    setValidationIssues([]);
  };

  const rolesList: AgentRole[] = ["planner", "plan_reviewer", "implementer", "fixer", "code_reviewer"];

  return (
    <div className="orchestrator-panel-container">
      <div className="orchestrator-header-bar">
        <div>
          <h2 className="orchestrator-main-title">
            {t("orchestrator.title") || "Multi-Agent Development Orchestrator"}
          </h2>
          <p className="orchestrator-subtitle">
            {t("orchestrator.subtitle") ||
              "Automate plan creation, plan review, implementation, validation gates, and code review with interchangeable models."}
          </p>
        </div>
      </div>

      <div className="orchestrator-grid-layout">
        {/* Left Column: Configuration */}
        <div className="orchestrator-col-config">
          <ProjectSelector
            projectPath={projectPath}
            onProjectPathChange={(p) => {
              setProjectPath(p);
              void saveConfig(p, activeWorkflowId, activePresetId, roleAssignments, validationGates, limits);
            }}
            metadata={metadata}
            onDetect={handleDetect}
            detecting={detecting}
          />

          <WorkflowSelector
            activeWorkflowId={activeWorkflowId}
            onSelectWorkflow={(wfId) => {
              setActiveWorkflowId(wfId);
              void saveConfig(projectPath, wfId, activePresetId, roleAssignments, validationGates, limits);
            }}
          />

          <PresetSelector
            activePresetId={activePresetId}
            onSelectPreset={handleSelectPreset}
            isCustom={isCustom}
          />

          <div className="orchestrator-card orchestrator-roles-section">
            <div className="orchestrator-card-header">
              <h3 className="orchestrator-card-title">
                {t("orchestrator.roles.sectionTitle") || "Role Assignments & Capabilities"}
              </h3>
            </div>
            <div className="orchestrator-roles-grid">
              {rolesList.map((role) => {
                const roleErr = validationErrors.find((e) => e.role === role);
                return (
                  <RoleAssignmentCard
                    key={role}
                    role={role}
                    assignment={roleAssignments[role]}
                    profiles={profiles}
                    onChangeProfile={handleChangeRoleProfile}
                    validationError={roleErr?.message}
                  />
                );
              })}
            </div>
          </div>

          <div className="orchestrator-two-col">
            <ValidationGatePanel
              gates={validationGates}
              projectPath={projectPath}
              onChangeGates={handleChangeGates}
            />
            <BudgetLimitPanel
              limits={limits}
              onChangeLimits={handleChangeLimits}
            />
          </div>
        </div>

        {/* Right Column: Execution & Control */}
        <div className="orchestrator-col-exec">
          <ExecutionView
            runId={currentRunId}
            taskPrompt={taskPrompt}
            onTaskPromptChange={setTaskPrompt}
            state={executionState}
            currentStep={currentStep}
            stepProgress={0}
            logs={logs}
            verdict={verdict}
            validationIssues={validationIssues}
            onStart={handleStart}
            onPause={handlePause}
            onResume={handleResume}
            onCancel={handleCancel}
            onReset={handleReset}
            onSubmitClarification={handleSubmitClarification}
            onResolveBlocking={handleResolveBlocking}
            canStart={canStart && !startPending}
            disabledReason={disabledReason}
          />
        </div>
      </div>
    </div>
  );
}
