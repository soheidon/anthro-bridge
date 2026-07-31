import { invoke } from "@tauri-apps/api/core";
import { arrayMove } from "@dnd-kit/sortable";

/**
 * Compute the new ID list by moving activeId to overId's position.
 * Returns `null` if no change is needed (self-drop or missing IDs).
 * Pure — no side effects.
 */
export function computeMove(
  activeId: string,
  overId: string,
  currentIds: string[],
): string[] | null {
  if (activeId === overId) return null;
  const oldIdx = currentIds.indexOf(activeId);
  const newIdx = currentIds.indexOf(overId);
  if (oldIdx === -1 || newIdx === -1) return null;
  return arrayMove(currentIds, oldIdx, newIdx);
}

/**
 * Persist a profile ID reorder to disk via Tauri.
 * Throws only on invoke failure.
 */
export async function persistProfileOrder(
  orderedIds: string[],
): Promise<void> {
  await invoke("reorder_openrouter_profiles", { profileIds: orderedIds });
}

export type ProfileReorderResult =
  | "success"
  | "noop"
  | "save_failed"
  | "refresh_failed";

/**
 * Full DnD reorder orchestration: compute → optimistic update → save → refresh.
 *
 * - Save failure: rolls back to the original order, returns "save_failed".
 * - Save succeeds but refresh fails: keeps the optimistic order (the save is
 *   already on disk), returns "refresh_failed".
 * - Always resolves — errors are caught, logged, and handled internally.
 */
export async function applyProfileReorder(
  activeId: string,
  overId: string,
  currentIds: string[],
  setOrderedIds: (ids: string[]) => void,
  refreshConfig: () => Promise<void>,
): Promise<ProfileReorderResult> {
  const nextIds = computeMove(activeId, overId, currentIds);
  if (!nextIds) return "noop";

  setOrderedIds(nextIds);

  try {
    await persistProfileOrder(nextIds);
  } catch (error) {
    console.error("Failed to save OpenRouter profile order", error);
    setOrderedIds(currentIds);
    return "save_failed";
  }

  try {
    await refreshConfig();
  } catch (error) {
    console.error("Profile order was saved, but config refresh failed", error);
    // Save succeeded — keep the optimistic order.
    return "refresh_failed";
  }

  return "success";
}
