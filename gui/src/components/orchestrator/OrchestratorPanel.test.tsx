import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import React from "react";
import { invoke } from "@tauri-apps/api/core";
import OrchestratorPanel from "./OrchestratorPanel";
import { LanguageProvider } from "../../i18n";
import { DEFAULT_ORCHESTRATOR_PROFILES, DEFAULT_ORCHESTRATOR_QUICK_SLOTS, DEFAULT_VALIDATION_GATES, DEFAULT_ITERATION_LIMITS } from "../../config/orchestratorPresets";
import { translations as enTranslations } from "../../i18n/lang/en";
import { translations as jaTranslations } from "../../i18n/lang/ja";
import { translations as deTranslations } from "../../i18n/lang/de";
import { translations as esTranslations } from "../../i18n/lang/es";
import { translations as frTranslations } from "../../i18n/lang/fr";
import { translations as koTranslations } from "../../i18n/lang/ko";
import { translations as zhCNTranslations } from "../../i18n/lang/zh-CN";
import { translations as zhTWTranslations } from "../../i18n/lang/zh-TW";

const eventHandlers = new Map<string, (event: { payload: any }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: any }) => void) => {
    eventHandlers.set(name, handler);
    return () => eventHandlers.delete(name);
  }),
}));

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

/** Typed waiting-candidate payload exactly as the backend publishes it. */
function convergenceWaitingPayload(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    runId: "convergence-1",
    revision: 7,
    candidateId: "cand-abcdef12",
    sequence: 1,
    title: "Converged Revision 2",
    planText: "# Revision 2 Plan\n\nCreated by human confirmation.",
    intent: "new_primary",
    proposedRevision: 2,
    ...overrides,
  });
}

