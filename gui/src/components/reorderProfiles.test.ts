import { describe, it, expect, vi, beforeEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { computeMove, persistProfileOrder, applyProfileReorder } from "./reorderProfiles";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  vi.clearAllMocks();
  mockInvoke.mockResolvedValue(null);
});

// ── computeMove ──────────────────────────────────────────────────────

describe("computeMove", () => {
  const ids = ["a", "b", "c"];

  it("returns null on self-drop", () => {
    expect(computeMove("a", "a", ids)).toBeNull();
  });

  it("swaps two items correctly", () => {
    expect(computeMove("a", "c", ids)).toEqual(["b", "c", "a"]);
  });

  it("returns null for missing active ID", () => {
    expect(computeMove("x", "b", ids)).toBeNull();
  });
});

// ── persistProfileOrder ──────────────────────────────────────────────

describe("persistProfileOrder", () => {
  it("calls invoke with correct ordered IDs", async () => {
    await persistProfileOrder(["b", "a", "c"]);

    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("reorder_openrouter_profiles", {
      profileIds: ["b", "a", "c"],
    });
  });

  it("throws when invoke fails", async () => {
    mockInvoke.mockRejectedValueOnce(new Error("save failed"));

    await expect(persistProfileOrder(["b", "a"])).rejects.toThrow(
      "save failed",
    );
  });
});

// ── applyProfileReorder ──────────────────────────────────────────────

describe("applyProfileReorder", () => {
  const baseIds = ["a", "b", "c"];
  let setOrderedIds: Mock<(ids: string[]) => void>;
  let refreshConfig: Mock<() => Promise<void>>;

  beforeEach(() => {
    setOrderedIds = vi.fn();
    refreshConfig = vi.fn().mockResolvedValue(undefined);
  });

  it('returns "noop" on self-drop', async () => {
    const result = await applyProfileReorder(
      "a", "a", baseIds, setOrderedIds, refreshConfig,
    );

    expect(result).toBe("noop");
    expect(setOrderedIds).not.toHaveBeenCalled();
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(refreshConfig).not.toHaveBeenCalled();
  });

  it('returns "success" on full persist', async () => {
    const result = await applyProfileReorder(
      "a", "b", baseIds, setOrderedIds, refreshConfig,
    );

    expect(result).toBe("success");
    expect(setOrderedIds).toHaveBeenCalledTimes(1);
    expect(setOrderedIds).toHaveBeenCalledWith(["b", "a", "c"]);
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(refreshConfig).toHaveBeenCalledTimes(1);
  });

  it('rolls back and returns "save_failed" on invoke rejection', async () => {
    mockInvoke.mockRejectedValueOnce(new Error("save failed"));

    const result = await applyProfileReorder(
      "a", "b", baseIds, setOrderedIds, refreshConfig,
    );

    expect(result).toBe("save_failed");
    expect(setOrderedIds).toHaveBeenCalledTimes(2);
    expect(setOrderedIds).toHaveBeenNthCalledWith(1, ["b", "a", "c"]);
    expect(setOrderedIds).toHaveBeenNthCalledWith(2, ["a", "b", "c"]);
    expect(refreshConfig).not.toHaveBeenCalled();
  });

  it('keeps optimistic order and returns "refresh_failed" when invoke succeeds but refreshConfig fails', async () => {
    refreshConfig.mockRejectedValueOnce(new Error("refresh failed"));

    const result = await applyProfileReorder(
      "a", "b", baseIds, setOrderedIds, refreshConfig,
    );

    expect(result).toBe("refresh_failed");
    expect(setOrderedIds).toHaveBeenCalledTimes(1);
    expect(setOrderedIds).toHaveBeenCalledWith(["b", "a", "c"]);
    expect(mockInvoke).toHaveBeenCalledTimes(1);
  });
});
