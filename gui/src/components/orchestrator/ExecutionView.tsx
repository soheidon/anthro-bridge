import React, { useState } from "react";
import { useTranslation } from "../../i18n";
import type { OrchestratorStep, HumanGateDecision } from "../../types/orchestrator";

export type ExecutionState =
  | "idle"
  | "running"
  | "paused"
  | "waiting_for_user"
  | "waiting_for_blocking_resolution"
  | "completed"
  | "failed"
  | "cancelled";

interface ExecutionViewProps {
  runId: string | null;
  taskPrompt: string;
  onTaskPromptChange: (prompt: string) => void;
  state: ExecutionState;
  currentStep: OrchestratorStep | null;
  stepProgress: number; // 0..100
  logs: string[];
  verdict: "READY" | "NOT_READY" | null;
  validationIssues: string[];
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onReset: () => void;
  onSubmitClarification: (response: string) => void;
  onResolveBlocking: (action: "retry" | "abort", guidance?: string) => void;
  onResolveHumanGate?: (decision: HumanGateDecision) => void;
  onConfirmWorkerStopped?: () => void;
  planText?: string | null;
  canStart: boolean;
  disabledReason?: string;
  runSettings?: React.ReactNode;
  workflowId?: string;
  waitingReason?: string | null;
  budgetScope?: "task" | "run" | null;
  antigravityDispatches?: number | null;
  antigravityDispatchLimit?: number | null;
}

interface StepDef {
  id: OrchestratorStep;
  labelKey: string;
  defaultLabel: string;
}

const WORKFLOW_STEPS: Record<string, StepDef[]> = {
  full_loop: [
    { id: "planning", labelKey: "orchestrator.steps.planning", defaultLabel: "Planning" },
    { id: "plan_review", labelKey: "orchestrator.steps.planReview", defaultLabel: "Plan Review" },
    { id: "implementation", labelKey: "orchestrator.steps.implementation", defaultLabel: "Implementation" },
    { id: "validation", labelKey: "orchestrator.steps.validation", defaultLabel: "Validation" },
    { id: "code_review", labelKey: "orchestrator.steps.codeReview", defaultLabel: "Code Review" },
    { id: "fixing", labelKey: "orchestrator.steps.fixing", defaultLabel: "Fixing" },
    { id: "completed", labelKey: "orchestrator.steps.completed", defaultLabel: "Done" },
  ],
  human_gated_loop: [
    { id: "planning", labelKey: "orchestrator.steps.planning", defaultLabel: "Plan Draft" },
    { id: "plan_integration", labelKey: "orchestrator.steps.planIntegration", defaultLabel: "Plan Integration" },
    { id: "plan_review", labelKey: "orchestrator.steps.planReview", defaultLabel: "Plan Review" },
    { id: "implementation", labelKey: "orchestrator.steps.implementation", defaultLabel: "Implementation" },
    { id: "validation", labelKey: "orchestrator.steps.validation", defaultLabel: "Validation" },
    { id: "code_review", labelKey: "orchestrator.steps.codeReview", defaultLabel: "Code Review" },
    { id: "human_gate", labelKey: "orchestrator.steps.humanGate", defaultLabel: "Human Gate" },
    { id: "completed", labelKey: "orchestrator.steps.completed", defaultLabel: "Done" },
  ],
  plan_only: [
    { id: "planning", labelKey: "orchestrator.steps.planning", defaultLabel: "Planning" },
    { id: "plan_review", labelKey: "orchestrator.steps.planReview", defaultLabel: "Plan Review" },
    { id: "completed", labelKey: "orchestrator.steps.completed", defaultLabel: "Done" },
  ],
  implement_only: [
    { id: "implementation", labelKey: "orchestrator.steps.implementation", defaultLabel: "Implementation" },
    { id: "validation", labelKey: "orchestrator.steps.validation", defaultLabel: "Validation" },
    { id: "fixing", labelKey: "orchestrator.steps.fixing", defaultLabel: "Fixing" },
    { id: "completed", labelKey: "orchestrator.steps.completed", defaultLabel: "Done" },
  ],
  review_only: [
    { id: "code_review", labelKey: "orchestrator.steps.codeReview", defaultLabel: "Code Review" },
    { id: "completed", labelKey: "orchestrator.steps.completed", defaultLabel: "Done" },
  ],
};

