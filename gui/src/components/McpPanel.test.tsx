import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import McpPanel from "./McpPanel";
import type { GatewayConfig } from "../types";

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
    minimax: {
      display_name: "MiniMax",
      upstream_url: "https://api.minimax.chat",
      api_key_env: "MINIMAX_API_KEY",
      default_model: "MiniMax-Text-01",
      force_anthropic_version: null,
      supports_count_tokens: true,
      supports_vision: false,
      supports_video: false,
      supports_thinking: false,
      model_map: {},
      visible_models: ["MiniMax-Text-01"],
    },
  },
  server: {
    host: "127.0.0.1",
    port: 4000,
    enable_cors: true,
  },
};

describe("McpPanel - DeepSeek PEAK/VALLEY Badge", () => {
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
      if (cmd === "get_mcp_status") {
        return {
          ready: true,
        };
      }
      if (cmd === "check_all_api_keys") {
        return {
          deepseek: { set: true, env_var: "DEEPSEEK_API_KEY" },
          minimax: { set: true, env_var: "MINIMAX_API_KEY" },
        };
      }
      return null;
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders PEAK badge on DeepSeek card during peak hours and not on other providers", async () => {
    // 02:00 UTC is inside DeepSeek Peak (01:00-04:00 UTC)
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date("2026-08-19T02:00:00Z"));

    render(<McpPanel config={dummyConfig} refreshConfig={vi.fn().mockResolvedValue(undefined)} />);

    await waitFor(() => {
      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
    });

    const deepseekBadge = screen.getByText("peakValley.peak");
    expect(deepseekBadge).toBeInTheDocument();
    expect(deepseekBadge).toHaveClass("provider-tile-pricing-badge");
    expect(deepseekBadge).toHaveClass("peak");

    // MiniMax card should exist but not have any pricing badge
    expect(screen.getByText("MiniMax")).toBeInTheDocument();
    const badges = document.querySelectorAll(".provider-tile-pricing-badge");
    expect(badges).toHaveLength(1);
  });

  it("renders VALLEY badge on DeepSeek card during valley hours and not on other providers", async () => {
    // 12:00 UTC is inside DeepSeek Valley (10:00-01:00 UTC)
    vi.useFakeTimers({ shouldAdvanceTime: true });
    vi.setSystemTime(new Date("2026-08-19T12:00:00Z"));

    render(<McpPanel config={dummyConfig} refreshConfig={vi.fn().mockResolvedValue(undefined)} />);

    await waitFor(() => {
      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
    });

    const deepseekBadge = screen.getByText("peakValley.valley");
    expect(deepseekBadge).toBeInTheDocument();
    expect(deepseekBadge).toHaveClass("provider-tile-pricing-badge");
    expect(deepseekBadge).toHaveClass("valley");

    const badges = document.querySelectorAll(".provider-tile-pricing-badge");
    expect(badges).toHaveLength(1);
  });

  it("renders Kimi Code tile with Thinking: Thinking only and no Disabled or reasoning effort", async () => {
    const configWithKimiCode: GatewayConfig = {
      ...dummyConfig,
      providers: {
        ...dummyConfig.providers,
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
      },
    };

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "kimi-code",
          model: "kimi-for-coding",
          thinking_mode: "thinking_only",
        };
      }
      if (cmd === "get_mcp_status") {
        return { ready: true };
      }
      if (cmd === "check_all_api_keys") {
        return {
          deepseek: { set: true, env_var: "DEEPSEEK_API_KEY" },
          "kimi-code": { set: true, env_var: "KIMI_CODE_API_KEY" },
        };
      }
      return null;
    });

    render(<McpPanel config={configWithKimiCode} refreshConfig={vi.fn().mockResolvedValue(undefined)} />);

    await waitFor(() => {
      expect(screen.getByText("Kimi Code")).toBeInTheDocument();
    });

    const kimiCodeTile = screen.getByText("Kimi Code").closest(".provider-tile") as HTMLElement;
    expect(kimiCodeTile).not.toBeNull();
    expect(kimiCodeTile.textContent).toContain("Thinking only");
    expect(kimiCodeTile.textContent).not.toContain("Disabled");
    expect(kimiCodeTile.textContent).not.toContain("high");
    expect(kimiCodeTile.textContent).not.toContain("max");
  });

  it("displays model display name in direct provider dashboard tile", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_mcp_config") {
        return {
          provider: "deepseek",
          model: "deepseek-flash",
          thinking_mode: "thinking",
          reasoning_effort: "high",
        };
      }
      if (cmd === "get_mcp_status") {
        return { ready: true };
      }
      if (cmd === "check_all_api_keys") {
        return {
          deepseek: { set: true, env_var: "DEEPSEEK_API_KEY" },
        };
      }
      return null;
    });

    render(<McpPanel config={dummyConfig} refreshConfig={vi.fn().mockResolvedValue(undefined)} />);

    await waitFor(() => {
      expect(screen.getByText("DeepSeek")).toBeInTheDocument();
    });

    const deepseekTile = screen.getByText("DeepSeek").closest(".provider-tile") as HTMLElement;
    expect(deepseekTile).not.toBeNull();
    expect(deepseekTile.textContent).toContain("DeepSeek V4.1 Flash");
  });

  it("displays model display name in OpenRouter provider dashboard tile", async () => {
    const configWithOpenRouter: GatewayConfig = {
      ...dummyConfig,
      active_provider: "openrouter",
      active_openrouter_profile_id: "chatgpt",
      providers: {
        ...dummyConfig.providers,
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
                  upstream_model: "openai/gpt-6-astra",
                  thinking_mode: "thinking",
                  reasoning_effort: "high",
                },
              },
            },
          ],
        },
      },
    };

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
      if (cmd === "get_mcp_status") {
        return { ready: true };
      }
      if (cmd === "check_all_api_keys") {
        return {
          openrouter: { set: true, env_var: "OPENROUTER_API_KEY" },
        };
      }
      return null;
    });

    render(<McpPanel config={configWithOpenRouter} refreshConfig={vi.fn().mockResolvedValue(undefined)} />);

    await waitFor(() => {
      expect(screen.getByText("OpenRouter: chatGPT")).toBeInTheDocument();
    });

    const tile = screen.getByText("OpenRouter: chatGPT").closest(".provider-tile") as HTMLElement;
    expect(tile).not.toBeNull();
    expect(tile.textContent).toContain("GPT-6 Astra");
  });
});
