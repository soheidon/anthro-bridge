import { describe, expect, it } from "vitest";
import { calculateDashboardCardCount, getVisibleOpenRouterProfiles } from "./dashboardTiles";
import { buildTiles } from "./components/ProviderTiles";

function provider() {
  return { profiles: undefined };
}

describe("dashboard tile card rules", () => {
  it("counts each non-OpenRouter provider once", () => {
    expect(calculateDashboardCardCount({
      providers: {
        deepseek: provider(),
        mimo: provider(),
        minimax: provider(),
        kimi: provider(),
      },
    })).toBe(4);
  });

  it("uses one fallback card for absent or empty OpenRouter profiles", () => {
    expect(calculateDashboardCardCount({
      providers: { openrouter: provider() },
    })).toBe(1);
    expect(calculateDashboardCardCount({
      providers: { openrouter: { profiles: [] } },
    })).toBe(1);
  });

  it("does not use fallback when all existing profiles are hidden", () => {
    const profiles = [{ hidden: true }, { hidden: true }];
    expect(getVisibleOpenRouterProfiles(profiles)).toEqual([]);
    expect(calculateDashboardCardCount({
      providers: { openrouter: { profiles } },
    })).toBe(0);
  });

  it("keeps profiles without hidden true visible", () => {
    expect(getVisibleOpenRouterProfiles([{}, { hidden: false }, { hidden: true }])).toEqual([
      {},
      { hidden: false },
    ]);
  });

  it("counts the seven-card fixture used by the dashboard", () => {
    const config = {
      active_provider: "deepseek",
      active_openrouter_profile_id: null,
      providers: {
        deepseek: provider(),
        mimo: provider(),
        minimax: provider(),
        kimi: provider(),
        openrouter: {
          profiles: [
            { hidden: false },
            {},
            { hidden: false },
            { hidden: true },
          ],
        },
      },
      server: { host: "127.0.0.1", port: 4000, enable_cors: false },
    };

    expect(calculateDashboardCardCount(config)).toBe(7);
    expect(buildTiles(config as never)).toHaveLength(7);
  });

  it("excludes hidden direct providers from the card count", () => {
    expect(calculateDashboardCardCount({
      providers: {
        deepseek: { hidden: true },
        mimo: provider(),
        minimax: provider(),
        kimi: provider(),
      },
    })).toBe(3);
  });

  it("omits a hidden direct provider tile", () => {
    const config = {
      active_provider: "deepseek",
      active_openrouter_profile_id: null,
      providers: {
        deepseek: { hidden: true, display_name: "DeepSeek", models: {} },
        mimo: { display_name: "MiMo", models: {} },
        minimax: { display_name: "MiniMax", models: {} },
        kimi: { display_name: "Kimi", models: {} },
        openrouter: { profiles: [] },
      },
      server: { host: "127.0.0.1", port: 4000, enable_cors: false },
    };

    const tiles = buildTiles(config as never);
    // 3 visible direct providers + 1 OpenRouter fallback tile.
    expect(tiles).toHaveLength(4);
    expect(tiles.some((t) => t.providerId === "deepseek")).toBe(false);
  });

  it("returns null for fallback and an empty array for all-hidden profiles", () => {
    expect(getVisibleOpenRouterProfiles(undefined)).toBeNull();
    expect(getVisibleOpenRouterProfiles([])).toBeNull();
    expect(getVisibleOpenRouterProfiles([{ hidden: true }])).toEqual([]);
  });
});

// ProviderTiles uses the same shared profile helper as calculateDashboardCardCount.

describe("shared dashboard profile rules", () => {
  it("keeps a visible profile fixture aligned with the tile builder", () => {
    const profiles = [{ hidden: false }, {}, { hidden: true }];
    expect(getVisibleOpenRouterProfiles(profiles)).toHaveLength(2);
  });
});
