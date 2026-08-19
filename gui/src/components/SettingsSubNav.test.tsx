import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { LanguageContext } from "../i18n";
import App from "../App";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    title: vi.fn().mockResolvedValue("Anthro Bridge"),
    isMaximized: vi.fn().mockResolvedValue(false),
    onResized: vi.fn().mockResolvedValue(() => {}),
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    setSize: vi.fn().mockResolvedValue(undefined),
    innerSize: vi.fn().mockResolvedValue({ width: 800, height: 600 }),
    outerSize: vi.fn().mockResolvedValue({ width: 800, height: 600 }),
    scaleFactor: vi.fn().mockResolvedValue(1),
  }),
}));

describe("Settings Sub-Navigation Integration", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "is_first_run") return false;
      if (cmd === "read_config") {
        return {
          server: { host: "127.0.0.1", port: 4000, enable_cors: true },
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
        };
      }
      if (cmd === "get_health") {
        return { reachable: true, port_listening: true, managed_child_running: false };
      }
      if (cmd === "resolve_claude_code_auto_compact") {
        return { enabled: false, window_size: 262144, pct_override: 90 };
      }
      if (cmd === "get_mcp_config") {
        return { provider: "deepseek", model: "deepseek-chat" };
      }
      if (cmd === "get_antigravity_mcp_status") {
        return { status: "not_configured" };
      }
      if (cmd === "get_antigravity_commands_status") {
        return { skills_dir: "", plan_command: { name: "anthro-plan", status: "not_installed" }, revise_command: { name: "anthro-revise", status: "not_installed" } };
      }
      if (cmd === "find_claude_configs") return [];
      if (cmd === "get_pricing_custom_models") return [];
      if (cmd === "get_timezone_setting") return "local";
      if (cmd === "get_raw_config") return { content: "", encoding_used: "UTF-8" };
      if (cmd === "list_logs") return [];
      if (cmd === "get_managed_log") return "";
      if (cmd === "get_user_language") return "ja";
      return null;
    });
  });

  it("navigates to Settings and switches between General, Claude Desktop, and Antigravity sub-tabs", async () => {
    const user = userEvent.setup();
    render(
      <LanguageContext.Provider value={{ lang: "ja", setLang: vi.fn() }}>
        <App />
      </LanguageContext.Provider>
    );

    // Wait for app load and switch to Settings tab (in test environment, translation keys are returned)
    const settingsTopTab = await screen.findByRole("button", { name: /header\.settings/i });
    await user.click(settingsTopTab);

    // Sidebar items should be visible
    const generalNav = await screen.findByRole("button", { name: /settings\.nav\.general/i });
    const claudeNav = screen.getByRole("button", { name: /settings\.nav\.claudeDesktop/i });
    const antigravityNav = screen.getByRole("button", { name: /settings\.nav\.antigravity/i });

    expect(generalNav).toBeInTheDocument();
    expect(claudeNav).toBeInTheDocument();
    expect(antigravityNav).toBeInTheDocument();

    // 1. Initial sub-tab: General
    expect(generalNav).toHaveClass("active");
    expect(screen.getByText(/language\.header/i)).toBeInTheDocument();
    expect(screen.getByText(/^peakValley\.pricingDisplayTimezone$/i)).toBeInTheDocument();
    expect(screen.queryByText(/apiKeyPanel\.header/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/claudeConfig\.header/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/antigravity\.header/i)).not.toBeInTheDocument();

    // 2. Click Claude Desktop
    await user.click(claudeNav);
    expect(claudeNav).toHaveClass("active");
    expect(generalNav).not.toHaveClass("active");
    expect(screen.getByText(/apiKeyPanel\.header/i)).toBeInTheDocument();
    expect(screen.getByText(/claudeConfig\.header/i)).toBeInTheDocument();
    expect(screen.queryByText(/language\.header/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/antigravity\.header/i)).not.toBeInTheDocument();

    // 3. Click Antigravity
    await user.click(antigravityNav);
    expect(antigravityNav).toHaveClass("active");
    expect(claudeNav).not.toHaveClass("active");
    expect(screen.getByText(/antigravity\.header/i)).toBeInTheDocument();
    expect(screen.queryByText(/claudeConfig\.header/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/apiKeyPanel\.header/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/language\.header/i)).not.toBeInTheDocument();

    // 4. Return to General
    await user.click(generalNav);
    expect(generalNav).toHaveClass("active");
    expect(antigravityNav).not.toHaveClass("active");
    expect(screen.getByText(/language\.header/i)).toBeInTheDocument();
    expect(screen.queryByText(/apiKeyPanel\.header/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/antigravity\.header/i)).not.toBeInTheDocument();
  });
});
