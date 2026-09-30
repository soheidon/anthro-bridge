import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_ORCHESTRATOR_PROFILES } from "../../config/orchestratorPresets";
import type { OrchestratorQuickSlot } from "../../types/orchestrator";
import { QuickProfileRoleCard } from "./QuickProfileRoleCard";

const deepseek = DEFAULT_ORCHESTRATOR_PROFILES.find((profile) => profile.id === "deepseek-v41-flash")!;
const mimo = DEFAULT_ORCHESTRATOR_PROFILES.find((profile) => profile.id === "mimo-v26-pro")!;
const slots: OrchestratorQuickSlot[] = [
  { id: "hidden-deepseek", profileId: deepseek.id, label: "DeepSeek Flash", visible: false, order: 0 },
  { id: "visible-mimo", profileId: mimo.id, label: "MiMo Pro", visible: true, order: 1 },
];

const translate = (key: string) => ({
  "orchestrator.roles.planner": "Planner",
  "orchestrator.quickSlots.choose": "Choose a compatible profile",
  "orchestrator.quickSlots.notInWorkspaceList": "Not shown in workspace list",
  "orchestrator.quickSlots.unavailable": "Unavailable for this workflow",
}[key] ?? key);

function renderCard(selectedProfileId: string, onSelect = vi.fn()) {
  render(
    <QuickProfileRoleCard
      role="planner"
      profiles={[deepseek, mimo]}
      quickSlots={slots}
      selectedProfileId={selectedProfileId}
      onSelect={onSelect}
      t={translate}
    />,
  );
  return { card: screen.getByRole("region", { name: "Planner" }), onSelect };
}

describe("QuickProfileRoleCard stale assignments", () => {
  it("keeps a hidden but assigned profile visible and marks it as absent from the workspace list", () => {
    const { card } = renderCard(deepseek.id);
    const trigger = within(card).getByRole("combobox");

    expect(trigger).toHaveTextContent("deepseek-v4.1-flash + thinking: High");
    expect(trigger).toHaveAccessibleName(/Planner: deepseek-v4\.1-flash \+ thinking: High, Not shown in workspace list/);
    expect(trigger).toHaveTextContent("Not shown in workspace list");
    expect(trigger).not.toHaveTextContent("Choose a compatible profile");
  });

  it("uses the normal placeholder when there is no current assignment", () => {
    const { card } = renderCard("");
    expect(within(card).getByRole("combobox")).toHaveTextContent("Choose a compatible profile");
  });

  it("allows explicitly replacing a hidden assignment with a visible compatible profile", () => {
    const onSelect = vi.fn();
    const { card } = renderCard(deepseek.id, onSelect);
    const trigger = within(card).getByRole("combobox");
    fireEvent.click(trigger);
    const option = within(screen.getByRole("listbox")).getByRole("option", { name: /mimo-v2\.6-pro \+ thinking/ });
    expect(option).toBeInTheDocument();
    fireEvent.click(option);

    expect(onSelect).toHaveBeenCalledWith(mimo.id);
  });

  it("shows an assigned but workflow-incompatible profile as unavailable", () => {
    const cli = DEFAULT_ORCHESTRATOR_PROFILES.find((profile) => profile.id === "codex-cli")!;
    render(
      <QuickProfileRoleCard
        role="code_reviewer"
        profiles={[cli, deepseek]}
        quickSlots={[{ id: "cli", profileId: cli.id, label: "Codex CLI", visible: true, order: 0 }]}
        selectedProfileId={cli.id}
        workflowId="review_only"
        invalidMessage="unsupported adapter"
        onSelect={vi.fn()}
        t={translate}
      />,
    );
    const trigger = screen.getByRole("combobox");

    expect(trigger).toHaveTextContent("codex-cli");
    expect(trigger).toHaveTextContent("Unavailable for this workflow");
  });
});
