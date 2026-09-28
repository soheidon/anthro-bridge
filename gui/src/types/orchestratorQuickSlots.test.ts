import { describe, expect, it } from "vitest";
import type { OrchestratorProfile, OrchestratorQuickSlot } from "./orchestrator";
import { getOtherProfiles, getQuickSlotButtons } from "./orchestrator";

const profiles: OrchestratorProfile[] = [
  { id: "reasoner", displayName: "Reasoner", adapter: "provider", capabilities: ["reasoning", "review"] },
  { id: "coder", displayName: "Coder", adapter: "cli", capabilities: ["workspace_write", "command_execution"] },
];
const slots: OrchestratorQuickSlot[] = [
  { id: "missing", profileId: "deleted", label: "Deleted", visible: true, order: 0 },
  { id: "hidden", profileId: "reasoner", label: "Hidden", visible: false, order: 1 },
  { id: "visible", profileId: "reasoner", label: "Reasoning", visible: true, order: 2 },
  { id: "coder-slot", profileId: "coder", label: "Coder", visible: true, order: 3 },
];

describe("Orchestrator quick slot filtering", () => {
  it("ignores missing slots and only returns visible profiles compatible with the role", () => {
    expect(getQuickSlotButtons(slots, profiles, "planner").map(({ profile }) => profile.id)).toEqual(["reasoner"]);
    expect(getQuickSlotButtons(slots, profiles, "implementer").map(({ profile }) => profile.id)).toEqual(["coder"]);
  });

  it("lists compatible profiles not already shown in a visible quick slot", () => {
    expect(getOtherProfiles(slots, profiles, "planner")).toEqual([]);
    expect(getOtherProfiles(slots, profiles, "implementer")).toEqual([]);
  });

  it("does not mutate quick slot order while sorting for display", () => {
    const reversed = [...slots].reverse();
    getQuickSlotButtons(reversed, profiles, "planner");
    expect(reversed.map((slot) => slot.id)).toEqual([...slots].reverse().map((slot) => slot.id));
  });
});
