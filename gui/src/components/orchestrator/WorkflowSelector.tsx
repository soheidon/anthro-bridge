import React from "react";
import { useTranslation } from "../../i18n";

export interface WorkflowOption {
  id: string;
  nameKey: string;
  defaultName: string;
  descKey: string;
  defaultDesc: string;
}

export const WORKFLOW_OPTIONS: WorkflowOption[] = [
  {
    id: "full_loop",
    nameKey: "orchestrator.workflow.fullLoop",
    defaultName: "Full Development Review Loop",
    descKey: "orchestrator.workflow.fullLoopDesc",
    defaultDesc: "Plan → Plan Review → Implement → Validation Gate → Code Review → Fix Loop",
  },
  {
    id: "plan_only",
    nameKey: "orchestrator.workflow.planOnly",
    defaultName: "Plan & Plan Review Only",
    descKey: "orchestrator.workflow.planOnlyDesc",
    defaultDesc: "Draft implementation plan and review against project specs without writing code",
  },
  {
    id: "implement_only",
    nameKey: "orchestrator.workflow.implementOnly",
    defaultName: "Implementation & Validation Only",
    descKey: "orchestrator.workflow.implementOnlyDesc",
    defaultDesc: "Execute existing approved plan, run validation gates, and fix errors",
  },
  {
    id: "review_only",
    nameKey: "orchestrator.workflow.reviewOnly",
    defaultName: "Code Review Only",
    descKey: "orchestrator.workflow.reviewOnlyDesc",
    defaultDesc: "Inspect git diff against specifications and implementation plan",
  },
];

interface WorkflowSelectorProps {
  activeWorkflowId: string;
  onSelectWorkflow: (id: string) => void;
}

export const WorkflowSelector: React.FC<WorkflowSelectorProps> = ({
  activeWorkflowId,
  onSelectWorkflow,
}) => {
  const { t } = useTranslation();
  const activeWorkflow = WORKFLOW_OPTIONS.find((w) => w.id === activeWorkflowId) || WORKFLOW_OPTIONS[0];

  return (
    <div className="orchestrator-card orchestrator-workflow-selector">
      <div className="orchestrator-card-header">
        <h3 className="orchestrator-card-title">{t("orchestrator.workflow.title") || "Workflow Mode"}</h3>
      </div>
      <div className="orchestrator-select-row">
        <select
          className="orchestrator-select"
          value={activeWorkflowId}
          onChange={(e) => onSelectWorkflow(e.target.value)}
        >
          {WORKFLOW_OPTIONS.map((wf) => (
            <option key={wf.id} value={wf.id}>
              {t(wf.nameKey as any) || wf.defaultName}
            </option>
          ))}
        </select>
      </div>
      <p className="orchestrator-desc">
        {t(activeWorkflow.descKey as any) || activeWorkflow.defaultDesc}
      </p>
    </div>
  );
};
