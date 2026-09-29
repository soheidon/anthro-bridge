import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import React from "react";
import { invoke } from "@tauri-apps/api/core";
import OrchestratorPanel from "./OrchestratorPanel";
import { LanguageProvider } from "../../i18n";
import { DEFAULT_ORCHESTRATOR_PROFILES, DEFAULT_ORCHESTRATOR_QUICK_SLOTS, DEFAULT_VALIDATION_GATES, DEFAULT_ITERATION_LIMITS } from "../../config/orchestratorPresets";

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
      if (cmd === "preview_plan_archive") {
        return { nextFileName: "V0.24.0-r1.md" };
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
    fireEvent.click(within(reviewerCard).getByRole("option", { name: /DeepSeek V4\.1 Flash \(Direct API\)/ }));

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
    fireEvent.click(within(plannerCard).getByRole("option", { name: /Ollama MiMo-V2\.6-9B \(Local\)/ }));
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
    expect(within(listbox).getByRole("option")).toHaveTextContent("DeepSeek V4.1 Flash (Direct API)");
    expect(within(listbox).getByRole("option")).toHaveTextContent("Thinking");
    expect(within(listbox).getByRole("option")).toHaveTextContent("High");
    expect(within(plannerCard).queryByRole("button", { name: "MiMo Pro" })).not.toBeInTheDocument();
    expect(within(plannerCard).queryByRole("button", { name: "DeepSeek Flash" })).not.toBeInTheDocument();
    expect(within(listbox).queryByRole("option", { name: /MiMo/ })).not.toBeInTheDocument();
    expect(profileSelect).toHaveTextContent("MiMo-V2.6-Pro (Direct API)");
    expect(profileSelect).toHaveTextContent("orchestrator.quickSlots.notInWorkspaceList");
    expect(within(plannerCard).queryByText("mimo-v2.6-pro + thinking")).not.toBeInTheDocument();
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
    expect(profileSelect).toHaveTextContent("DeepSeek V4.1 Flash (Direct API)");
    expect(profileSelect).toHaveTextContent("Max");
    expect(within(plannerCard).queryByText("DeepSeek V4.1 Flash (Direct API)", { selector: "p.orchestrator-selected-profile" })).not.toBeInTheDocument();
    expect(within(plannerCard).queryByText("User customized profile label")).not.toBeInTheDocument();
    expect(within(plannerCard).queryByText("orchestrator.roles.planner:")).not.toBeInTheDocument();
    fireEvent.click(profileSelect);
    expect(within(plannerCard).getByRole("option", { name: /DeepSeek V4\.1 Flash \(Direct API\)/ })).toHaveTextContent("Max");
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
    expect(within(planner).getByRole("combobox")).toHaveTextContent("Kimi K3 (Direct API)");
    expect(within(planReviewer).getByRole("combobox")).toHaveTextContent("MiMo-V2.6-Pro (Direct API)");
    fireEvent.click(within(planner).getByRole("combobox"));
    fireEvent.click(within(planReviewer).getByRole("combobox"));
    expect(within(planner).getByRole("option", { name: /Kimi K3 \(Direct API\)/ })).toHaveTextContent("Thinking");
    expect(within(planReviewer).getByRole("option", { name: /MiMo-V2\.6-Pro \(Direct API\)/ })).toHaveTextContent("Thinking");
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
    expect(within(planner).getByRole("combobox")).toHaveTextContent("OpenRouter");
    expect(within(planner).getByRole("combobox")).toHaveTextContent("GPT-5.6 Sol");
    expect(within(reviewer).getByRole("combobox")).toHaveTextContent("Ollama");
    expect(within(reviewer).getByRole("combobox")).toHaveTextContent("Ollama Qwen3.6 27B (Local)");
    fireEvent.click(within(planner).getByRole("combobox"));
    fireEvent.click(within(reviewer).getByRole("combobox"));
    expect(within(planner).getByRole("option", { name: /GPT-5\.6 Sol/ })).toHaveTextContent("High");
    expect(within(reviewer).getByRole("option", { name: /Qwen3\.6 27B/ })).toHaveTextContent("Local");
    expect(within(reviewer).queryByText("Ollama: MiMo-V2.6-9B (Local)")).not.toBeInTheDocument();
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
  });
});