export const ExecutionView: React.FC<ExecutionViewProps> = ({
  runId,
  taskPrompt,
  onTaskPromptChange,
  state,
  currentStep,
  logs,
  verdict,
  validationIssues,
  onStart,
  onPause,
  onResume,
  onCancel,
  onReset,
  onSubmitClarification,
  onResolveBlocking,
  onResolveHumanGate,
  onConfirmWorkerStopped,
  planText,
  canStart,
  disabledReason,
  runSettings,
  workflowId,
  waitingReason,
  budgetScope,
  antigravityDispatches,
  antigravityDispatchLimit,
}) => {
  const { t } = useTranslation();
  const [clarificationInput, setClarificationInput] = useState("");
  const [guidanceInput, setGuidanceInput] = useState("");
  const [humanFeedback, setHumanFeedback] = useState("");

  const isRunning = state === "running";
  const isPaused = state === "paused";
  const isWaitingClarification = state === "waiting_for_user";
  const isWaitingBlocking = state === "waiting_for_blocking_resolution";
  const isFinished = state === "completed" || state === "failed" || state === "cancelled";

  const steps = workflowId ? (WORKFLOW_STEPS[workflowId] || []) : WORKFLOW_STEPS.full_loop;

  const handleSubmitClarification = (e: React.FormEvent) => {
    e.preventDefault();
    if (!clarificationInput.trim()) return;
    onSubmitClarification(clarificationInput.trim());
    setClarificationInput("");
  };

  const handleRetryBlocking = (e: React.FormEvent) => {
    e.preventDefault();
    onResolveBlocking("retry", guidanceInput.trim() || undefined);
    setGuidanceInput("");
  };

  const handleAbortBlocking = () => {
    onResolveBlocking("abort");
  };

  return (
    <div className="orchestrator-card orchestrator-execution-view">
      <div className="orchestrator-card-header">
        <h3 className="orchestrator-card-title">
          {t("orchestrator.exec.title") || "Execution & Orchestration Control"}
        </h3>
        <div style={{ display: "flex", gap: "0.5rem", alignItems: "center" }}>
          {runId && <span className="orchestrator-run-id-badge">ID: {runId.slice(0, 8)}</span>}
          <span className={`orchestrator-state-badge state-${state}`}>
            {state.replace(/_/g, " ").toUpperCase()}
          </span>
        </div>
      </div>

      <div className="orchestrator-task-input-section">
        <label className="orchestrator-label">
          {t("orchestrator.exec.taskPromptLabel") || "Task Description & Instructions"}:
        </label>
        <textarea
          className="orchestrator-textarea orchestrator-task-textarea"
          rows={3}
          placeholder="e.g. Implement user login session caching and add comprehensive unit tests."
          value={taskPrompt}
          onChange={(e) => onTaskPromptChange(e.target.value)}
          disabled={isRunning || isPaused || isWaitingClarification || isWaitingBlocking}
        />
      </div>

      {runSettings}

      {/* Run controls stay adjacent to the task and transient settings. */}
      <div className="orchestrator-controls-row">
        {state === "idle" && (
          <button
            type="button"
            className="orchestrator-btn orchestrator-btn-primary orchestrator-btn-lg"
            onClick={onStart}
            disabled={!canStart || !taskPrompt.trim()}
            title={disabledReason}
          >
            ▶ {t("orchestrator.exec.startBtn") || "Start Run"}
          </button>
        )}
        {isRunning && (
          <>
            <button
              type="button"
              className="orchestrator-btn orchestrator-btn-warning"
              onClick={onPause}
            >
              ⏸ {t("orchestrator.exec.pauseBtn") || "Pause"}
            </button>
            <button
              type="button"
              className="orchestrator-btn orchestrator-btn-danger"
              onClick={onCancel}
            >
              ⏹ {t("orchestrator.exec.cancelBtn") || "Cancel"}
            </button>
          </>
        )}
        {isPaused && (
          <>
            <button
              type="button"
              className="orchestrator-btn orchestrator-btn-primary"
              onClick={onResume}
            >
              ▶ {t("orchestrator.exec.resumeBtn") || "Resume"}
            </button>
            <button
              type="button"
              className="orchestrator-btn orchestrator-btn-danger"
              onClick={onCancel}
            >
              ⏹ {t("orchestrator.exec.cancelBtn") || "Cancel"}
            </button>
          </>
        )}
        {(isFinished || (isWaitingClarification && waitingReason === "budget_exhausted")) && (
          <button
            type="button"
            className="orchestrator-btn orchestrator-btn-secondary"
            onClick={onReset}
          >
            🔄 {t("orchestrator.exec.resetBtn") || "New Run"}
          </button>
        )}
        {disabledReason && state === "idle" && (
          <span className="orchestrator-disabled-reason">⚠️ {disabledReason}</span>
        )}
      </div>

      {/* Stepper view */}
      {steps.length > 0 && (
        <div className="orchestrator-stepper">
          {steps.map((step, idx) => {
            const isTerminalComplete = state === "completed" && step.id === "completed";
            const isActive = isTerminalComplete || (state !== "completed" && currentStep === step.id);
            const isPassed =
              state === "completed"
                ? step.id !== "completed"
                : Boolean(currentStep && steps.findIndex((s) => s.id === currentStep) > idx);

            return (
              <div
                key={step.id}
                className={`stepper-step ${isActive ? "active" : ""} ${isPassed ? "passed" : ""}`}
              >
                <div className="stepper-dot">{isPassed ? "✓" : idx + 1}</div>
                <span className="stepper-label">
                  {t(step.labelKey as any) || step.defaultLabel}
                </span>
              </div>
            );
          })}
        </div>
      )}

      {/* Human Gate Final Operator Approval Card */}
      {currentStep === "human_gate" && (
        <div
          className="orchestrator-card orchestrator-resolution-card"
          style={{ borderColor: "#3b82f6", background: "rgba(59, 130, 246, 0.08)" }}
        >
          <h4 style={{ color: "#2563eb", marginBottom: "0.5rem" }}>🛡️ Human Operator Final Approval (HumanGate)</h4>
          <p style={{ fontSize: "0.85rem", marginBottom: "0.75rem" }}>
            Automated verification passed and Code Reviewer gave approval. Review the proposed changes, then approve to finalize, request manual changes with feedback, or abort.
          </p>
          {planText && (
            <div style={{ maxHeight: "150px", overflowY: "auto", background: "rgba(0,0,0,0.1)", padding: "0.5rem", borderRadius: "4px", fontSize: "0.8rem", marginBottom: "0.75rem", whiteSpace: "pre-wrap" }}>
              <strong>Approved Plan:</strong><br />
              {planText}
            </div>
          )}
          <textarea
            className="orchestrator-textarea"
            rows={2}
            placeholder="Feedback for Fixer (required only if requesting changes)..."
            value={humanFeedback}
            onChange={(e) => setHumanFeedback(e.target.value)}
          />
          <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.75rem", flexWrap: "wrap" }}>
            <button
              type="button"
              className="orchestrator-btn orchestrator-btn-primary"
              onClick={() => onResolveHumanGate?.({ type: "approve" })}
            >
              ✅ Approve & Complete
            </button>
            <button
              type="button"
              className="orchestrator-btn orchestrator-btn-warning"
              disabled={!humanFeedback.trim()}
              onClick={() => onResolveHumanGate?.({ type: "request_changes", feedback: humanFeedback.trim() })}
            >
              🔄 Request Changes
            </button>
            <button
              type="button"
              className="orchestrator-btn orchestrator-btn-danger"
              onClick={() => onResolveHumanGate?.({ type: "abort" })}
            >
              ⏹ Abort Run
            </button>
          </div>
        </div>
      )}

      {/* Budget Exhausted Dedicated Waiting Card */}
      {isWaitingClarification && currentStep !== "human_gate" && waitingReason === "budget_exhausted" && (
        <div
          className="orchestrator-card orchestrator-resolution-card orchestrator-budget-exhausted-card"
          style={{ borderColor: "#ef4444", background: "rgba(239, 68, 68, 0.08)" }}
        >
          <h4 style={{ color: "#dc2626", marginBottom: "0.5rem" }}>
            🛑 {budgetScope === "task"
              ? t("orchestrator.budget.exhaustedTask", {
                  current: String(antigravityDispatches ?? 2),
                  limit: String(antigravityDispatchLimit ?? 2),
                })
              : t("orchestrator.budget.exhaustedRun", {
                  current: String(antigravityDispatches ?? 6),
                  limit: String(antigravityDispatchLimit ?? 6),
                })}
          </h4>
          <p style={{ fontSize: "0.85rem", marginBottom: "0.75rem" }}>
            {t("orchestrator.budget.exhausted")}
          </p>
          <div style={{ fontSize: "0.8rem", opacity: 0.85, marginBottom: "0.75rem" }}>
            {t("orchestrator.budget.notice")}
          </div>
          <div style={{ display: "flex", gap: "0.5rem" }}>
            <button
              type="button"
              className="orchestrator-btn orchestrator-btn-secondary"
              onClick={onReset}
            >
              🔄 {t("orchestrator.exec.resetBtn") || "New Run"}
            </button>
          </div>
        </div>
      )}

      {/* Worker Disconnection Card */}
      {isWaitingClarification && currentStep !== "human_gate" && waitingReason === "worker_disconnected" && (
        <div
          className="orchestrator-card orchestrator-resolution-card orchestrator-worker-disconnected-card"
          style={{ borderColor: "#eab308", background: "rgba(234, 179, 8, 0.08)" }}
        >
          <h4 style={{ color: "#ca8a04", marginBottom: "0.5rem" }}>
            ⚠️ {t("orchestrator.worker.disconnectedTitle") || "Antigravity Worker Disconnected"}
          </h4>
          <p style={{ fontSize: "0.85rem", marginBottom: "0.75rem" }}>
            {t("orchestrator.worker.disconnectedDesc") || "The Antigravity worker process disconnected or timed out without reporting progress."}
          </p>
          {onConfirmWorkerStopped && (
            <div style={{ marginBottom: "0.75rem", padding: "0.5rem", background: "rgba(234, 179, 8, 0.15)", borderRadius: "4px" }}>
              <p style={{ fontSize: "0.8rem", margin: "0 0 0.5rem 0" }}>
                {t("orchestrator.worker.confirmStoppedNotice") || "Confirm that the old worker process has completely stopped to prevent simultaneous modifications:"}
              </p>
              <div style={{ display: "flex", gap: "0.5rem" }}>
                <button
                  type="button"
                  className="orchestrator-btn orchestrator-btn-warning"
                  onClick={onConfirmWorkerStopped}
                >
                  🔓 {t("orchestrator.worker.confirmStoppedBtn") || "Confirm Worker Stopped & Resume Claim"}
                </button>
                <button type="button" className="orchestrator-btn orchestrator-btn-danger" onClick={onCancel}>
                  {t("orchestrator.exec.cancelBtn") || "Cancel"}
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Clarification / General Waiting Card */}
      {isWaitingClarification && currentStep !== "human_gate" && waitingReason !== "budget_exhausted" && waitingReason !== "worker_disconnected" && (
        <div
          className="orchestrator-card orchestrator-resolution-card"
          style={{ borderColor: "#eab308", background: "rgba(234, 179, 8, 0.08)" }}
        >
          <h4 style={{ color: "#ca8a04", marginBottom: "0.5rem" }}>💬 Operator Clarification Required</h4>
          <p style={{ fontSize: "0.85rem", marginBottom: "0.75rem" }}>
            The orchestrator is waiting for input before proceeding.
          </p>
          <form onSubmit={handleSubmitClarification}>
            <textarea
              className="orchestrator-textarea"
              rows={2}
              placeholder="Enter clarification or instructions..."
              value={clarificationInput}
              onChange={(e) => setClarificationInput(e.target.value)}
            />
            <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.5rem" }}>
              <button type="submit" className="orchestrator-btn orchestrator-btn-primary" disabled={!clarificationInput.trim()}>
                Submit Clarification
              </button>
              <button type="button" className="orchestrator-btn orchestrator-btn-danger" onClick={onCancel}>
                Cancel Run
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Blocking Finding Resolution Card */}
      {isWaitingBlocking && (
        <div
          className="orchestrator-card orchestrator-resolution-card"
          style={{ borderColor: "#ef4444", background: "rgba(239, 68, 68, 0.08)" }}
        >
          <h4 style={{ color: "#dc2626", marginBottom: "0.5rem" }}>⚠️ Repeated Blocking Findings Encountered</h4>
          <p style={{ fontSize: "0.85rem", marginBottom: "0.75rem" }}>
            A blocking issue was not resolved in consecutive iterations. Choose to retry with specific guidance or abort the run.
          </p>
          <form onSubmit={handleRetryBlocking}>
            <textarea
              className="orchestrator-textarea"
              rows={2}
              placeholder="Guidance for Fixer (optional, e.g. 'Use serde untagged enum instead')..."
              value={guidanceInput}
              onChange={(e) => setGuidanceInput(e.target.value)}
            />
            <div style={{ display: "flex", gap: "0.5rem", marginTop: "0.5rem" }}>
              <button type="submit" className="orchestrator-btn orchestrator-btn-primary">
                Retry with Guidance
              </button>
              <button type="button" className="orchestrator-btn orchestrator-btn-danger" onClick={handleAbortBlocking}>
                Abort Run
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Verdict & Review Status */}
      {verdict && (
        <div className={`orchestrator-verdict-card verdict-${verdict.toLowerCase()}`}>
          <div className="verdict-title">
            Review Verdict: <strong>{verdict}</strong>
          </div>
          {validationIssues.length > 0 && (
            <ul className="verdict-issues">
              {validationIssues.map((issue, i) => (
                <li key={i}>{issue}</li>
              ))}
            </ul>
          )}
        </div>
      )}

      {/* Execution Logs */}
      {logs.length > 0 && (
        <div className="orchestrator-log-viewer">
          <div className="log-viewer-header">
            <span>Execution Log</span>
            <span className="log-count">{logs.length} lines</span>
          </div>
          <div className="log-viewer-body">
            {logs.map((line, idx) => (
              <div key={idx} className="log-line">
                {line}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
