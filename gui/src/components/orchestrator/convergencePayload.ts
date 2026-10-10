import type {
  ConvergenceProgress,
  ConvergenceProgressPhase,
  PlanConvergenceWaitingCandidate,
} from "../../types/orchestrator";

const PROGRESS_PHASES: readonly ConvergenceProgressPhase[] = [
  "planner_dispatch",
  "reviewer_reserve",
  "reviewer_dispatch",
  "verdict",
];

const DECISIONS = ["APPROVE", "REQUEST_CHANGES", "ESCALATE"] as const;

function parsePositiveInteger(raw: string | undefined): number | undefined {
  if (raw === undefined) return undefined;
  const value = Number(raw);
  return Number.isInteger(value) && value >= 0 ? value : undefined;
}

/**
 * Decodes the machine-readable convergence progress carried in the event's
 * iteration info as `key:value` pairs. Returns null for anything that is not a
 * well-formed convergence progress record, so no free-form backend prose is
 * ever interpreted as progress.
 */
export function parseConvergenceProgress(
  iterationInfo: string | null | undefined,
): ConvergenceProgress | null {
  if (!iterationInfo || !iterationInfo.startsWith("phase:")) return null;

  const fields = new Map<string, string>();
  for (const part of iterationInfo.split(",")) {
    const separator = part.indexOf(":");
    if (separator <= 0) continue;
    fields.set(part.slice(0, separator), part.slice(separator + 1));
  }

  const phase = fields.get("phase") as ConvergenceProgressPhase | undefined;
  if (!phase || !PROGRESS_PHASES.includes(phase)) return null;

  const sequence = parsePositiveInteger(fields.get("sequence"));
  if (sequence === undefined || sequence < 1) return null;

  const rawDecision = fields.get("decision");
  const decision = DECISIONS.find((candidate) => candidate === rawDecision);

  return {
    phase,
    sequence,
    decision,
    reviewsUsed: parsePositiveInteger(fields.get("used")),
    reviewsLimit: parsePositiveInteger(fields.get("limit")),
  };
}

/**
 * Decodes the typed waiting candidate published with a convergence human gate.
 * Anything that does not match the contract is rejected so the UI never renders
 * unvalidated backend payloads as a confirmable candidate.
 */
export function parseConvergenceWaitingCandidate(
  planText: string | null | undefined,
): PlanConvergenceWaitingCandidate | null {
  if (!planText) return null;

  let raw: unknown;
  try {
    raw = JSON.parse(planText);
  } catch {
    return null;
  }
  if (typeof raw !== "object" || raw === null) return null;

  const record = raw as Record<string, unknown>;
  const { runId, candidateId, title, planText: body, intent, revision, sequence } = record;
  if (
    typeof runId !== "string" ||
    typeof candidateId !== "string" ||
    typeof title !== "string" ||
    typeof body !== "string" ||
    typeof revision !== "number" ||
    typeof sequence !== "number"
  ) {
    return null;
  }
  if (intent !== "new_primary" && intent !== "append_section") return null;

  const proposedRevision = record.proposedRevision;
  return {
    runId,
    revision,
    candidateId,
    sequence,
    title,
    planText: body,
    intent,
    proposedRevision: typeof proposedRevision === "number" ? proposedRevision : undefined,
  };
}
