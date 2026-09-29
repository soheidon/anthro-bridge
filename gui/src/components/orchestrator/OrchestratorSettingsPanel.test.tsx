import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import OrchestratorSettingsPanel from "./OrchestratorSettingsPanel";
import { DEFAULT_ITERATION_LIMITS, DEFAULT_VALIDATION_GATES } from "../../config/orchestratorPresets";
import type { OrchestratorConfig } from "../../types/orchestrator";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
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
    expect(within(group).getByText("DeepSeek V4.1 Flash (Direct API)")).toBeInTheDocument();
    expect(screen.getByDisplayValue("My reviewer")).toBeInTheDocument();
    expect(screen.queryByLabelText("orchestrator.settings.displayName")).not.toBeInTheDocument();

    fireEvent.change(within(group).getByLabelText("orchestrator.settings.model"), { target: { value: "deepseek-flash" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      { config: { profiles: [{ ...persistedConfig.profiles![0], model: "deepseek-flash", displayName: "DeepSeek V4.1 Flash (Direct API)", reasoningEffort: "high" }] } },
    ));
    expect(container.querySelector('[data-profile-id="custom-reviewer"]')).toHaveTextContent("DeepSeek V4.1 Flash (Direct API)");
    expect(screen.getByDisplayValue("My reviewer")).toBeInTheDocument();
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

    fireEvent.change(screen.getByRole("combobox", { name: "Ollama Old Model (Local) apiKeyPanel.ollamaLocal.modelTag" }), {
      target: { value: "gemma4:26b" },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { profiles: [{ ...ollamaProfile, ollamaModel: "gemma4:26b", displayName: "Ollama Gemma4 26B (Local)" }] },
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
    expect(screen.getByRole("combobox", { name: "Ollama Saved Model Latest (Local) apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model:latest");
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
    expect(screen.getByRole("combobox", { name: "Ollama Saved Model Latest (Local) apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model:latest");
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
    await waitFor(() => expect(within(profileCard).getByRole("combobox", { name: "Ollama Saved Model (Local) apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model"));
    expect(within(profileCard).queryByRole("option", { name: "old-endpoint-model" })).not.toBeInTheDocument();
  });

  it("edits quick-slot visibility and iteration defaults through partial updates", async () => {
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByDisplayValue("My reviewer");
    await expandProvider("deepseek", "DeepSeek");
    await expandProvider("deepseek", "DeepSeek");

    const profileCard = document.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    fireEvent.click(within(profileCard).getByRole("checkbox", { name: "orchestrator.settings.visible" }));
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
    await screen.findByDisplayValue("My Custom Slot");
    const profileCard = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const profileVisibility = within(profileCard).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    expect(profileVisibility).toBeChecked();

    const slotRow = container.querySelector('[data-slot-id="user-slot"]') as HTMLElement;
    expect(profileCard).toContainElement(slotRow);
    expect(within(slotRow).queryByRole("checkbox")).not.toBeInTheDocument();
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
    expect(within(slotRow).getByDisplayValue("My Custom Slot")).toBeInTheDocument();
    expect(within(slotRow).getByRole("button", { name: "Move My Custom Slot down" })).toBeInTheDocument();
  });

  it("creates a slot from the Profile checkbox, then recreates it with a new ID after deletion", async () => {
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
    await screen.findByDisplayValue("Existing Custom Name");
    await expandProvider("deepseek", "DeepSeek");
    const slotFreeCard = container.querySelector('[data-profile-id="slot-free-profile"]') as HTMLElement;
    const visibility = within(slotFreeCard).getByRole("checkbox", { name: "orchestrator.settings.visible" });
    expect(visibility).not.toBeChecked();
    fireEvent.click(visibility);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [existingSlot, expect.objectContaining({
        profileId: "slot-free-profile",
        label: "DeepSeek V4.1 Flash (Direct API)",
        visible: true,
        order: 5,
      })] },
    }));
    const createSaves = invokeMock.mock.calls.filter(([command]) => command === "update_orchestrator_config");
    const createdSlot = (createSaves[createSaves.length - 1][1] as { config: { quickSlots: Array<{ id: string; profileId: string }> } }).config.quickSlots[1];
    expect(createdSlot.id).toMatch(/^slot-/);
    expect(visibility).toBeChecked();

    const removeCreatedSlot = screen.getByRole("button", { name: "Remove DeepSeek V4.1 Flash (Direct API)" });
    await waitFor(() => expect(removeCreatedSlot).not.toBeDisabled());
    fireEvent.click(removeCreatedSlot);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [existingSlot] },
    }));
    await waitFor(() => expect(visibility).not.toBeDisabled());
    expect(visibility).not.toBeChecked();

    fireEvent.click(visibility);
    await waitFor(() => {
      const saves = invokeMock.mock.calls.filter(([command]) => command === "update_orchestrator_config");
      const saved = (saves[saves.length - 1][1] as { config: { quickSlots: Array<{ id: string; profileId: string; order: number }> } }).config.quickSlots;
      expect(saved).toHaveLength(2);
      expect(saved[1]).toMatchObject({ profileId: "slot-free-profile", order: 5 });
      expect(saved[1].id).not.toBe(createdSlot.id);
    });
    expect(visibility).toBeChecked();
  });

  it("keeps hidden Quick Slots manageable without a second visibility checkbox", async () => {
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
    await screen.findByDisplayValue("Hidden Slot");
    const hiddenRow = container.querySelector('[data-slot-id="hidden-slot"]') as HTMLElement;
    expect(hiddenRow).toBeInTheDocument();
    expect(within(hiddenRow).queryByRole("checkbox")).not.toBeInTheDocument();

    fireEvent.change(within(hiddenRow).getByDisplayValue("Hidden Slot"), { target: { value: "Renamed Hidden Slot" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [persistedConfig.quickSlots![0], { ...hiddenSlot, label: "Renamed Hidden Slot" }] },
    }));

    fireEvent.click(screen.getByRole("button", { name: "Move Renamed Hidden Slot up" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        quickSlots: [
          { ...persistedConfig.quickSlots![0], order: 1 },
          { ...hiddenSlot, label: "Renamed Hidden Slot", order: 0 },
        ],
      },
    }));
    expect(hiddenRow).toBeInTheDocument();
    expect(within(hiddenRow).getByDisplayValue("Renamed Hidden Slot")).toBeInTheDocument();
  });

  it("keeps a Quick Slot with a missing profile association visible without exposing reassignment", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [] };
      return null;
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.defaults");
    expect(container.querySelector('[data-slot-id="user-slot"]')).not.toBeInTheDocument();
    expect([...container.querySelectorAll(".orchestrator-settings-section > h3")]
      .some((heading) => heading.textContent === "orchestrator.settings.quickSlots")).toBe(false);

    fireEvent.change(screen.getByLabelText("orchestrator.settings.maxFixIterations"), { target: { value: "4" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { iterationLimits: { ...DEFAULT_ITERATION_LIMITS, maxFixIterations: 4 } },
    }));
  });

  it("uses conceptual labels and read-only commands for existing gates only", async () => {
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);

    expect(await screen.findByText("orchestrator.settings.gateTypeCheck")).toBeInTheDocument();
    expect(screen.getByText("orchestrator.settings.gateTests")).toBeInTheDocument();
    expect(screen.getByText("orchestrator.settings.gateRepositoryState")).toBeInTheDocument();
    expect(screen.getByText("npx tsc --noEmit")).toBeInTheDocument();
    expect(screen.getByText("npm test -- --run")).toBeInTheDocument();
    expect(screen.getByText("git status --short")).toBeInTheDocument();
    expect(screen.queryByText("Build / Compile")).not.toBeInTheDocument();
    expect(screen.queryByText("Format Check")).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: /configured command/i })).not.toBeInTheDocument();
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

  it("keeps Advanced / Diagnostics collapsed until expanded", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") {
        return {
          ...persistedConfig,
          authorizedCustomGates: [{
            gateId: "custom-gate",
            executable: "custom-runner",
            args: ["--safe"],
            canonicalWorkingDir: "C:/project",
            commandHash: "sha256:approved",
          }],
        };
      }
      return null;
    });
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByText("orchestrator.settings.gateTypeCheck");
    const details = container.querySelector("details.orchestrator-settings-diagnostics");

    expect(details).not.toBeNull();
    expect(details).not.toHaveAttribute("open");
    expect(within(details as HTMLElement).getByText("orchestrator.settings.redactionActive")).not.toBeVisible();
    expect(within(details as HTMLElement).getByText("custom-gate: sha256:approved")).not.toBeVisible();
    fireEvent.click(screen.getByText("orchestrator.settings.advanced"));
    expect(details).toHaveAttribute("open");
    expect(within(details as HTMLElement).getByText("orchestrator.settings.processIsolationActive")).toBeVisible();
    expect(within(details as HTMLElement).getByText("custom-gate: sha256:approved")).toBeVisible();
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
    expect(container.querySelectorAll("details[data-provider]")).toHaveLength(8);
    expect(screen.queryByRole("button", { name: /addProfile.*Other/i })).not.toBeInTheDocument();
    expect(container.querySelector('[data-profile-id="future-provider-profile"]')).not.toBeInTheDocument();

    const group = await expandProvider("deepseek", "DeepSeek");
    fireEvent.change(within(group).getByLabelText("orchestrator.settings.contextWindow"), { target: { value: "64000" } });

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        profiles: [
          { ...persistedConfig.profiles![0], contextWindowTokens: 64000, displayName: "DeepSeek V4.1 Flash (Direct API)" },
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
    fireEvent.click(within(unusedCard).getByRole("button", { name: "orchestrator.settings.deleteProfile" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        profiles: [persistedConfig.profiles![0]],
        quickSlots: [persistedConfig.quickSlots![0]],
      },
    }));
  });

  it("updates capability-dependent thinking and effort settings while preserving the profile identity", async () => {
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const card = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const modelSelect = within(card).getByRole("combobox", { name: "orchestrator.settings.model" });
    expect(within(card).getByRole("combobox", { name: "orchestrator.settings.reasoningEffort" })).toHaveValue("low");
    fireEvent.change(modelSelect, { target: { value: "deepseek-v4-pro" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { profiles: [{ ...persistedConfig.profiles![0], model: "deepseek-v4-pro", reasoningEffort: "high", displayName: "DeepSeek V4 Pro 0813 (Direct API)" }] },
    }));

    fireEvent.change(within(card).getByRole("combobox", { name: "orchestrator.settings.thinkingMode" }), { target: { value: "normal" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { profiles: [{ ...persistedConfig.profiles![0], model: "deepseek-v4-pro", thinkingMode: "normal", reasoningEffort: undefined, displayName: "DeepSeek V4 Pro 0813 (Direct API)" }] },
    }));
  });

  it("updates the derived Profile name when its adapter changes without renaming its Quick Slot", async () => {
    const { container } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const card = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;

    fireEvent.change(within(card).getByRole("combobox", { name: "orchestrator.settings.adapter" }), {
      target: { value: "cli" },
    });

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: {
        profiles: [{ ...persistedConfig.profiles![0], adapter: "cli", displayName: "Codex CLI (Local Agent)" }],
      },
    }));
    await expandProvider("cli", "Codex CLI");
    expect(container.querySelector('[data-profile-id="custom-reviewer"]')).toHaveTextContent("Codex CLI (Local Agent)");
    expect(screen.getByDisplayValue("My reviewer")).toBeInTheDocument();
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
    await screen.findByDisplayValue("My reviewer");
    await expandProvider("deepseek", "DeepSeek");

    const secondProfileCard = container.querySelector('[data-profile-id="unused-profile"]') as HTMLElement;
    const secondSlotRow = within(secondProfileCard).getByDisplayValue("Second").closest("[data-slot-id]") as HTMLElement;
    expect(secondSlotRow).toBeInTheDocument();
    expect(within(secondSlotRow).queryByRole("combobox")).not.toBeInTheDocument();
    expect([...container.querySelectorAll(".orchestrator-settings-section > h3")]
      .some((heading) => heading.textContent === "orchestrator.settings.quickSlots")).toBe(false);
    expect(screen.queryByRole("button", { name: "orchestrator.settings.addSlot" })).not.toBeInTheDocument();

    await waitFor(() => expect(screen.getByRole("button", { name: "Move Second up" })).not.toBeDisabled());
    fireEvent.click(screen.getByRole("button", { name: "Move Second up" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [
        { ...persistedConfig.quickSlots![0], order: 1 },
        { ...secondSlot, order: 0 },
      ] },
    }));
    expect(within(secondSlotRow).getByDisplayValue("Second")).toBeInTheDocument();
    expect(invokeMock.mock.calls.filter(([command]) => command === "update_orchestrator_config")).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "Remove Second" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [{ ...persistedConfig.quickSlots![0], order: 1 }] },
    }));
    expect(container.querySelector('[data-slot-id="second-slot"]')).not.toBeInTheDocument();
    expect(within(container).getByRole("heading", { name: "orchestrator.settings.defaults" })).toBeInTheDocument();
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
    expect(await screen.findByDisplayValue("Slot B")).toBeInTheDocument();
    const slotBRow = container.querySelector('[data-slot-id="slot-b"]') as HTMLElement;
    expect(within(slotBRow).queryByRole("combobox")).not.toBeInTheDocument();

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
});
