import type { WorkflowType } from "../../types/orchestrator";

interface Props {
  activeWorkflowId: string;
  onSelect: (workflow: WorkflowType) => void;
  t: (key: any) => string;
}

export function WorkflowTabs({ activeWorkflowId, onSelect, t }: Props) {
  return (
    <div className="orchestrator-workflow-tabs" role="tablist" aria-label={t("orchestrator.workflow.title")}>
      <button
        type="button"
        role="tab"
        aria-selected={activeWorkflowId === "full_loop"}
        className={activeWorkflowId === "full_loop" ? "active" : ""}
        onClick={() => onSelect("full_loop")}
      >
        {t("orchestrator.workflow.fullLoop")}
      </button>
      {([["plan_only", "planOnly"], ["implement_only", "implementOnly"], ["review_only", "reviewOnly"]] as const).map(([workflow, labelKey]) => (
        <button
          type="button"
          role="tab"
          aria-selected={activeWorkflowId === workflow}
          title={t("orchestrator.validation.workflowUnavailable")}
          disabled
          key={workflow}
        >
          {t(`orchestrator.workflow.${labelKey}`)}
        </button>
      ))}
      {!(["full_loop", "plan_only", "implement_only", "review_only"] as string[]).includes(activeWorkflowId) && (
        <button type="button" role="tab" aria-selected="true" disabled>
          {t("orchestrator.validation.workflowUnavailable")} ({activeWorkflowId})
        </button>
      )}
    </div>
  );
}
