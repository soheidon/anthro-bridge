import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, fireEvent, waitFor } from "@testing-library/react";
import React from "react";
import { invoke } from "@tauri-apps/api/core";
import OrchestratorPanel from "./OrchestratorPanel";
import { LanguageProvider } from "../../i18n";
import { DEFAULT_ORCHESTRATOR_PROFILES, DEFAULT_VALIDATION_GATES, DEFAULT_ITERATION_LIMITS } from "../../config/orchestratorPresets";

const eventHandlers = new Map<string, (event: { payload: any }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: any }) => void) => {
    eventHandlers.set(name, handler);
    return () => eventHandlers.delete(name);
  }),
}));

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

describe("OrchestratorPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    eventHandlers.clear();
    invokeMock.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === "get_user_language") {
        return "en";
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

  it("switches preset and updates role assignments", async () => {
    render(
      <LanguageProvider>
        <OrchestratorPanel />
      </LanguageProvider>
    );

    await waitFor(() => {
      expect(screen.getByText("Cheap Hybrid (Local Reviewer)")).toBeDefined();
    });

    const cheapPresetBtn = screen.getByText("Cheap Hybrid (Local Reviewer)");
    fireEvent.click(cheapPresetBtn);

    // Cheap hybrid sets plan_reviewer to ollama-mimo-9b
    await waitFor(() => {
      const selects = screen.getAllByRole("combobox");
      const values = selects.map((s) => (s as HTMLSelectElement).value);
      expect(values).toContain("ollama-mimo-9b");
    });
  });

  it("prevents execution when implementer lacks workspace_write capability", async () => {
    render(
      <LanguageProvider>
        <OrchestratorPanel />
      </LanguageProvider>
    );

    await waitFor(() => {
      expect(screen.getByDisplayValue("C:\\mock\\project")).toBeDefined();
      expect(screen.getByText("Cargo.toml ✓")).toBeDefined();
    });

    const selects = screen.getAllByRole("combobox");
    // Select #3 is implementer
    const implementerSelect = selects.find((s) => (s as HTMLSelectElement).value === "codex-cli");
    expect(implementerSelect).toBeDefined();

    if (implementerSelect) {
      fireEvent.change(implementerSelect, { target: { value: "mimo-v26-pro" } });
    }

    await waitFor(() => {
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
    await waitFor(() => expect(screen.getAllByRole("combobox").some(
      (select) => (select as HTMLSelectElement).value === "custom-planner"
    )).toBe(true));
    expect(screen.getAllByRole("option", { name: /Custom Planner/ }).length).toBeGreaterThan(0);

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
    rejectStart(new Error("start failed"));
    await waitFor(() => expect(screen.getByText(/FAILED/)).toBeDefined());
    eventHandlers.get("orchestrator:log")!({ payload: { runId: "old-run", message: "stale after start failure" } });
    expect(screen.queryByText(/stale after start failure/)).toBeNull();
  });
});
