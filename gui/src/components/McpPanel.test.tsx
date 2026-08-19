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
});
