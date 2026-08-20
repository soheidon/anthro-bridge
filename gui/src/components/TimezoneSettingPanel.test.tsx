import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import TimezoneSettingPanel from "./TimezoneSettingPanel";

describe("TimezoneSettingPanel", () => {
  it("renders timezone selector with optgroups and options", () => {
    render(<TimezoneSettingPanel />);

    expect(screen.getByRole("combobox")).toBeInTheDocument();
    // Default mock uses key/en fallback
    expect(screen.getByRole("option", { name: /Asia\/Tokyo/i })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /Europe\/Paris/i })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /America\/New_York/i })).toBeInTheDocument();
  });
});
