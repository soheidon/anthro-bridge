import { getCompatibleProfiles, getQuickSlotButtons } from "../../types/orchestrator";
import type { AgentRole, OrchestratorProfile, OrchestratorQuickSlot } from "../../types/orchestrator";
import { getOrchestratorProfileDisplayName, getOrchestratorProfileProviderLabel } from "../../config/orchestratorProfileDisplayName";
import { ProfileSelect, type ProfileSelectOption } from "./ProfileSelect";

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
  plan_integrator: "orchestrator.roles.planIntegrator",
  plan_reviewer: "orchestrator.roles.planReviewer",
  implementer: "orchestrator.roles.implementer",
  fixer: "orchestrator.roles.fixer",
  code_reviewer: "orchestrator.roles.codeReviewer",
};

function toProfileSelectOption(profile: OrchestratorProfile): ProfileSelectOption {
  const displayName = getOrchestratorProfileDisplayName(profile);
  return {
    id: profile.id,
    provider: getOrchestratorProfileProviderLabel(profile),
    model: displayName,
    displayName,
    badges: [],
  };
}

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
  const options = getQuickSlotButtons(quickSlots, profiles, role, workflowId);
  const roleKey = ROLE_NAME_KEYS[role] || `orchestrator.roles.${role}`;
  const roleLabel = t(roleKey);
  const isReviewOnlyIncompatible = workflowId === "review_only" && Boolean(invalidMessage);
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId);
  const selectedIsVisible = options.some(({ profile }) => profile.id === selectedProfileId);
  const compatibleProfileIds = new Set(getCompatibleProfiles(profiles, role, workflowId).map((profile) => profile.id));
  const selectedFallbackOption = selectedProfile
    ? toProfileSelectOption(selectedProfile)
    : undefined;
  const selectedFallback = selectedFallbackOption && !selectedIsVisible
    ? {
        ...selectedFallbackOption,
        badges: [
          ...(selectedFallbackOption.badges ?? []),
          t(compatibleProfileIds.has(selectedFallbackOption.id)
            ? "orchestrator.quickSlots.notInWorkspaceList"
            : "orchestrator.quickSlots.unavailable"),
        ],
      }
    : undefined;

  return (
    <section
      className={`orchestrator-quick-role-card ${isReviewOnlyIncompatible ? "has-warning" : ""}`}
      aria-label={roleLabel}
    >
      <h4>{roleLabel}</h4>
      {isReviewOnlyIncompatible && (
        <p className="orchestrator-invalid-assignment">
          {t("orchestrator.validation.reviewOnlyUnsupportedProfile")}
        </p>
      )}
      {isReviewOnlyIncompatible && (
        <p className="orchestrator-review-only-explanation">
          {t("orchestrator.validation.reviewOnlyExplanation")}
        </p>
      )}
      {invalidMessage && <p role="alert" className="orchestrator-invalid-assignment">{invalidMessage}</p>}
      <ProfileSelect
        label={roleLabel}
        placeholder={t("orchestrator.quickSlots.choose")}
        options={options.map(({ profile }) => toProfileSelectOption(profile))}
        value={selectedProfileId}
        selectedFallback={selectedFallback}
        disabled={options.length === 0}
        onChange={onSelect}
      />
    </section>
  );
}
