import { describe, it, expect } from "vitest";
import { isReasoningEffortOption, normalizeReasoningEffort } from "./reasoningEffort";

describe("isReasoningEffortOption", () => {
  it("accepts the five reasoning-effort levels", () => {
    for (const v of ["low", "medium", "high", "xhigh", "max"]) {
      expect(isReasoningEffortOption(v)).toBe(true);
    }
  });

  it("rejects thinking toggles and unknowns", () => {
    expect(isReasoningEffortOption("on")).toBe(false);
    expect(isReasoningEffortOption("off")).toBe(false);
    expect(isReasoningEffortOption("")).toBe(false);
    expect(isReasoningEffortOption("ultra")).toBe(false);
  });
});

describe("normalizeReasoningEffort", () => {
  it("returns empty when thinking is disabled (Normal)", () => {
    expect(normalizeReasoningEffort("normal", "max", ["low", "high", "max"])).toBe("");
  });

  it("returns empty from thinking-enabled when mode is not thinking", () => {
    expect(normalizeReasoningEffort("anything", "high", ["low", "high", "max"])).toBe("");
  });

  it("preserves the current value when it is allowed", () => {
    expect(normalizeReasoningEffort("thinking", "low", ["low", "high", "max"])).toBe("low");
    expect(normalizeReasoningEffort("thinking", "high", ["low", "high", "max"])).toBe("high");
    expect(normalizeReasoningEffort("thinking", "max", ["low", "high", "max"])).toBe("max");
  });

  it("normalizes a legacy/invalid value to the model high default", () => {
    expect(normalizeReasoningEffort("thinking", "medium", ["low", "high", "max"])).toBe("high");
    expect(normalizeReasoningEffort("thinking", "on", ["low", "high", "max"])).toBe("high");
  });

  it("falls back to the first allowed value when there is no high", () => {
    expect(normalizeReasoningEffort("thinking", "medium", ["low", "max"])).toBe("low");
  });

  it("preserves current value when no option list is declared", () => {
    expect(normalizeReasoningEffort("thinking", "medium", undefined)).toBe("medium");
    expect(normalizeReasoningEffort("thinking", "high", undefined)).toBe("high");
  });

  it("normalizes legacy low/medium to high for DeepSeek Pro options", () => {
    expect(normalizeReasoningEffort("thinking", "medium", ["high", "max"])).toBe("high");
    expect(normalizeReasoningEffort("thinking", "low", ["high", "max"])).toBe("high");
  });
});
