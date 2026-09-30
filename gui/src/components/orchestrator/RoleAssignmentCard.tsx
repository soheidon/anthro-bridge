import React from "react";
import { useTranslation } from "../../i18n";
import type {
  AgentRole,
  OrchestratorProfile,
  RoleAssignment,
} from "../../types/orchestrator";

interface RoleAssignmentCardProps {
  role: AgentRole;
  assignment: RoleAssignment;
  profiles: OrchestratorProfile[];
  onChangeProfile: (role: AgentRole, profileId: string) => void;
  validationError?: string;
}

const ROLE_INFO: Record<
  AgentRole,
  { nameKey: string; defaultName: string; descKey: string; defaultDesc: string }
> = {
  planner: {
    nameKey: "orchestrator.roles.planner",
    defaultName: "Planner",
    descKey: "orchestrator.roles.plannerDesc",
    defaultDesc: "Generates deep architectural design & implementation steps based on specs and repository state.",
  },
  plan_integrator: {
    nameKey: "orchestrator.roles.planIntegrator",
    defaultName: "Plan Integrator",
    descKey: "orchestrator.roles.planIntegratorDesc",
    defaultDesc: "Integrates plan drafts with Antigravity codebase awareness and conventions.",
  },
  plan_reviewer: {
    nameKey: "orchestrator.roles.planReviewer",
    defaultName: "Plan Reviewer",
    descKey: "orchestrator.roles.planReviewerDesc",
    defaultDesc: "Audits the implementation plan against SPEC.md, constraints, and architecture before code writing.",
  },
  implementer: {
    nameKey: "orchestrator.roles.implementer",
    defaultName: "Implementer",
    descKey: "orchestrator.roles.implementerDesc",
    defaultDesc: "Executes code changes, edits files, and creates new modules according to the approved plan.",
  },
  fixer: {
    nameKey: "orchestrator.roles.fixer",
    defaultName: "Fixer",
    descKey: "orchestrator.roles.fixerDesc",
    defaultDesc: "Resolves test failures, typecheck errors, and code review findings during iteration loops.",
  },
  code_reviewer: {
    nameKey: "orchestrator.roles.codeReviewer",
    defaultName: "Code Reviewer",
    descKey: "orchestrator.roles.codeReviewerDesc",
    defaultDesc: "Audits git diff and changes against approved plan with strict READY / NOT READY verdicts.",
  },
};

export const RoleAssignmentCard: React.FC<RoleAssignmentCardProps> = ({
  role,
  assignment,
  profiles,
  onChangeProfile,
  validationError,
}) => {
  const { t } = useTranslation();
  const info = ROLE_INFO[role];
  const currentProfile = profiles.find((p) => p.id === assignment.profileId);

  return (
    <div className={`orchestrator-role-card ${validationError ? "has-error" : ""}`}>
      <div className="orchestrator-role-header">
        <div className="orchestrator-role-title-row">
          <span className="orchestrator-role-name">
            {t(info.nameKey as any) || info.defaultName}
          </span>
          {currentProfile && (
            <span className="orchestrator-adapter-tag">
              {currentProfile.adapter.toUpperCase()}
            </span>
          )}
        </div>
        <p className="orchestrator-role-desc">
          {t(info.descKey as any) || info.defaultDesc}
        </p>
      </div>

      <div className="orchestrator-role-profile-select">
        <label className="orchestrator-label">
          {t("orchestrator.roles.assignedProfile") || "Assigned Profile"}:
        </label>
        <select
          className="orchestrator-select"
          value={assignment.profileId}
          onChange={(e) => onChangeProfile(role, e.target.value)}
        >
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.displayName} ({p.adapter})
            </option>
          ))}
        </select>
      </div>

      {currentProfile && (
        <div className="orchestrator-capabilities-list">
          {currentProfile.capabilities.map((cap) => (
            <span key={cap} className="orchestrator-cap-badge">
              {cap}
            </span>
          ))}
        </div>
      )}

      {validationError && (
        <div className="orchestrator-role-error-banner">
          ⚠️ {validationError}
        </div>
      )}
    </div>
  );
};
