import { getOtherProfiles, getQuickSlotButtons } from "../../types/orchestrator";
import type { AgentRole, OrchestratorProfile, OrchestratorQuickSlot } from "../../types/orchestrator";

interface Props {
  role: AgentRole;
  profiles: OrchestratorProfile[];
  quickSlots: OrchestratorQuickSlot[];
  selectedProfileId: string;
  workflowId?: string;
  invalidMessage?: string;
  onSelect: (profileId: string) => void;
  t: (key: any) => string;
}

const ROLE_NAME_KEYS: Record<AgentRole, string> = {
  planner: "orchestrator.roles.planner",
  plan_reviewer: "orchestrator.roles.planReviewer",
  implementer: "orchestrator.roles.implementer",
  fixer: "orchestrator.roles.fixer",
  code_reviewer: "orchestrator.roles.codeReviewer",
};

export function QuickProfileRoleCard({
  role,
  profiles,
  quickSlots,
  selectedProfileId,
  workflowId,
  invalidMessage,
  onSelect,
  t,
}: Props) {
  const buttons = getQuickSlotButtons(quickSlots, profiles, role, workflowId);
  const others = getOtherProfiles(quickSlots, profiles, role, workflowId);
  const selected = profiles.find((profile) => profile.id === selectedProfileId);
  const roleKey = ROLE_NAME_KEYS[role] || `orchestrator.roles.${role}`;
  const roleLabel = t(roleKey);
  const isReviewOnlyIncompatible = workflowId === "review_only" && Boolean(invalidMessage);

  return (
    <section
      className={`orchestrator-quick-role-card ${isReviewOnlyIncompatible ? "has-warning" : ""}`}
      aria-label={roleLabel}
    >
      <h4>{roleLabel}</h4>
      {selected && (
        <p className={`orchestrator-selected-profile ${isReviewOnlyIncompatible ? "unsupported" : ""}`}>
          {selected.displayName}
          {isReviewOnlyIncompatible && ` — ${t("orchestrator.validation.reviewOnlyUnsupportedProfile")}`}
        </p>
      )}
      {isReviewOnlyIncompatible && (
        <p className="orchestrator-review-only-explanation">
          {t("orchestrator.validation.reviewOnlyExplanation")}
        </p>
      )}
      {invalidMessage && <p role="alert" className="orchestrator-invalid-assignment">{invalidMessage}</p>}
      <div className="orchestrator-quick-slot-buttons">
        {buttons.map(({ slot, profile }) => (
          <button
            type="button"
            key={slot.id}
            aria-pressed={selectedProfileId === profile.id}
            className={selectedProfileId === profile.id ? "active" : ""}
            onClick={() => onSelect(profile.id)}
          >
            {slot.label}
          </button>
        ))}
      </div>
      <div className="orchestrator-quick-other-row">
        <label htmlFor={`orchestrator-other-${role}`}>
          {t("orchestrator.quickSlots.other")}:
        </label>
        <select
          id={`orchestrator-other-${role}`}
          aria-label={`${roleLabel} ${t("orchestrator.quickSlots.other")}`}
          value={others.some((profile) => profile.id === selectedProfileId) ? selectedProfileId : ""}
          onChange={(event) => event.target.value && onSelect(event.target.value)}
        >
          <option value="">{t("orchestrator.quickSlots.choose")}</option>
          {others.map((profile) => <option key={profile.id} value={profile.id}>{profile.displayName}</option>)}
        </select>
      </div>
    </section>
  );
}
