import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
    await screen.findByText("orchestrator.settings.defaults");
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
    await screen.findByText("orchestrator.settings.defaults");
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
    await screen.findByText("orchestrator.settings.defaults");
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
    expect(container.querySelectorAll("details[data-provider]")).toHaveLength(8);
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

  it("renders the delete button in the profile header row with actions-only modifier for single profiles", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return { ...persistedConfig, assignments: {} };
      return null;
    });
    const { container, unmount } = render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await expandProvider("deepseek", "DeepSeek");
    const singleCard = container.querySelector('[data-profile-id="custom-reviewer"]') as HTMLElement;
    const singleHeader = singleCard.querySelector(".orchestrator-profile-header") as HTMLElement;
    expect(singleHeader).toHaveClass("actions-only");
    expect(singleCard.querySelector(".orchestrator-profile-name")).not.toBeInTheDocument();
    expect(within(singleHeader).getByRole("button", { name: "orchestrator.settings.deleteProfile" })).toBeInTheDocument();
    expect(singleCard.querySelector(".orchestrator-profile-actions")).not.toBeInTheDocument();

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
    expect(multiHeader).not.toHaveClass("actions-only");
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
    await screen.findByText("orchestrator.settings.defaults");
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
    await screen.findByText("orchestrator.settings.defaults");
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
});
