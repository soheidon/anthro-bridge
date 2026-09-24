import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { ClaudeConfigPanelContent } from "./ClaudeConfigPanel";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const LAUNCH_COMMAND =
  "$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude";

// userEvent.setup() installs its own real navigator.clipboard, so this must be
// called after setup() to ensure the component talks to the spy.
function installClipboardSpy() {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    value: { writeText },
    configurable: true,
  });
  return writeText;
}

describe("ClaudeConfigPanel launch command copy", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "find_claude_configs") return [];
      if (cmd === "build_claude_code_launch_command") {
        return { command: LAUNCH_COMMAND, applyEnvironment: true, status: "applied" };
      }
      return null;
    });
  });

  it("copies the launch command returned by the Tauri command", async () => {
    const user = userEvent.setup();
    const writeTextMock = installClipboardSpy();
    render(<ClaudeConfigPanelContent />);
    await user.click(screen.getByRole("button", { name: /claudeConfig\.header/ }));
    await user.click(screen.getByRole("button", { name: /launch/i }));

    expect(invokeMock).toHaveBeenCalledWith("build_claude_code_launch_command");
    expect(writeTextMock).toHaveBeenCalledWith(LAUNCH_COMMAND);
  });

  it("keeps the launch copy state isolated from the config copy button", async () => {
    const user = userEvent.setup();
    installClipboardSpy();
    render(<ClaudeConfigPanelContent />);
    await user.click(screen.getByRole("button", { name: /claudeConfig\.header/ }));

    const configCopyButton = screen.getByRole("button", { name: "claudeConfig.copy" });
    await user.click(screen.getByRole("button", { name: /launch/i }));

    await screen.findByRole("button", { name: "claudeConfig.copied" });
    expect(configCopyButton).toHaveTextContent("claudeConfig.copy");
  });

  it("handles Claude Code 3P Provider radio toggle, settings update, and vision toggle", async () => {
    const user = userEvent.setup();
    const mockConfig = {
      config_version: "1.0",
      active_provider: "deepseek",
      providers: {
        deepseek: {
          display_name: "DeepSeek",
          base_url: "https://api.deepseek.com",
          default_model: "deepseek-v4-pro",
          supports_vision: false,
          supports_video: false,
          supports_thinking: true,
        },
      },
      server: { host: "127.0.0.1", port: 4000, enable_cors: false },
      claude_code: {
        third_party_provider: {
          enabled: false,
          provider: "ollama",
          base_url: "http://127.0.0.1:11434",
          model: "mimo-v2.6-distill-qwen-9b",
          thinking_mode: "normal",
          supports_vision: false,
        },
      },
    };

    const refreshConfigMock = vi.fn().mockResolvedValue(undefined);

    render(
      <ClaudeConfigPanelContent
        config={mockConfig as any}
        refreshConfig={refreshConfigMock}
      />
    );

    // Expand panel
    await user.click(screen.getByRole("button", { name: /claudeConfig\.header/ }));

    // Switch to Ollama Local
    const ollamaRadio = screen.getByLabelText(/claudeConfig\.useOllamaLocal/i);
    await user.click(ollamaRadio);

    expect(invokeMock).toHaveBeenCalledWith("update_claude_code_third_party_settings", {
      settings: {
        enabled: true,
        provider: "ollama",
        base_url: "http://127.0.0.1:11434",
        model: "mimo-v2.6-distill-qwen-9b",
        thinking_mode: "normal",
        supports_vision: false,
        context_window: null,
      },
    });

    // Check that Ollama settings inputs are displayed
    expect(screen.getByPlaceholderText("mimo-v2.6-distill-qwen-9b")).toBeInTheDocument();

    // Toggle vision support via select dropdown
    const visionSelect = screen.getAllByRole("combobox")[1];
    await user.selectOptions(visionSelect, "true");

    expect(invokeMock).toHaveBeenCalledWith("update_claude_code_third_party_settings", {
      settings: {
        enabled: true,
        provider: "ollama",
        base_url: "http://127.0.0.1:11434",
        model: "mimo-v2.6-distill-qwen-9b",
        thinking_mode: "normal",
        supports_vision: true,
        context_window: null,
      },
    });
  });
});
