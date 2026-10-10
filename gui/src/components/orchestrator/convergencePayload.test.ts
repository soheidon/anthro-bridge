import { describe, expect, it } from "vitest";
import {
  parseConvergenceProgress,
  parseConvergenceWaitingCandidate,
} from "./convergencePayload";

describe("parseConvergenceProgress", () => {
  it("decodes every phase and the typed verdict counters", () => {
    expect(parseConvergenceProgress("phase:planner_dispatch,sequence:1")).toEqual({
      phase: "planner_dispatch",
      sequence: 1,
      decision: undefined,
      reviewsUsed: undefined,
      reviewsLimit: undefined,
    });
    expect(parseConvergenceProgress("phase:reviewer_reserve,sequence:2")).toEqual({
      phase: "reviewer_reserve",
      sequence: 2,
      decision: undefined,
      reviewsUsed: undefined,
      reviewsLimit: undefined,
    });
    expect(parseConvergenceProgress("phase:reviewer_dispatch,sequence:2")).toEqual({
      phase: "reviewer_dispatch",
      sequence: 2,
      decision: undefined,
      reviewsUsed: undefined,
      reviewsLimit: undefined,
    });
    expect(
      parseConvergenceProgress("phase:verdict,sequence:2,decision:REQUEST_CHANGES,used:2,limit:3"),
    ).toEqual({
      phase: "verdict",
      sequence: 2,
      decision: "REQUEST_CHANGES",
      reviewsUsed: 2,
      reviewsLimit: 3,
    });
  });

  it("rejects prose, unknown phases, unknown decisions and malformed counts", () => {
    expect(parseConvergenceProgress(undefined)).toBeNull();
    expect(parseConvergenceProgress("")).toBeNull();
    expect(parseConvergenceProgress("Reviewer is working")).toBeNull();
    expect(parseConvergenceProgress("phase:not_a_phase,sequence:1")).toBeNull();
    expect(parseConvergenceProgress("phase:verdict,sequence:0")).toBeNull();
    expect(parseConvergenceProgress("phase:verdict,sequence:abc")).toBeNull();
    expect(parseConvergenceProgress("phase:verdict,sequence:1,decision:MAYBE")).toEqual({
      phase: "verdict",
      sequence: 1,
      decision: undefined,
      reviewsUsed: undefined,
      reviewsLimit: undefined,
    });
  });
});

describe("parseConvergenceWaitingCandidate", () => {
  const valid = JSON.stringify({
    runId: "run-1",
    revision: 7,
    candidateId: "cand-1",
    sequence: 1,
    title: "Title",
    planText: "Body",
    intent: "new_primary",
    proposedRevision: 2,
  });

  it("decodes the typed candidate contract", () => {
    expect(parseConvergenceWaitingCandidate(valid)).toEqual({
      runId: "run-1",
      revision: 7,
      candidateId: "cand-1",
      sequence: 1,
      title: "Title",
      planText: "Body",
      intent: "new_primary",
      proposedRevision: 2,
    });
  });

  it("rejects non-JSON, missing fields and unknown intents", () => {
    expect(parseConvergenceWaitingCandidate(null)).toBeNull();
    expect(parseConvergenceWaitingCandidate("Saved candidate preview")).toBeNull();
    expect(parseConvergenceWaitingCandidate("[]")).toBeNull();
    expect(
      parseConvergenceWaitingCandidate(JSON.stringify({ runId: "run-1", intent: "new_primary" })),
    ).toBeNull();
    expect(
      parseConvergenceWaitingCandidate(valid.replace('"new_primary"', '"something_else"')),
    ).toBeNull();
  });
});
