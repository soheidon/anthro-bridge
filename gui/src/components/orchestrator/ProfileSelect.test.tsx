import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProfileSelect, type ProfileSelectOption } from "./ProfileSelect";

const options: ProfileSelectOption[] = [
  { id: "deepseek", provider: "DeepSeek", model: "deepseek-flash + thinking: High", displayName: "deepseek-flash + thinking: High", badges: [] },
  { id: "mimo", provider: "MiMo", model: "mimo-v2.6-pro + thinking", displayName: "mimo-v2.6-pro + thinking", badges: [] },
  { id: "ollama", provider: "Ollama", model: "qwen3.6:27b", displayName: "qwen3.6:27b", badges: [] },
  { id: "codex", provider: "Codex CLI", model: "codex-cli", displayName: "codex-cli", badges: [] },
];

function renderSelect(onChange = vi.fn(), value = "deepseek") {
  render(
    <ProfileSelect
      label="Planner"
      placeholder="Choose a compatible profile"
      options={options}
      value={value}
      onChange={onChange}
    />,
  );
  return { trigger: screen.getByRole("combobox", { name: /^Planner:/ }), onChange };
}

describe("ProfileSelect", () => {
  it("shows the canonical selected profile name in the trigger", () => {
    const { trigger } = renderSelect();
    expect(trigger).toHaveAccessibleName(/Planner: deepseek-flash \+ thinking: High/);
    expect(trigger).toHaveTextContent("deepseek-flash + thinking: High");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("includes the placeholder with the control label when no profile is selected", () => {
    renderSelect(vi.fn(), "");
    expect(screen.getByRole("combobox")).toHaveAccessibleName(/Planner.*Choose a compatible profile/);
  });

  it("groups choices by provider and selects an option by mouse", () => {
    const { trigger, onChange } = renderSelect();
    fireEvent.click(trigger);

    const listbox = screen.getByRole("listbox", { name: "Planner" });
    expect(within(listbox).getByRole("group", { name: "DeepSeek" })).toBeInTheDocument();
    expect(within(listbox).getByRole("group", { name: "Ollama" })).toBeInTheDocument();
    expect(within(listbox).getByRole("option", { name: "deepseek-flash + thinking: High" })).toHaveAttribute("aria-selected", "true");

    fireEvent.click(within(listbox).getByRole("option", { name: "qwen3.6:27b" }));
    expect(onChange).toHaveBeenCalledWith("ollama");
    expect(trigger).toHaveFocus();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("supports ArrowUp/ArrowDown and Enter while keeping focus on the trigger", () => {
    const { trigger, onChange } = renderSelect();
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(trigger).toHaveAttribute("aria-activedescendant");

    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith("mimo");
    expect(trigger).toHaveFocus();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("opens at the last option with ArrowUp and supports Space to open/select", () => {
    const { trigger, onChange } = renderSelect(vi.fn(), "");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowUp" });
    expect(trigger).toHaveAttribute("aria-activedescendant", expect.stringContaining("option-3"));
    fireEvent.keyDown(trigger, { key: "Escape" });
    fireEvent.keyDown(trigger, { key: " " });
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(trigger, { key: " " });
    expect(onChange).toHaveBeenCalledWith("deepseek");
    expect(trigger).toHaveFocus();
  });

  it("closes on Escape without changing the selected profile", () => {
    const { trigger, onChange } = renderSelect();
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(trigger, { key: "Escape" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(onChange).not.toHaveBeenCalled();
    expect(trigger).toHaveFocus();
  });

  it("closes on Tab and leaves normal focus traversal unblocked", () => {
    const { trigger } = renderSelect();
    fireEvent.click(trigger);
    fireEvent.keyDown(trigger, { key: "Tab" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });
});
