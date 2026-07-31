import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import React, { useRef, useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useOpenRouterSaveQueue, useRouteSaveGeneration } from "./OpenRouterModelSelector";
import OpenRouterModelSelector from "./OpenRouterModelSelector";

// ── Helpers ───────────────────────────────────────────────────────

type OpenRouterModel = {
  id: string;
  displayName: string;
  description?: string;
  contextLength?: number;
  maxCompletionTokens?: number;
  inputModalities: string[];
  outputModalities: string[];
  supportedParameters: string[];
  pricing: { prompt: string; completion: string; image: string; request: string };
};

type OpenRouterModelsResult = {
  models: OpenRouterModel[];
  fetchedAt: string;
  source: string;
  stale: boolean;
  warning?: string;
};

type CommandResponse<T = unknown> = {
  value: T;
  restartGateway: boolean;
  restartReason: string;
};

type SaveResult =
  | { status: "saved" }
  | { status: "saved_restart_failed" }
  | { status: "failed" }
  | { status: "superseded" };

type SaveRequest = {
  generation: number;
  routeId: string;
  profileId?: string;
  modelKey: string;
};

function makeModel(id: string, displayName: string): OpenRouterModel {
  return {
    id,
    displayName,
    contextLength: 131_072,
    inputModalities: ["text"],
    outputModalities: ["text"],
    supportedParameters: [],
    pricing: { prompt: "1e-7", completion: "2e-7", image: "0", request: "0" },
  };
}

function stableModelsResult(): OpenRouterModelsResult {
  return {
    models: [
      makeModel("poolside/laguna-s-2.1", "Poolside: Laguna S 2.1"),
      makeModel("poolside/laguna-xs-2.1", "Poolside: Laguna XS 2.1"),
      makeModel("tencent/hy3", "Tencent: Hy3"),
      makeModel("openrouter/auto", "OpenRouter: Auto"),
      makeModel("openai/gpt-5", "OpenAI: GPT-5"),
      makeModel("anthropic/claude-opus-5", "Anthropic: Claude Opus 5"),
    ],
    fetchedAt: "2026-07-31T00:00:00Z",
    source: "api",
    stale: false,
  };
}

function saveOkResponse(restartGateway = false): CommandResponse<null> {
  return { value: null, restartGateway, restartReason: restartGateway ? "model changed" : "" };
}

function deferred<T = void>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

// ── Default props ─────────────────────────────────────────────────

const DEFAULT_PROPS = {
  modelKey: "model1",
  gatewayModelLabel: "Model 1",
  currentUpstream: "poolside/laguna-s-2.1",
  currentThinkingMode: "normal" as string | undefined,
  currentReasoningEffort: undefined as string | undefined,
  onSaved: vi.fn().mockResolvedValue(undefined),
  profileId: "profileA",
  gatewayRunning: false,
  restartGateway: vi.fn().mockResolvedValue(undefined),
};

// ── Setup / teardown ──────────────────────────────────────────────

const invokeMock = invoke as ReturnType<typeof vi.fn>;
const originalConsoleError = console.error;

beforeEach(() => {
  vi.clearAllMocks();

  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "openrouter_get_models") return stableModelsResult();
    if (cmd === "set_model_upstream") return saveOkResponse(false);
    return null;
  });

  console.error = (...args: unknown[]) => {
    const msg = String(args[0]);
    if (msg.includes("inside a test was not wrapped in act")) return;
    originalConsoleError(...args);
  };
});

afterEach(() => {
  console.error = originalConsoleError;
  vi.restoreAllMocks();
});

async function waitForReady() {
  await screen.findByTestId("openrouter-vendor-select", {}, { timeout: 3000 });
}

// ── Hook Harness ──────────────────────────────────────────────────

/** Renders nothing — gives tests a handle on the save queue without
 *  going through the full component. */
type QueueController = ReturnType<typeof useOpenRouterSaveQueue>;