describe("OrchestratorPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    eventHandlers.clear();
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_user_language") {
        return "en";
      }
      if (cmd === "preview_plan_archive") {
        return { nextFileName: "V0.24.0-r1.md" };
      }
      if (cmd === "list_interrupted_runs") {
        return [];
      }
      if (cmd === "get_orchestrator_config") {
        return {
          projectPath: "C:\\mock\\project",
          activeWorkflowId: "full_loop",
          activePresetId: "balanced",
          assignments: {
            planner: { role: "planner", profileId: "mimo-v26-pro" },
            plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
            implementer: { role: "implementer", profileId: "codex-cli" },
            fixer: { role: "fixer", profileId: "codex-cli", escalationRole: "implementer" },
            code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
          },
          validationGates: DEFAULT_VALIDATION_GATES,
          iterationLimits: DEFAULT_ITERATION_LIMITS,
          quickSlots: DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
        };
      }
      if (cmd === "detect_project_metadata") {
        const p = args?.projectPath || "C:\\mock\\project";
        return {
          path: p,
          exists: true,
          isDirectory: true,
          projectType: "Rust",
          detectedFiles: {
            specMd: true,
            implementationPlanMd: false,
            agentsMd: true,
            readmeMd: true,
            packageJson: false,
            pyprojectToml: false,
            cargoToml: true,
            git: true,
          },
        };
      }
      if (cmd === "update_orchestrator_config") {
        return null;
      }
      return null;
    });
  });

  it("renders OrchestratorPanel and loads initial config", async () => {
    render(
      <LanguageProvider>
        <OrchestratorPanel />
      </LanguageProvider>
    );

    await waitFor(() => {
      expect(screen.getByDisplayValue("C:\\mock\\project")).toBeDefined();
    });
  });

  it("starts opt-in convergence through Tauri and hides impossible waiting controls", async () => {
    const original = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "start_plan_convergence") return { runId: "convergence-1" };
      return original(cmd, args);
    });
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    const toggle = screen.getByRole("switch", { name: "orchestrator.planConvergence.optIn" });
    expect(toggle).toHaveAttribute("aria-checked", "false");
    fireEvent.click(toggle);
    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Revise the plan" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("start_plan_convergence", expect.objectContaining({
      taskPrompt: "Revise the plan", convergenceConfig: { optIn: true, totalTimeoutSecs: 900 },
    })));
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "start_orchestrator_run")).toBe(false);
    expect(screen.queryByRole("button", { name: /orchestrator\.exec\.pauseBtn/ })).toBeNull();
    act(() => eventHandlers.get("orchestrator:step_update")?.({ payload: {
      runId: "convergence-1", step: "waiting_for_user", waitingReason: "NEW_PRIMARY_PLAN_CONFIRMATION",
      message: "RAW BACKEND PROSE MUST NOT LEAK",
      planText: convergenceWaitingPayload({ runId: "convergence-1" }),
    } }));
    expect(screen.getByText("orchestrator.planConvergence.waitingReason.NEW_PRIMARY_PLAN_CONFIRMATION")).toBeInTheDocument();
    expect(screen.getByText("Converged Revision 2")).toBeInTheDocument();
    expect(screen.queryByText("Submit Clarification")).toBeNull();
    expect(screen.queryByText("Approve & Complete")).toBeNull();
    expect(screen.queryByText(/RAW BACKEND PROSE/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.resetBtn/ }));
    expect(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ })).toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "resolve_human_gate" || cmd === "submit_clarification")).toBe(false);
  });

  it("loads recovery summaries without preflight and requires explicit confirmation before restore", async () => {
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_user_language") return "en";
      if (cmd === "list_interrupted_runs") return [{
        runId: "run-interrupted-1", workflowType: "human_gated_loop", projectPath: "C:\\mock\\project",
        status: "interrupted", currentState: "implementation", lastSuccessfulState: "plan_review",
        revision: 4, createdAtUnix: 1, updatedAtUnix: 2, isResumable: false,
      }];
      if (cmd === "preflight_run_recovery") return {
        runId: args.runId, journalRevision: 4, checkpointId: "checkpoint-1", checkpointKind: "entry_baseline",
        checkpointStage: "implementation", checkpointDigest: "digest", currentFingerprint: "current-fingerprint",
        backupId: "backup-candidate-1", backupDestination: "C:\\runs\\run-interrupted-1\\backups\\backup-candidate-1",
        workspaceMatches: false, canResume: false, canRestore: true, canAdopt: false, affectedPaths: ["src/main.rs", ".git/index (staged state)"],
      };
      if (cmd === "resolve_run_recovery") return { backupId: "backup-1", checkpointId: "checkpoint-1", resumeStage: "implementation", journalRevision: 4 };
      if (cmd === "get_orchestrator_config") return {
        projectPath: "C:\\mock\\project", activeWorkflowId: "full_loop", activePresetId: "balanced",
        assignments: { planner: { role: "planner", profileId: "mimo-v26-pro" }, plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" }, implementer: { role: "implementer", profileId: "codex-cli" }, fixer: { role: "fixer", profileId: "codex-cli", escalationRole: "implementer" }, code_reviewer: { role: "code_reviewer", profileId: "codex-cli" } },
        validationGates: DEFAULT_VALIDATION_GATES, iterationLimits: DEFAULT_ITERATION_LIMITS, quickSlots: DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
      };
      if (cmd === "detect_project_metadata") return {
        path: args?.projectPath || "C:\\mock\\project", exists: true, isDirectory: true, projectType: "Rust",
        detectedFiles: { specMd: true, implementationPlanMd: false, agentsMd: true, readmeMd: true, packageJson: false, pyprojectToml: false, cargoToml: true, git: true },
        suggestedGates: [],
      };
      if (cmd === "preview_plan_archive") return { nextFileName: "V0.24.0-r1.md" };
      return null;
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    expect(await screen.findByText("human_gated_loop · implementation · run-interrupted-1")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("list_interrupted_runs");
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "preflight_run_recovery")).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "orchestrator.recovery.inspect" }));
    expect(await screen.findByText("orchestrator.recovery.drift")).toBeInTheDocument();
    expect(screen.getByText("orchestrator.recovery.affectedPaths: 2")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "orchestrator.recovery.adopt" })).toBeDisabled();
    expect(invokeMock).toHaveBeenCalledWith("preflight_run_recovery", { runId: "run-interrupted-1" });

    fireEvent.click(screen.getByRole("button", { name: "orchestrator.recovery.restore" }));
    const confirmation = await screen.findByRole("alertdialog");
    expect(confirmation).toBeInTheDocument();
    expect(within(confirmation).getByText("C:\\runs\\run-interrupted-1\\backups\\backup-candidate-1")).toBeInTheDocument();
    expect(within(confirmation).getByText("src/main.rs")).toBeInTheDocument();
    expect(within(confirmation).getByText(".git/index (staged state)")).toBeInTheDocument();
    expect(within(confirmation).getByText("orchestrator.recovery.restoreScope")).toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "resolve_run_recovery")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "orchestrator.recovery.confirmProceed" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("resolve_run_recovery", {
      runId: "run-interrupted-1", choice: "restore", expectedRevision: 4,
      expectedFingerprint: "current-fingerprint", expectedBackupId: "backup-candidate-1", confirmed: true,
    }));
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "start_orchestrator_run")).toBe(false);
  });

  it("requires confirmation and starts only a preflight-approved bounded resume route", async () => {
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_user_language") return "en";
      if (cmd === "list_interrupted_runs") return [{
        runId: "run-resume-1", workflowType: "human_gated_loop", projectPath: "C:\\mock\\project",
        status: "interrupted", currentState: "implementation", revision: 8, createdAtUnix: 1,
        updatedAtUnix: 2, isResumable: true,
      }];
      if (cmd === "preflight_run_recovery") return {
        runId: args.runId, journalRevision: 8, checkpointId: "checkpoint-impl", checkpointKind: "entry_baseline",
        checkpointStage: "implementation", resumeStage: "implementation", checkpointDigest: "digest",
        currentFingerprint: "fingerprint", backupId: "backup", backupDestination: "C:\\runs\\backup",
        workspaceMatches: true, canResume: true, canRestore: false, canAdopt: false, affectedPaths: [],
      };
      if (cmd === "resume_interrupted_run") return { runId: "run-resume-1" };
      if (cmd === "get_orchestrator_config") return {
        projectPath: "C:\\mock\\project", activeWorkflowId: "full_loop", activePresetId: "balanced",
        assignments: { planner: { role: "planner", profileId: "mimo-v26-pro" }, plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" }, implementer: { role: "implementer", profileId: "codex-cli" }, fixer: { role: "fixer", profileId: "codex-cli", escalationRole: "implementer" }, code_reviewer: { role: "code_reviewer", profileId: "codex-cli" } },
        validationGates: DEFAULT_VALIDATION_GATES, iterationLimits: DEFAULT_ITERATION_LIMITS, quickSlots: DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
      };
      if (cmd === "detect_project_metadata") return { path: args?.projectPath || "C:\\mock\\project", exists: true, isDirectory: true, projectType: "Rust", detectedFiles: { specMd: true, implementationPlanMd: false, agentsMd: true, readmeMd: true, packageJson: false, pyprojectToml: false, cargoToml: true, git: true }, suggestedGates: [] };
      if (cmd === "preview_plan_archive") return { nextFileName: "V0.24.0-r1.md" };
      return null;
    });
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    fireEvent.click(await screen.findByRole("button", { name: "orchestrator.recovery.inspect" }));
    const resumeButton = await screen.findByRole("button", { name: "orchestrator.recovery.resume" });
    expect(resumeButton).toBeEnabled();
    fireEvent.click(resumeButton);
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("orchestrator.recovery.confirmResume")).toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "resume_interrupted_run")).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", { name: "orchestrator.recovery.confirmProceed" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("resume_interrupted_run", {
      runId: "run-resume-1", expectedRevision: 8, expectedFingerprint: "fingerprint", confirmedWorkerStopped: true,
    }));
  });

  it("defaults archive folder to project .plan and sends it only with the run", async () => {
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    const archiveDirectory = screen.getByLabelText("orchestrator.project.planFile");
    expect(archiveDirectory).toHaveValue("C:\\mock\\project\\.plan");
    expect(await screen.findByText("V0.24.0-r1.md")).toBeInTheDocument();

    fireEvent.change(archiveDirectory, { target: { value: "C:\\mock\\project\\docs\\plans" } });
    expect(invokeMock.mock.calls
      .filter(([cmd]) => cmd === "update_orchestrator_config")
      .every(([, args]) => !JSON.stringify(args).includes("docs\\plans")))
      .toBe(true);

    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Write approved plan" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "start_orchestrator_run",
      expect.objectContaining({
        planArchiveOptions: { directory: "C:\\mock\\project\\docs\\plans" },
      }),
    ));
  });

  it("preserves a custom archive folder through metadata refresh and resets it when the project changes", async () => {
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    const customPath = "C:\\mock\\project\\docs\\plans";
    fireEvent.change(screen.getByLabelText("orchestrator.project.planFile"), { target: { value: customPath } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.project\.detectBtn/ }));
    await waitFor(() => expect(screen.getByLabelText("orchestrator.project.planFile")).toHaveValue(customPath));

    fireEvent.change(document.querySelector(".orchestrator-project-selector .orchestrator-path-input")!, {
      target: { value: "C:\\mock\\other-project" },
    });
    expect(screen.getByLabelText("orchestrator.project.planFile")).toHaveValue("C:\\mock\\other-project\\.plan");
  });

  it("uses backend preview only for display and never asks for overwrite confirmation", async () => {
    const confirmSpy = vi.spyOn(window, "confirm");
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    expect(await screen.findByText("V0.24.0-r1.md")).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Overwrite with approved plan" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "start_orchestrator_run",
      expect.objectContaining({ planArchiveOptions: { directory: "C:\\mock\\project\\.plan" } }),
    ));
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "inspect_plan_output_target")).toBe(false);
    expect(confirmSpy).not.toHaveBeenCalled();
  });

  it("does not require a plan path or inspect a target for Implement Only", async () => {
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.implementOnly/ }));
    expect(screen.queryByLabelText("orchestrator.project.planFile")).not.toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Implementation only" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "start_orchestrator_run",
      expect.objectContaining({ workflowType: "implement_only", planArchiveOptions: null }),
    ));
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "inspect_plan_output_target")).toBe(false);
  });

  it("enables all recognized workflow modes and switches active workflow", async () => {
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    expect(screen.getByRole("tab", { name: "orchestrator.workflow.fullLoop" })).toHaveAttribute("aria-selected", "true");
    for (const key of ["planOnly", "implementOnly", "reviewOnly"]) {
      const tab = screen.getByRole("tab", { name: new RegExp(`orchestrator\\.workflow\\.${key}`) });
      expect(tab).not.toBeDisabled();
    }

    fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({ config: expect.objectContaining({ activeWorkflowId: "plan_only" }) }),
    ));
  });

  it("scopes visible role cards to active workflow", async () => {
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    // full_loop has all 5 roles
    expect(screen.getByRole("region", { name: "orchestrator.roles.planner" })).toBeDefined();
    expect(screen.getByRole("region", { name: "orchestrator.roles.implementer" })).toBeDefined();
    expect(screen.getByRole("region", { name: "orchestrator.roles.codeReviewer" })).toBeDefined();

    // switch to plan_only -> only planner and plan_reviewer
    fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));
    expect(screen.getByRole("region", { name: "orchestrator.roles.planner" })).toBeDefined();
    expect(screen.getByRole("region", { name: "orchestrator.roles.planReviewer" })).toBeDefined();
    expect(screen.queryByRole("region", { name: "orchestrator.roles.implementer" })).toBeNull();
    expect(screen.queryByRole("region", { name: "orchestrator.roles.codeReviewer" })).toBeNull();

    // switch to review_only -> only code_reviewer
    fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.reviewOnly/ }));
    expect(screen.queryByRole("region", { name: "orchestrator.roles.planner" })).toBeNull();
    expect(screen.getByRole("region", { name: "orchestrator.roles.codeReviewer" })).toBeDefined();
  });

  it.each([
    ["plan_only", "planOnly"],
    ["implement_only", "implementOnly"],
    ["review_only", "reviewOnly"],
  ] as const)("preserves persisted %s workflow during unrelated workspace saves", async (workflow, labelKey) => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return { ...(await originalInvoke(cmd, args)), activeWorkflowId: workflow };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    expect(screen.getByRole("tab", { name: `orchestrator.workflow.${labelKey}` }))
      .toHaveAttribute("aria-selected", "true");

    fireEvent.change(screen.getByDisplayValue("C:\\mock\\project"), {
      target: { value: "C:\\mock\\updated" },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({
        config: expect.objectContaining({
          projectPath: "C:\\mock\\updated",
          activeWorkflowId: workflow,
        }),
      }),
    ));
    expect(invokeMock.mock.calls.some(([cmd, args]) =>
      cmd === "update_orchestrator_config" && args?.config?.activeWorkflowId === "full_loop",
    )).toBe(false);
  });

  it("renders all 6 roles for human_gated_loop workflow and applies the default human-gated preset on button click", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "human_gated_loop",
          profiles: DEFAULT_ORCHESTRATOR_PROFILES,
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    // All 6 roles should be rendered
    expect(screen.getByRole("region", { name: "orchestrator.roles.planner" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "orchestrator.roles.planIntegrator" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "orchestrator.roles.planReviewer" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "orchestrator.roles.implementer" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "orchestrator.roles.fixer" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "orchestrator.roles.codeReviewer" })).toBeInTheDocument();

    // Click Apply Default Configuration button
    const applyDefaultBtn = screen.getByRole("button", { name: "orchestrator.presets.applyDefault" });
    fireEvent.click(applyDefaultBtn);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({
        config: expect.objectContaining({
          activePresetId: "human-gated-development-loop",
          assignments: {
            planner: { role: "planner", profileId: "deepseek-v41-flash" },
            plan_integrator: { role: "plan_integrator", profileId: "antigravity-harness" },
            plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
            implementer: { role: "implementer", profileId: "antigravity-harness" },
            fixer: { role: "fixer", profileId: "antigravity-harness", escalationRole: "implementer" },
            code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
          },
          iterationLimits: {
            maxPlanReviewIterations: 3,
            maxFixIterations: 5,
            maxCodeReviewIterations: 3,
          },
        }),
      }),
    ));
  });

  it("applies human-gated preset with custom Antigravity profile when canonical antigravity-harness is missing", async () => {
    const customAntigravity = {
      id: "custom-ag-profile-55",
      displayName: "Custom Antigravity",
      adapter: "antigravity" as const,
      capabilities: ["workspace_read", "workspace_write", "command_execution", "reasoning"] as any[],
    };
    const profilesWithoutCanonical = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.id !== "antigravity-harness").concat(customAntigravity);

    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "human_gated_loop",
          profiles: profilesWithoutCanonical,
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    const applyDefaultBtn = screen.getByRole("button", { name: "orchestrator.presets.applyDefault" });
    fireEvent.click(applyDefaultBtn);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({
        config: expect.objectContaining({
          activePresetId: "human-gated-development-loop",
          assignments: expect.objectContaining({
            plan_integrator: expect.objectContaining({ profileId: "custom-ag-profile-55" }),
            implementer: expect.objectContaining({ profileId: "custom-ag-profile-55" }),
            fixer: expect.objectContaining({ profileId: "custom-ag-profile-55" }),
          }),
        }),
      }),
    ));
  });

  it("fails atomically without updating state or saving config when no compatible Antigravity profile exists", async () => {
    const profilesWithoutAntigravity = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.adapter !== "antigravity");

    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "human_gated_loop",
          activePresetId: "custom",
          profiles: profilesWithoutAntigravity,
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    invokeMock.mockClear();

    const applyDefaultBtn = screen.getByRole("button", { name: "orchestrator.presets.applyDefault" });
    fireEvent.click(applyDefaultBtn);

    // A localized message is rendered, not resolver diagnostic prose.
    expect(await screen.findByText("orchestrator.presets.presetApplyError")).toBeInTheDocument();
    expect(screen.queryByText(/No compatible Google Antigravity profile found/i)).not.toBeInTheDocument();
    expect(document.querySelector(".orchestrator-preset-error-banner")).toHaveTextContent(
      "orchestrator.presets.presetApplyError"
    );

    // No config write was performed
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "update_orchestrator_config")).toBe(false);
  });

  it("fails atomically without updating state or saving config when a required non-Antigravity profile is missing", async () => {
    const profilesWithoutCodex = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.id !== "codex-cli");

    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "human_gated_loop",
          activePresetId: "custom",
          profiles: profilesWithoutCodex,
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    invokeMock.mockClear();

    const applyDefaultBtn = screen.getByRole("button", { name: "orchestrator.presets.applyDefault" });
    fireEvent.click(applyDefaultBtn);

    // Resolver diagnostics are not shown as user-facing English text.
    expect(await screen.findByText("orchestrator.presets.presetApplyError")).toBeInTheDocument();
    expect(screen.queryByText(/Profile not found for role "code_reviewer": codex-cli/i)).not.toBeInTheDocument();
    expect(document.querySelector(".orchestrator-preset-error-banner")).toHaveTextContent(
      "orchestrator.presets.presetApplyError"
    );

    // No config write was performed
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "update_orchestrator_config")).toBe(false);
  });

  it.each([
    ["en", enTranslations],
    ["ja", jaTranslations],
    ["de", deTranslations],
    ["es", esTranslations],
    ["fr", frTranslations],
    ["ko", koTranslations],
    ["zh-CN", zhCNTranslations],
    ["zh-TW", zhTWTranslations],
  ])("defines a translated preset-resolution error for %s", (_lang, translations) => {
    const message = translations["orchestrator.presets.presetApplyError"];
    expect(message).toBeTruthy();
    expect(message).not.toBe("orchestrator.presets.presetApplyError");
  });

  it("clears a prior preset error after a later successful application", async () => {
    const profilesWithoutAntigravity = DEFAULT_ORCHESTRATOR_PROFILES.filter((p) => p.adapter !== "antigravity");
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "human_gated_loop",
          profiles: profilesWithoutAntigravity,
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    const applyButton = screen.getByRole("button", { name: "orchestrator.presets.applyDefault" });
    fireEvent.click(applyButton);
    expect(await screen.findByText("orchestrator.presets.presetApplyError")).toBeInTheDocument();

    const canonicalAntigravity = DEFAULT_ORCHESTRATOR_PROFILES.find((p) => p.id === "antigravity-harness");
    expect(canonicalAntigravity).toBeDefined();
    profilesWithoutAntigravity.push(canonicalAntigravity!);
    fireEvent.click(applyButton);

    await waitFor(() => {
      expect(document.querySelector(".orchestrator-preset-error-banner")).not.toBeInTheDocument();
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", expect.anything());
    });
  });

  it.each([
    ["plan_integrator", "planIntegrator"],
    ["implementer", "implementer"],
    ["fixer", "fixer"],
  ] as const)(
    "allows selecting antigravity-harness for %s in human_gated_loop without changing unrelated assignments",
    async (role, regionKey) => {
      const baselineAssignments = {
        planner: { role: "planner" as const, profileId: "mimo-v26-pro" },
        plan_integrator: { role: "plan_integrator" as const, profileId: "codex-cli" },
        plan_reviewer: { role: "plan_reviewer" as const, profileId: "deepseek-v41-flash" },
        implementer: { role: "implementer" as const, profileId: "codex-cli" },
        fixer: { role: "fixer" as const, profileId: "codex-cli", escalationRole: "implementer" as const },
        code_reviewer: { role: "code_reviewer" as const, profileId: "codex-cli" },
      };
      const expectedAssignments = {
        ...baselineAssignments,
        [role]: { ...baselineAssignments[role], profileId: "antigravity-harness" },
      };
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          return {
            ...(await originalInvoke(cmd, args)),
            activeWorkflowId: "human_gated_loop",
            assignments: baselineAssignments,
            profiles: DEFAULT_ORCHESTRATOR_PROFILES,
          };
        }
        return originalInvoke(cmd, args);
      });

      render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
      await screen.findByDisplayValue("C:\\mock\\project");

      const roleCard = screen.getByRole("region", { name: `orchestrator.roles.${regionKey}` });
      const selectTrigger = within(roleCard).getByRole("combobox");
      fireEvent.click(selectTrigger);

      const listbox = within(roleCard).getByRole("listbox");
      const antigravityOption = within(listbox).getByRole("option", { name: /Google Antigravity Harness/ });
      expect(antigravityOption).toBeInTheDocument();
      fireEvent.click(antigravityOption);

      await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
        "update_orchestrator_config",
        expect.objectContaining({
          config: expect.objectContaining({
            assignments: expectedAssignments,
          }),
        }),
      ));
    }
  );

  it("preserves custom assignments and custom preset marker when switching workflow tabs and only applies defaults via button", async () => {
    const customAssignments = {
      planner: { role: "planner" as const, profileId: "openrouter-gpt-56-sol" },
      plan_integrator: { role: "plan_integrator" as const, profileId: "antigravity-harness" },
      plan_reviewer: { role: "plan_reviewer" as const, profileId: "kimi-k3" },
      implementer: { role: "implementer" as const, profileId: "codex-cli" },
      fixer: { role: "fixer" as const, profileId: "codex-cli", escalationRole: "implementer" as const },
      code_reviewer: { role: "code_reviewer" as const, profileId: "ollama-mimo-9b" },
    };

    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "full_loop",
          activePresetId: "custom",
          assignments: customAssignments,
          profiles: DEFAULT_ORCHESTRATOR_PROFILES,
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    // Switch to human_gated_loop tab
    fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.humanGatedLoop/ }));

    // Must persist the new workflow ID while preserving custom assignments and activePresetId="custom"
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({
        config: expect.objectContaining({
          activeWorkflowId: "human_gated_loop",
          activePresetId: "custom",
          assignments: customAssignments,
        }),
      }),
    ));

    // Confirm default preset was NOT silently applied
    expect(invokeMock.mock.calls.some(([cmd, args]) =>
      cmd === "update_orchestrator_config" && args?.config?.activePresetId === "human-gated-development-loop"
    )).toBe(false);

    // Now click Apply Default Configuration button
    const applyDefaultBtn = screen.getByRole("button", { name: "orchestrator.presets.applyDefault" });
    fireEvent.click(applyDefaultBtn);

    // Now default preset is applied explicitly
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({
        config: expect.objectContaining({
          activeWorkflowId: "human_gated_loop",
          activePresetId: "human-gated-development-loop",
          assignments: {
            planner: { role: "planner", profileId: "deepseek-v41-flash" },
            plan_integrator: { role: "plan_integrator", profileId: "antigravity-harness" },
            plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
            implementer: { role: "implementer", profileId: "antigravity-harness" },
            fixer: { role: "fixer", profileId: "antigravity-harness", escalationRole: "implementer" },
            code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
          },
        }),
      }),
    ));
  });

  it("retains expected role list and assignment behavior for non-Human-Gated workflows when canonical Antigravity profile is present", async () => {
    const baselineAssignments = {
      planner: { role: "planner" as const, profileId: "mimo-v26-pro" },
      plan_reviewer: { role: "plan_reviewer" as const, profileId: "deepseek-v41-flash" },
      implementer: { role: "implementer" as const, profileId: "codex-cli" },
      fixer: { role: "fixer" as const, profileId: "codex-cli", escalationRole: "implementer" as const },
      code_reviewer: { role: "code_reviewer" as const, profileId: "codex-cli" },
    };

    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "full_loop",
          activePresetId: "balanced",
          assignments: baselineAssignments,
          profiles: DEFAULT_ORCHESTRATOR_PROFILES,
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    // full_loop has exactly the standard 5 roles, and plan_integrator is NOT rendered
    const plannerCard = screen.getByRole("region", { name: "orchestrator.roles.planner" });
    const planReviewerCard = screen.getByRole("region", { name: "orchestrator.roles.planReviewer" });
    const implementerCard = screen.getByRole("region", { name: "orchestrator.roles.implementer" });
    const fixerCard = screen.getByRole("region", { name: "orchestrator.roles.fixer" });
    const codeReviewerCard = screen.getByRole("region", { name: "orchestrator.roles.codeReviewer" });

    expect(plannerCard).toBeInTheDocument();
    expect(planReviewerCard).toBeInTheDocument();
    expect(implementerCard).toBeInTheDocument();
    expect(fixerCard).toBeInTheDocument();
    expect(codeReviewerCard).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "orchestrator.roles.planIntegrator" })).not.toBeInTheDocument();

    // Preserves baseline assignments for all five rendered roles exactly
    expect(within(plannerCard).getByRole("combobox")).toHaveTextContent("mimo-v2.6-pro + thinking");
    expect(within(planReviewerCard).getByRole("combobox")).toHaveTextContent("deepseek-v4.1-flash + thinking: High");
    expect(within(implementerCard).getByRole("combobox")).toHaveTextContent("codex-cli");
    expect(within(fixerCard).getByRole("combobox")).toHaveTextContent("codex-cli");
    expect(within(codeReviewerCard).getByRole("combobox")).toHaveTextContent("codex-cli");

    // No silent application of human-gated preset or assignment rewrites on load
    const updateCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "update_orchestrator_config");
    for (const [, args] of updateCalls) {
      if (args?.config?.activePresetId !== undefined) {
        expect(args.config.activePresetId).toBe("balanced");
      }
      if (args?.config?.activeWorkflowId !== undefined) {
        expect(args.config.activeWorkflowId).toBe("full_loop");
      }
      if (args?.config?.assignments !== undefined) {
        expect(args.config.assignments).toEqual(baselineAssignments);
      }
    }
    expect(updateCalls.some(([, args]) => args?.config?.activePresetId === "human-gated-development-loop")).toBe(false);
  });

  it("updates selected standard role assignment in full_loop without affecting other roles or adding plan_integrator", async () => {
    const baselineAssignments = {
      planner: { role: "planner" as const, profileId: "mimo-v26-pro" },
      plan_reviewer: { role: "plan_reviewer" as const, profileId: "deepseek-v41-flash" },
      implementer: { role: "implementer" as const, profileId: "codex-cli" },
      fixer: { role: "fixer" as const, profileId: "codex-cli", escalationRole: "implementer" as const },
      code_reviewer: { role: "code_reviewer" as const, profileId: "codex-cli" },
    };

    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "full_loop",
          activePresetId: "balanced",
          assignments: baselineAssignments,
          profiles: DEFAULT_ORCHESTRATOR_PROFILES,
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    const plannerCard = screen.getByRole("region", { name: "orchestrator.roles.planner" });
    const selectTrigger = within(plannerCard).getByRole("combobox");
    fireEvent.click(selectTrigger);

    const listbox = within(plannerCard).getByRole("listbox");
    const option = within(listbox).getByRole("option", { name: /deepseek-v4\.1-flash/ });
    fireEvent.click(option);

    const expectedAssignments = {
      planner: { role: "planner" as const, profileId: "deepseek-v41-flash" },
      plan_reviewer: { role: "plan_reviewer" as const, profileId: "deepseek-v41-flash" },
      implementer: { role: "implementer" as const, profileId: "codex-cli" },
      fixer: { role: "fixer" as const, profileId: "codex-cli", escalationRole: "implementer" as const },
      code_reviewer: { role: "code_reviewer" as const, profileId: "codex-cli" },
    };

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({
        config: expect.objectContaining({
          activeWorkflowId: "full_loop",
          activePresetId: "custom",
          assignments: expectedAssignments,
        }),
      }),
    ));

    // Confirm plan_integrator card remains absent
    expect(screen.queryByRole("region", { name: "orchestrator.roles.planIntegrator" })).not.toBeInTheDocument();
  });

  it("enforces read-only profile validation for review_only workflow", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "review_only",
          assignments: {
            planner: { role: "planner", profileId: "mimo-v26-pro" },
            plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
            implementer: { role: "implementer", profileId: "codex-cli" },
            fixer: { role: "fixer", profileId: "codex-cli" },
            code_reviewer: { role: "code_reviewer", profileId: "codex-cli" }, // CLI is mutating/not read-only!
          },
        };
      }
      if (cmd === "start_orchestrator_run") return { runId: "review-run" };
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    // Should show validation error that review-only requires read-only adapter
    expect(screen.getByRole("alert")).toBeDefined();
    expect(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ })).toBeDisabled();

    // Select DeepSeek Flash (read-only Provider adapter)
    const reviewerCard = screen.getByRole("region", { name: "orchestrator.roles.codeReviewer" });
    const reviewerSelect = within(reviewerCard).getByRole("combobox");
    fireEvent.click(reviewerSelect);
    fireEvent.click(within(reviewerCard).getByRole("option", { name: /deepseek-v4\.1-flash/ }));

    await waitFor(() => {
      expect(screen.queryByRole("alert")).toBeNull();
    });

    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Review task" } });
    expect(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ })).not.toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "start_orchestrator_run",
      expect.objectContaining({ workflowType: "review_only" }),
    ));
  });

  it("preserves an unknown workflow, displays it as unsupported, and blocks execution", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return { ...(await originalInvoke(cmd, args)), activeWorkflowId: "future_workflow_v9" };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    expect(screen.getByRole("tab", { name: /future_workflow_v9/ })).toHaveAttribute("aria-selected", "true");
    fireEvent.change(screen.getByDisplayValue("C:\\mock\\project"), { target: { value: "C:\\mock\\updated" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({ config: expect.objectContaining({ projectPath: "C:\\mock\\updated", activeWorkflowId: "future_workflow_v9" }) }),
    ));

    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Must not start unknown workflow" } });
    expect(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ })).toBeDisabled();
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "start_orchestrator_run")).toBe(false);
  });

  it("displays detected project metadata and badges", async () => {
    render(
      <LanguageProvider>
        <OrchestratorPanel />
      </LanguageProvider>
    );

    await waitFor(() => {
      expect(screen.getByText("SPEC.md ✓")).toBeDefined();
      expect(screen.getByText("Cargo.toml ✓")).toBeDefined();
      expect(screen.getByText("Git Repository ✓")).toBeDefined();
    });
  });

  it("selects a compatible role profile from the quick-slot dropdown", async () => {
    render(
      <LanguageProvider>
        <OrchestratorPanel />
      </LanguageProvider>
    );

    const plannerCard = screen.getByRole("region", { name: "orchestrator.roles.planner" });
    const profileSelect = await within(plannerCard).findByRole("combobox");
    fireEvent.click(profileSelect);
    fireEvent.click(within(plannerCard).getByRole("option", { name: /mimo-v2\.6:9b/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({ config: expect.objectContaining({ assignments: expect.objectContaining({ planner: expect.objectContaining({ profileId: "ollama-mimo-9b" }) }) }) }),
    ));
  });

  it("lists only checked compatible quick slots in the role dropdown", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        const config = await originalInvoke(cmd, args);
        return {
          ...config,
          quickSlots: [
            { id: "visible-deepseek", profileId: "deepseek-v41-flash", label: "Checked DeepSeek", visible: true, order: 1 },
            { id: "hidden-mimo", profileId: "mimo-v26-pro", label: "Unchecked MiMo", visible: false, order: 0 },
          ],
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    const plannerCard = screen.getByRole("region", { name: "orchestrator.roles.planner" });
    const profileSelect = within(plannerCard).getByRole("combobox");
    fireEvent.click(profileSelect);
    const listbox = within(plannerCard).getByRole("listbox");
    expect(within(listbox).getAllByRole("option")).toHaveLength(1);
    expect(within(listbox).getByRole("option")).toHaveTextContent("deepseek-v4.1-flash + thinking: High");
    expect(within(plannerCard).queryByRole("button", { name: "MiMo Pro" })).not.toBeInTheDocument();
    expect(within(plannerCard).queryByRole("button", { name: "DeepSeek Flash" })).not.toBeInTheDocument();
    expect(within(listbox).queryByRole("option", { name: /mimo-v2\.6-pro/ })).not.toBeInTheDocument();
    expect(profileSelect).toHaveTextContent("mimo-v2.6-pro + thinking");
    expect(profileSelect).toHaveTextContent("orchestrator.quickSlots.notInWorkspaceList");
  });

  it("generates the dashboard label for Direct DeepSeek from its model instead of the saved display name", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        const config = await originalInvoke(cmd, args);
        return {
          ...config,
          profiles: DEFAULT_ORCHESTRATOR_PROFILES.map((profile) => profile.id === "deepseek-v41-flash"
            ? { ...profile, displayName: "User customized profile label", reasoningEffort: "max" }
            : profile),
          assignments: {
            ...config.assignments,
            planner: { role: "planner", profileId: "deepseek-v41-flash" },
          },
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    const plannerCard = screen.getByRole("region", { name: "orchestrator.roles.planner" });
    const profileSelect = within(plannerCard).getByRole("combobox");
    expect(profileSelect).toHaveTextContent("deepseek-v4.1-flash + thinking: Max");
    expect(within(plannerCard).queryByText("User customized profile label")).not.toBeInTheDocument();
    expect(within(plannerCard).queryByText("orchestrator.roles.planner:")).not.toBeInTheDocument();
    fireEvent.click(profileSelect);
    expect(within(plannerCard).getByRole("option", { name: /deepseek-v4\.1-flash \+ thinking: Max/ })).toBeInTheDocument();
  });

  it("uses Gateway model and Thinking summaries for Kimi and MiMo cards and dropdown options", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        const config = await originalInvoke(cmd, args);
        return {
          ...config,
          profiles: DEFAULT_ORCHESTRATOR_PROFILES,
          assignments: {
            ...config.assignments,
            planner: { role: "planner", profileId: "kimi-k3" },
            plan_reviewer: { role: "plan_reviewer", profileId: "mimo-v26-pro" },
          },
          quickSlots: [
            { id: "kimi-slot", profileId: "kimi-k3", label: "Kimi Custom Slot Label", visible: true, order: 0 },
            { id: "mimo-slot", profileId: "mimo-v26-pro", label: "MiMo Custom Slot Label", visible: true, order: 1 },
          ],
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    const planner = screen.getByRole("region", { name: "orchestrator.roles.planner" });
    const planReviewer = screen.getByRole("region", { name: "orchestrator.roles.planReviewer" });
    expect(within(planner).getByRole("combobox")).toHaveTextContent("kimi-k3 + thinking");
    expect(within(planReviewer).getByRole("combobox")).toHaveTextContent("mimo-v2.6-pro + thinking");
    fireEvent.click(within(planner).getByRole("combobox"));
    fireEvent.click(within(planReviewer).getByRole("combobox"));
    expect(within(planner).getByRole("option", { name: /kimi-k3 \+ thinking/ })).toBeInTheDocument();
    expect(within(planReviewer).getByRole("option", { name: /mimo-v2\.6-pro \+ thinking/ })).toBeInTheDocument();
    expect(within(planner).queryByText("Kimi Custom Slot Label")).not.toBeInTheDocument();
    expect(within(planReviewer).queryByText("MiMo Custom Slot Label")).not.toBeInTheDocument();
  });

  it("prefixes OpenRouter and shows the configured Ollama model in the role dropdown", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        const config = await originalInvoke(cmd, args);
        return {
          ...config,
          profiles: DEFAULT_ORCHESTRATOR_PROFILES.map((profile) =>
            profile.id === "ollama-mimo-9b" ? { ...profile, ollamaModel: "qwen3.6:27b" } : profile,
          ),
          assignments: {
            ...config.assignments,
            planner: { role: "planner", profileId: "openrouter-gpt-56-sol" },
            plan_reviewer: { role: "plan_reviewer", profileId: "ollama-mimo-9b" },
          },
          quickSlots: [
            { id: "openrouter-slot", profileId: "openrouter-gpt-56-sol", label: "OpenRouter custom", visible: true, order: 0 },
            { id: "ollama-slot", profileId: "ollama-mimo-9b", label: "Local MiMo 9B", visible: true, order: 1 },
          ],
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    const planner = screen.getByRole("region", { name: "orchestrator.roles.planner" });
    const reviewer = screen.getByRole("region", { name: "orchestrator.roles.planReviewer" });
    expect(within(planner).getByRole("combobox")).toHaveTextContent("openai/gpt-5.6-sol + thinking: High");
    expect(within(reviewer).getByRole("combobox")).toHaveTextContent("qwen3.6:27b");
    fireEvent.click(within(planner).getByRole("combobox"));
    fireEvent.click(within(reviewer).getByRole("combobox"));
    expect(within(planner).getByRole("option", { name: /openai\/gpt-5\.6-sol \+ thinking: High/ })).toBeInTheDocument();
    expect(within(reviewer).getByRole("option", { name: /qwen3\.6:27b/ })).toBeInTheDocument();
    expect(within(planner).queryByText("OpenRouter custom")).not.toBeInTheDocument();
    expect(within(reviewer).queryByText("Local MiMo 9B")).not.toBeInTheDocument();
  });

  it("omits dashboard-only overrides and starts with the persisted snapshot settings", async () => {
    const savedGate = { ...DEFAULT_VALIDATION_GATES[0], id: "test-gate", name: "Test Gate", enabled: false };
    const savedLimits = { ...DEFAULT_ITERATION_LIMITS, maxFixIterations: 4 };
    const standard = async (cmd: string, args: any) => {
      if (cmd === "get_user_language") return "en";
      if (cmd === "get_orchestrator_config") return {
        projectPath: "C:\\mock\\project",
        activeWorkflowId: "full_loop",
        assignments: {
          planner: { role: "planner", profileId: "mimo-v26-pro" },
          plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
          implementer: { role: "implementer", profileId: "codex-cli" },
          fixer: { role: "fixer", profileId: "codex-cli" },
          code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
        },
        validationGates: [savedGate],
        iterationLimits: savedLimits,
        quickSlots: DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
      };
      if (cmd === "detect_project_metadata") return { path: args.projectPath, exists: true, isDirectory: true, projectType: "Rust", detectedFiles: {} };
      if (cmd === "start_orchestrator_run") return { runId: "override-run" };
      return null;
    };
    invokeMock.mockImplementation(standard);

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");
    expect(screen.queryByText("orchestrator.advancedRun.title")).not.toBeInTheDocument();
    expect(screen.queryByText("orchestrator.advancedRun.validationGates")).not.toBeInTheDocument();
    expect(screen.queryByText("orchestrator.advancedRun.iterationLimits")).not.toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "run with overrides" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

    await waitFor(() => {
      const call = invokeMock.mock.calls.find(([cmd]) => cmd === "start_orchestrator_run");
      expect(call).toBeDefined();
      expect(call?.[1]).toEqual(expect.objectContaining({
        workflowType: "full_loop",
        snapshot: expect.objectContaining({
          validationGates: [savedGate],
          iterationLimits: savedLimits,
          assignments: expect.objectContaining({
            planner: expect.objectContaining({ id: "mimo-v26-pro" }),
            plan_reviewer: expect.objectContaining({ id: "deepseek-v41-flash" }),
            implementer: expect.objectContaining({ id: "codex-cli" }),
          }),
        }),
      }));
      expect(call?.[1]).not.toHaveProperty("transientOverrides");
    });
  });

  it("prevents execution when implementer lacks workspace_write capability", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        const config = await originalInvoke(cmd, args);
        return { ...config, assignments: { ...config.assignments, implementer: { ...config.assignments.implementer, profileId: "mimo-v26-pro" } } };
      }
      return originalInvoke(cmd, args);
    });
    render(
      <LanguageProvider>
        <OrchestratorPanel />
      </LanguageProvider>
    );

    await waitFor(() => {
      expect(screen.getByDisplayValue("C:\\mock\\project")).toBeDefined();
      expect(screen.getByText("Cargo.toml ✓")).toBeDefined();
      expect(
        screen.getAllByText(/missing capability.*workspace_write/i).length
      ).toBeGreaterThan(0);
    });
  });

  it("uses persisted custom profiles and omits profiles from workspace saves", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    const customProfile = { ...DEFAULT_ORCHESTRATOR_PROFILES[0], id: "custom-planner", displayName: "Custom Planner" };
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        const config = await originalInvoke(cmd, args);
        return {
          ...config,
          profiles: [...DEFAULT_ORCHESTRATOR_PROFILES, customProfile],
          assignments: {
            ...config.assignments,
            planner: { role: "planner", profileId: customProfile.id },
          },
        };
      }
      if (cmd === "start_orchestrator_run") return { runId: "custom-run" };
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    const plannerCard = screen.getByRole("region", { name: "orchestrator.roles.planner" });
    await screen.findByDisplayValue("C:\\mock\\project");
    expect(within(plannerCard).queryByText("Custom Planner")).not.toBeInTheDocument();
    expect(within(plannerCard).queryByText("mimo-v2.6-pro + thinking", { selector: "p.orchestrator-selected-profile" })).not.toBeInTheDocument();
    expect(within(plannerCard).getByRole("combobox")).toHaveValue("");
    expect(within(plannerCard).queryByRole("option", { name: "Custom Planner" })).not.toBeInTheDocument();

    const projectInput = await screen.findByDisplayValue("C:\\mock\\project");
    fireEvent.change(projectInput, { target: { value: "C:\\mock\\other" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.objectContaining({ config: expect.objectContaining({ projectPath: "C:\\mock\\other" }) }),
    ));
    const saveCall = invokeMock.mock.calls.find(([cmd, args]) =>
      cmd === "update_orchestrator_config" && args.config.projectPath === "C:\\mock\\other"
    );
    expect(saveCall?.[1].config).not.toHaveProperty("profiles");

    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Run with custom profile" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "start_orchestrator_run",
      expect.objectContaining({ snapshot: expect.objectContaining({ assignments: expect.objectContaining({ planner: customProfile }) }) }),
    ));
  });

  it("does not substitute a built-in profile for a missing saved assignment", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        const config = await originalInvoke(cmd, args);
        return {
          ...config,
          profiles: DEFAULT_ORCHESTRATOR_PROFILES,
          assignments: {
            ...config.assignments,
            planner: { role: "planner", profileId: "missing-custom-planner" },
          },
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await waitFor(() => expect(screen.getAllByText(/planner.*has no assigned profile/i).length).toBeGreaterThan(0));
    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Do not run" } });
    expect(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }).hasAttribute("disabled")).toBe(true);
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "start_orchestrator_run")).toBe(false);
  });

  it("passes the original task to the run command without copying it into logs", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
    invokeMock.mockImplementation((cmd: string, args: any) =>
      cmd === "start_orchestrator_run" ? Promise.resolve({ runId: "secret-run" }) : originalInvoke(cmd, args)
    );
    const task = "Check configuration with sk-testsecret123456789 unchanged";
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await waitFor(() => expect(screen.getByDisplayValue("C:\\mock\\project")).toBeDefined());
    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: task } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "start_orchestrator_run",
      expect.objectContaining({ taskPrompt: task }),
    ));
    expect(document.querySelector(".log-viewer-body")?.textContent).not.toContain(task);
  });

  it("ignores old run events while start is pending and accepts only the returned run id", async () => {
    let resolveStart!: (value: { runId: string }) => void;
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
    invokeMock.mockImplementation((cmd: string, args: any) => {
      if (cmd === "start_orchestrator_run") {
        return new Promise((resolve) => { resolveStart = resolve; });
      }
      return originalInvoke(cmd, args);
    });
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await waitFor(() => expect(eventHandlers.has("orchestrator:step_update")).toBe(true));
    await waitFor(() => expect(screen.getByDisplayValue("C:\\mock\\project")).toBeDefined());
    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Run a test task" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

    const stepHandler = eventHandlers.get("orchestrator:step_update")!;
    const logHandler = eventHandlers.get("orchestrator:log")!;
    stepHandler({ payload: { runId: "old-run", step: "failed", message: "stale step" } });
    logHandler({ payload: { runId: "old-run", message: "stale log" } });
    expect(screen.queryByText(/stale step|stale log/)).toBeNull();
    await waitFor(() => expect(resolveStart).toBeTypeOf("function"));
    await act(async () => {
      resolveStart({ runId: "new-run" });
      await Promise.resolve();
    });
    await waitFor(() => expect(screen.getByText("ID: new-run")).toBeDefined());
    act(() => {
      stepHandler({ payload: { runId: "old-run", step: "failed", message: "stale step" } });
      logHandler({ payload: { runId: "old-run", message: "stale log" } });
      stepHandler({ payload: { runId: "new-run", step: "plan_generation", message: "current step" } });
      logHandler({ payload: { runId: "new-run", message: "current log" } });
    });
    expect(screen.queryByText(/stale step|stale log/)).toBeNull();
    expect(screen.getByText(/current step/)).toBeDefined();
    expect(screen.getByText(/current log/)).toBeDefined();
  });

  it("continues rejecting prior-run events when a new start fails", async () => {
    let rejectStart!: (reason: Error) => void;
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
    invokeMock.mockImplementation((cmd: string, args: any) => {
      if (cmd === "start_orchestrator_run") {
        return new Promise((_resolve, reject) => { rejectStart = reject; });
      }
      return originalInvoke(cmd, args);
    });
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await waitFor(() => expect(eventHandlers.has("orchestrator:step_update")).toBe(true));
    await waitFor(() => expect(screen.getByDisplayValue("C:\\mock\\project")).toBeDefined());
    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Run a test task" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));
    await waitFor(() => expect(rejectStart).toBeTypeOf("function"));
    rejectStart(new Error("start failed"));
    await waitFor(() => expect(screen.getByText(/Error: start failed/)).toBeDefined());
    eventHandlers.get("orchestrator:log")!({ payload: { runId: "old-run", message: "stale after start failure" } });
    expect(screen.queryByText(/stale after start failure/)).toBeNull();
  });

  it("resolves role titles using localized keys without raw enum underscores", async () => {
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    // Headings and accessible names must match translation keys
    expect(screen.getByRole("region", { name: "orchestrator.roles.planReviewer" })).toBeDefined();
    expect(screen.getByRole("region", { name: "orchestrator.roles.codeReviewer" })).toBeDefined();

    // Raw enum keys with underscores must not appear
    expect(screen.queryByRole("region", { name: "orchestrator.roles.plan_reviewer" })).toBeNull();
    expect(screen.queryByRole("region", { name: "orchestrator.roles.code_reviewer" })).toBeNull();
  });

  it("displays explicit unsupported note and explanation for ineligible saved profile in review_only", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_orchestrator_config") {
        return {
          ...(await originalInvoke(cmd, args)),
          activeWorkflowId: "review_only",
          assignments: {
            planner: { role: "planner", profileId: "mimo-v26-pro" },
            plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
            implementer: { role: "implementer", profileId: "codex-cli" },
            fixer: { role: "fixer", profileId: "codex-cli" },
            code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
          },
        };
      }
      return originalInvoke(cmd, args);
    });

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    // Ineligible profile is still selected and displayed
    expect(screen.getByText(/orchestrator\.validation\.reviewOnlyUnsupportedProfile/)).toBeDefined();
    expect(screen.getByText(/orchestrator\.validation\.reviewOnlyExplanation/)).toBeDefined();
    expect(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ })).toBeDisabled();

    // No config write was performed merely on render
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === "update_orchestrator_config")).toBe(false);
  });

  it("renders workflow-specific step sequences for each workflow mode and hides stepper for unknown workflows", async () => {
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    // full_loop: 7 steps
    const stepperStepsFull = document.querySelectorAll(".orchestrator-stepper .stepper-step");
    expect(stepperStepsFull.length).toBe(7);
    expect(within(stepperStepsFull[0] as HTMLElement).getByText("orchestrator.steps.planning")).toBeDefined();
    expect(within(stepperStepsFull[6] as HTMLElement).getByText("orchestrator.steps.completed")).toBeDefined();

    // switch to plan_only: 3 steps
    fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));
    const stepperStepsPlan = document.querySelectorAll(".orchestrator-stepper .stepper-step");
    expect(stepperStepsPlan.length).toBe(3);
    expect(within(stepperStepsPlan[0] as HTMLElement).getByText("orchestrator.steps.planning")).toBeDefined();
    expect(within(stepperStepsPlan[1] as HTMLElement).getByText("orchestrator.steps.planReview")).toBeDefined();
    expect(within(stepperStepsPlan[2] as HTMLElement).getByText("orchestrator.steps.completed")).toBeDefined();
    expect(within(stepperStepsPlan[2] as HTMLElement).getByText("3")).toBeDefined(); // numbered 3, not 7

    // switch to implement_only: 4 steps
    fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.implementOnly/ }));
    const stepperStepsImpl = document.querySelectorAll(".orchestrator-stepper .stepper-step");
    expect(stepperStepsImpl.length).toBe(4);
    expect(within(stepperStepsImpl[0] as HTMLElement).getByText("orchestrator.steps.implementation")).toBeDefined();
    expect(within(stepperStepsImpl[1] as HTMLElement).getByText("orchestrator.steps.validation")).toBeDefined();
    expect(within(stepperStepsImpl[2] as HTMLElement).getByText("orchestrator.steps.fixing")).toBeDefined();
    expect(within(stepperStepsImpl[3] as HTMLElement).getByText("orchestrator.steps.completed")).toBeDefined();

    // switch to review_only: 2 steps
    fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.reviewOnly/ }));
    const stepperStepsReview = document.querySelectorAll(".orchestrator-stepper .stepper-step");
    expect(stepperStepsReview.length).toBe(2);
    expect(within(stepperStepsReview[0] as HTMLElement).getByText("orchestrator.steps.codeReview")).toBeDefined();
    expect(within(stepperStepsReview[1] as HTMLElement).getByText("orchestrator.steps.completed")).toBeDefined();
  });

  it("applies orchestrator-task-textarea class to task prompt textarea", async () => {
    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await screen.findByDisplayValue("C:\\mock\\project");

    const taskTextarea = screen.getByPlaceholderText(/Implement user login session caching/);
    expect(taskTextarea.className).toContain("orchestrator-task-textarea");
  });

  it("marks previous steps as passed and the final step as active on terminal completion", async () => {
    const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
    invokeMock.mockImplementation((cmd: string, args: any) =>
      cmd === "start_orchestrator_run" ? Promise.resolve({ runId: "test-complete-run" }) : originalInvoke(cmd, args)
    );

    render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
    await waitFor(() => expect(eventHandlers.has("orchestrator:step_update")).toBe(true));
    await screen.findByDisplayValue("C:\\mock\\project");

    fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Run complete test" } });
    fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

    await waitFor(() => expect(screen.getByText("ID: test-com")).toBeDefined());

    const stepHandler = eventHandlers.get("orchestrator:step_update")!;
    act(() => {
      stepHandler({ payload: { runId: "test-complete-run", step: "complete", message: "All tasks done" } });
    });

    await waitFor(() => {
      const stepperSteps = document.querySelectorAll(".orchestrator-stepper .stepper-step");
      expect(stepperSteps.length).toBe(7);
      // Steps 0..5 should have "passed" class and "✓"
      for (let i = 0; i < 6; i++) {
        expect(stepperSteps[i].className).toContain("passed");
        expect(within(stepperSteps[i] as HTMLElement).getByText("✓")).toBeDefined();
      }
      // Step 6 (completed) should be active
      expect(stepperSteps[6].className).toContain("active");
    });
  });

  describe("Workflow-Lock & Post-Run Lifecycle Separation", () => {
    it("T1: disables workflow switching during an active run", async () => {
      let resolveStart!: (value: { runId: string }) => void;
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
      invokeMock.mockImplementation((cmd: string, args: any) => {
        if (cmd === "start_orchestrator_run") {
          return new Promise((resolve) => { resolveStart = resolve; });
        }
        return originalInvoke(cmd, args);
      });

      render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
      await screen.findByDisplayValue("C:\\mock\\project");

      // Switch to plan_only first while idle
      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));

      // Start run
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Run plan task" } });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      // All tabs should be disabled during active run
      for (const key of ["fullLoop", "planOnly", "implementOnly", "reviewOnly"]) {
        const tab = screen.getByRole("tab", { name: new RegExp(`orchestrator\\.workflow\\.${key}`) });
        expect(tab).toBeDisabled();
      }

      // Attempt clicking implement_only tab while active
      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.implementOnly/ }));

      // Complete start
      await waitFor(() => expect(resolveStart).toBeTypeOf("function"));
      await act(async () => {
        resolveStart({ runId: "active-lock-run" });
        await Promise.resolve();
      });

      // Stepper must remain plan_only (3 steps) and not implement_only (4 steps)
      const stepperSteps = document.querySelectorAll(".orchestrator-stepper .stepper-step");
      expect(stepperSteps.length).toBe(3);
      expect(within(stepperSteps[0] as HTMLElement).getByText("orchestrator.steps.planning")).toBeDefined();
    });

    it("T2: keeps stepper bound to workflow captured at run start during execution", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
      invokeMock.mockImplementation((cmd: string, args: any) =>
        cmd === "start_orchestrator_run" ? Promise.resolve({ runId: "bound-run" }) : originalInvoke(cmd, args)
      );

      render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
      await waitFor(() => expect(eventHandlers.has("orchestrator:step_update")).toBe(true));
      await screen.findByDisplayValue("C:\\mock\\project");

      // Select plan_only
      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Plan task" } });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => expect(screen.getByText("ID: bound-ru")).toBeDefined());

      const stepHandler = eventHandlers.get("orchestrator:step_update")!;
      act(() => {
        stepHandler({ payload: { runId: "bound-run", step: "plan_review", message: "Reviewing plan" } });
      });

      // Stepper must still have exactly 3 steps for plan_only
      const stepperSteps = document.querySelectorAll(".orchestrator-stepper .stepper-step");
      expect(stepperSteps.length).toBe(3);
      // Step 0 (planning) should be passed
      expect(stepperSteps[0].className).toContain("passed");
      // Step 1 (planReview) should be active
      expect(stepperSteps[1].className).toContain("active");
    });

    it("T3: displays terminal stepper and verdict for the completed run's workflow", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
      invokeMock.mockImplementation((cmd: string, args: any) =>
        cmd === "start_orchestrator_run" ? Promise.resolve({ runId: "term-run" }) : originalInvoke(cmd, args)
      );

      render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
      await waitFor(() => expect(eventHandlers.has("orchestrator:step_update")).toBe(true));
      await screen.findByDisplayValue("C:\\mock\\project");

      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Terminal test" } });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => expect(screen.getByText("ID: term-run")).toBeDefined());

      const stepHandler = eventHandlers.get("orchestrator:step_update")!;
      act(() => {
        stepHandler({
          payload: {
            runId: "term-run",
            step: "complete",
            message: "Plan verified",
            reviewResult: { verdict: "approved" },
          },
        });
      });

      await waitFor(() => {
        // Plan_only terminal state has 3 steps
        const stepperSteps = document.querySelectorAll(".orchestrator-stepper .stepper-step");
        expect(stepperSteps.length).toBe(3);
        expect(stepperSteps[0].className).toContain("passed");
        expect(stepperSteps[1].className).toContain("passed");
        expect(stepperSteps[2].className).toContain("active");
        expect(screen.getByText("READY")).toBeDefined();
      });

      // Tabs should be re-enabled
      for (const key of ["fullLoop", "planOnly", "implementOnly", "reviewOnly"]) {
        expect(screen.getByRole("tab", { name: new RegExp(`orchestrator\\.workflow\\.${key}`) })).not.toBeDisabled();
      }
    });

    it("T4: clears previous run presentation and displays new workflow as not-run when selecting another workflow after completion", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
      invokeMock.mockImplementation((cmd: string, args: any) =>
        cmd === "start_orchestrator_run" ? Promise.resolve({ runId: "leak-test-run" }) : originalInvoke(cmd, args)
      );

      render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
      await waitFor(() => expect(eventHandlers.has("orchestrator:step_update")).toBe(true));
      await screen.findByDisplayValue("C:\\mock\\project");

      // 1. Run plan_only to completion
      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Task 1" } });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => expect(screen.getByText("ID: leak-tes")).toBeDefined());

      const stepHandler = eventHandlers.get("orchestrator:step_update")!;
      act(() => {
        stepHandler({
          payload: {
            runId: "leak-test-run",
            step: "complete",
            message: "Finished plan",
            reviewResult: { verdict: "approved" },
          },
        });
      });

      await waitFor(() => expect(screen.getByText("READY")).toBeDefined());

      // 2. Select implement_only tab after completion
      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.implementOnly/ }));

      // 3. Assert previous run presentation is cleared:
      // State is now IDLE
      expect(screen.getByText("IDLE")).toBeDefined();
      // Verdict is cleared
      expect(screen.queryByText("READY")).toBeNull();
      // Stepper now has 4 steps for implement_only
      const stepperSteps = document.querySelectorAll(".orchestrator-stepper .stepper-step");
      expect(stepperSteps.length).toBe(4);
      // Crucial invariant: None of the implement_only steps (including Done) are marked completed or passed!
      for (let i = 0; i < 4; i++) {
        expect(stepperSteps[i].className).not.toContain("passed");
        expect(stepperSteps[i].className).not.toContain("active");
      }

      // Logs from prior run are preserved
      expect(document.querySelector(".log-viewer-body")?.textContent).toContain("Finished plan");
    });

    it("T5: clears running workflow snapshot and re-enables workflow selection on start failure", async () => {
      let rejectStart!: (reason: Error) => void;
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
      invokeMock.mockImplementation((cmd: string, args: any) => {
        if (cmd === "start_orchestrator_run") {
          return new Promise((_resolve, reject) => { rejectStart = reject; });
        }
        return originalInvoke(cmd, args);
      });

      render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
      await screen.findByDisplayValue("C:\\mock\\project");

      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Failing start task" } });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      // Reject start
      await waitFor(() => expect(rejectStart).toBeTypeOf("function"));
      rejectStart(new Error("network error on start"));

      await waitFor(() => {
        expect(screen.getByText(/Error: network error on start/)).toBeDefined();
      });
      expect(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ })).toBeVisible();
      expect(document.querySelector(".log-viewer-body")?.textContent).toContain("Error: network error on start");

      // Tabs must be enabled
      for (const key of ["fullLoop", "planOnly", "implementOnly", "reviewOnly"]) {
        expect(screen.getByRole("tab", { name: new RegExp(`orchestrator\\.workflow\\.${key}`) })).not.toBeDisabled();
      }

      // Can switch workflow cleanly
      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.implementOnly/ }));
      const stepperSteps = document.querySelectorAll(".orchestrator-stepper .stepper-step");
      expect(stepperSteps.length).toBe(4);
    });

    it("T6: explicit reset clears running workflow snapshot and returns to idle", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<unknown>;
      invokeMock.mockImplementation((cmd: string, args: any) =>
        cmd === "start_orchestrator_run" ? Promise.resolve({ runId: "reset-snap-run" }) : originalInvoke(cmd, args)
      );

      render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
      await waitFor(() => expect(eventHandlers.has("orchestrator:step_update")).toBe(true));
      await screen.findByDisplayValue("C:\\mock\\project");

      fireEvent.click(screen.getByRole("tab", { name: /orchestrator\.workflow\.planOnly/ }));
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), { target: { value: "Task to reset" } });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => expect(screen.getByText("ID: reset-sn")).toBeDefined());

      const stepHandler = eventHandlers.get("orchestrator:step_update")!;
      act(() => {
        stepHandler({ payload: { runId: "reset-snap-run", step: "complete", message: "Done" } });
      });

      await waitFor(() => expect(screen.getByRole("button", { name: /orchestrator\.exec\.resetBtn/ })).toBeDefined());

      // Click New Run (reset)
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.resetBtn/ }));

      expect(screen.getByText("IDLE")).toBeDefined();
      expect(screen.queryByText("ID: reset-sn")).toBeNull();

      // Stepper for plan_only is in idle state with no steps active or passed
      const stepperSteps = document.querySelectorAll(".orchestrator-stepper .stepper-step");
      expect(stepperSteps.length).toBe(3);
      for (let i = 0; i < 3; i++) {
        expect(stepperSteps[i].className).not.toContain("passed");
        expect(stepperSteps[i].className).not.toContain("active");
      }
    });

    it("rejects out-of-order stale project detection responses and preserves latest project metadata and gates", async () => {
      let resolveProjectA: ((val: any) => void) | null = null;
      let resolveProjectB: ((val: any) => void) | null = null;

      const projectAResponse = {
        path: "C:\\projects\\slow-a",
        exists: true,
        isDirectory: true,
        projectType: "Project A Type",
        detectedFiles: { packageJson: true },
        suggestedGates: [
          {
            id: "gate-a",
            name: "Gate A",
            category: "tests" as const,
            executable: "npm",
            args: ["test:a"],
            enabled: true,
            failOnError: true,
          },
        ],
      };

      const projectBResponse = {
        path: "C:\\projects\\fast-b",
        exists: true,
        isDirectory: true,
        projectType: "Project B Type",
        detectedFiles: { cargoToml: true },
        suggestedGates: [
          {
            id: "gate-b",
            name: "Gate B",
            category: "tests" as const,
            executable: "cargo",
            args: ["test:b"],
            enabled: true,
            failOnError: true,
          },
        ],
      };

      invokeMock.mockImplementation((cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          return Promise.resolve({
            projectPath: "C:\\projects\\slow-a",
            activeWorkflowId: "full_loop",
            activePresetId: "balanced",
            autoValidationEnabled: true,
            validationGates: [],
            iterationLimits: DEFAULT_ITERATION_LIMITS,
            quickSlots: DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
            assignments: {
              planner: { role: "planner", profileId: "mimo-v26-pro" },
              plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
              implementer: { role: "implementer", profileId: "codex-cli" },
              fixer: { role: "fixer", profileId: "codex-cli", escalationRole: "implementer" },
              code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
            },
          });
        }
        if (cmd === "detect_project_metadata") {
          const path = args?.projectPath;
          if (path === "C:\\projects\\slow-a") {
            return new Promise((resolve) => {
              resolveProjectA = () => resolve(projectAResponse);
            });
          }
          if (path === "C:\\projects\\fast-b") {
            return new Promise((resolve) => {
              resolveProjectB = () => resolve(projectBResponse);
            });
          }
        }
        if (cmd === "update_orchestrator_config") return Promise.resolve(null);
        return Promise.resolve(null);
      });

      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );

      // Mount triggers detection for Project A (now pending)
      const input = await screen.findByDisplayValue("C:\\projects\\slow-a");
      expect(input).toBeInTheDocument();

      // User changes path to Project B and triggers scan via Enter
      fireEvent.change(input, { target: { value: "C:\\projects\\fast-b" } });
      fireEvent.keyDown(input, { key: "Enter", code: "Enter" });

      // Complete Project B first (Fast response)
      expect(resolveProjectB).toBeDefined();
      act(() => {
        resolveProjectB!(projectBResponse);
      });

      // Project B metadata and gates should be reflected
      await waitFor(() => {
        expect(screen.getByText("Project B Type")).toBeInTheDocument();
      });

      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: {
          validationGates: [
            expect.objectContaining({
              id: "gate-b",
              executable: "cargo",
            }),
          ],
        },
      });

      // Clear mock calls to observe any stale writes
      invokeMock.mockClear();

      // Now complete Project A (Late/stale response)
      expect(resolveProjectA).toBeDefined();
      act(() => {
        resolveProjectA!(projectAResponse);
      });

      // Allow any microtasks / timers to run
      await new Promise((r) => setTimeout(r, 50));

      // Metadata must remain Project B Type, NOT revert to Project A
      expect(screen.getByText("Project B Type")).toBeInTheDocument();
      expect(screen.queryByText("Project A Type")).not.toBeInTheDocument();

      // Stale Project A gates must NOT have been saved or merged into config
      const updateConfigCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "update_orchestrator_config");
      expect(updateConfigCalls.length).toBe(0);
    });

    it("rejects in-flight detection response for project A if path is changed to B before scanning B", async () => {
      let resolveProjectA: ((val: any) => void) | null = null;

      const projectAResponse = {
        path: "C:\\projects\\project-a",
        exists: true,
        isDirectory: true,
        projectType: "Project A Type",
        detectedFiles: { packageJson: true },
        suggestedGates: [
          {
            id: "gate-a",
            name: "Gate A",
            category: "tests" as const,
            executable: "npm",
            args: ["test:a"],
            enabled: true,
            failOnError: true,
          },
        ],
      };

      invokeMock.mockImplementation((cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          return Promise.resolve({
            projectPath: "C:\\projects\\project-a",
            activeWorkflowId: "full_loop",
            activePresetId: "balanced",
            autoValidationEnabled: true,
            validationGates: [],
            iterationLimits: DEFAULT_ITERATION_LIMITS,
            quickSlots: DEFAULT_ORCHESTRATOR_QUICK_SLOTS,
            assignments: {
              planner: { role: "planner", profileId: "mimo-v26-pro" },
              plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
              implementer: { role: "implementer", profileId: "codex-cli" },
              fixer: { role: "fixer", profileId: "codex-cli", escalationRole: "implementer" },
              code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
            },
          });
        }
        if (cmd === "detect_project_metadata") {
          const path = args?.projectPath;
          if (path === "C:\\projects\\project-a") {
            return new Promise((resolve) => {
              resolveProjectA = () => resolve(projectAResponse);
            });
          }
        }
        if (cmd === "update_orchestrator_config") return Promise.resolve(null);
        return Promise.resolve(null);
      });

      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );

      // Mount triggers detection for Project A (now pending)
      const input = await screen.findByDisplayValue("C:\\projects\\project-a");
      expect(input).toBeInTheDocument();

      // User changes path to Project B without scanning B yet
      fireEvent.change(input, { target: { value: "C:\\projects\\project-b" } });

      // Clear previous save config calls
      invokeMock.mockClear();

      // Response for Project A now resolves
      expect(resolveProjectA).toBeDefined();
      act(() => {
        resolveProjectA!(projectAResponse);
      });

      await new Promise((r) => setTimeout(r, 50));

      // Metadata for A must NOT be displayed
      expect(screen.queryByText("Project A Type")).not.toBeInTheDocument();

      // Gate A must NOT be merged/saved to config
      const updateConfigCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "update_orchestrator_config");
      expect(updateConfigCalls.length).toBe(0);
    });
  });

  describe("Phase B - Lean Antigravity Mode & Task Dispatch Budget", () => {
    it("defaults leanAntigravityMode to false and toggling persists preference", async () => {
      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );
      await screen.findByDisplayValue("C:\\mock\\project");

      const toggleBtn = screen.getByRole("switch", { name: "orchestrator.leanMode.title" });
      expect(toggleBtn).toBeInTheDocument();
      expect(toggleBtn).toHaveAttribute("aria-checked", "false");

      fireEvent.click(toggleBtn);

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          "update_orchestrator_config",
          expect.objectContaining({
            config: expect.objectContaining({
              leanAntigravityMode: true,
            }),
          })
        );
      });
      expect(toggleBtn).toHaveAttribute("aria-checked", "true");
    });

    it("snapshots leanAntigravityMode on run start and captures current preference", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          return {
            ...(await originalInvoke(cmd, args)),
            leanAntigravityMode: true,
          };
        }
        if (cmd === "start_orchestrator_run") {
          return { runId: "test-lean-run-1" };
        }
        return originalInvoke(cmd, args);
      });

      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );
      await screen.findByDisplayValue("C:\\mock\\project");

      const toggleBtn = screen.getByRole("switch", { name: "orchestrator.leanMode.title" });
      expect(toggleBtn).toHaveAttribute("aria-checked", "true");

      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), {
        target: { value: "Run with lean mode" },
      });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          "start_orchestrator_run",
          expect.objectContaining({
            snapshot: expect.objectContaining({
              leanAntigravityMode: true,
            }),
          })
        );
      });
    });

    it("displays truthful dispatch counts and limit in human_gated_loop and updates upon step progress event", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          const base = await originalInvoke(cmd, args);
          return {
            ...base,
            activeWorkflowId: "human_gated_loop",
            profiles: DEFAULT_ORCHESTRATOR_PROFILES,
            assignments: {
              planner: { role: "planner", profileId: "deepseek-v41-flash" },
              plan_integrator: { role: "plan_integrator", profileId: "antigravity-harness" },
              plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
              implementer: { role: "implementer", profileId: "antigravity-harness" },
              fixer: { role: "fixer", profileId: "antigravity-harness", escalationRole: "implementer" },
              code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
            },
          };
        }
        if (cmd === "start_orchestrator_run") {
          return { runId: "test-hg-dispatch-run" };
        }
        return originalInvoke(cmd, args);
      });

      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );
      await screen.findByDisplayValue("C:\\mock\\project");

      // Initial budget info in human_gated_loop
      expect(screen.getByText(/orchestrator\.budget\.dispatchCount/)).toBeInTheDocument();
      expect(screen.getByText("orchestrator.budget.notice")).toBeInTheDocument();

      // Start run
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), {
        target: { value: "HG run" },
      });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("start_orchestrator_run", expect.anything());
      });

      // Emit step update with dispatch metrics
      const stepListener = eventHandlers.get("orchestrator:step_update");
      expect(stepListener).toBeDefined();

      act(() => {
        stepListener!({
          payload: {
            runId: "test-hg-dispatch-run",
            step: "implementation",
            message: "Awaiting Antigravity implementation...",
            antigravityDispatches: 2,
            antigravityDispatchLimit: 6,
          },
        });
      });

      await waitFor(() => {
        expect(screen.getByText(/orchestrator\.budget\.dispatchCount/)).toBeInTheDocument();
      });
    });

    it("renders task-scope budget exhaustion specifically when budgetScope is task", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          const base = await originalInvoke(cmd, args);
          return {
            ...base,
            activeWorkflowId: "human_gated_loop",
            profiles: DEFAULT_ORCHESTRATOR_PROFILES,
            assignments: {
              planner: { role: "planner", profileId: "deepseek-v41-flash" },
              plan_integrator: { role: "plan_integrator", profileId: "antigravity-harness" },
              plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
              implementer: { role: "implementer", profileId: "antigravity-harness" },
              fixer: { role: "fixer", profileId: "antigravity-harness", escalationRole: "implementer" },
              code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
            },
          };
        }
        if (cmd === "start_orchestrator_run") {
          return { runId: "test-hg-dispatch-run" };
        }
        return originalInvoke(cmd, args);
      });

      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );

      await screen.findByDisplayValue("C:\\mock\\project");

      // Start run
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), {
        target: { value: "HG run" },
      });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("start_orchestrator_run", expect.anything());
      });

      const stepListener = eventHandlers.get("orchestrator:step_update");
      expect(stepListener).toBeDefined();

      act(() => {
        stepListener!({
          payload: {
            runId: "test-hg-dispatch-run",
            step: "waiting_for_user",
            message: "",
            antigravityDispatches: 2,
            antigravityDispatchLimit: 2,
            budgetScope: "task",
            waitingReason: "budget_exhausted",
          },
        });
      });

      await waitFor(() => {
        expect(screen.getAllByText(/orchestrator\.budget\.exhaustedTask/).length).toBeGreaterThan(0);
      });

      // Assert no clarification or worker reclaim buttons are rendered in budget exhaustion card
      expect(screen.queryByPlaceholderText(/Enter clarification or instructions/)).not.toBeInTheDocument();
      expect(screen.queryByText(/Submit Clarification/)).not.toBeInTheDocument();
      expect(screen.queryByText(/Confirm Worker Stopped & Resume Claim/)).not.toBeInTheDocument();

      // Reset / New Run clears budget exhausted presentation
      const resetBtn = screen.getAllByRole("button", { name: /orchestrator\.exec\.resetBtn/ })[0];
      fireEvent.click(resetBtn);

      await waitFor(() => {
        expect(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ })).toBeInTheDocument();
      });
    });

    it("renders run-scope budget exhaustion specifically when budgetScope is run", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          const base = await originalInvoke(cmd, args);
          return {
            ...base,
            activeWorkflowId: "human_gated_loop",
            profiles: DEFAULT_ORCHESTRATOR_PROFILES,
            assignments: {
              planner: { role: "planner", profileId: "deepseek-v41-flash" },
              plan_integrator: { role: "plan_integrator", profileId: "antigravity-harness" },
              plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
              implementer: { role: "implementer", profileId: "antigravity-harness" },
              fixer: { role: "fixer", profileId: "antigravity-harness", escalationRole: "implementer" },
              code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
            },
          };
        }
        if (cmd === "start_orchestrator_run") {
          return { runId: "test-hg-dispatch-run" };
        }
        return originalInvoke(cmd, args);
      });

      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );

      await screen.findByDisplayValue("C:\\mock\\project");

      // Start run
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), {
        target: { value: "HG run" },
      });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("start_orchestrator_run", expect.anything());
      });

      const stepListener = eventHandlers.get("orchestrator:step_update");
      expect(stepListener).toBeDefined();

      act(() => {
        stepListener!({
          payload: {
            runId: "test-hg-dispatch-run",
            step: "waiting_for_user",
            message: "",
            antigravityDispatches: 6,
            antigravityDispatchLimit: 6,
            budgetScope: "run",
            waitingReason: "budget_exhausted",
          },
        });
      });

      await waitFor(() => {
        expect(screen.getAllByText(/orchestrator\.budget\.exhaustedRun/).length).toBeGreaterThan(0);
      });

      // Assert no clarification or worker reclaim buttons are rendered in budget exhaustion card
      expect(screen.queryByPlaceholderText(/Enter clarification or instructions/)).not.toBeInTheDocument();
      expect(screen.queryByText(/Submit Clarification/)).not.toBeInTheDocument();
      expect(screen.queryByText(/Confirm Worker Stopped & Resume Claim/)).not.toBeInTheDocument();
    });

    it("renders worker-disconnected card when waitingReason is worker_disconnected", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          const base = await originalInvoke(cmd, args);
          return {
            ...base,
            activeWorkflowId: "human_gated_loop",
            profiles: DEFAULT_ORCHESTRATOR_PROFILES,
            assignments: {
              planner: { role: "planner", profileId: "deepseek-v41-flash" },
              plan_integrator: { role: "plan_integrator", profileId: "antigravity-harness" },
              plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
              implementer: { role: "implementer", profileId: "antigravity-harness" },
              fixer: { role: "fixer", profileId: "antigravity-harness", escalationRole: "implementer" },
              code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
            },
          };
        }
        if (cmd === "start_orchestrator_run") {
          return { runId: "test-hg-disconnect-run" };
        }
        return originalInvoke(cmd, args);
      });

      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );

      await screen.findByDisplayValue("C:\\mock\\project");

      // Start run
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), {
        target: { value: "HG run" },
      });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("start_orchestrator_run", expect.anything());
      });

      const stepListener = eventHandlers.get("orchestrator:step_update");
      expect(stepListener).toBeDefined();

      act(() => {
        stepListener!({
          payload: {
            runId: "test-hg-disconnect-run",
            step: "waiting_for_user",
            message: "Antigravity worker disconnected or lease timed out.",
            waitingReason: "worker_disconnected",
          },
        });
      });

      await waitFor(() => {
        expect(screen.getByText(/orchestrator\.worker\.disconnectedTitle|Antigravity Worker Disconnected/)).toBeInTheDocument();
        expect(screen.getByText(/orchestrator\.worker\.disconnectedDesc|The Antigravity worker process disconnected/)).toBeInTheDocument();
        expect(screen.getByText(/orchestrator\.worker\.confirmStoppedNotice|Confirm that the old worker process has completely stopped/)).toBeInTheDocument();
        expect(screen.getByText(/orchestrator\.worker\.confirmStoppedBtn|Confirm Worker Stopped & Resume Claim/)).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /orchestrator\.exec\.cancelBtn|Cancel/ })).toBeInTheDocument();
      });

      // Clarification input must NOT be present on worker_disconnected
      expect(screen.queryByPlaceholderText(/Enter clarification or instructions/)).not.toBeInTheDocument();
      expect(screen.queryByText(/Submit Clarification/)).not.toBeInTheDocument();
    });

    it("renders clarification card when waitingReason is clarification_required", async () => {
      const originalInvoke = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "get_orchestrator_config") {
          const base = await originalInvoke(cmd, args);
          return {
            ...base,
            activeWorkflowId: "full_loop",
            profiles: DEFAULT_ORCHESTRATOR_PROFILES,
            assignments: {
              planner: { role: "planner", profileId: "deepseek-v41-flash" },
              plan_reviewer: { role: "plan_reviewer", profileId: "deepseek-v41-flash" },
              implementer: { role: "implementer", profileId: "codex-cli" },
              fixer: { role: "fixer", profileId: "codex-cli", escalationRole: "implementer" },
              code_reviewer: { role: "code_reviewer", profileId: "codex-cli" },
            },
          };
        }
        if (cmd === "start_orchestrator_run") {
          return { runId: "test-clarification-run" };
        }
        return originalInvoke(cmd, args);
      });

      render(
        <LanguageProvider>
          <OrchestratorPanel />
        </LanguageProvider>
      );

      await screen.findByDisplayValue("C:\\mock\\project");

      // Start run
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), {
        target: { value: "Full loop run" },
      });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("start_orchestrator_run", expect.anything());
      });

      const stepListener = eventHandlers.get("orchestrator:step_update");
      expect(stepListener).toBeDefined();

      act(() => {
        stepListener!({
          payload: {
            runId: "test-clarification-run",
            step: "waiting_for_user",
            message: "Clarification needed: Is database migration required?",
            waitingReason: "clarification_required",
          },
        });
      });

      await waitFor(() => {
        expect(screen.getByPlaceholderText(/Enter clarification or instructions/)).toBeInTheDocument();
        expect(screen.getByText(/Submit Clarification/)).toBeInTheDocument();
      });

      // Worker reclaim button must NOT be present on clarification_required
      expect(screen.queryByText(/Confirm Worker Stopped & Resume Claim/)).not.toBeInTheDocument();
    });

    it.each([
      ["en", enTranslations],
      ["ja", jaTranslations],
      ["de", deTranslations],
      ["es", esTranslations],
      ["fr", frTranslations],
      ["ko", koTranslations],
      ["zh-CN", zhCNTranslations],
      ["zh-TW", zhTWTranslations],
    ])("defines all Phase B translation keys for %s", (_lang, translations) => {
      const keys = [
        "orchestrator.leanMode.title",
        "orchestrator.leanMode.desc",
        "orchestrator.budget.dispatchCount",
        "orchestrator.budget.exhaustedTask",
        "orchestrator.budget.exhaustedRun",
        "orchestrator.budget.notice",
        "orchestrator.budget.exhausted",
        "orchestrator.worker.disconnectedTitle",
        "orchestrator.worker.disconnectedDesc",
        "orchestrator.worker.confirmStoppedNotice",
        "orchestrator.worker.confirmStoppedBtn",
        "orchestrator.recovery.title",
        "orchestrator.recovery.description",
        "orchestrator.recovery.inspect",
        "orchestrator.recovery.loading",
        "orchestrator.recovery.details",
        "orchestrator.recovery.stage",
        "orchestrator.recovery.match",
        "orchestrator.recovery.drift",
        "orchestrator.recovery.affectedPaths",
        "orchestrator.recovery.blocked",
        "orchestrator.recovery.restore",
        "orchestrator.recovery.adopt",
        "orchestrator.recovery.cancel",
        "orchestrator.recovery.confirmRestore",
        "orchestrator.recovery.confirmAdopt",
        "orchestrator.recovery.resumeUnavailable",
        "orchestrator.recovery.resume",
        "orchestrator.recovery.confirmResume",
        "orchestrator.recovery.backupDestination",
        "orchestrator.recovery.restoreScope",
        "orchestrator.recovery.affectedPathsList",
        "orchestrator.recovery.confirmTitle",
        "orchestrator.recovery.cancelConfirm",
        "orchestrator.recovery.confirmProceed",
        "orchestrator.recovery.error",
      ] as const;

      for (const key of keys) {
        const value = (translations as any)[key];
        expect(value).toBeTruthy();
        expect(value).not.toBe(key);
      }
    });

    it.each([
      ["en", enTranslations],
      ["ja", jaTranslations],
      ["de", deTranslations],
      ["es", esTranslations],
      ["fr", frTranslations],
      ["ko", koTranslations],
      ["zh-CN", zhCNTranslations],
      ["zh-TW", zhTWTranslations],
    ])("defines all convergence progress and confirmation keys for %s", (_lang, translations) => {
      const keys = [
        "orchestrator.planConvergence.liveProgress.plannerDispatch",
        "orchestrator.planConvergence.liveProgress.reviewerReserve",
        "orchestrator.planConvergence.liveProgress.reviewerDispatch",
        "orchestrator.planConvergence.liveProgress.verdict",
        "orchestrator.planConvergence.liveProgress.rolePlanner",
        "orchestrator.planConvergence.liveProgress.roleReviewer",
        "orchestrator.planConvergence.liveProgress.round",
        "orchestrator.planConvergence.liveProgress.decisionApprove",
        "orchestrator.planConvergence.liveProgress.decisionRequestChanges",
        "orchestrator.planConvergence.liveProgress.decisionEscalate",
        "orchestrator.planConvergence.waiting.newPrimaryPlanTitle",
        "orchestrator.planConvergence.waiting.newPrimaryPlanDesc",
        "orchestrator.planConvergence.waiting.confirmButton",
        "orchestrator.planConvergence.waiting.rejectButton",
        "orchestrator.planConvergence.waiting.candidateIdLabel",
        "orchestrator.planConvergence.waiting.targetRevisionLabel",
        "orchestrator.planConvergence.waiting.staleError",
        "orchestrator.planConvergence.waiting.actionPending",
        "orchestrator.planConvergence.waiting.confirmError",
        "orchestrator.planConvergence.newPlanCreated",
        "orchestrator.planConvergence.newPlanRejected",
      ] as const;

      for (const key of keys) {
        const value = (translations as any)[key];
        expect(value).toBeTruthy();
        expect(value).not.toBe(key);
      }
    });
  });

  describe("plan convergence confirmation and live progress", () => {
    async function startOptInConvergence(
      convergenceResult: unknown = {
        kind: "created",
        runId: "convergence-1",
        createdPlanId: "V0.23.0-r2",
        createdPath: ".plan/V0.23.0-r2.md",
        fileDigest: "a".repeat(64),
      },
    ) {
      const original = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "start_plan_convergence") return { runId: "convergence-1" };
        if (cmd === "confirm_converged_new_plan") return convergenceResult;
        return original(cmd, args);
      });
      render(<LanguageProvider><OrchestratorPanel /></LanguageProvider>);
      await screen.findByDisplayValue("C:\\mock\\project");
      fireEvent.click(screen.getByRole("switch", { name: "orchestrator.planConvergence.optIn" }));
      fireEvent.change(screen.getByPlaceholderText(/Implement user login session caching/), {
        target: { value: "Revise the plan" },
      });
      fireEvent.click(screen.getByRole("button", { name: /orchestrator\.exec\.startBtn/ }));
      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("start_plan_convergence", expect.anything()),
      );
    }

    function emitWaiting(payload: Record<string, unknown>) {
      act(() => eventHandlers.get("orchestrator:step_update")?.({
        payload: {
          runId: "convergence-1",
          step: "waiting_for_user",
          waitingReason: "NEW_PRIMARY_PLAN_CONFIRMATION",
          message: "RAW BACKEND PROSE MUST NOT LEAK",
          ...payload,
        },
      }));
    }

    it("confirms the reviewed new-primary candidate through the production command", async () => {
      await startOptInConvergence();
      emitWaiting({ planText: convergenceWaitingPayload() });

      expect(screen.getByText("Converged Revision 2")).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "orchestrator.planConvergence.waiting.confirmButton" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "orchestrator.planConvergence.waiting.rejectButton" }),
      ).toBeInTheDocument();

      fireEvent.click(
        screen.getByRole("button", { name: "orchestrator.planConvergence.waiting.confirmButton" }),
      );

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("confirm_converged_new_plan", {
          runId: "convergence-1",
          expectedRevision: 7,
          candidateId: "cand-abcdef12",
          action: "confirm",
        }),
      );
    });

    it("rejects the reviewed candidate without creating a plan", async () => {
      await startOptInConvergence({ kind: "rejected", runId: "convergence-1" });
      emitWaiting({ planText: convergenceWaitingPayload() });

      fireEvent.click(
        screen.getByRole("button", { name: "orchestrator.planConvergence.waiting.rejectButton" }),
      );

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("confirm_converged_new_plan", {
          runId: "convergence-1",
          expectedRevision: 7,
          candidateId: "cand-abcdef12",
          action: "reject",
        }),
      );
      expect(screen.queryByText("orchestrator.planConvergence.newPlanCreated")).toBeNull();
    });

    it("does not act on a candidate belonging to a different run", async () => {
      await startOptInConvergence();
      emitWaiting({ planText: convergenceWaitingPayload({ runId: "some-other-run" }) });

      fireEvent.click(
        screen.getByRole("button", { name: "orchestrator.planConvergence.waiting.confirmButton" }),
      );

      expect(invokeMock.mock.calls.some(([cmd]) => cmd === "confirm_converged_new_plan")).toBe(false);
      expect(
        await screen.findByText("orchestrator.planConvergence.waiting.staleError"),
      ).toBeInTheDocument();
    });

    it("refuses a confirmation while another run owns the active-run boundary", async () => {
      await startOptInConvergence();
      const base = invokeMock.getMockImplementation() as (cmd: string, args: any) => Promise<any>;
      invokeMock.mockImplementation(async (cmd: string, args: any) => {
        if (cmd === "confirm_converged_new_plan") {
          throw {
            code: "PC_CONFLICT_RUN_ACTIVE",
            message: "RAW BACKEND PROSE MUST NOT LEAK",
          };
        }
        return base(cmd, args);
      });
      emitWaiting({ planText: convergenceWaitingPayload() });

      fireEvent.click(
        screen.getByRole("button", { name: "orchestrator.planConvergence.waiting.confirmButton" }),
      );

      expect(
        await screen.findByText("orchestrator.planConvergence.waiting.staleError"),
      ).toBeInTheDocument();
      expect(screen.queryByText(/RAW BACKEND PROSE/)).toBeNull();
      // A refused action leaves the reviewed candidate actionable.
      expect(
        screen.getByRole("button", { name: "orchestrator.planConvergence.waiting.confirmButton" }),
      ).toBeInTheDocument();
      expect(screen.queryByText("orchestrator.planConvergence.newPlanCreated")).toBeNull();
    });

    it("shows no plan creation controls for an invalid model response", async () => {
      await startOptInConvergence();
      act(() => eventHandlers.get("orchestrator:step_update")?.({
        payload: {
          runId: "convergence-1",
          step: "waiting_for_user",
          waitingReason: "INVALID_MODEL_RESPONSE",
          message: "RAW BACKEND PROSE MUST NOT LEAK",
        },
      }));

      expect(
        screen.queryByRole("button", { name: "orchestrator.planConvergence.waiting.confirmButton" }),
      ).toBeNull();
      expect(
        screen.queryByRole("button", { name: "orchestrator.planConvergence.waiting.rejectButton" }),
      ).toBeNull();
    });

    it("renders live convergence progress from typed events", async () => {
      await startOptInConvergence();

      act(() => eventHandlers.get("orchestrator:step_update")?.({
        payload: {
          runId: "convergence-1",
          step: "plan_review",
          message: "RAW BACKEND PROSE MUST NOT LEAK",
          iterationInfo: "phase:verdict,sequence:2,decision:APPROVE,used:2,limit:3",
        },
      }));

      const progress = screen.getByLabelText("convergence-progress");
      expect(progress).toBeInTheDocument();
      expect(within(progress).getByText("orchestrator.planConvergence.liveProgress.round"))
        .toBeInTheDocument();
      expect(within(progress).getByText("orchestrator.planConvergence.liveProgress.verdict"))
        .toBeInTheDocument();
      expect(screen.queryByText(/RAW BACKEND PROSE/)).toBeNull();
    });

    it("ignores malformed progress payloads", async () => {
      await startOptInConvergence();

      act(() => eventHandlers.get("orchestrator:step_update")?.({
        payload: {
          runId: "convergence-1",
          step: "plan_review",
          message: "progress",
          iterationInfo: "phase:not_a_phase,sequence:0",
        },
      }));

      expect(screen.queryByLabelText("convergence-progress")).toBeNull();
    });
  });
});
