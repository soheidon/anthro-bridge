import type { OrchestratorProfile } from "../types/orchestrator";
import { MODEL_CATALOG } from "../modelCapabilities";

export interface OrchestratorProviderGroup {
  key: string;
  name: string;
  adapter: OrchestratorProfile["adapter"];
  providerId?: string;
}

// Static cloud-provider identity comes from the shared model catalog. Ollama
// and CLI are fixed adapters, not entries in the static cloud model catalog.
export const ORCHESTRATOR_PROVIDER_GROUPS: OrchestratorProviderGroup[] = [
  ...MODEL_CATALOG.providers.map((provider) => ({
    key: provider.id,
    name: provider.label,
    adapter: "provider" as const,
    providerId: provider.id,
  })),
  { key: "ollama", name: "Ollama (Local)", adapter: "ollama" },
  { key: "cli", name: "Codex CLI", adapter: "cli" },
];

const groupsByKey = new Map(ORCHESTRATOR_PROVIDER_GROUPS.map((group) => [group.key, group]));

export function inferProfileGroupKey(profile: OrchestratorProfile): string {
  if (profile.adapter === "provider") {
    const provider = ORCHESTRATOR_PROVIDER_GROUPS.find(
      (group) => group.adapter === "provider" && group.providerId === profile.providerId,
    );
    return provider?.key ?? "other";
  }
  return ORCHESTRATOR_PROVIDER_GROUPS.some((group) => group.adapter === profile.adapter && !group.providerId)
    ? profile.adapter
    : "other";
}

export function canonicalExecutionFieldsForGroup(groupKey: string): Pick<OrchestratorProfile, "adapter" | "providerId"> | null {
  const group = groupsByKey.get(groupKey);
  if (!group) return null;
  return { adapter: group.adapter, providerId: group.providerId };
}

export function canonicalizeProfileExecutionFields(profile: OrchestratorProfile): OrchestratorProfile {
  const fields = canonicalExecutionFieldsForGroup(inferProfileGroupKey(profile));
  return fields ? { ...profile, ...fields } : profile;
}
