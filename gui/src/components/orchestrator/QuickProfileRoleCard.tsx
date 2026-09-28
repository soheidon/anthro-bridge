import { getOtherProfiles, getQuickSlotButtons } from "../../types/orchestrator";
import type { AgentRole, OrchestratorProfile, OrchestratorQuickSlot } from "../../types/orchestrator";

interface Props {
  role: AgentRole;
  profiles: OrchestratorProfile[];
  quickSlots: OrchestratorQuickSlot[];
  selectedProfileId: string;
  invalidMessage?: string;
  onSelect: (profileId: string) => void;
  t: (key: any) => string;
}

export function QuickProfileRoleCard({
  role,
  profiles,
  quickSlots,
  selectedProfileId,
  invalidMessage,
  onSelect,
  t,
}: Props) {
  const buttons = getQuickSlotButtons(quickSlots, profiles, role);
  const others = getOtherProfiles(quickSlots, profiles, role);
  const selected = profiles.find((profile) => profile.id === selectedProfileId);

  return (
    <section className="orchestrator-quick-role-card" aria-label={t(`orchestrator.roles.${role}`)}>
      <h4>{t(`orchestrator.roles.${role}`)}</h4>
      {selected && <p className="orchestrator-selected-profile">{selected.displayName}</p>}
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
      <label>
        {t("orchestrator.quickSlots.other")}
        <select
          aria-label={`${t(`orchestrator.roles.${role}`)} ${t("orchestrator.quickSlots.other")}`}
          value={others.some((profile) => profile.id === selectedProfileId) ? selectedProfileId : ""}
          onChange={(event) => event.target.value && onSelect(event.target.value)}
        >
          <option value="">{t("orchestrator.quickSlots.choose")}</option>
          {others.map((profile) => <option key={profile.id} value={profile.id}>{profile.displayName}</option>)}
        </select>
      </label>
    </section>
  );
}