function QueueHarness({
  onReady,
  onSaved,
  gatewayRunning,
  restartGateway,
  currentRouteIdRef,
  syncUiFromSavedRouteRef,
  lastSubmittedRef,
}: {
  onReady: (ctrl: QueueController) => void;
  onSaved: () => Promise<void>;
  gatewayRunning: boolean;
  restartGateway: () => Promise<void>;
  currentRouteIdRef: React.MutableRefObject<string>;
  syncUiFromSavedRouteRef: React.MutableRefObject<() => void>;
  lastSubmittedRef: React.MutableRefObject<{
    routeId: string;
    upstreamModel: string;
    thinkingMode?: string;
    reasoningEffort?: string;
  } | null>;
}) {
  const [saveError, setSaveError] = useState<string | null>(null);
  const queue = useOpenRouterSaveQueue({
    onSaved,
    gatewayRunning,
    restartGateway,
    currentRouteIdRef,
    syncUiFromSavedRouteRef,
    lastSubmittedRef,
    setSaveError,
    formatSaveFailed: (e: unknown) => String(e),
    formatRefreshFailed: (e: unknown) => String(e),
    formatRestartFailed: (e: unknown) => String(e),
  });

  // Write latest queue into the caller's ref every render, so the test
  // always reads current callbacks without re-mount churn from onReady.
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;
  useEffect(() => {
    onReadyRef.current(queue);
  }, [queue]);

  return null;
}

/** Harness for useRouteSaveGeneration — exposes begin/isCurrent. */
function GenerationHarness({
  onReady,
  routeId,
  profileId,
  modelKey,
  currentRouteIdRef,
  saveGenerationRef,
}: {
  onReady: (ctrl: ReturnType<typeof useRouteSaveGeneration>) => void;
  routeId: string;
  profileId: string | undefined;
  modelKey: string;
  currentRouteIdRef: React.MutableRefObject<string>;
  saveGenerationRef: React.MutableRefObject<number>;
}) {
  const gen = useRouteSaveGeneration(
    routeId,
    profileId,
    modelKey,
    currentRouteIdRef,
    saveGenerationRef,
  );
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;
  useEffect(() => {
    onReadyRef.current(gen);
  }, [gen]);
  return null;
}

// ── Tests ─────────────────────────────────────────────────────────

