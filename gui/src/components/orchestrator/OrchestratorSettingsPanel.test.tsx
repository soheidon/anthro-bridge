import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import OrchestratorSettingsPanel from "./OrchestratorSettingsPanel";
import { DEFAULT_ITERATION_LIMITS, DEFAULT_VALIDATION_GATES } from "../../config/orchestratorPresets";
import { translations as jaTranslations } from "../../i18n/lang/ja";
import type { OrchestratorConfig } from "../../types/orchestrator";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const summaryTranslator = (key: unknown) => ({
  "apiKeyPanel.thinkingModeOn": "Thinking",
  "apiKeyPanel.thinkingOnly": "Thinking only",
  "apiKeyPanel.reasoningEffortHigh": "High",
}[String(key)] ?? String(key));
const expandProvider = async (providerKey: string, label: string) => {
  const details = await waitFor(() => {
    const element = document.querySelector(`details[data-provider="${providerKey}"]`);
    if (!element) throw new Error(`Missing provider group ${providerKey}`);
    return element as HTMLDetailsElement;
  });
  expect(details.querySelector("summary")).toHaveTextContent(label);
  if (!details.open) fireEvent.click(details.querySelector("summary")!);
  return details;
};
const persistedConfig: OrchestratorConfig = {
  profiles: [{
    id: "custom-reviewer", displayName: "Persisted Reviewer", adapter: "provider",
    providerId: "deepseek", model: "deepseek-v4.1-flash", thinkingMode: "thinking",
    capabilities: ["reasoning", "review", "workspace_read"],
  }],
  assignments: {
    planner: { role: "planner", profileId: "custom-reviewer" },
    plan_integrator: { role: "plan_integrator", profileId: "custom-reviewer" },
    plan_reviewer: { role: "plan_reviewer", profileId: "custom-reviewer" },
    implementer: { role: "implementer", profileId: "custom-reviewer" },
    fixer: { role: "fixer", profileId: "custom-reviewer" },
    code_reviewer: { role: "code_reviewer", profileId: "custom-reviewer" },
  },
  quickSlots: [{ id: "user-slot", profileId: "custom-reviewer", label: "My reviewer", visible: true, order: 0 }],
  validationGates: DEFAULT_VALIDATION_GATES,
  iterationLimits: DEFAULT_ITERATION_LIMITS,
  authorizedCustomGates: [],
};

