import type { WorkflowType } from "../../types/orchestrator";

interface Props {
  activeWorkflowId: string;
  onSelect: (workflow: WorkflowType) => void;
  disabled?: boolean;
  t: (key: any) => string;
}

export function WorkflowTabs({ activeWorkflowId, onSelect, disabled, t }: Props) {
  const workflows: Array<{ id: WorkflowType; labelKey: string }> = [
    { id: "full_loop", labelKey: "fullLoop" },
    { id: "human_gated_loop", labelKey: "humanGatedLoop" },
    { id: "plan_only", labelKey: "planOnly" },
    { id: "implement_only", labelKey: "implementOnly" },
    { id: "review_only", labelKey: "reviewOnly" },
  ];

  const isUnknownWorkflow = !workflows.some((w) => w.id === activeWorkflowId);

  return (
    <div className="orchestrator-workflow-tabs" role="tablist" aria-label={t("orchestrator.workflow.title")}>
      {workflows.map(({ id, labelKey }) => (
        <button
          key={id}
          type="button"
          role="tab"
          aria-selected={activeWorkflowId === id}
          className={activeWorkflowId === id ? "active" : ""}
          disabled={disabled}
          onClick={() => onSelect(id)}
        >
          {t(`orchestrator.workflow.${labelKey}`)}
        </button>
      ))}
      {isUnknownWorkflow && (
        <button type="button" role="tab" aria-selected="true" disabled>
          {t("orchestrator.validation.workflowUnavailable")} ({activeWorkflowId})
        </button>
      )}
    </div>
  );
}
