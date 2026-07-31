import { describe, expect, it } from "vitest";
import {
  DASHBOARD_GRID_COLUMNS,
  DASHBOARD_GRID_ROW_GAP,
  DASHBOARD_TILE_MIN_HEIGHT,
  calculateAvailableLogicalHeight,
  calculateDashboardGridRows,
  calculateInitialWindowHeight,
} from "./windowSizing";

describe("calculateDashboardGridRows", () => {
  it("calculates three-column row boundaries", () => {
    expect(calculateDashboardGridRows(0)).toBe(0);
    expect(calculateDashboardGridRows(1)).toBe(1);
    expect(calculateDashboardGridRows(2)).toBe(1);
    expect(calculateDashboardGridRows(3)).toBe(1);
    expect(calculateDashboardGridRows(4)).toBe(2);
    expect(calculateDashboardGridRows(5)).toBe(2);
    expect(calculateDashboardGridRows(6)).toBe(2);
    expect(calculateDashboardGridRows(7)).toBe(3);
  });

  it("normalizes invalid count and column values", () => {
    expect(calculateDashboardGridRows(-1)).toBe(0);
    expect(calculateDashboardGridRows(2.9)).toBe(1);
    expect(calculateDashboardGridRows(Number.NaN)).toBe(0);
    expect(calculateDashboardGridRows(4, 0)).toBe(2);
    expect(calculateDashboardGridRows(4, 2)).toBe(2);
  });
});

describe("calculateInitialWindowHeight", () => {
  const options = {
    baseHeight: 660,
    baseRows: 2,
    rowHeight: DASHBOARD_TILE_MIN_HEIGHT + DASHBOARD_GRID_ROW_GAP + 10,
    minHeight: 640,
  };

  it("uses the base height through two rows", () => {
    expect(calculateInitialWindowHeight(0, options)).toBe(660);
    expect(calculateInitialWindowHeight(1, options)).toBe(660);
    expect(calculateInitialWindowHeight(2, options)).toBe(660);
  });

  it("adds one row increment for each row beyond the base", () => {
    expect(calculateInitialWindowHeight(3, options)).toBe(772);
    expect(calculateInitialWindowHeight(4, options)).toBe(884);
  });

  it("clamps to minimum and maximum heights", () => {
    expect(calculateInitialWindowHeight(0, { ...options, minHeight: 750 })).toBe(750);
    expect(calculateInitialWindowHeight(20, { ...options, maxHeight: 800 })).toBe(800);
    expect(calculateInitialWindowHeight(20, { ...options, maxHeight: 600 })).toBe(640);
  });

  it("normalizes invalid rows and options", () => {
    expect(calculateInitialWindowHeight(-1, options)).toBe(660);
    expect(calculateInitialWindowHeight(2.9, options)).toBe(660);
    expect(calculateInitialWindowHeight(Number.NaN, options)).toBe(660);
    expect(calculateInitialWindowHeight(3, {
      baseHeight: Number.NaN,
      baseRows: Number.POSITIVE_INFINITY,
      rowHeight: 102,
      minHeight: 630,
    })).toBe(630);
  });

  it("uses the configured three-column layout constants", () => {
    expect(DASHBOARD_GRID_COLUMNS).toBe(3);
    expect(DASHBOARD_TILE_MIN_HEIGHT + DASHBOARD_GRID_ROW_GAP + 10).toBe(112);
  });
});

describe("calculateAvailableLogicalHeight", () => {
  it("accounts for window decorations and scale factor", () => {
    expect(calculateAvailableLogicalHeight({
      scaleFactor: 1.25,
      outerY: 100,
      outerHeight: 1000,
      innerHeight: 960,
      workAreaY: 0,
      workAreaHeight: 1200,
    })).toBe(848);
  });

  it("supports negative monitor coordinates", () => {
    expect(calculateAvailableLogicalHeight({
      scaleFactor: 1,
      outerY: -500,
      outerHeight: 800,
      innerHeight: 760,
      workAreaY: -600,
      workAreaHeight: 1000,
    })).toBe(860);
  });

  it("does not exceed the work area when the window starts above it", () => {
    expect(calculateAvailableLogicalHeight({
      scaleFactor: 1,
      outerY: -300,
      outerHeight: 800,
      innerHeight: 760,
      workAreaY: 0,
      workAreaHeight: 900,
    })).toBe(860);
  });

  it("clamps a window already below the work area to zero", () => {
    expect(calculateAvailableLogicalHeight({
      scaleFactor: 1,
      outerY: 1000,
      outerHeight: 800,
      innerHeight: 760,
      workAreaY: 0,
      workAreaHeight: 900,
    })).toBe(0);
  });

  it("returns undefined for invalid scale or measurements", () => {
    expect(calculateAvailableLogicalHeight({
      scaleFactor: 0,
      outerY: 0,
      outerHeight: 800,
      innerHeight: 760,
      workAreaY: 0,
      workAreaHeight: 900,
    })).toBeUndefined();
    expect(calculateAvailableLogicalHeight({
      scaleFactor: 1,
      outerY: Number.NaN,
      outerHeight: 800,
      innerHeight: 760,
      workAreaY: 0,
      workAreaHeight: 900,
    })).toBeUndefined();
  });
});