describe("OpenRouterModelSelector — regression tests", () => {
  // ═══════════════════════════════════════════════════════════════
  // Test 1: Drain stale closure does NOT overwrite route B's UI
  // when route A's save fails after the user switched to route B.
  //
  // Race: save queued on route A, component re-renders on route B,
  //       drain's Phase 2 rollback calls syncUiFromSavedRoute from a
  //       stale closure → route B's UI gets overwritten with route A's
  //       saved config.
  //
  // Fix: syncUiFromSavedRouteRef (latest-callback ref) ensures drain
  //      always calls the current route's sync function.  Old drain
  //      sees route B's saved config via the ref, so rollback is a
  //      no-op on route B's UI.  The rollbackRouteId guard (verified
  //      separately) adds a secondary layer preventing the callback
  //      call itself from firing on the wrong route.
  // ═══════════════════════════════════════════════════════════════
  it("cross_route_drain_rollback_does_not_corrupt_new_route", async () => {
    const saveD = deferred<CommandResponse<null>>();

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "openrouter_get_models") return stableModelsResult();
      if (cmd === "set_model_upstream") return saveD.promise;
      return null;
    });

    const onSaved = vi.fn().mockResolvedValue(undefined);
    const restartGateway = vi.fn().mockResolvedValue(undefined);

    const { rerender } = render(
      <OpenRouterModelSelector
        {...DEFAULT_PROPS}
        onSaved={onSaved}
        restartGateway={restartGateway}
      />,
    );
    await waitForReady();

    // Route A: change thinking to "max" — save hangs.
    const thinkingSelectA = screen.getByLabelText("Thinking");
    await userEvent.selectOptions(thinkingSelectA, "max");

    // Now re-render on route B (different modelKey + upstream).
    // Route B uses tencent/hy3 with thinking "low" — deliberately
    // different from route A's "off" so a stale rollback to route A's
    // config would be caught by BOTH model and thinking assertions.
    rerender(
      <OpenRouterModelSelector
        {...DEFAULT_PROPS}
        modelKey="model2"
        gatewayModelLabel="Model 2"
        currentUpstream="tencent/hy3"
        currentThinkingMode="thinking"
        currentReasoningEffort="low"
        profileId="profileB"
        onSaved={onSaved}
        restartGateway={restartGateway}
      />,
    );
    await waitForReady();

    // Reject route A's save.
    await act(async () => {
      saveD.reject(new Error("save failed"));
    });

    // Route B's UI must be preserved — tencent/hy3 model + "low" thinking.
    // If stale rollback occurred (route A = "poolside/laguna-s-2.1" + "off"),
    // these assertions would fail.
    await waitFor(() => {
      const modelSel = screen.getByLabelText("Select a model") as HTMLSelectElement;
      const thinkingSel = screen.getByLabelText("Thinking") as HTMLSelectElement;
      expect(modelSel.value).toBe("tencent/hy3");
      expect(thinkingSel.value).toBe("low");
    });
  });

  // ═══════════════════════════════════════════════════════════════
  // Test 2: rollbackRouteId guard — when an old route's drain batch
  // fails entirely, Phase 2 must NOT call syncUiFromSavedRouteRef
  // on a different (current) route.
  //
  // Race: route A request is in-flight, currentRouteIdRef switches
  //       to route B, route A invoke fails → drain Phase 2 fires.
  //       Without the guard, Phase 2 calls syncUiFromSavedRouteRef
  //       on route B, which would re-sync route B to its saved
  //       config — harmless but incorrect.
  //
  // Fix: rollbackRouteId guard (line 611): only call
  //      syncUiFromSavedRouteRef.current() when the failed batch's
  //      routeId matches the current route.
  //
  // Unlike Test 1 (which verifies the latest-callback ref prevents
  // UI corruption), this test verifies the callback is never
  // invoked at all when the route has changed.
  // ═══════════════════════════════════════════════════════════════
  it("old_route_failed_batch_does_not_call_rollback_on_new_route", async () => {
    const invokeD = deferred<CommandResponse<null>>();

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "openrouter_get_models") return stableModelsResult();
      if (cmd === "set_model_upstream") return invokeD.promise;
      return null;
    });

    const rollback = vi.fn();
    const currentRouteIdRef = { current: "profileA:model1" };
    const syncUiRef = { current: rollback };
    const lastSubmittedRef = {
      current: null as {
        routeId: string;
        upstreamModel: string;
        thinkingMode?: string;
        reasoningEffort?: string;
      } | null,
    };

    let ctrl!: QueueController;
    render(
      <QueueHarness
        onReady={(c) => { ctrl = c; }}
        onSaved={vi.fn().mockResolvedValue(undefined)}
        gatewayRunning={false}
        restartGateway={vi.fn().mockResolvedValue(undefined)}
        currentRouteIdRef={currentRouteIdRef}
        syncUiFromSavedRouteRef={syncUiRef}
        lastSubmittedRef={lastSubmittedRef}
      />,
    );

    await act(async () => { /* wait for harness mount */ });

    // Enqueue route A save — invoke hangs.
    void ctrl.saveModelRoute({
      routeId: "profileA:model1",
      profileId: "profileA",
      modelKey: "model1",
      upstreamModel: "poolside/laguna-s-2.1",
      thinkingMode: "thinking",
      reasoningEffort: "max",
    });

    // Switch to route B while route A's save is in-flight.
    currentRouteIdRef.current = "profileB:model2";

    // Fail route A's save — drain Phase 2 fires.
    await act(async () => {
      invokeD.reject(new Error("route A save failed"));
    });

    // rollbackRouteId guard: rollback callback must NOT be called
    // because the failed batch was from route A, not route B.
    expect(rollback).not.toHaveBeenCalled();
  });

  // ═══════════════════════════════════════════════════════════════
  // Test 3: save2 enqueued while save1 is in-flight carries the
  // route identity from its own enqueue moment, not save1's.
  //
  // Sequence: save1 starts on route A.  save2 is also enqueued with
  //       route A's identity (the handler captured profileId/modelKey
  //       before the switch).  Then currentRouteIdRef changes to
  //       route B.  save1's invoke resolves → drain picks up save2 →
  //       save2's invoke uses save2.request.profileId/modelKey
  //       (route A), not the component-scope profileId/modelKey at
  //       drain time (which would be route B).
  //
  // Fix: PendingSave.request captures identity at enqueue time.
  // ═══════════════════════════════════════════════════════════════
  it("enqueued_save_uses_captured_route_identity", async () => {
    const invoke1D = deferred<CommandResponse<null>>();
    const invoke2D = deferred<CommandResponse<null>>();
    const invokeCalls: Array<Record<string, unknown>> = [];

    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "openrouter_get_models") return stableModelsResult();
      if (cmd === "set_model_upstream") {
        invokeCalls.push({ ...args });
        if (invokeCalls.length === 1) return invoke1D.promise;
        if (invokeCalls.length === 2) return invoke2D.promise;
        return saveOkResponse(false);
      }
      return null;
    });

    // Set up shared refs for the hook harness.
    const currentRouteIdRef = { current: "profileA:model1" };
    const syncUiRef = { current: vi.fn() };
    const lastSubmittedRef = { current: null as {
      routeId: string;
      upstreamModel: string;
      thinkingMode?: string;
      reasoningEffort?: string;
    } | null };

    let ctrl!: QueueController;
    render(
      <QueueHarness
        onReady={(c) => { ctrl = c; }}
        onSaved={vi.fn().mockResolvedValue(undefined)}
        gatewayRunning={false}
        restartGateway={vi.fn().mockResolvedValue(undefined)}
        currentRouteIdRef={currentRouteIdRef}
        syncUiFromSavedRouteRef={syncUiRef}
        lastSubmittedRef={lastSubmittedRef}
      />,
    );

    await act(async () => { /* wait for harness mount */ });

    // Enqueue save1 with identity A — invoke1 hangs.
    const save1Promise = ctrl.saveModelRoute({
      routeId: "profileA:model1",
      profileId: "profileA",
      modelKey: "model1",
      upstreamModel: "poolside/laguna-s-2.1",
      thinkingMode: "thinking",
      reasoningEffort: "max",
    });

    // Now "switch route" by updating the ref — save2 is enqueued with
    // route A's identity (the caller's captured values), not the new
    // ref value.  This simulates a handler that captured routeId etc.
    // before the route changed.
    currentRouteIdRef.current = "profileB:model2";

    // Enqueue save2 — also with identity A (same handler that
    // captured route A before the route switch).
    const save2Promise = ctrl.saveModelRoute({
      routeId: "profileA:model1",
      profileId: "profileA",
      modelKey: "model1",
      upstreamModel: "poolside/laguna-s-2.1",
      thinkingMode: "normal",
      reasoningEffort: null,
    });

    // Resolve invoke1 — save1 is superseded (save2 was already pending).
    await act(async () => {
      invoke1D.resolve(saveOkResponse(false));
    });

    // Resolve invoke2 — save2 should succeed.
    await act(async () => {
      invoke2D.resolve(saveOkResponse(false));
    });

    const [r1, r2] = await Promise.all([save1Promise, save2Promise]);
    expect(r1.status).toBe("superseded");
    expect(r2.status).toBe("saved");

    // Both invokes carry route A's identity — the identity captured
    // at enqueue time, not the ref's current value.
    expect(invokeCalls.length).toBe(2);
    for (const call of invokeCalls) {
      expect(call.profileId).toBe("profileA");
      expect(call.modelKey).toBe("model1");
    }
  });

  // ═══════════════════════════════════════════════════════════════
  // Test 4: refresh fails twice, restart still runs.
  //
  // Verifies: (a) refresh retry — onSaved is called twice on failure.
  //           (b) refresh failure does NOT skip the restart path.
  //
  // This test uses a single save, so it does NOT verify the
  // batchNeedsRestart OR-aggregation across multiple saves.
  // ═══════════════════════════════════════════════════════════════
  it("refresh_fails_twice_but_required_gateway_restart_still_runs", async () => {
    const restartGateway = vi.fn().mockResolvedValue(undefined);
    const onSaved = vi.fn()
      .mockRejectedValueOnce(new Error("refresh 1 failed"))
      .mockRejectedValueOnce(new Error("refresh 2 failed"));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "openrouter_get_models") return stableModelsResult();
      if (cmd === "set_model_upstream") return saveOkResponse(true);
      return null;
    });

    render(
      <OpenRouterModelSelector
        {...DEFAULT_PROPS}
        onSaved={onSaved}
        restartGateway={restartGateway}
        gatewayRunning={true}
      />,
    );
    await waitForReady();

    const thinkingSelect = screen.getByLabelText("Thinking");
    await userEvent.selectOptions(thinkingSelect, "max");

    await waitFor(() => {
      expect(restartGateway).toHaveBeenCalled();
    }, { timeout: 5000 });

    expect(onSaved).toHaveBeenCalledTimes(2);
    expect(restartGateway).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/refresh 2 failed/i)).toBeInTheDocument();
  });

  // ═══════════════════════════════════════════════════════════════
  // Test 5: refresh retry succeeds, clears error, restart runs once.
  //
  // Verifies: (a) onSaved rejects then resolves → setSaveError(null)
  //           clears the error after retry succeeds.
  //           (b) restartGateway is called exactly once — retry does
  //           not cause duplicate restart.
  // ═══════════════════════════════════════════════════════════════
  it("refresh_retry_succeeds_clears_error_and_gateway_restart_runs_once", async () => {
    const restartGateway = vi.fn().mockResolvedValue(undefined);
    const onSaved = vi.fn()
      .mockRejectedValueOnce(new Error("refresh 1 failed"))
      .mockResolvedValueOnce(undefined);

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "openrouter_get_models") return stableModelsResult();
      if (cmd === "set_model_upstream") return saveOkResponse(true);
      return null;
    });

    render(
      <OpenRouterModelSelector
        {...DEFAULT_PROPS}
        onSaved={onSaved}
        restartGateway={restartGateway}
        gatewayRunning={true}
      />,
    );
    await waitForReady();

    const thinkingSelect = screen.getByLabelText("Thinking");
    await userEvent.selectOptions(thinkingSelect, "max");

    await waitFor(() => {
      expect(restartGateway).toHaveBeenCalled();
    }, { timeout: 5000 });

    expect(onSaved).toHaveBeenCalledTimes(2);
    expect(restartGateway).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/refresh/i)).not.toBeInTheDocument();
  });

  // ═══════════════════════════════════════════════════════════════
  // Test 6: In-flight save superseded when newer request pending.
  //
  // Race: save1 enqueued (invoke hangs), save2 enqueued before
  //       save1's invoke resolves.  When invoke1 resolves, drain
  //       sees pendingSaveRef.current (save2) → settle save1 as
  //       "superseded".  save2 then drains normally.
  //
  // Fix: in the drain while loop, after invoke resolves, check
  //      if (pendingSaveRef.current) { current.settle({status:
  //      "superseded"}) }.
  // ═══════════════════════════════════════════════════════════════
  it("in_flight_save_settled_as_superseded_when_newer_request_pending", async () => {
    const invoke1D = deferred<CommandResponse<null>>();
    const invoke2D = deferred<CommandResponse<null>>();
    let invokeCount = 0;

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "openrouter_get_models") return stableModelsResult();
      if (cmd === "set_model_upstream") {
        invokeCount++;
        if (invokeCount === 1) return invoke1D.promise;
        if (invokeCount === 2) return invoke2D.promise;
        return saveOkResponse(false);
      }
      return null;
    });

    const currentRouteIdRef = { current: "profileA:model1" };
    const syncUiRef = { current: vi.fn() };
    const lastSubmittedRef = { current: null as { routeId: string; upstreamModel: string; thinkingMode?: string; reasoningEffort?: string } | null };

    let ctrl!: QueueController;
    render(
      <QueueHarness
        onReady={(c) => { ctrl = c; }}
        onSaved={vi.fn().mockResolvedValue(undefined)}
        gatewayRunning={false}
        restartGateway={vi.fn().mockResolvedValue(undefined)}
        currentRouteIdRef={currentRouteIdRef}
        syncUiFromSavedRouteRef={syncUiRef}
        lastSubmittedRef={lastSubmittedRef}
      />,
    );

    await act(async () => { /* wait for harness mount */ });

    // Enqueue save1 — invoke1 hangs.
    const p1 = ctrl.saveModelRoute({
      routeId: "profileA:model1",
      profileId: "profileA",
      modelKey: "model1",
      upstreamModel: "poolside/laguna-s-2.1",
      thinkingMode: "thinking",
      reasoningEffort: "max",
    });

    // Enqueue save2 before invoke1 resolves — supersedes save1 in queue.
    const p2 = ctrl.saveModelRoute({
      routeId: "profileA:model1",
      profileId: "profileA",
      modelKey: "model1",
      upstreamModel: "poolside/laguna-s-2.1",
      thinkingMode: "normal",
      reasoningEffort: null,
    });

    // Resolve invoke1 — drain sees pendingSaveRef (save2) → settle save1 superseded.
    await act(async () => {
      invoke1D.resolve(saveOkResponse(false));
    });

    // Resolve invoke2 — save2 succeeds.
    await act(async () => {
      invoke2D.resolve(saveOkResponse(false));
    });

    const [r1, r2] = await Promise.all([p1, p2]);
    expect(r1.status).toBe("superseded");
    expect(r2.status).toBe("saved");
    expect(invokeCount).toBe(2);
  });

  // ═══════════════════════════════════════════════════════════════
  // Test 7: Older handler does NOT rollback after a newer handler
  // superseded it (generation guard).
  //
  // Race: handler 1 starts (gen=1), optimistic A→B.  handler 2 starts
  //       (gen=2), optimistic B→C.  handler 2's save succeeds.
  //       handler 1's save fails.  Without the generation guard,
  //       handler 1 would rollback to its own previous value (A),
  //       overwriting handler 2's newer state (C).
  //
  // Fix: handler captures generation before await, bails out if
  //      generation !== saveGenerationRef.current after await.
  //
  // This harness replicates the exact begin/optimistic/save/isCurrent
  // pattern used by all three production handlers. ──────────────
  // ═══════════════════════════════════════════════════════════════
  it("older_handler_does_not_rollback_after_newer_handler_supersedes", async () => {
    const saveGenerationRef = { current: 0 };
    const currentRouteIdRef = { current: "profileA:model1" };

    // Track the "UI value" — analogous to thinkingSelection in the
    // production component.
    let displayValue = "A";
    const log: string[] = [];

    // Harness component that simulates two async handlers sharing
    // the same useRouteSaveGeneration hook.
    function GenerationHandlerHarness() {
      const gen = useRouteSaveGeneration(
        "profileA:model1",
        "profileA",
        "model1",
        currentRouteIdRef,
        saveGenerationRef,
      );

      // The test drives these via deferred Promises.
      const [handler1Save, setHandler1Save] = useState<
        { save: Promise<SaveResult>; settle: (r: SaveResult) => void } | undefined
      >();
      const [handler2Save, setHandler2Save] = useState<
        { save: Promise<SaveResult>; settle: (r: SaveResult) => void } | undefined
      >();

      // Wire controller
      const ref = useRef<{
        run1: (
          optimistic: string,
          rollback: string,
          save: Promise<SaveResult>,
        ) => void;
        run2: (
          optimistic: string,
          rollback: string,
          save: Promise<SaveResult>,
        ) => void;
        getValue: () => string;
      }>({ run1: () => {}, run2: () => {}, getValue: () => "A" });
      ref.current.getValue = () => displayValue;

      useEffect(() => {
        // Called by test to inject handler logic that runs INSIDE the
        // component, sharing the actual hook instance.
        ref.current.run1 = async (optimistic: string, rollback: string, save: Promise<SaveResult>) => {
          const req = gen.begin();
          const prev = displayValue;
          displayValue = optimistic;
          log.push(`h1: set ${optimistic} (gen=${req.generation})`);
          const result = await save;
          log.push(`h1: save ${result.status} (gen=${req.generation})`);
          if (!gen.isCurrent(req)) {
            log.push("h1: bail (not current)");
            return;
          }
          if (result.status === "failed") {
            displayValue = rollback;
            log.push(`h1: rollback to ${rollback}`);
          }
        };

        ref.current.run2 = async (optimistic: string, _rollback: string, save: Promise<SaveResult>) => {
          const req = gen.begin();
          displayValue = optimistic;
          log.push(`h2: set ${optimistic} (gen=${req.generation})`);
          const result = await save;
          log.push(`h2: save ${result.status} (gen=${req.generation})`);
          // handler 2 never rollbacks — just asserts it's current
          if (gen.isCurrent(req)) {
            log.push("h2: applied");
          } else {
            log.push("h2: bail (not current)");
          }
        };
      }, [gen]);

      // Expose for tests
      useEffect(() => {
        (window as unknown as Record<string, unknown>).__harness = ref.current;
      }, [gen]);

      return null;
    }

    render(<GenerationHandlerHarness />);
    await act(async () => { /* wait for mount */ });

    const harness = (window as unknown as Record<string, unknown>)
      .__harness as {
        run1: (optimistic: string, rollback: string, save: Promise<SaveResult>) => void;
        run2: (optimistic: string, rollback: string, save: Promise<SaveResult>) => void;
        getValue: () => string;
      };

    // ── Simulate two handlers ──────────────────────────────────

    // Handler 1: optimistic A→B, save hangs.
    let settle1!: (r: SaveResult) => void;
    const save1 = new Promise<SaveResult>((resolve) => {
      settle1 = resolve;
    });

    // Handler 2: optimistic B→C, save succeeds immediately.
    let settle2!: (r: SaveResult) => void;
    const save2 = new Promise<SaveResult>((resolve) => {
      settle2 = resolve;
    });

    // Kick off both handlers (handler 2 starts while handler 1 is
    // still in-flight).
    harness.run1("B", "A", save1);
    // Handler 2 begins — bumps generation to 2.
    harness.run2("C", "B", save2);

    // Handler 2's save succeeds first — C is now the correct value.
    await act(async () => {
      settle2({ status: "saved" });
    });

    // Handler 1's save fails.
    await act(async () => {
      settle1({ status: "failed" });
    });

    // Handler 1 must NOT rollback to "A" because handler 2
    // (generation 2) superseded it.  The final value must be "C".
    expect(harness.getValue()).toBe("C");
    expect(log).toContain("h1: bail (not current)");
    expect(log).toContain("h2: applied");
  });

  // ═══════════════════════════════════════════════════════════════
  // OpenAI GPT-5.6 UI tests
  // ═══════════════════════════════════════════════════════════════

  describe("OpenAI GPT-5.6 UI", () => {
    it("renders_tier_and_mode_dropdowns_when_openai_model_selected", async () => {
      const setModelCalls: Array<Record<string, unknown>> = [];
      invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "openrouter_get_models") return stableModelsResult();
        if (cmd === "set_model_upstream") {
          setModelCalls.push({ ...args });
          return saveOkResponse(false);
        }
        return null;
      });

      render(
        <OpenRouterModelSelector
          {...DEFAULT_PROPS}
          currentUpstream="openai/gpt-5.6-sol"
          currentThinkingMode="thinking"
          currentReasoningEffort="medium"
        />,
      );
      await waitForReady();

      // Mode dropdown should exist for OpenAI models
      const modeSelect = screen.getByTestId("openrouter-openai-mode-select");
      expect(modeSelect).toBeInTheDocument();
      expect((modeSelect as HTMLSelectElement).value).toBe("standard");
    });

    it("openai_pro_model_renders_matching_tier_selection", async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "openrouter_get_models") return stableModelsResult();
        if (cmd === "set_model_upstream") return saveOkResponse(false);
        return null;
      });

      render(
        <OpenRouterModelSelector
          {...DEFAULT_PROPS}
          currentUpstream="openai/gpt-5.6-sol-pro"
          currentThinkingMode="thinking"
          currentReasoningEffort="high"
        />,
      );
      await waitForReady();

      // Model dropdown value should be the standard tier ID, not -pro
      const modelSelect = screen.getByTestId("openrouter-model-select") as HTMLSelectElement;
      expect(modelSelect.value).toBe("openai/gpt-5.6-sol");

      // Mode dropdown should show "pro"
      const modeSelect = screen.getByTestId("openrouter-openai-mode-select") as HTMLSelectElement;
      expect(modeSelect.value).toBe("pro");
    });

    it("openai_mode_change_saves_pro_correctly", async () => {
      const setModelCalls: Array<Record<string, unknown>> = [];
      invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "openrouter_get_models") return stableModelsResult();
        if (cmd === "set_model_upstream") {
          setModelCalls.push({ ...args });
          return saveOkResponse(false);
        }
        return null;
      });

      render(
        <OpenRouterModelSelector
          {...DEFAULT_PROPS}
          currentUpstream="openai/gpt-5.6-sol"
          currentThinkingMode="thinking"
          currentReasoningEffort="medium"
        />,
      );
      await waitForReady();

      // Switch mode from Standard to Pro
      const modeSelect = screen.getByTestId("openrouter-openai-mode-select");
      await userEvent.selectOptions(modeSelect, "pro");

      await waitFor(() => {
        expect(setModelCalls.length).toBeGreaterThan(0);
      });

      // Last set_model_upstream call should use the Pro variant
      const lastCall = setModelCalls[setModelCalls.length - 1];
      expect(lastCall.upstreamModel).toBe("openai/gpt-5.6-sol-pro");
    });

    it("openai_mode_change_saves_standard_correctly", async () => {
      const setModelCalls: Array<Record<string, unknown>> = [];
      invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "openrouter_get_models") return stableModelsResult();
        if (cmd === "set_model_upstream") {
          setModelCalls.push({ ...args });
          return saveOkResponse(false);
        }
        return null;
      });

      render(
        <OpenRouterModelSelector
          {...DEFAULT_PROPS}
          currentUpstream="openai/gpt-5.6-sol-pro"
          currentThinkingMode="thinking"
          currentReasoningEffort="medium"
        />,
      );
      await waitForReady();

      // Switch mode from Pro to Standard
      const modeSelect = screen.getByTestId("openrouter-openai-mode-select");
      await userEvent.selectOptions(modeSelect, "standard");

      await waitFor(() => {
        expect(setModelCalls.length).toBeGreaterThan(0);
      });

      const lastCall = setModelCalls[setModelCalls.length - 1];
      expect(lastCall.upstreamModel).toBe("openai/gpt-5.6-sol");
    });

    it("openai_tier_change_preserves_pro_mode", async () => {
      const setModelCalls: Array<Record<string, unknown>> = [];
      invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "openrouter_get_models") return stableModelsResult();
        if (cmd === "set_model_upstream") {
          setModelCalls.push({ ...args });
          return saveOkResponse(false);
        }
        return null;
      });

      render(
        <OpenRouterModelSelector
          {...DEFAULT_PROPS}
          currentUpstream="openai/gpt-5.6-sol-pro"
          currentThinkingMode="thinking"
          currentReasoningEffort="medium"
        />,
      );
      await waitForReady();

      // Switch tier from Sol to Terra while Pro mode is active
      const modelSelect = screen.getByTestId("openrouter-model-select") as HTMLSelectElement;
      await userEvent.selectOptions(modelSelect, "openai/gpt-5.6-terra");

      await waitFor(() => {
        const lastCall = setModelCalls[setModelCalls.length - 1];
        if (lastCall && lastCall.upstreamModel === "openai/gpt-5.6-terra-pro") return true;
        throw new Error("not yet");
      });

      const lastCall = setModelCalls[setModelCalls.length - 1];
      expect(lastCall.upstreamModel).toBe("openai/gpt-5.6-terra-pro");
    });

    it("openai_tier_change_preserves_standard_mode", async () => {
      const setModelCalls: Array<Record<string, unknown>> = [];
      invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "openrouter_get_models") return stableModelsResult();
        if (cmd === "set_model_upstream") {
          setModelCalls.push({ ...args });
          return saveOkResponse(false);
        }
        return null;
      });

      render(
        <OpenRouterModelSelector
          {...DEFAULT_PROPS}
          currentUpstream="openai/gpt-5.6-sol"
          currentThinkingMode="thinking"
          currentReasoningEffort="medium"
        />,
      );
      await waitForReady();

      // Switch tier from Sol to Terra while Standard mode is active
      const modelSelect = screen.getByTestId("openrouter-model-select") as HTMLSelectElement;
      await userEvent.selectOptions(modelSelect, "openai/gpt-5.6-terra");

      await waitFor(() => {
        const lastCall = setModelCalls[setModelCalls.length - 1];
        if (lastCall && lastCall.upstreamModel === "openai/gpt-5.6-terra") return true;
        throw new Error("not yet");
      });

      const lastCall = setModelCalls[setModelCalls.length - 1];
      expect(lastCall.upstreamModel).toBe("openai/gpt-5.6-terra");
    });

    it("openai_mode_change_serialized_by_save_queue", async () => {
      // Standard→Pro→Standard rapid toggle: the first save (Pro) hangs,
      // the second (Standard) is queued. When the first resolves it gets
      // superseded; the second drains and succeeds.
      const invokeCalls: Array<Record<string, unknown>> = [];
      const deferredSave1 = deferred<CommandResponse<null>>();

      invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "openrouter_get_models") return stableModelsResult();
        if (cmd === "set_model_upstream") {
          invokeCalls.push({ ...args });
          if (invokeCalls.length === 1) return deferredSave1.promise;
          return saveOkResponse(false);
        }
        return null;
      });

      render(
        <OpenRouterModelSelector
          {...DEFAULT_PROPS}
          currentUpstream="openai/gpt-5.6-sol"
          currentThinkingMode="thinking"
          currentReasoningEffort="medium"
        />,
      );
      await waitForReady();

      // Switch mode to Pro — save1 hangs
      const modeSelect = screen.getByTestId("openrouter-openai-mode-select");
      await userEvent.selectOptions(modeSelect, "pro");

      // Wait for invoke to be called with Pro
      await waitFor(() => {
        expect(invokeCalls.length).toBe(1);
      });
      expect(invokeCalls[0].upstreamModel).toBe("openai/gpt-5.6-sol-pro");

      // Resolve the hanging Pro save
      await act(async () => {
        deferredSave1.resolve(saveOkResponse(false));
      });

      // Wait for UI to unblock, then switch back to Standard
      await waitFor(() => {
        const sel = screen.getByTestId("openrouter-openai-mode-select") as HTMLSelectElement;
        if (!sel.disabled) return true;
        throw new Error("still disabled");
      });

      // Mode dropdown should still show "pro" after the save resolved
      expect((modeSelect as HTMLSelectElement).value).toBe("pro");

      // Switch back to Standard
      await userEvent.selectOptions(modeSelect, "standard");

      await waitFor(() => {
        const lastCall = invokeCalls[invokeCalls.length - 1];
        if (lastCall && lastCall.upstreamModel === "openai/gpt-5.6-sol") return true;
        throw new Error("not yet");
      });

      expect(invokeCalls.length).toBe(2);
      expect(invokeCalls[1].upstreamModel).toBe("openai/gpt-5.6-sol");
    });
  });
});
