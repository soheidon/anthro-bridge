import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import OrchestratorSettingsPanel from "./OrchestratorSettingsPanel";
import { DEFAULT_ITERATION_LIMITS, DEFAULT_VALIDATION_GATES } from "../../config/orchestratorPresets";
import type { OrchestratorConfig } from "../../types/orchestrator";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);
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
    invokeMock.mockImplementation(async (command) => {
      if (command === "get_orchestrator_config") return persistedConfig;
      return null;
    });
  });

  it("loads persisted profiles/slots and saves only the edited profile field", async () => {
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);

    const profileSelect = await screen.findByRole("combobox", { name: "orchestrator.settings.selectProfile" });
    expect(profileSelect).toHaveValue("custom-reviewer");
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

    fireEvent.click(await screen.findByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_ollama_models_for_settings", {
      endpoint: "http://127.0.0.1:11434",
    }));
    expect(await screen.findByRole("status")).toHaveTextContent("2 apiKeyPanel.ollamaLocal.modelsFound");

    fireEvent.change(screen.getByRole("combobox", { name: "apiKeyPanel.ollamaLocal.modelTag" }), {
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
    fireEvent.click(await screen.findByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
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
    fireEvent.click(await screen.findByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    expect(await screen.findByRole("status")).toHaveTextContent(messageKey);
    expect(screen.getByRole("combobox", { name: "apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model:latest");
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
    fireEvent.click(await screen.findByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    expect(await screen.findByRole("status")).toHaveTextContent("apiKeyPanel.ollamaLocal.noModelsFound");
    expect(screen.getByRole("combobox", { name: "apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model:latest");
  });

  it("discards a refresh response after switching to a different profile", async () => {
    let resolveRefresh!: (value: unknown) => void;
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
      if (command === "list_ollama_models_for_settings") return new Promise((resolve) => { resolveRefresh = resolve; });
      return Promise.resolve(null);
    });
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    fireEvent.click(await screen.findByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    fireEvent.change(screen.getByRole("combobox", { name: "orchestrator.settings.selectProfile" }), { target: { value: "ollama-b" } });
    resolveRefresh({ status: "success", models: ["model-from-a"] });
    await waitFor(() => expect(screen.getByRole("combobox", { name: "apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("model-b"));
    expect(screen.queryByRole("option", { name: "model-from-a" })).not.toBeInTheDocument();
  });

  it("does not let an older refresh overwrite a newer profile refresh", async () => {
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
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    fireEvent.click(await screen.findByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    fireEvent.change(screen.getByRole("combobox", { name: "orchestrator.settings.selectProfile" }), { target: { value: "ollama-b" } });
    fireEvent.click(screen.getByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    expect(pending).toHaveLength(2);

    pending[1]({ status: "success", models: ["model-from-b"] });
    await waitFor(() => expect(screen.getByRole("combobox", { name: "apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("model-b"));
    expect(screen.getByRole("option", { name: "model-from-b" })).toBeInTheDocument();
    pending[0]({ status: "success", models: ["model-from-a"] });
    await waitFor(() => expect(screen.getByRole("option", { name: "model-from-b" })).toBeInTheDocument());
    expect(screen.queryByRole("option", { name: "model-from-a" })).not.toBeInTheDocument();
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
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    fireEvent.click(await screen.findByRole("button", { name: "apiKeyPanel.ollamaLocal.refresh" }));
    fireEvent.change(screen.getByDisplayValue("http://127.0.0.1:11434"), { target: { value: "http://127.0.0.1:11435" } });
    resolveRefresh({ status: "success", models: ["old-endpoint-model"] });
    await waitFor(() => expect(screen.getByRole("combobox", { name: "apiKeyPanel.ollamaLocal.modelTag" })).toHaveValue("saved-model"));
    expect(screen.queryByRole("option", { name: "old-endpoint-model" })).not.toBeInTheDocument();
  });

  it("edits quick-slot visibility and iteration defaults through partial updates", async () => {
    render(<OrchestratorSettingsPanel t={(key) => String(key)} />);
    await screen.findByDisplayValue("My reviewer");

    fireEvent.click(screen.getByRole("checkbox", { name: "orchestrator.settings.visible" }));
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
});
