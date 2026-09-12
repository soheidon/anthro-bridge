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
          review_command: {
            name: "anthro-review",
            slash_command: "/anthro-review",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-review\\SKILL.md",
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
          review_command: { name: "anthro-review", status: "not_installed" },
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
  it("renders Commands Not Installed state with Install buttons and InstallAll button", async () => {
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
          review_command: {
            name: "anthro-review",
            slash_command: "/anthro-review",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-review\\SKILL.md",
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



    expect(screen.getAllByText(/antigravity\.commandStatusNotInstalled/i).length).toBe(3);
    expect(screen.getAllByRole("button", { name: /antigravity\.commandBtnInstall/i }).length).toBe(3);
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
          review_command: {
            name: "anthro-review",
            slash_command: "/anthro-review",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-review\\SKILL.md",
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



    expect(screen.getAllByText(/antigravity\.commandStatusInstalled/i).length).toBe(3);
    expect(screen.getByRole("button", { name: /antigravity\.btnRemove/i })).toBeInTheDocument(); // MCP Remove button
    expect(screen.getAllByRole("button", { name: /antigravity\.commandBtnRemove/i }).length).toBe(3); // Commands Remove buttons
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
          review_command: {
            name: "anthro-review",
            slash_command: "/anthro-review",
            status: "not_installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-review\\SKILL.md",
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
          review_command: {
            name: "anthro-review",
            slash_command: "/anthro-review",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-review\\SKILL.md",
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
      expect(screen.getAllByText(/antigravity\.commandStatusInstalled/i).length).toBe(3);
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
          review_command: {
            name: "anthro-review",
            slash_command: "/anthro-review",
            status: "installed",
            skill_path: "C:\\Users\\User\\.gemini\\config\\skills\\anthro-review\\SKILL.md",
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

describe("McpSettingPanel - Thinking-only and Capability-driven MCP UI", () => {
  const kimiCodeConfig: GatewayConfig = {
    active_provider: "kimi-code",
    providers: {
      "kimi-code": {
        display_name: "Kimi Code",
        upstream_url: "https://api.kimi.com/coding",
        api_key_env: "KIMI_CODE_API_KEY",
        default_model: "kimi-for-coding",
        force_anthropic_version: null,
        supports_count_tokens: false,
        supports_vision: true,
        supports_video: true,
        supports_thinking: true,
        model_map: {},
        visible_models: ["kimi-for-coding", "kimi-for-coding-highspeed"],
        models: {
          "claude-opus-5": {
            upstream_model: "kimi-for-coding",
            thinking_mode: "thinking_only",
          },
        },
      },
      deepseek: {
        display_name: "DeepSeek",
        upstream_url: "https://api.deepseek.com",
        api_key_env: "DEEPSEEK_API_KEY",
        default_model: "deepseek-v4-pro",
        force_anthropic_version: null,
        supports_count_tokens: true,
        supports_vision: false,
        supports_video: false,
        supports_thinking: true,
        model_map: {},
        visible_models: ["deepseek-v4-pro", "deepseek-v4-flash"],
        models: {
          "claude-opus-5": {
            upstream_model: "deepseek-v4-pro",
            thinking_mode: "thinking",
            reasoning_effort: "high",
          },
        },
      },
    },
    server: {
      host: "127.0.0.1",
      port: 4000,
      enable_cors: true,
    },
  };

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "kimi-code",
          model: "kimi-for-coding",
          thinking_mode: "thinking_only",
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\path\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        };
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: { name: "anthro-plan", slash_command: "/anthro-plan", status: "installed" },
          revise_command: { name: "anthro-revise", slash_command: "/anthro-revise", status: "installed" },
          review_command: { name: "anthro-review", slash_command: "/anthro-review", status: "installed" },
        };
      }
      return {};
    });
  });

  it("renders Kimi Code with thinking_only summary and no dropdowns for thinking mode or reasoning effort", async () => {
    render(<McpSettingPanel config={kimiCodeConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Kimi Code")).toBeInTheDocument();
    });

    // Check summary text does NOT say Normal
    expect(screen.getByText("kimi-for-coding (Thinking only)")).toBeInTheDocument();
    expect(screen.queryByText("kimi-for-coding (Normal)")).not.toBeInTheDocument();

    // Expand Kimi Code row
    await act(async () => {
      screen.getByText("Kimi Code").click();
    });

    // Italic "Thinking only" text should be present
    expect(screen.getAllByText("Thinking only").length).toBeGreaterThan(0);

    // Thinking mode select and effort select should NOT be rendered for thinking_only
    const selects = screen.getAllByRole("combobox");
    expect(selects).toHaveLength(1);
    expect(selects[0]).toHaveValue("kimi-for-coding");
  });

  it("regression: renders DeepSeek with toggleable thinking mode and reasoning effort", async () => {
    render(<McpSettingPanel config={kimiCodeConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
    });

    // Expand DeepSeek row
    await act(async () => {
      screen.getByText("DeepSeek").click();
    });

    // DeepSeek should have Model, Thinking Mode, and Reasoning Effort dropdowns
    const selects = screen.getAllByRole("combobox");
    expect(selects.length).toBeGreaterThanOrEqual(3);
  });
});

describe("McpSettingPanel - Direct DeepSeek Model Selection & Legacy Compatibility", () => {
  const directDeepSeekConfig: GatewayConfig = {
    active_provider: "deepseek",
    providers: {
      deepseek: {
        display_name: "DeepSeek",
        upstream_url: "https://api.deepseek.com",
        api_key_env: "DEEPSEEK_API_KEY",
        default_model: "deepseek-flash",
        force_anthropic_version: null,
        supports_count_tokens: true,
        supports_vision: false,
        supports_video: false,
        supports_thinking: true,
        model_map: {},
        visible_models: ["claude-opus-5", "claude-sonnet-5"],
        models: {
          "claude-opus-5": {
            upstream_model: "deepseek-flash",
            thinking_mode: "thinking",
            reasoning_effort: "max",
          },
          "claude-sonnet-4-6": {
            upstream_model: "deepseek-v4-pro",
            thinking_mode: "normal",
          },
          "claude-sonnet-4-5": {
            upstream_model: "deepseek-v4-flash",
            thinking_mode: "thinking",
          },
        },
      },
    },
    server: {
      host: "127.0.0.1",
      port: 4000,
      enable_cors: true,
    },
  };

  it("renders only standard Direct DeepSeek models with display names in normal dropdown", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-flash",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\path\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        };
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: { name: "anthro-plan", slash_command: "/anthro-plan", status: "installed" },
          revise_command: { name: "anthro-revise", slash_command: "/anthro-revise", status: "installed" },
          review_command: { name: "anthro-review", slash_command: "/anthro-review", status: "installed" },
        };
      }
      return {};
    });

    render(<McpSettingPanel config={directDeepSeekConfig} refreshConfig={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
    });

    // Expand DeepSeek row
    await act(async () => {
      screen.getByText("DeepSeek").click();
    });

    // Find the model select dropdown
    const selects = screen.getAllByRole("combobox");
    const modelSelect = selects[0];
    const options = Array.from(modelSelect.querySelectorAll("option")).map((o) => ({
      value: o.value,
      label: o.textContent,
    }));

    // Must only have standard models with display names
    expect(options).toEqual([
      { value: "deepseek-flash", label: "DeepSeek V4.1 Flash" },
      { value: "deepseek-v4-pro", label: "DeepSeek V4 Pro 0813" },
    ]);

    // Legacy models must NOT be present as normal options
    expect(options.some((o) => o.value === "deepseek-v4-flash")).toBe(false);
    expect(options.some((o) => o.value === "deepseek-v4-flash-vision-exp")).toBe(false);

    // Effort selector for deepseek-flash must have Low, High, Max (no Medium)
    const effortSelect = selects[2];
    const effortOptions = Array.from(effortSelect.querySelectorAll("option")).map((o) => o.textContent);
    expect(effortOptions).toEqual(["Low", "High", "Max"]);
    expect(effortOptions).not.toContain("Medium");
  });

  it("preserves saved legacy model as a selected-only option without breaking", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-v4-flash",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return {
          status: "configured",
          config_path: "C:\\Users\\User\\.gemini\\config\\mcp_config.json",
          config_dir: "C:\\Users\\User\\.gemini\\config",
          registered_command: "C:\\path\\anthro-bridge.exe",
          registered_args: ["--mcp-server"],
          error: null,
        };
      }
      if (cmd === "get_antigravity_commands_status") {
        return {
          skills_dir: "C:\\Users\\User\\.gemini\\config\\skills",
          plan_command: { name: "anthro-plan", slash_command: "/anthro-plan", status: "installed" },
          revise_command: { name: "anthro-revise", slash_command: "/anthro-revise", status: "installed" },
          review_command: { name: "anthro-review", slash_command: "/anthro-review", status: "installed" },
        };
      }
      return {};
    });

    render(<McpSettingPanel config={directDeepSeekConfig} refreshConfig={vi.fn()} />);

    // Header summary reflects loaded legacy model from get_mcp_config
    await waitFor(() => {
      expect(screen.getByText(/deepseek-v4-flash/i)).toBeInTheDocument();
    });

    // Expand DeepSeek row
    await act(async () => {
      screen.getByText("DeepSeek").click();
    });

    const selects = screen.getAllByRole("combobox");
    const modelSelect = selects[0];
    const options = Array.from(modelSelect.querySelectorAll("option")).map((o) => ({
      value: o.value,
      label: o.textContent,
    }));

    // Must contain the legacy saved model marked as legacy
    expect(options).toContainEqual({
      value: "deepseek-v4-flash",
      label: "deepseek-v4-flash (Legacy / saved)",
    });
    expect(modelSelect).toHaveValue("deepseek-v4-flash");
  });

  describe("OpenRouter OpenAI Model Dropdown & Reasoning", () => {
    const openRouterConfig: GatewayConfig = {
      active_provider: "openrouter",
      active_openrouter_profile_id: "chatgpt",
      providers: {
        openrouter: {
          ...dummyConfig.providers.deepseek,
          display_name: "OpenRouter",
          profiles: [
            {
              id: "chatgpt",
              display_name: "OpenRouter: chatGPT",
              model_map: {},
              visible_models: [],
              models: {
                "claude-opus-5": {
                  upstream_model: "openai/gpt-5.6-sol",
                  thinking_mode: "thinking",
                  reasoning_effort: "high",
                },
                "claude-sonnet-5": {
                  upstream_model: "openai/gpt-5.6-terra",
                  thinking_mode: "thinking",
                  reasoning_effort: "high",
                },
                "claude-haiku-4-5": {
                  upstream_model: "openai/gpt-5.6-luna",
                  thinking_mode: "thinking",
                  reasoning_effort: "high",
                },
              },
            },
          ],
        },
      },
      server: {
        host: "127.0.0.1",
        port: 4000,
        enable_cors: true,
      },
    };

    it("lists all 9 OpenAI models with display names and no batch variants in OpenRouter chatGPT profile", async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "get_mcp_config") {
          return {
            provider: "openrouter",
            profile_id: "chatgpt",
            model: "openai/gpt-5.6-sol",
            thinking_mode: "thinking",
            reasoning_effort: "high",
          };
        }
        if (cmd === "get_antigravity_mcp_status") {
          return { status: "configured", registered_command: "cmd", registered_args: [] };
        }
        if (cmd === "get_antigravity_commands_status") {
          return {
            skills_dir: "C:\\skills",
            plan_command: { name: "anthro-plan", status: "installed" },
            revise_command: { name: "anthro-revise", status: "installed" },
            review_command: { name: "anthro-review", status: "installed" },
          };
        }
        return {};
      });

      render(<McpSettingPanel config={openRouterConfig} refreshConfig={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("OpenRouter")).toBeInTheDocument();
      });

      // Expand OpenRouter row
      await act(async () => {
        screen.getByText("OpenRouter").click();
      });

      const selects = screen.getAllByRole("combobox");
      // selects: [0: profileSelect, 1: modelSelect, 2: thinkingSelect, 3: effortSelect]
      const modelSelect = selects[1];
      const options = Array.from(modelSelect.querySelectorAll("option")).map((o) => ({
        value: o.value,
        label: o.textContent,
      }));

      expect(options).toEqual([
        { value: "openai/gpt-6-astra", label: "GPT-6 Astra" },
        { value: "openai/gpt-6-astra-pro", label: "GPT-6 Astra Pro" },
        { value: "openai/gpt-astra-latest", label: "GPT Astra Latest" },
        { value: "openai/gpt-5.6-sol", label: "GPT-5.6 Sol" },
        { value: "openai/gpt-5.6-sol-pro", label: "GPT-5.6 Sol Pro" },
        { value: "openai/gpt-5.6-terra", label: "GPT-5.6 Terra" },
        { value: "openai/gpt-5.6-terra-pro", label: "GPT-5.6 Terra Pro" },
        { value: "openai/gpt-5.6-luna", label: "GPT-5.6 Luna" },
        { value: "openai/gpt-5.6-luna-pro", label: "GPT-5.6 Luna Pro" },
      ]);

      expect(options.some((o) => o.value.toLowerCase().includes("batch"))).toBe(false);
      expect(modelSelect).toHaveValue("openai/gpt-5.6-sol");
    });

    it("renders 5 reasoning effort choices for GPT-6 Astra and thinking only for Astra Pro", async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "get_mcp_config") {
          return {
            provider: "openrouter",
            profile_id: "chatgpt",
            model: "openai/gpt-6-astra",
            thinking_mode: "thinking",
            reasoning_effort: "high",
          };
        }
        if (cmd === "get_antigravity_mcp_status") {
          return { status: "configured", registered_command: "cmd", registered_args: [] };
        }
        if (cmd === "get_antigravity_commands_status") {
          return {
            skills_dir: "C:\\skills",
            plan_command: { name: "anthro-plan", status: "installed" },
            revise_command: { name: "anthro-revise", status: "installed" },
            review_command: { name: "anthro-review", status: "installed" },
          };
        }
        return {};
      });

      const { unmount } = render(<McpSettingPanel config={openRouterConfig} refreshConfig={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("OpenRouter")).toBeInTheDocument();
      });

      await act(async () => {
        screen.getByText("OpenRouter").click();
      });

      // Astra: effort select is available with 5 options
      const selects = screen.getAllByRole("combobox");
      const effortSelect = selects[3];
      const effortOptions = Array.from(effortSelect.querySelectorAll("option")).map((o) => o.value);
      expect(effortOptions).toEqual(["low", "medium", "high", "xhigh", "max"]);

      unmount();

      // Astra Pro: thinking_only
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "get_mcp_config") {
          return {
            provider: "openrouter",
            profile_id: "chatgpt",
            model: "openai/gpt-6-astra-pro",
            thinking_mode: "thinking_only",
            reasoning_effort: "",
          };
        }
        if (cmd === "get_antigravity_mcp_status") {
          return { status: "configured", registered_command: "cmd", registered_args: [] };
        }
        if (cmd === "get_antigravity_commands_status") {
          return {
            skills_dir: "C:\\skills",
            plan_command: { name: "anthro-plan", status: "installed" },
            revise_command: { name: "anthro-revise", status: "installed" },
            review_command: { name: "anthro-review", status: "installed" },
          };
        }
        return {};
      });

      render(<McpSettingPanel config={openRouterConfig} refreshConfig={vi.fn()} />);

      await waitFor(() => {
        expect(screen.getByText("OpenRouter")).toBeInTheDocument();
      });

      await act(async () => {
        screen.getByText("OpenRouter").click();
      });

      expect(screen.getByText("Thinking only")).toBeInTheDocument();
      // Only profile select and model select are rendered (no thinking mode toggle, no effort select)
      const astraProSelects = screen.getAllByRole("combobox");
      expect(astraProSelects).toHaveLength(2);
    });
  });
});
