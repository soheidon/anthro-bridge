import type { ReasoningEffortOption } from "./modelCapabilities";

const REASONING_EFFORT_VALUES: readonly ReasoningEffortOption[] = [
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
];

export function isReasoningEffortOption(value: string): value is ReasoningEffortOption {
  return (REASONING_EFFORT_VALUES as readonly string[]).includes(value);
}

// Normalize a reasoning-effort selection for the given thinking mode and the
// model's allowed options. Returns "" when thinking is disabled (Normal) so the
// stored value never holds a stale effort. When options are absent (a model
// without an explicit reasoning-effort list, e.g. DeepSeek V4 Pro) the current
// value is preserved. An invalid/legacy value within Thinking mode falls back to
// the model's "high" default (or the first allowed value).
export function normalizeReasoningEffort(
  mode: string,
  current: string,
  options?: ReasoningEffortOption[],
): string {
  if (mode !== "thinking") return "";
  if (!options?.length) return current;
  if (isReasoningEffortOption(current) && options.includes(current)) return current;
  return options.includes("high") ? "high" : options[0] ?? "";
}
