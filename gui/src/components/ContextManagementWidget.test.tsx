import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ContextManagementWidget from "./ContextManagementWidget";
import type { EffectiveAutoCompact } from "../types";

function makeEffective(partial: Partial<EffectiveAutoCompact>): EffectiveAutoCompact {
  return {
    globallyEnabled: false,
    mode: "auto",
    status: "disabled",
    applyEnvironment: false,
    windowTokens: null,
    triggerPercent: null,
    estimatedTriggerTokens: null,
    targetKind: null,
    targetId: null,
    targetName: null,
    routes: [],
    ...partial,
  };
}

function renderWidget(
  effective: EffectiveAutoCompact | null,
  overrides: Partial<Parameters<typeof ContextManagementWidget>[0]> = {},
) {
  const props = {
    effective,
    onToggle: vi.fn(),
    ...overrides,
  };
  render(<ContextManagementWidget {...props} />);
  return props;
}

describe("ContextManagementWidget", () => {
  it("renders the title with an unchecked switch when globally disabled", () => {
    renderWidget(makeEffective({ globallyEnabled: false }));

    expect(screen.getByText("Context management")).toBeInTheDocument();
    expect(screen.getByRole("switch")).toHaveAttribute("aria-checked", "false");
  });

  it("renders a checked switch when globally enabled", () => {
    renderWidget(makeEffective({ globallyEnabled: true }));

    expect(screen.getByRole("switch")).toHaveAttribute("aria-checked", "true");
  });

  it("annotates the auto minimum tooltip", () => {
    renderWidget(makeEffective({}));

    expect(
      screen.getByTitle(
        "Auto-applies the safe minimum of the context lengths of the models configured for the 3 connection routes. Changes take effect on the next Claude Code launch.",
      ),
    ).toBeInTheDocument();
  });

  it("turns the toggle ON from OFF", async () => {
    const user = userEvent.setup();
    const props = renderWidget(makeEffective({ globallyEnabled: false }));

    await user.click(screen.getByRole("switch"));
    expect(props.onToggle).toHaveBeenCalledWith(true);
  });

  it("turns the toggle OFF from ON", async () => {
    const user = userEvent.setup();
    const props = renderWidget(makeEffective({ globallyEnabled: true }));

    await user.click(screen.getByRole("switch"));
    expect(props.onToggle).toHaveBeenCalledWith(false);
  });

  it("does not fire when disabled", async () => {
    const user = userEvent.setup();
    const props = renderWidget(makeEffective({}), { disabled: true });

    await user.click(screen.getByRole("switch"));
    expect(props.onToggle).not.toHaveBeenCalled();
  });
});