describe("OrchestratorSettingsPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    localStorage.removeItem("anthro-bridge.orchestrator-settings.expanded-providers");
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return persistedConfig;
      return null;
    });
  });

  it("derives profile names instead of exposing an editable field and persists only canonical names on edits", async () => {
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    const group = await expandProvider("deepseek", "DeepSeek");
    expect(group.open).toBe(true);
    expect(within(group).queryByText("deepseek-v4.1-flash + thinking: High")).not.toBeInTheDocument();
    expect(within(group).getByRole("checkbox", { name: "orchestrator.settings.visible" })).toBeChecked();
    expect(screen.queryByLabelText("orchestrator.settings.displayName")).not.toBeInTheDocument();

    fireEvent.change(within(group).getByLabelText("orchestrator.settings.model"), { target: { value: "deepseek-flash" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      { config: { profiles: [{ ...persistedConfig.profiles![0], model: "deepseek-flash", displayName: "deepseek-flash + thinking: High", reasoningEffort: "high" }] } },
    ));
    expect(within(group.querySelector("summary")!).getByText(/deepseek-flash \+ thinking: High/)).toBeInTheDocument();
    expect(within(group).getByRole("checkbox", { name: "orchestrator.settings.visible" })).toBeChecked();
    expect(invokeMock.mock.calls[0][0]).toBe("get_orchestrator_config");
    expect(invokeMock.mock.calls.some(([command, args]) => command === "update_orchestrator_config" && "quickSlots" in (args as { config: object }).config)).toBe(false);
  });

  it("refreshes Ollama models from the configured endpoint and lets the user select one", async () => {
    const ollamaProfile = {
      id: "local-ollama", displayName: "Local Ollama", adapter: "ollama" as const,
      ollamaEndpoint: "http://127.0.0.1:11434", ollamaModel: "old-model",
      capabilities: ["reasoning", "review", "workspace_read"],
    };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [ollamaProfile] };
      if (command === "list_ollama_models_for_settings") return { status: "success", models: ["gemma4:26b", "qwen3.6:27b"] };
      return null;
    });
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("ollama", "Ollama (Local)");
    fireEvent.click(screen.getByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_ollama_models_for_settings", {
      endpoint: "http://127.0.0.1:11434",
    }));
    expect(await screen.findByRole("status")).toHaveTextContent("2 apiKeyPanel.ollamaLocal.modelsFound");

    fireEvent.change(screen.getByRole("combobox", { name: "old-model apiKeyPanel.ollamaLocal.modelTag" }), {
      target: { value: "gemma4:26b" },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { profiles: [{ ...ollamaProfile, ollamaModel: "gemma4:26b", displayName: "gemma4:26b" }] },
    }));
  });

  it("reports when the Ollama service is unavailable", async () => {
    const ollamaProfile = {
      id: "local-ollama", displayName: "Local Ollama", adapter: "ollama" as const,
      ollamaEndpoint: "http://127.0.0.1:11434", ollamaModel: "gemma4:26b",
      capabilities: ["reasoning", "review", "workspace_read"],
    };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [ollamaProfile] };
      if (command === "list_ollama_models_for_settings") return { status: "error", code: "connection_failed" };
      return null;
    });
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("ollama", "Ollama (Local)");
    fireEvent.click(screen.getByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    expect(await screen.findByRole("status")).toHaveTextContent("apiKeyPanel.ollamaLocal.error.connection_failed");
  });

  it.each([
    ["timeout", "apiKeyPanel.ollamaLocal.error.timeout"],
    ["invalid_endpoint", "apiKeyPanel.ollamaLocal.error.invalid_endpoint"],
    ["api_error", "apiKeyPanel.ollamaLocal.error.api_error"],
    ["invalid_response", "apiKeyPanel.ollamaLocal.error.invalid_response"],
  ] as const)("shows the localized Ollama error for %s and preserves the saved model", async (code, messageKey) => {
    const ollamaProfile = {
      id: "local-ollama", displayName: "Local Ollama", adapter: "ollama" as const,
      ollamaEndpoint: "http://127.0.0.1:11434", ollamaModel: "saved-model:latest",
      capabilities: ["reasoning", "review", "workspace_read"],
    };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [ollamaProfile] };
      if (command === "list_ollama_models_for_settings") return { status: "error", code };
      return null;
    });
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("ollama", "Ollama (Local)");
    fireEvent.click(screen.getByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    expect(await screen.findByRole("status")).toHaveTextContent(messageKey);
    expect(screen.getByRole("combobox", { name: "saved-model:latest apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model:latest");
    expect(invokeMock).not.toHaveBeenCalledWith("update_orchestrator_config", expect.objectContaining({
      config: expect.objectContaining({ profiles: expect.anything() }),
    }));
  });

  it("keeps the saved model visible when a successful refresh returns no models", async () => {
    const ollamaProfile = {
      id: "local-ollama", displayName: "Local Ollama", adapter: "ollama" as const,
      ollamaEndpoint: "http://127.0.0.1:11434", ollamaModel: "saved-model:latest",
      capabilities: ["reasoning", "review", "workspace_read"],
    };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [ollamaProfile] };
      if (command === "list_ollama_models_for_settings") return { status: "success", models: [] };
      return null;
    });
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("ollama", "Ollama (Local)");
    fireEvent.click(screen.getByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    expect(await screen.findByRole("status")).toHaveTextContent("apiKeyPanel.ollamaLocal.noModelsFound");
    expect(screen.getByRole("combobox", { name: "saved-model:latest apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model:latest");
  });

  it("keeps concurrent Ollama refresh results isolated to their profile", async () => {
    const pending: Array<(value: unknown) => void> = [];
    const profileA = {
      id: "ollama-a", displayName: "Ollama A", adapter: "ollama" as const,
      ollamaEndpoint: "http://127.0.0.1:11434", ollamaModel: "model-a",
      capabilities: ["reasoning", "review", "workspace_read"],
    };
    const profileB = {
      ...profileA, id: "ollama-b", displayName: "Ollama B", ollamaEndpoint: "http://127.0.0.1:11435", ollamaModel: "model-b",
    };
    invokeMock.mockImplementation((command) => {
      if (command === "get_orchestrator_config") return Promise.resolve({ ...persistedConfig, profiles: [profileA, profileB] });
      if (command === "list_ollama_models_for_settings") return new Promise((resolve) => { pending.push(resolve); });
      return Promise.resolve(null);
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("ollama", "Ollama (Local)");
    const cardA = container.querySelector('[data-profile-id="ollama-a"]') as HTMLElement;
    const cardB = container.querySelector('[data-profile-id="ollama-b"]') as HTMLElement;
    fireEvent.click(within(cardA).getByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    fireEvent.click(within(cardB).getByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    expect(pending).toHaveLength(2);
    pending[1]({ status: "success", models: ["model-from-b"] });
    pending[0]({ status: "success", models: ["model-from-a"] });
    await waitFor(() => {
      expect(within(cardA).getByRole("option", { name: "model-from-a" })).toBeInTheDocument();
      expect(within(cardB).getByRole("option", { name: "model-from-b" })).toBeInTheDocument();
    });
    expect(within(cardA).queryByRole("option", { name: "model-from-b" })).not.toBeInTheDocument();
    expect(within(cardB).queryByRole("option", { name: "model-from-a" })).not.toBeInTheDocument();
  });

  it("discards a refresh response after the endpoint changes", async () => {
    let resolveRefresh!: (value: unknown) => void;
    const ollamaProfile = {
      id: "ollama-a", displayName: "Ollama A", adapter: "ollama" as const,
      ollamaEndpoint: "http://127.0.0.1:11434", ollamaModel: "saved-model",
      capabilities: ["reasoning", "review", "workspace_read"],
    };
    invokeMock.mockImplementation((command) => {
      if (command === "get_orchestrator_config") return Promise.resolve({ ...persistedConfig, profiles: [ollamaProfile] });
      if (command === "list_ollama_models_for_settings") return new Promise((resolve) => { resolveRefresh = resolve; });
      return Promise.resolve(null);
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("ollama", "Ollama (Local)");
    const profileCard = container.querySelector('[data-profile-id="ollama-a"]') as HTMLElement;
    fireEvent.click(within(profileCard).getByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    fireEvent.change(within(profileCard).getByDisplayValue("http://127.0.0.1:11434"), { target: { value: "http://127.0.0.1:11435" } });
    resolveRefresh({ status: "success", models: ["old-endpoint-model"] });
    await waitFor(() => expect(within(profileCard).getByRole("combobox", { name: "saved-model apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model"));
    expect(within(profileCard).queryByRole("option", { name: "old-endpoint-model" })).not.toBeInTheDocument();
  });

  it("edits quick-slot visibility and iteration defaults through partial updates", async () => {
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.iterationLimits");
    await expandProvider("deepseek", "DeepSeek");
    await expandProvider("deepseek", "DeepSeek");

    const profileCard = document.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const visibilityCheckbox = within(profileCard).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    fireEvent.click(visibilityCheckbox);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      { config: { quickSlots: [{ ...persistedConfig.quickSlots![0], visible: false }] } },
    ));

    fireEvent.change(screen.getByLabelText("orchestrator.settings.maxFixIterations"), { target: { value: "4" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      { config: { iterationLimits: { ...DEFAULT_ITERATION_LIMITS, maxFixIterations: 4 } } },
    ));
  });

  it("uses the Profile checkbox as the sole visibility control and preserves slot identity, label, and order", async () => {
    const customSlot = { ...persistedConfig.quickSlots![0], label: "My Custom Slot", order: 7, visible: true };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, quickSlots: [customSlot] };
      return null;
    });

    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.iterationLimits");
    await expandProvider("deepseek", "DeepSeek");
    const profileCard = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const profileVisibility = within(profileCard).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    expect(profileVisibility).toBeChecked();

    expect(container.querySelector('[data-slot-id]')).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Move/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Remove.*Direct API/i })).not.toBeInTheDocument();

    fireEvent.click(profileVisibility);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [{ ...customSlot, visible: false }] },
    }));
    await waitFor(() => expect(profileVisibility).not.toBeDisabled());
    expect(profileVisibility).not.toBeChecked();

    fireEvent.click(profileVisibility);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [{ ...customSlot, visible: true }] },
    }));
    expect(profileVisibility).toBeChecked();
  });

  it("creates a slot from the Profile checkbox and preserves it on uncheck", async () => {
    const slotFreeProfile = {
      ...persistedConfig.profiles![0],
      id: "slot-free-profile",
      displayName: "Slot Free Profile",
    };
    const existingSlot = { ...persistedConfig.quickSlots![0], label: "Existing Custom Name", order: 4 };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          profiles: [...persistedConfig.profiles!, slotFreeProfile],
          quickSlots: [existingSlot],
        };
      }
      return null;
    });

    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.iterationLimits");
    await expandProvider("deepseek", "DeepSeek");
    const slotFreeCard = container.querySelector('[data-profile-id="slot-free-profile"]') as HTMLElement;
    const visibility = within(slotFreeCard).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    expect(visibility).not.toBeChecked();
    fireEvent.click(visibility);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [existingSlot, expect.objectContaining({
        profileId: "slot-free-profile",
        label: "deepseek-v4.1-flash + thinking",
        visible: true,
        order: 5,
      })] },
    }));
    const createSaves = invokeMock.mock.calls.filter(([command]) => command === "update_orchestrator_config");
    const createdSlot = (createSaves[createSaves.length - 1][1] as { config: { quickSlots: Array<{ id: string; profileId: string }> } }).config.quickSlots[1];
    expect(createdSlot.id).toMatch(/^slot-/);
    expect(visibility).toBeChecked();

    fireEvent.click(visibility);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [existingSlot, expect.objectContaining({
        id: createdSlot.id,
        profileId: "slot-free-profile",
        visible: false,
      })] },
    }));
    expect(visibility).not.toBeChecked();
  });

  it("keeps hidden Quick Slots manageable via the workspace visibility checkbox", async () => {
    const secondProfile = { ...persistedConfig.profiles![0], id: "hidden-profile", displayName: "Hidden Profile" };
    const hiddenSlot = { id: "hidden-slot", profileId: "hidden-profile", label: "Hidden Slot", visible: false, order: 1 };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return {
        ...persistedConfig,
        profiles: [...persistedConfig.profiles!, secondProfile],
        quickSlots: [persistedConfig.quickSlots![0], hiddenSlot],
      };
      return null;
    });

    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const hiddenCard = container.querySelector('[data-profile-id="hidden-profile"]') as HTMLElement;
    const visibility = within(hiddenCard).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    expect(visibility).not.toBeChecked();
    expect(container.querySelector('[data-slot-id="hidden-slot"]')).not.toBeInTheDocument();

    fireEvent.click(visibility);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        quickSlots: [
          persistedConfig.quickSlots![0],
          { ...hiddenSlot, visible: true },
        ],
      },
    }));
    expect(visibility).toBeChecked();
  });

  it("keeps a Quick Slot with a missing profile association visible without exposing reassignment", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [] };
      return null;
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.iterationLimits");
    expect(container.querySelector('[data-slot-id="user-slot"]')).not.toBeInTheDocument();
    expect([...container.querySelectorAll(".orchestrator-settings-section > h3")]
      .some((heading) => heading.textContent === "orchestrator.settings.quickSlots")).toBe(false);

    fireEvent.change(screen.getByLabelText("orchestrator.settings.maxFixIterations"), { target: { value: "4" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { iterationLimits: { ...DEFAULT_ITERATION_LIMITS, maxFixIterations: 4 } },
    }));
  });

  it("uses conceptual category labels and read-only commands for existing gates", async () => {
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);

    expect(await screen.findByText("orchestrator.validation.category.static_check")).toBeInTheDocument();
    expect(screen.getByText("orchestrator.validation.category.tests")).toBeInTheDocument();
    expect(screen.getByText("orchestrator.validation.category.repository_check")).toBeInTheDocument();
    expect(screen.getByText("npx tsc --noEmit")).toBeInTheDocument();
    expect(screen.getByText("npm test -- --run")).toBeInTheDocument();
    expect(screen.getByText("git status --short")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /configured command/i })).not.toBeInTheDocument();
  });

  it("falls back to legacy gate labels when category is omitted", async () => {
    const legacyGates = [
      { id: "typecheck", name: "TypeScript Check", executable: "npx", args: ["tsc"], enabled: true, failOnError: true },
      { id: "test", name: "Test Suite", executable: "npm", args: ["test"], enabled: true, failOnError: true },
      { id: "git-status", name: "Git Status Check", executable: "git", args: ["status"], enabled: true, failOnError: false },
    ];
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return { ...persistedConfig, validationGates: legacyGates };
      }
      return null;
    });
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);

    expect(await screen.findByText("orchestrator.settings.gateTypeCheck")).toBeInTheDocument();
    expect(screen.getByText("orchestrator.settings.gateTests")).toBeInTheDocument();
    expect(screen.getByText("orchestrator.settings.gateRepositoryState")).toBeInTheDocument();
  });

  it("preserves unknown gate names and sends only gate ID and enabled state", async () => {
    const customGate = {
      id: "custom-test-name",
      name: "Test Suite",
      executable: "custom-runner",
      args: ["--strict"],
      enabled: false,
      workingDir: "subproject",
      failOnError: true,
      isAdvancedCustom: true,
    };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return { ...persistedConfig, validationGates: [customGate] };
      }
      return null;
    });
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);

    const checkbox = await screen.findByRole("checkbox", { name: "Test Suite" });
    expect(screen.getByText("custom-runner --strict")).toBeInTheDocument();
    expect(screen.queryByText("orchestrator.settings.gateTests")).not.toBeInTheDocument();
    fireEvent.click(checkbox);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_validation_gate_enabled", {
      gateId: "custom-test-name",
      enabled: true,
    }));
    expect(invokeMock).not.toHaveBeenCalledWith("update_orchestrator_config", expect.anything());
  });

  it("does not render the Diagnostics section or disabled default-workflow row in Settings UI", async () => {
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.validation.category.static_check");

    expect(container.querySelector("details.orchestrator-settings-diagnostics")).not.toBeInTheDocument();
    expect(screen.queryByText("orchestrator.settings.advanced")).not.toBeInTheDocument();
    expect(screen.queryByText("orchestrator.settings.redactionActive")).not.toBeInTheDocument();
    expect(screen.queryByText("orchestrator.settings.processIsolationActive")).not.toBeInTheDocument();
    expect(screen.queryByText("orchestrator.settings.authorizedCustomGates")).not.toBeInTheDocument();
    expect(screen.queryByText("orchestrator.settings.defaultWorkflow")).not.toBeInTheDocument();
  });

  it("renders iteration limits as horizontal dropdowns and preserves non-standard values like 0, 15, and 120", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          iterationLimits: {
            maxPlanReviewIterations: 0,
            maxFixIterations: 15,
            maxCodeReviewIterations: 120,
          },
        };
      }
      return null;
    });

    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.iterationLimits");

    const limitsContainer = container.querySelector(".orchestrator-settings-iteration-limits");
    expect(limitsContainer).toBeInTheDocument();

    const planSelect = screen.getByLabelText("orchestrator.settings.maxPlanReviewIterations") as HTMLSelectElement;
    const fixSelect = screen.getByLabelText("orchestrator.settings.maxFixIterations") as HTMLSelectElement;
    const codeSelect = screen.getByLabelText("orchestrator.settings.maxCodeReviewIterations") as HTMLSelectElement;

    expect(planSelect.value).toBe("0");
    expect(within(planSelect).getByRole("option", { name: "0" })).toBeInTheDocument();

    expect(fixSelect.value).toBe("15");
    expect(within(fixSelect).getByRole("option", { name: "15" })).toBeInTheDocument();

    expect(codeSelect.value).toBe("120");
    expect(within(codeSelect).getByRole("option", { name: "120" })).toBeInTheDocument();

    // No auto-save on load merely for rendering non-standard values
    expect(invokeMock).not.toHaveBeenCalledWith("update_orchestrator_config", expect.anything());

    // Selecting a standard value works
    fireEvent.change(planSelect, { target: { value: "3" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        iterationLimits: {
          maxPlanReviewIterations: 3,
          maxFixIterations: 15,
          maxCodeReviewIterations: 120,
        },
      },
    }));
  });

  it("shows a canonical model and thinking summary while collapsed, then reveals the existing controls", async () => {
    const profile = { ...persistedConfig.profiles![0], reasoningEffort: "high" };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [profile] };
      return null;
    });

    const { container } = render(<OrchestratorSettingsPanel t={summaryTranslator} />);
    await screen.findByText("orchestrator.settings.profiles");
    const group = container.querySelector('details[data-provider="deepseek"]') as HTMLDetailsElement;
    const summary = group.querySelector("summary") as HTMLElement;

    expect(group.open).toBe(false);
    expect(summary).toHaveTextContent("DeepSeek");
    expect(summary).toHaveTextContent("deepseek-v4.1-flash + thinking: High");
    expect(within(group).getByLabelText("orchestrator.settings.model")).not.toBeVisible();

    fireEvent.click(summary);
    expect(group.open).toBe(true);
    expect(within(group).getByLabelText("orchestrator.settings.model")).toBeVisible();
    expect(within(group).getByLabelText("orchestrator.settings.thinkingMode")).toBeVisible();
    expect(within(group).getByLabelText("orchestrator.settings.reasoningEffort")).toBeVisible();
  });

  it("uses canonical compact summaries for OpenRouter, Ollama, and CLI profiles", async () => {
    const profiles: OrchestratorConfig["profiles"] = [
      {
        id: "openrouter-summary", displayName: "Legacy name", adapter: "provider", providerId: "openrouter",
        model: "openai/gpt-5.6-sol", thinkingMode: "thinking", reasoningEffort: "high", capabilities: ["reasoning"],
      },
      {
        id: "openrouter-summary-2", displayName: "Another legacy name", adapter: "provider", providerId: "openrouter",
        model: "openai/gpt-5.6-terra", thinkingMode: "thinking", reasoningEffort: "medium", capabilities: ["reasoning"],
      },
      {
        id: "kimi-summary", displayName: "Legacy Kimi name", adapter: "provider", providerId: "kimi",
        model: "kimi-k3", thinkingMode: "thinking", capabilities: ["reasoning"],
      },
      {
        id: "ollama-summary", displayName: "Legacy local name", adapter: "ollama", ollamaModel: "mimo-v2.6:9b",
        capabilities: ["reasoning"],
      },
      {
        id: "cli-summary", displayName: "Legacy CLI name", adapter: "cli", executable: "codex", args: ["exec"],
        capabilities: ["reasoning"],
      },
    ];
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles, quickSlots: [] };
      return null;
    });

    const { container } = render(<OrchestratorSettingsPanel t={summaryTranslator} />);
    await screen.findByText("orchestrator.settings.profiles");
    expect(container.querySelector('details[data-provider="openrouter"] summary')).toHaveTextContent("openai/gpt-5.6-sol + thinking: High");
    expect(container.querySelector('details[data-provider="openrouter"] summary')).toHaveTextContent("2 orchestrator.settings.profiles");
    expect(container.querySelector('details[data-provider="kimi"] summary')).toHaveTextContent("kimi-k3 + thinking");
    expect(container.querySelector('details[data-provider="ollama"] summary')).toHaveTextContent("mimo-v2.6:9b");
    expect(container.querySelector('details[data-provider="cli"] summary')).toHaveTextContent("codex-cli");
  });

  it("starts provider groups collapsed and creates a provider-specific profile without replacing other config", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [], quickSlots: [] };
      return null;
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.profiles");
    expect([...container.querySelectorAll("details[data-provider]")].every((group) => !(group as HTMLDetailsElement).open)).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "+ orchestrator.settings.addProfile · DeepSeek" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", expect.objectContaining({
      config: expect.objectContaining({ profiles: expect.arrayContaining([expect.objectContaining({ adapter: "provider", providerId: "deepseek" })]) }),
    })));
    const profileSave = invokeMock.mock.calls.find(([command, args]) => command === "update_orchestrator_config" && "profiles" in (args as { config: object }).config);
    expect(profileSave).toBeDefined();
    expect(Object.keys((profileSave?.[1] as { config: object }).config)).toEqual(["profiles"]);
    const createdProfiles = (profileSave?.[1] as { config: { profiles: OrchestratorConfig["profiles"] } }).config.profiles ?? [];
    expect(createdProfiles).toHaveLength(1);
    expect(createdProfiles[0]).toMatchObject({ providerId: "deepseek", model: "deepseek-v4.1-flash", reasoningEffort: "high" });
    expect((container.querySelector('details[data-provider="deepseek"]') as HTMLDetailsElement).open).toBe(true);
  });

  it("hides the page title and Other group while preserving unknown profiles in saved config", async () => {
    const unknownProfile = {
      id: "future-provider-profile",
      displayName: "Future Provider Profile",
      adapter: "provider" as const,
      providerId: "future-provider",
      model: "future-model-v1",
      capabilities: ["reasoning" as const],
    };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return { ...persistedConfig, profiles: [...persistedConfig.profiles!, unknownProfile] };
      }
      return null;
    });

    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");

    expect(screen.queryByRole("heading", { name: "orchestrator.settings.title" })).not.toBeInTheDocument();
    expect(container.querySelector('details[data-provider="other"]')).not.toBeInTheDocument();
    expect(container.querySelectorAll("details[data-provider]")).toHaveLength(9);
    expect(screen.queryByRole("button", { name: /addProfile.*Other/i })).not.toBeInTheDocument();
    expect(container.querySelector('[data-profile-id="future-provider-profile"]')).not.toBeInTheDocument();

    const group = await expandProvider("deepseek", "DeepSeek");
    fireEvent.change(within(group).getByLabelText("orchestrator.settings.contextWindow"), { target: { value: "64000" } });

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        profiles: [
          { ...persistedConfig.profiles![0], contextWindowTokens: 64000, displayName: "deepseek-v4.1-flash + thinking" },
          unknownProfile,
        ],
      },
    }));
  });

  it("guards deletion of assigned profiles and removes quick slots when an unassigned profile is deleted", async () => {
    const secondProfile = { ...persistedConfig.profiles![0], id: "unused-profile", displayName: "Unused Profile" };
    const secondSlot = { id: "unused-slot", profileId: "unused-profile", label: "Unused", visible: false, order: 1 };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return {
        ...persistedConfig,
        profiles: [...persistedConfig.profiles!, secondProfile],
        quickSlots: [...persistedConfig.quickSlots!, secondSlot],
      };
      return null;
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const assignedCard = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const assignedDelete = within(assignedCard).getByRole("button", { name: "orchestrator.settings.deleteProfile" });
    expect(assignedDelete).toBeDisabled();
    fireEvent.click(assignedDelete);
    expect(invokeMock).not.toHaveBeenCalledWith("update_orchestrator_config", expect.objectContaining({
      config: expect.objectContaining({ profiles: expect.not.arrayContaining([expect.objectContaining({ id: "custom-reviewer" })]) }),
    }));

    const unusedCard = container.querySelector('[data-profile-id="unused-profile"]') as HTMLElement;
    expect(unusedCard.querySelector(".orchestrator-profile-header")).toBeInTheDocument();
    expect(unusedCard.querySelector(".orchestrator-profile-actions")).not.toBeInTheDocument();
    fireEvent.click(within(unusedCard).getByRole("button", { name: "orchestrator.settings.deleteProfile" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        profiles: [persistedConfig.profiles![0]],
        quickSlots: [persistedConfig.quickSlots![0]],
      },
    }));
  });

  it("renders the configuration name and delete button in the profile header for both single and multi-profile providers", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, assignments: {} };
      return null;
    });
    const { container, unmount } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const singleCard = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const singleHeader = singleCard.querySelector(".orchestrator-profile-header") as HTMLElement;
    expect(within(singleHeader).getByText("deepseek-v4.1-flash + thinking")).toBeInTheDocument();
    expect(within(singleHeader).getByRole("button", { name: "orchestrator.settings.deleteProfile" })).toBeInTheDocument();

    unmount();

    const secondProfile = { ...persistedConfig.profiles![0], id: "second-profile", displayName: "Second Profile" };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [...persistedConfig.profiles!, secondProfile] };
      return null;
    });
    const { container: multiContainer } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const multiCard = multiContainer.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const multiHeader = multiCard.querySelector(".orchestrator-profile-header") as HTMLElement;
    expect(within(multiHeader).getByText("deepseek-v4.1-flash + thinking")).toBeInTheDocument();
    expect(within(multiHeader).getByRole("button", { name: "orchestrator.settings.deleteProfile" })).toBeInTheDocument();
  });

  it("updates capability-dependent thinking and effort settings while preserving the profile identity", async () => {
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const card = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const modelSelect = within(card).getByRole("combobox", { name: "orchestrator.settings.model" });
    expect(within(card).getByRole("combobox", { name: "orchestrator.settings.reasoningEffort" })).toHaveValue("low");
    fireEvent.change(modelSelect, { target: { value: "deepseek-v4-pro" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { profiles: [{ ...persistedConfig.profiles![0], model: "deepseek-v4-pro", reasoningEffort: "high", displayName: "deepseek-v4-pro + thinking: High" }] },
    }));

    fireEvent.change(within(card).getByRole("combobox", { name: "orchestrator.settings.thinkingMode" }), { target: { value: "normal" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { profiles: [{ ...persistedConfig.profiles![0], model: "deepseek-v4-pro", thinkingMode: "normal", reasoningEffort: undefined, displayName: "deepseek-v4-pro" }] },
    }));
  });

  it("hides Adapter and Provider ID controls and preserves canonical mapping on Profile edits", async () => {
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    const group = await expandProvider("deepseek", "DeepSeek");
    const card = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    expect(within(card).queryByLabelText("orchestrator.settings.adapter")).not.toBeInTheDocument();
    expect(within(card).queryByLabelText("orchestrator.settings.providerId")).not.toBeInTheDocument();

    fireEvent.change(within(group).getByLabelText("orchestrator.settings.contextWindow"), { target: { value: "64000" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        profiles: [{ ...persistedConfig.profiles![0], contextWindowTokens: 64000, displayName: "deepseek-v4.1-flash + thinking" }],
      },
    }));
    expect(within(card).getByRole("checkbox", { name: "orchestrator.settings.visible" })).toBeChecked();
  });

  it("relocates Context Window to per-Profile Advanced Details disclosure and outside primary row", async () => {
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const card = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;

    const primaryRow = card.querySelector(".orchestrator-profile-controls-row") as HTMLElement;
    expect(within(primaryRow).queryByLabelText("orchestrator.settings.contextWindow")).not.toBeInTheDocument();

    const advancedDetails = card.querySelector(".orchestrator-profile-advanced-details") as HTMLDetailsElement;
    expect(advancedDetails).toBeInTheDocument();
    expect(within(advancedDetails).getByText("apiKeyPanel.ollamaLocal.advancedSettings")).toBeInTheDocument();

    const contextInput = within(advancedDetails).getByLabelText("orchestrator.settings.contextWindow");
    expect(contextInput).toBeInTheDocument();

    fireEvent.change(contextInput, { target: { value: "128000" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        profiles: [{ ...persistedConfig.profiles![0], contextWindowTokens: 128000, displayName: "deepseek-v4.1-flash + thinking" }],
      },
    }));
  });

  it("renders localized assignment chips and never raw orchestrator.roles snake_case keys", async () => {
    const customT = (key: unknown) => {
      const dict: Record<string, string> = {
        "orchestrator.roles.planner": "Planner (設計)",
        "orchestrator.roles.planReviewer": "Plan Reviewer (計画レビュー)",
        "orchestrator.roles.implementer": "Implementer (実装)",
        "orchestrator.roles.fixer": "Fixer (修正)",
        "orchestrator.roles.codeReviewer": "Code Reviewer (コードレビュー)",
      };
      return dict[String(key)] ?? String(key);
    };
    const { container } = render(<OrchestratorSettingsPanel t={customT} />);
    await expandProvider("deepseek", "DeepSeek");
    const card = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;

    expect(within(card).getByText("Code Reviewer (コードレビュー)")).toBeInTheDocument();
    expect(within(card).getByText("Plan Reviewer (計画レビュー)")).toBeInTheDocument();
    expect(within(card).queryByText("orchestrator.roles.code_reviewer")).not.toBeInTheDocument();
    expect(within(card).queryByText("orchestrator.roles.plan_reviewer")).not.toBeInTheDocument();
  });

  it("renders Japanese labels consistent with MCP/API-key panel (モード and 推論強度)", async () => {
    const jaT = (key: unknown) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key);
    const { container } = render(<OrchestratorSettingsPanel t={jaT} />);
    await expandProvider("deepseek", "DeepSeek");
    const card = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;

    expect(within(card).getByText("モード")).toBeInTheDocument();
    expect(within(card).getByText("推論強度")).toBeInTheDocument();
    expect(within(card).queryByText("Thinkingモード")).not.toBeInTheDocument();
    expect(within(card).queryByText("Reasoning effort")).not.toBeInTheDocument();
  });

  it("omits redundant profile heading for single-profile and renders heading for multi-profile groups", async () => {
    const secondProfile = { ...persistedConfig.profiles![0], id: "second-profile", model: "deepseek-v4-pro" };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return {
        ...persistedConfig,
        profiles: [persistedConfig.profiles![0], secondProfile],
      };
      return null;
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");

    const firstCard = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const secondCard = container.querySelector('[data-profile-id="second-profile"]') as HTMLElement;

    expect(within(firstCard).getByText("deepseek-v4.1-flash + thinking")).toBeInTheDocument();
    expect(within(secondCard).getByText("deepseek-v4-pro + thinking")).toBeInTheDocument();
  });

  it("exposes capabilities checkboxes within an accessible named group", async () => {
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const card = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;

    const capabilitiesGroup = within(card).getByRole("group", { name: "orchestrator.settings.capabilities" });
    expect(capabilitiesGroup).toBeInTheDocument();
    expect(within(capabilitiesGroup).getByRole("checkbox", { name: "orchestrator.capability.reasoning" })).toBeChecked();
    expect(within(capabilitiesGroup).getByRole("checkbox", { name: "orchestrator.capability.review" })).toBeChecked();
    expect(within(capabilitiesGroup).getByRole("checkbox", { name: "orchestrator.capability.workspace_read" })).toBeChecked();
    expect(within(capabilitiesGroup).getByRole("checkbox", { name: "orchestrator.capability.workspace_write" })).not.toBeChecked();
    expect(within(capabilitiesGroup).getByRole("checkbox", { name: "orchestrator.capability.command_execution" })).not.toBeChecked();
  });

  it("manages Quick Slots inside Profile cards without rewriting assignments or profiles", async () => {
    const secondProfile = { ...persistedConfig.profiles![0], id: "unused-profile", displayName: "Unused Profile" };
    const secondSlot = { id: "second-slot", profileId: "unused-profile", label: "Second", visible: true, order: 1 };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return {
        ...persistedConfig,
        profiles: [...persistedConfig.profiles!, secondProfile],
        quickSlots: [...persistedConfig.quickSlots!, secondSlot],
      };
      return null;
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.iterationLimits");
    await expandProvider("deepseek", "DeepSeek");

    const secondProfileCard = container.querySelector('[data-profile-id="unused-profile"]') as HTMLElement;
    expect(container.querySelector('[data-slot-id="second-slot"]')).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Move/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Remove.*Direct API/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "orchestrator.settings.addSlot" })).not.toBeInTheDocument();
    expect([...container.querySelectorAll(".orchestrator-settings-section > h3")]
      .some((heading) => heading.textContent === "orchestrator.settings.quickSlots")).toBe(false);

    const visibilityCheckbox = within(secondProfileCard).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    expect(visibilityCheckbox).toBeChecked();

    fireEvent.click(visibilityCheckbox);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [
        persistedConfig.quickSlots![0],
        { ...secondSlot, visible: false },
      ] },
    }));
  });

  it("uses Profile visibility as the only association control and does not duplicate slots", async () => {
    const secondProfile = { ...persistedConfig.profiles![0], id: "profile-b", displayName: "Profile B" };
    const secondSlot = { id: "slot-b", profileId: "profile-b", label: "Slot B", visible: true, order: 1 };
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return {
        ...persistedConfig,
        profiles: [...persistedConfig.profiles!, secondProfile],
        quickSlots: [persistedConfig.quickSlots![0], secondSlot],
      };
      return null;
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.iterationLimits");
    expect(container.querySelector('[data-slot-id="slot-b"]')).not.toBeInTheDocument();

    await expandProvider("deepseek", "DeepSeek");
    const firstProfile = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const secondProfileCard = container.querySelector('[data-profile-id="profile-b"]') as HTMLElement;
    const firstVisibility = within(firstProfile).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    const secondVisibility = within(secondProfileCard).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    expect(firstVisibility).toBeChecked();
    expect(secondVisibility).toBeChecked();

    fireEvent.click(firstVisibility);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [{ ...persistedConfig.quickSlots![0], visible: false }, secondSlot] },
    }));
    await waitFor(() => expect(firstVisibility).not.toBeDisabled());
    fireEvent.click(firstVisibility);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [{ ...persistedConfig.quickSlots![0], visible: true }, secondSlot] },
    }));

    const lastSave = invokeMock.mock.calls
      .filter(([command]) => command === "update_orchestrator_config")
      .slice(-1)[0][1] as { config: { quickSlots: Array<{ id: string; profileId: string }> } };
    expect(lastSave.config.quickSlots.map((slot) => slot.profileId)).toEqual(["custom-reviewer", "profile-b"]);
    expect(new Set(lastSave.config.quickSlots.map((slot) => slot.profileId)).size).toBe(2);
  });

  it("displays notice when unapplied suggested validation gates exist and applies them via merge", async () => {
    const suggestedGates = [
      {
        id: "gui:typecheck",
        name: "Static Check (npx tsc in gui)",
        category: "static_check" as const,
        executable: "npx",
        args: ["tsc", "--noEmit"],
        enabled: true,
        workingDir: "gui",
        failOnError: true,
      },
      {
        id: "gui/src-tauri:cargo-check",
        name: "Static Check (cargo check in gui/src-tauri)",
        category: "static_check" as const,
        executable: "cargo",
        args: ["check"],
        enabled: true,
        workingDir: "gui/src-tauri",
        failOnError: true,
      },
    ];

    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          projectPath: "C:\\Users\\Sohei\\dev\\anthro-bridge",
          validationGates: [
            {
              id: "typecheck",
              name: "Old Type Check",
              category: "static_check" as const,
              executable: "npx",
              args: ["tsc"],
              enabled: false,
              failOnError: true,
            },
          ],
        };
      }
      if (command === "detect_project_metadata") {
        return {
          path: "C:\\Users\\Sohei\\dev\\anthro-bridge",
          exists: true,
          isDirectory: true,
          projectType: "Mixed",
          detectedFiles: { packageJson: true, cargoToml: true, git: true },
          suggestedGates,
        };
      }
      if (command === "update_orchestrator_config") return null;
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const notice = await screen.findByText("検出された新しい検証項目があります");
    expect(notice).toBeInTheDocument();

    const applyBtn = screen.getByRole("button", { name: "適用" });
    expect(applyBtn).toBeInTheDocument();

    fireEvent.click(applyBtn);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: {
          validationGates: expect.arrayContaining([
            expect.objectContaining({ id: "gui:typecheck", enabled: true }),
            expect.objectContaining({ id: "gui/src-tauri:cargo-check", enabled: true }),
          ]),
        },
      });
    });
  });

  it("displays unapplied notice when gate ID matches but command/arguments differ, and updates on apply", async () => {
    const suggestedGates = [
      {
        id: "typecheck",
        name: "Static Check (npm run typecheck)",
        category: "static_check" as const,
        executable: "npm",
        args: ["run", "typecheck"],
        enabled: true,
        failOnError: true,
      },
    ];

    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          projectPath: "C:\\Users\\Sohei\\dev\\my-app",
          validationGates: [
            {
              id: "typecheck",
              name: "Static Check",
              category: "static_check" as const,
              executable: "npx",
              args: ["tsc"],
              enabled: false,
              failOnError: true,
            },
          ],
        };
      }
      if (command === "detect_project_metadata") {
        return {
          path: "C:\\Users\\Sohei\\dev\\my-app",
          exists: true,
          isDirectory: true,
          projectType: "TypeScript",
          detectedFiles: { packageJson: true },
          suggestedGates,
        };
      }
      if (command === "update_orchestrator_config") return null;
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const notice = await screen.findByText("検出された新しい検証項目があります");
    expect(notice).toBeInTheDocument();

    const applyBtn = screen.getByRole("button", { name: "適用" });
    fireEvent.click(applyBtn);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: {
          validationGates: [
            expect.objectContaining({
              id: "typecheck",
              executable: "npm",
              args: ["run", "typecheck"],
              enabled: false, // user's toggle preserved
            }),
          ],
        },
      });
    });
  });

  it("displays unapplied notice when custom gate has colliding ID with suggested gate", async () => {
    const suggestedGates = [
      {
        id: "test",
        name: "Tests (cargo test)",
        category: "tests" as const,
        executable: "cargo",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];

    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          projectPath: "C:\\Users\\Sohei\\dev\\rust-app",
          validationGates: [
            {
              id: "test",
              name: "Custom Runner",
              category: "custom" as const,
              executable: "./test.sh",
              args: ["--quick"],
              enabled: true,
              failOnError: true,
              isAdvancedCustom: true,
            },
          ],
        };
      }
      if (command === "detect_project_metadata") {
        return {
          path: "C:\\Users\\Sohei\\dev\\rust-app",
          exists: true,
          isDirectory: true,
          projectType: "Rust",
          detectedFiles: { cargoToml: true },
          suggestedGates,
        };
      }
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    // Custom gate collides with suggested gate ID; because custom gates are preserved and not overwritten,
    // the suggested gate is not reflected, so notice must appear
    const notice = await screen.findByText("検出された新しい検証項目があります");
    expect(notice).toBeInTheDocument();
  });

  it("does not display unapplied notice when all suggested gates are fully reflected in config", async () => {
    const suggestedGates = [
      {
        id: "cargo-check",
        name: "Static Check (cargo check)",
        category: "static_check" as const,
        executable: "cargo",
        args: ["check"],
        enabled: true,
        failOnError: true,
      },
    ];

    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          projectPath: "C:\\Users\\Sohei\\dev\\rust-app",
          validationGates: [
            {
              id: "cargo-check",
              name: "Static Check (cargo check)",
              category: "static_check" as const,
              executable: "cargo",
              args: ["check"],
              enabled: false, // user disabled, but definition matches
              failOnError: true,
            },
          ],
        };
      }
      if (command === "detect_project_metadata") {
        return {
          path: "C:\\Users\\Sohei\\dev\\rust-app",
          exists: true,
          isDirectory: true,
          projectType: "Rust",
          detectedFiles: { cargoToml: true },
          suggestedGates,
        };
      }
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("detect_project_metadata", {
        projectPath: "C:\\Users\\Sohei\\dev\\rust-app",
      });
    });

    expect(screen.queryByText("検出された新しい検証項目があります")).not.toBeInTheDocument();
  });

  it("preserves both built-in update and custom gate when custom and built-in gates coexist with same ID", async () => {
    const suggestedGates = [
      {
        id: "test",
        name: "Tests (cargo test)",
        category: "tests" as const,
        executable: "cargo",
        args: ["test"],
        enabled: true,
        failOnError: true,
      },
    ];

    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          projectPath: "C:\\Users\\Sohei\\dev\\rust-app",
          validationGates: [
            {
              id: "test",
              name: "Old Built-in",
              category: "tests" as const,
              executable: "cargo",
              args: ["check", "--tests"],
              enabled: false,
              failOnError: true,
            },
            {
              id: "test",
              name: "Custom Integration Runner",
              category: "custom" as const,
              executable: "./integration.sh",
              args: ["--strict"],
              enabled: true,
              failOnError: true,
              isAdvancedCustom: true,
            },
          ],
        };
      }
      if (command === "detect_project_metadata") {
        return {
          path: "C:\\Users\\Sohei\\dev\\rust-app",
          exists: true,
          isDirectory: true,
          projectType: "Rust",
          detectedFiles: { cargoToml: true },
          suggestedGates,
        };
      }
      if (command === "update_orchestrator_config") return null;
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const notice = await screen.findByText("検出された新しい検証項目があります");
    expect(notice).toBeInTheDocument();

    const applyBtn = screen.getByRole("button", { name: "適用" });
    fireEvent.click(applyBtn);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: {
          validationGates: [
            expect.objectContaining({
              id: "test",
              executable: "cargo",
              args: ["test"],
              enabled: false,
            }),
            expect.objectContaining({
              id: "test",
              executable: "./integration.sh",
              isAdvancedCustom: true,
              enabled: true,
            }),
          ],
        },
      });
    });
  });

  it("defaults auto-validation toggle to unchecked (opt-in false)", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          autoValidationEnabled: false,
        };
      }
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const toggle = await screen.findByRole("switch", { name: /自動検証/ });
    expect(toggle).toHaveAttribute("aria-checked", "false");
  });

  it("immediately merges preloaded suggested gates and saves when user enables auto-validation", async () => {
    const suggestedGates = [
      {
        id: "gui:typecheck",
        name: "TypeScript Check",
        category: "static_check" as const,
        executable: "npx",
        args: ["tsc", "--noEmit"],
        workingDir: "gui",
        enabled: true,
        failOnError: true,
      },
    ];

    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          projectPath: "C:\\Users\\Sohei\\dev\\my-app",
          autoValidationEnabled: false,
          validationGates: [],
        };
      }
      if (command === "detect_project_metadata") {
        return {
          path: "C:\\Users\\Sohei\\dev\\my-app",
          exists: true,
          isDirectory: true,
          projectType: "TypeScript",
          detectedFiles: { packageJson: true },
          suggestedGates,
        };
      }
      if (command === "update_orchestrator_config") return null;
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const toggle = await screen.findByRole("switch", { name: /自動検証/ });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    // Wait until suggested gates are preloaded
    await screen.findByText("検出された新しい検証項目があります");

    fireEvent.click(toggle);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: {
          autoValidationEnabled: true,
          validationGates: [
            expect.objectContaining({
              id: "gui:typecheck",
              executable: "npx",
              workingDir: "gui",
            }),
          ],
        },
      });
    });
  });

  it("groups validation gates by workingDir and renders Repository Root for root gates in detailed accordion", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          validationGates: [
            {
              id: "root-gate",
              name: "Root Gate",
              category: "repository_check" as const,
              executable: "git",
              args: ["status"],
              enabled: true,
              failOnError: true,
              workingDir: null,
            },
            {
              id: "gui-gate",
              name: "GUI Check",
              category: "static_check" as const,
              executable: "cargo",
              args: ["check"],
              enabled: true,
              failOnError: true,
              workingDir: "gui/src-tauri",
            },
            {
              id: "mcp-gate",
              name: "MCP Check",
              category: "static_check" as const,
              executable: "cargo",
              args: ["check"],
              enabled: true,
              failOnError: true,
              workingDir: "mcp-server",
            },
          ],
        };
      }
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    expect(await screen.findByText("リポジトリ全体")).toBeInTheDocument();
    expect(screen.getByText("gui/src-tauri")).toBeInTheDocument();
    expect(screen.getByText("mcp-server")).toBeInTheDocument();
  });

  it("renders simplified validation UI with a single section, toggle switch row, and accordion", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          autoValidationEnabled: false,
          validationGates: [
            {
              id: "gate-1",
              name: "Gate 1",
              category: "static_check" as const,
              executable: "npx",
              args: ["tsc"],
              enabled: true,
              failOnError: true,
            },
          ],
        };
      }
      return null;
    });

    const { container } = render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const switchBtn = await screen.findByRole("switch", { name: /自動検証/ });
    expect(switchBtn).toBeInTheDocument();
    expect(switchBtn).toHaveAttribute("aria-checked", "false");

    // Check that auto-validation row exists without nested card wrapper
    const autoValidationRow = container.querySelector(".orchestrator-auto-validation-row");
    expect(autoValidationRow).toBeInTheDocument();

    // Check inline row contains label, switch button, and description in one row
    const inlineWrapper = autoValidationRow?.querySelector(".toggle-switch-inline-wrapper");
    expect(inlineWrapper).toBeInTheDocument();
    expect(inlineWrapper?.querySelector(".toggle-switch-label")).toHaveTextContent("自動検証");
    expect(inlineWrapper?.querySelector('button[role="switch"]')).toBe(switchBtn);

    // Check description is rendered to the right of the switch in the same inline row
    const desc = inlineWrapper?.querySelector(".toggle-switch-description");
    expect(desc).toBeInTheDocument();
    expect(desc).toHaveTextContent("プロジェクト構成に応じて必要な検証を自動実行します");

    // Check that detailed settings is an accordion details element
    const detailsEl = container.querySelector("details.orchestrator-validation-details");
    expect(detailsEl).toBeInTheDocument();
    expect(detailsEl?.querySelector("summary")).toHaveTextContent("詳細設定");

    // Toggle switch interactively via switch button click
    fireEvent.click(switchBtn);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: expect.objectContaining({
          autoValidationEnabled: true,
        }),
      });
    });
  });

  it("toggles auto-validation when clicking the visible title label", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          autoValidationEnabled: false,
          validationGates: [],
        };
      }
      if (command === "update_orchestrator_config") return null;
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const switchBtn = await screen.findByRole("switch", { name: /自動検証/ });
    expect(switchBtn).toHaveAttribute("aria-checked", "false");

    const labelText = screen.getByText("自動検証");
    fireEvent.click(labelText);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: expect.objectContaining({
          autoValidationEnabled: true,
        }),
      });
    });
  });

  it("restores autoValidationEnabled: true on reload and displays ON state", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          autoValidationEnabled: true,
          validationGates: [],
        };
      }
      return null;
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const switchBtn = await screen.findByRole("switch", { name: /自動検証/ });
    expect(switchBtn).toBeInTheDocument();
    expect(switchBtn).toHaveAttribute("aria-checked", "true");
    expect(switchBtn).toHaveClass("toggle-switch-on");
  });

  it("ignores stale detect_project_metadata responses after unmount", async () => {
    let resolveMetadata: ((val: any) => void) | null = null;

    invokeMock.mockImplementation((command) => {
      if (command === "get_orchestrator_config") {
        return Promise.resolve({
          ...persistedConfig,
          projectPath: "C:\\projects\\my-app",
          autoValidationEnabled: true,
          validationGates: [],
        });
      }
      if (command === "detect_project_metadata") {
        return new Promise((resolve) => {
          resolveMetadata = resolve;
        });
      }
      if (command === "update_orchestrator_config") return Promise.resolve(null);
      return Promise.resolve(null);
    });

    const { unmount } = render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    await waitFor(() => expect(resolveMetadata).toBeTypeOf("function"));

    unmount();
    invokeMock.mockClear();

    // Late resolve after unmount
    act(() => {
      resolveMetadata!({
        path: "C:\\projects\\my-app",
        exists: true,
        isDirectory: true,
        projectType: "Late Project",
        detectedFiles: {},
        suggestedGates: [
          {
            id: "late-gate",
            name: "Late Gate",
            category: "tests" as const,
            executable: "npm",
            args: ["test"],
            enabled: true,
            failOnError: true,
          },
        ],
      });
    });

    await new Promise((r) => setTimeout(r, 50));

    expect(invokeMock).not.toHaveBeenCalledWith(
      "update_orchestrator_config",
      expect.anything()
    );
  });

  it("discards detection response when auto-validation is enabled if auto-validation is toggled off before resolution", async () => {
    let resolveAppA: ((val: any) => void) | null = null;

    invokeMock.mockImplementation((command, args) => {
      if (command === "get_orchestrator_config") {
        return Promise.resolve({
          ...persistedConfig,
          projectPath: "C:\\projects\\app-a",
          autoValidationEnabled: false,
          validationGates: [],
        });
      }
      if (command === "detect_project_metadata") {
        const p = (args as any)?.projectPath;
        if (p === "C:\\projects\\app-a") {
          return new Promise((resolve) => {
            resolveAppA = resolve;
          });
        }
      }
      if (command === "update_orchestrator_config") return Promise.resolve(null);
      return Promise.resolve(null);
    });

    render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    const toggle = await screen.findByRole("switch", { name: /自動検証/ });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    // Toggle auto-validation ON, which initiates detection for app-a
    fireEvent.click(toggle);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: {
          autoValidationEnabled: true,
          validationGates: [],
        },
      });
    });

    // Before app-a detection resolves, user toggles auto-validation back OFF
    fireEvent.click(toggle);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
        config: {
          autoValidationEnabled: false,
        },
      });
    });

    // Clear mock calls
    invokeMock.mockClear();

    // Now app-a resolves
    expect(resolveAppA).toBeDefined();
    act(() => {
      resolveAppA!({
        path: "C:\\projects\\app-a",
        exists: true,
        isDirectory: true,
        projectType: "App A Type",
        detectedFiles: {},
        suggestedGates: [
          {
            id: "gate-app-a",
            name: "Gate App A",
            category: "tests" as const,
            executable: "npm",
            args: ["test:a"],
            enabled: true,
            failOnError: true,
          },
        ],
      });
    });

    await new Promise((r) => setTimeout(r, 50));

    // Stale gate-app-a must NOT have been saved into config because auto-validation was turned off
    const saveCalls = invokeMock.mock.calls.filter(([cmd, args]) => {
      return cmd === "update_orchestrator_config" && JSON.stringify(args).includes("gate-app-a");
    });
    expect(saveCalls.length).toBe(0);
  });

  it("discards detection response in settings when project path changes while detection is in-flight", async () => {
    let resolvePathA: ((val: any) => void) | null = null;
    let resolvePathB: ((val: any) => void) | null = null;

    invokeMock.mockImplementation((command, args) => {
      if (command === "get_orchestrator_config") {
        return Promise.resolve({
          ...persistedConfig,
          projectPath: "C:\\projects\\path-a",
          autoValidationEnabled: true,
          validationGates: [],
        });
      }
      if (command === "detect_project_metadata") {
        const p = (args as any)?.projectPath;
        if (p === "C:\\projects\\path-a") {
          return new Promise((resolve) => {
            resolvePathA = resolve;
          });
        }
        if (p === "C:\\projects\\path-b") {
          return new Promise((resolve) => {
            resolvePathB = resolve;
          });
        }
      }
      if (command === "update_orchestrator_config") return Promise.resolve(null);
      return Promise.resolve(null);
    });

    const { unmount } = render(
      <OrchestratorSettingsPanel
        t={(key) => jaTranslations[key as keyof typeof jaTranslations] ?? String(key)}
      />
    );

    // Initial mount triggers detection for path-a
    await waitFor(() => expect(resolvePathA).toBeTypeOf("function"));

    // Path A finishes after unmount or after path changed
    invokeMock.mockClear();

    // Now resolve Path A late
    act(() => {
      resolvePathA!({
        path: "C:\\projects\\path-a",
        exists: true,
        isDirectory: true,
        projectType: "Path A Type",
        detectedFiles: {},
        suggestedGates: [
          {
            id: "gate-path-a",
            name: "Gate Path A",
            category: "tests" as const,
            executable: "npm",
            args: ["test:a"],
            enabled: true,
            failOnError: true,
          },
        ],
      });
    });

    unmount();
    await new Promise((r) => setTimeout(r, 50));

    // Stale gate-path-a must NOT be saved
    const saveCalls = invokeMock.mock.calls.filter(([cmd, args]) => {
      return cmd === "update_orchestrator_config" && JSON.stringify(args).includes("gate-path-a");
    });
    expect(saveCalls.length).toBe(0);
  });
});
