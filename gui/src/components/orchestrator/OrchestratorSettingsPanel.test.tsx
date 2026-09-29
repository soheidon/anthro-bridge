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

  it("loads persisted profiles/slots and saves only the edited profile field", async () => {
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    const group = await expandProvider("deepseek", "DeepSeek");
    expect(group.open).toBe(true);
    const displayNameInput = screen.getByLabelText("orchestrator.settings.displayName");
    expect(displayNameInput).toHaveValue("Persisted Reviewer");

    fireEvent.change(displayNameInput, { target: { value: "Renamed Reviewer" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "update_orchestrator_config",
      { config: { profiles: [{ ...persistedConfig.profiles![0], displayName: "Renamed Reviewer" }] } },
    ));
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

    fireEvent.change(screen.getByRole("combobox", { name: "Local Ollama apiKeyPanel.ollamaLocal.modelTag" }), {
      target: { value: "gemma4:26b" },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { profiles: [{ ...ollamaProfile, ollamaModel: "gemma4:26b" }] },
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
    expect(screen.getByRole("combobox", { name: "Local Ollama apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model:latest");
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
    expect(screen.getByRole("combobox", { name: "Local Ollama apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model:latest");
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
    await waitFor(() => expect(within(profileCard).getByRole("combobox", { name: "Ollama A apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model"));
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

  it("shows missing saved quick-slot targets without substituting another profile", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, profiles: [] };
      return null;
    });
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    const slotProfile = await screen.findByRole("combobox", { name: "user-slot orchestrator.settings.slotProfile" });
    expect(slotProfile).toHaveValue("custom-reviewer");
    expect(within(slotProfile).getByRole("option", { name: "orchestrator.settings.unavailableProfile (custom-reviewer)" })).toBeInTheDocument();
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
      config: { profiles: [{ ...persistedConfig.profiles![0], model: "deepseek-v4-pro", reasoningEffort: "high" }] },
    }));

    fireEvent.change(within(card).getByRole("combobox", { name: "orchestrator.settings.thinkingMode" }), { target: { value: "normal" } });
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { profiles: [{ ...persistedConfig.profiles![0], model: "deepseek-v4-pro", thinkingMode: "normal", reasoningEffort: undefined }] },
    }));
  });

  it("adds, reorders, and removes Quick Slots without rewriting assignments or profiles", async () => {
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

    fireEvent.click(screen.getByRole("button", { name: "Move Second up" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [
        { ...persistedConfig.quickSlots![0], order: 1 },
        { ...secondSlot, order: 0 },
      ] },
    }));
    const unusedSlotProfile = within(container.querySelector('[data-slot-id="second-slot"]') as HTMLElement)
      .getByRole("combobox", { name: "second-slot orchestrator.settings.slotProfile" });
    expect(within(unusedSlotProfile).queryByRole("option", { name: "Persisted Reviewer" })).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.filter(([command]) => command === "update_orchestrator_config")).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "Remove Second" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", {
      config: { quickSlots: [persistedConfig.quickSlots![0]] },
    }));

    fireEvent.click(screen.getByRole("button", { name: "orchestrator.settings.addSlot" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("update_orchestrator_config", expect.objectContaining({
      config: expect.objectContaining({ quickSlots: expect.arrayContaining([expect.objectContaining({ label: "orchestrator.settings.newSlot", profileId: "unused-profile" })]) }),
    })));
    const slotSaves = () => invokeMock.mock.calls.filter(([command]) => command === "update_orchestrator_config");
    const slotSave = () => slotSaves()[slotSaves().length - 1];
    const savedQuickSlots = () => ((slotSave()?.[1] as { config: { quickSlots: Array<{ profileId: string }> } }).config.quickSlots);
    expect(savedQuickSlots().map((slot) => slot.profileId)).toEqual(["custom-reviewer", "unused-profile"]);
    expect(Object.keys((slotSave()?.[1] as { config: object }).config)).toEqual(["quickSlots"]);
    const callCountWhenFull = slotSaves().length;
    const addSlotButton = screen.getByRole("button", { name: "orchestrator.settings.addSlot" });
    expect(addSlotButton).toBeDisabled();
    fireEvent.click(addSlotButton);
    expect(slotSaves()).toHaveLength(callCountWhenFull);

    fireEvent.click(screen.getByRole("button", { name: "Remove orchestrator.settings.newSlot" }));
    await waitFor(() => expect(slotSaves()).toHaveLength(callCountWhenFull + 1));
    fireEvent.click(addSlotButton);
    await waitFor(() => expect(slotSaves()).toHaveLength(callCountWhenFull + 2));
    expect(savedQuickSlots().map((slot) => slot.profileId)).toEqual(["custom-reviewer", "unused-profile"]);
    expect(within(container).getByRole("heading", { name: "orchestrator.settings.defaults" })).toBeInTheDocument();
  });

  it("rejects a duplicate profileId at the Quick Slot mutation boundary", async () => {
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
    const view = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    const slotB = await screen.findByRole("combobox", { name: "slot-b orchestrator.settings.slotProfile" }) as HTMLSelectElement;
    expect(within(slotB).queryByRole("option", { name: "Persisted Reviewer" })).not.toBeInTheDocument();

    // Inject an option only in the test DOM to bypass the UI's candidate filtering
    // and exercise the production onChange -> updateSlot guard directly.
    const bypassOption = document.createElement("option");
    bypassOption.value = "custom-reviewer";
    bypassOption.textContent = "Persisted Reviewer";
    slotB.append(bypassOption);
    fireEvent.change(slotB, { target: { value: "custom-reviewer" } });

    expect(invokeMock).not.toHaveBeenCalledWith("update_orchestrator_config", expect.anything());
    slotB.removeChild(bypassOption);
    view.rerender(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    expect(screen.getByRole("combobox", { name: "user-slot orchestrator.settings.slotProfile" })).toHaveValue("custom-reviewer");
    expect(screen.getByRole("combobox", { name: "slot-b orchestrator.settings.slotProfile" })).toHaveValue("profile-b");
    expect(invokeMock).not.toHaveBeenCalledWith("update_orchestrator_config", expect.anything());
  });
});
