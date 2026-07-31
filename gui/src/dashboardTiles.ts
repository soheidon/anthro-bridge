export type DashboardCardCountConfig = {
  providers: Record<string, {
    profiles?: Array<{ hidden?: boolean }>;
  }>;
};

/**
 * Returns null when profiles are absent or empty, meaning one fallback tile.
 * Returns only visible profiles when profiles exist; an empty array means all are hidden.
 */
export function getVisibleOpenRouterProfiles<T extends { hidden?: boolean }>(
  profiles: T[] | undefined,
): T[] | null {
  if (!profiles || profiles.length === 0) {
    return null;
  }

  return profiles.filter((profile) => profile.hidden !== true);
}

export function calculateDashboardCardCount(
  config: DashboardCardCountConfig,
): number {
  let count = 0;

  for (const [providerId, provider] of Object.entries(config.providers)) {
    if (providerId !== "openrouter") {
      count += 1;
      continue;
    }

    const visibleProfiles = getVisibleOpenRouterProfiles(provider.profiles);
    count += visibleProfiles === null ? 1 : visibleProfiles.length;
  }

  return count;
}
