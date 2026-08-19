import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import McpSettingPanel from "./McpSettingPanel";
import type { GatewayConfig, AntigravityMcpInfo, AntigravityCommandsInfo } from "../types";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const dummyConfig: GatewayConfig = {
  active_provider: "deepseek",
  providers: {
    deepseek: {
      display_name: "DeepSeek",
      upstream_url: "https://api.deepseek.com",
      api_key_env: "DEEPSEEK_API_KEY",
      default_model: "deepseek-chat",
      force_anthropic_version: null,
      supports_count_tokens: true,
      supports_vision: false,
      supports_video: false,
      supports_thinking: true,
      model_map: {},
      visible_models: ["deepseek-chat", "deepseek-reasoner"],
    },
  },
  server: {
    host: "127.0.0.1",
    port: 4000,
    enable_cors: true,
  },
};

describe("McpSettingPanel - Antigravity Integration", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-pro",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "not_configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: null,
          registered_args: null,
          error: null,
        } as AntigravityMcpInfo;
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: {
            name: "anthro-plan",
            slash_command: "/anthro-plan",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-plan\\SKILL.md",
            error: null,
          },
          revise_command: {
            name: "anthro-revise",
            slash_command: "/anthro-revise",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-revise\\SKILL.md",
            error: null,
          },
        } as AntigravityCommandsInfo;
      }
      if (cmd === "select_executable_dialog") {
        return "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe";
      }
      if (cmd === "configure_antigravity_mcp") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        } as AntigravityMcpInfo;
      }
      return {};
    });
  });

  it("renders Not configured state with empty selection, disabled update button, and change button when expanded", async () => {
    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText(/antigravity\.header/i)).toBeInTheDocument();
    });

    expect(screen.getAllByText(/antigravity\.statusNotConfigured/i).length).toBeGreaterThan(0);

    // Update button is disabled because no executable is selected yet
    const updateBtn = screen.getByRole("button", { name: /antigravity\.btnUpdate/i });
    expect(updateBtn).toBeDisabled();
    expect(screen.getByRole("button", { name: /antigravity\.btnChangeExe/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /antigravity\.btnOpenFolder/i })).toBeInTheDocument();
  });

  it("renders Configured state with registered path, disabled update button, and Remove button when expanded", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") return {};
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "",
          plan_command: { name: "anthro-plan", status: "not_installed" },
          revise_command: { name: "anthro-revise", status: "not_installed" },
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        } as AntigravityMcpInfo;
      }
      return null;
    });

    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getAllByText(/antigravity\.statusConfigured/i).length).toBeGreaterThan(0);
    });



    // Update button is disabled because selectedExePath matches registered_command
    expect(screen.getByRole("button", { name: /antigravity\.btnUpdate/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /antigravity\.btnRemove/i })).toBeInTheDocument();
  });

  it("switches to Mismatch state and enables Update button after selecting a different exe", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-pro",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "",
          plan_command: { name: "anthro-plan", status: "not_installed" },
          revise_command: { name: "anthro-revise", status: "not_installed" },
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        } as AntigravityMcpInfo;
      }
      if (cmd === "select_executable_dialog") {
        return "C:\\Users\\User\\dev\\anthro-bridge\\gui\\src-tauri\\target\\release\\anthro-bridge.exe";
      }
      return null;
    });

    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getAllByText(/antigravity\.statusConfigured/i).length).toBeGreaterThan(0);
    });

    // Click Change button
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /antigravity\.btnChangeExe/i }));
    });

    // Status becomes mismatch (outdated key), Update button enabled
    expect(screen.getAllByText(/antigravity\.statusOutdated/i).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /antigravity\.btnUpdate/i })).toBeEnabled();
  });

  it("renders Invalid state with error message", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-pro",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "",
          plan_command: { name: "anthro-plan", status: "not_installed" },
          revise_command: { name: "anthro-revise", status: "not_installed" },
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "invalid",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: null,
          registered_args: null,
          error: "Syntax error on line 1",
        } as AntigravityMcpInfo;
      }
      return null;
    });

    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getAllByText(/antigravity\.statusInvalid/i).length).toBeGreaterThan(0);
    });
  });

  it("triggers open_antigravity_mcp_config_folder on Open Settings Folder click", async () => {
    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText(/antigravity\.header/i)).toBeInTheDocument();
    });



    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /antigravity\.btnOpenFolder/i }));
    });

    expect(invokeMock).toHaveBeenCalledWith("open_antigravity_mcp_config_folder");
  });

  it("triggers configure_antigravity_mcp with chosen exePath on update click", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-pro",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "",
          plan_command: { name: "anthro-plan", status: "not_installed" },
          revise_command: { name: "anthro-revise", status: "not_installed" },
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "not_configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: null,
          registered_args: null,
          error: null,
        } as AntigravityMcpInfo;
      }
      if (cmd === "select_executable_dialog") {
        return "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe";
      }
      if (cmd === "configure_antigravity_mcp") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        } as AntigravityMcpInfo;
      }
      return null;
    });

    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getAllByText(/antigravity\.statusNotConfigured/i).length).toBeGreaterThan(0);
    });

    // Select exe
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /antigravity\.btnChangeExe/i }));
    });

    // Update button enabled -> click
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /antigravity\.btnUpdate/i }));
    });

    expect(invokeMock).toHaveBeenCalledWith("configure_antigravity_mcp", {
      exePath: "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
    });

    await waitFor(() => {
      expect(screen.getAllByText(/antigravity\.statusConfigured/i).length).toBeGreaterThan(0);
    });
  });

  // ── Antigravity Commands Tests ──
  it("renders Commands NotInstalled state with Install and Open Folder buttons", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-pro",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        } as AntigravityMcpInfo;
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: {
            name: "anthro-plan",
            slash_command: "/anthro-plan",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-plan\\SKILL.md",
            error: null,
          },
          revise_command: {
            name: "anthro-revise",
            slash_command: "/anthro-revise",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-revise\\SKILL.md",
            error: null,
          },
        };
      }
      return null;
    });

    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText(/antigravity\.header/i)).toBeInTheDocument();
    });



    expect(screen.getAllByText(/antigravity\.commandStatusNotInstalled/i).length).toBe(2);
    expect(screen.getAllByRole("button", { name: /antigravity\.commandBtnInstall/i }).length).toBe(2);
    expect(screen.getByRole("button", { name: /antigravity\.btnInstallAll/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /antigravity\.btnOpenSkillsFolder/i })).toBeInTheDocument();
  });

  it("renders Commands Installed state with Remove buttons and no InstallAll button", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-pro",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        } as AntigravityMcpInfo;
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: {
            name: "anthro-plan",
            slash_command: "/anthro-plan",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-plan\\SKILL.md",
            error: null,
          },
          revise_command: {
            name: "anthro-revise",
            slash_command: "/anthro-revise",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-revise\\SKILL.md",
            error: null,
          },
        };
      }
      return null;
    });

    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText(/antigravity\.header/i)).toBeInTheDocument();
    });



    expect(screen.getAllByText(/antigravity\.commandStatusInstalled/i).length).toBe(2);
    expect(screen.getByRole("button", { name: /antigravity\.btnRemove/i })).toBeInTheDocument(); // MCP Remove button
    expect(screen.getAllByRole("button", { name: /antigravity\.commandBtnRemove/i }).length).toBe(2); // Commands Remove buttons
    expect(screen.queryByRole("button", { name: /antigravity\.btnInstallAll/i })).not.toBeInTheDocument();
  });

  it("triggers install_all_antigravity_commands on Install All click", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-pro",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\Users\\User\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        } as AntigravityMcpInfo;
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: {
            name: "anthro-plan",
            slash_command: "/anthro-plan",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-plan\\SKILL.md",
            error: null,
          },
          revise_command: {
            name: "anthro-revise",
            slash_command: "/anthro-revise",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-revise\\SKILL.md",
            error: null,
          },
        };
      }
      if (cmd === "install_all_antigravity_commands") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: {
            name: "anthro-plan",
            slash_command: "/anthro-plan",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-plan\\SKILL.md",
            error: null,
          },
          revise_command: {
            name: "anthro-revise",
            slash_command: "/anthro-revise",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-revise\\SKILL.md",
            error: null,
          },
        };
      }
      return null;
    });

    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText(/antigravity\.header/i)).toBeInTheDocument();
    });



    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /antigravity\.btnInstallAll/i }));
    });

    expect(invokeMock).toHaveBeenCalledWith("install_all_antigravity_commands");
    await waitFor(() => {
      expect(screen.getAllByText(/antigravity\.commandStatusInstalled/i).length).toBe(2);
    });
  });

  it("shows MCP warning when MCP is not configured", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-pro",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "not_configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: null,
          registered_args: null,
          error: null,
        } as AntigravityMcpInfo;
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: {
            name: "anthro-plan",
            slash_command: "/anthro-plan",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-plan\\SKILL.md",
            error: null,
          },
          revise_command: {
            name: "anthro-revise",
            slash_command: "/anthro-revise",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-revise\\SKILL.md",
            error: null,
          },
        };
      }
      return null;
    });

    render(<McpSettingPanel config={dummyConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText(/antigravity\.header/i)).toBeInTheDocument();
    });



    expect(screen.getByText(/antigravity\.commandsMcpWarning/i)).toBeInTheDocument();
  });
});
