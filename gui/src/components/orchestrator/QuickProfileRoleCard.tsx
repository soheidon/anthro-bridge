import { getCompatibleProfiles, getQuickSlotButtons } from "../../types/orchestrator";
import type { AgentRole, OrchestratorProfile, OrchestratorQuickSlot } from "../../types/orchestrator";
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
  plan_reviewer: "orchestrator.roles.planReviewer",
  implementer: "orchestrator.roles.implementer",
  fixer: "orchestrator.roles.fixer",
  code_reviewer: "orchestrator.roles.codeReviewer",
};

const PROVIDER_LABELS: Record<string, string> = {
  deepseek: "DeepSeek",
  kimi: "Kimi",
  kimi_code: "Kimi Code",
  minimax: "MiniMax",
  mimo: "MiMo",
  openrouter: "OpenRouter",
};

function formatModelLabel(model: string): string {
  const lower = model.toLowerCase();
  const aliases: Record<string, string> = {
    "deepseek-v4.1-flash": "DeepSeek Flash",
    "deepseek-flash": "DeepSeek Flash",
    "kimi-k3": "Kimi K3",
    "kimi-for-coding": "Kimi for Coding",
    "mimo-v2.6-pro": "MiMo V2.6 Pro",
    "mimo-v2.6-flash": "MiMo V2.6 Flash",
    "gpt-5.6-sol": "GPT-5.6 Sol",
  };
  if (aliases[lower]) return aliases[lower];

  return model
    .split(/[-_:\s]+/)
    .map((part) => {
      const token = part.toLowerCase();
      if (token === "gpt") return "GPT";
      if (token === "mimo") return "MiMo";
      if (token === "deepseek") return "DeepSeek";
      if (token === "qwen") return "Qwen";
      if (token === "for") return "for";
      if (/^\d+[a-z]$/.test(token)) return token.slice(0, -1) + token.slice(-1).toUpperCase();
      if (/^v\d/.test(token)) return `V${token.slice(1)}`;
      return token ? token[0].toUpperCase() + token.slice(1) : part;
    })
    .join(" ");
}

function toProfileSelectOption(profile: OrchestratorProfile, slotLabel: string): ProfileSelectOption {
  if (profile.adapter === "provider") {
    const providerId = profile.providerId ?? "provider";
    const provider = PROVIDER_LABELS[providerId.toLowerCase()]
      ?? providerId.replace(/[_-]+/g, " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
    let model = profile.model?.trim() || profile.displayName || slotLabel;
    if (providerId === "deepseek" && model === "deepseek-v4.1-flash") model = "deepseek-flash";
    if (providerId === "openrouter") model = model.replace(/^[^/]+\//, "");
    const badges = profile.thinkingMode === "thinking"
      ? ["Thinking", ...(profile.reasoningEffort ? [formatModelLabel(profile.reasoningEffort)] : [])]
      : [];
    return { id: profile.id, provider, model: formatModelLabel(model), badges };
  }

  if (profile.adapter === "ollama") {
    const model = profile.ollamaModel?.trim()
      || profile.displayName.replace(/^Ollama\s*:?\s*/i, "")
      || slotLabel;
    return { id: profile.id, provider: "Ollama", model: formatModelLabel(model), badges: ["Local"] };
  }

  if (profile.adapter === "cli") {
    return { id: profile.id, provider: "Codex CLI", model: "Codex CLI", badges: ["Local Agent"] };
  }

    return { id: profile.id, provider: "MCP", model: profile.displayName || slotLabel, badges: [] };
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
    ? toProfileSelectOption(selectedProfile, selectedProfile.displayName)
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
        options={options.map(({ slot, profile }) => toProfileSelectOption(profile, slot.label))}
        value={selectedProfileId}
        selectedFallback={selectedFallback}
        disabled={options.length === 0}
        onChange={onSelect}
      />
    </section>
  );
}
