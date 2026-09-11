import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, within, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import ApiKeyPanel, { ModelSelector, type ModelSelectorProps } from "./ApiKeyPanel";
import type { GatewayConfig } from "../types";

// ── Setup ─────────────────────────────────────────────────────────

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function baseProps(overrides: Partial<ModelSelectorProps> = {}): ModelSelectorProps {
  return {
    providerId: "deepseek",
    modelKey: "claude-sonnet-5",
    gatewayModelLabel: "Sonnet 5 →",
    currentUpstream: "deepseek-v4-flash",
    thinkingModePolicy: "toggleable",
    currentThinkingMode: "thinking",
    currentReasoningEffort: "",
    onSaved: vi.fn().mockResolvedValue(undefined),
    gatewayRunning: false,
    restartGateway: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

function resetInvoke() {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "set_model_upstream") return { restartGateway: false, restartReason: "" };
    if (cmd === "get_config") return null;
    return null;
  });
}

beforeEach(() => {
  resetInvoke();
});

// The current reason-effort value the Flash model's selector would save.
function lastUpstreamSave(): Record<string, unknown> | undefined {
  const calls = invokeMock.mock.calls.filter((c: unknown[]) => c[0] === "set_model_upstream");
  const last = calls[calls.length - 1];
  return (last?.[1] as Record<string, unknown>) ?? undefined;
}

async function setReasoningEffortSelect(value: string) {
  const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
  const effortSelect = within(flashRow).getByLabelText(/reasoning effort/i);
  await act(async () => {
    await userEvent.selectOptions(effortSelect, value);
  });
}

// ── Tests ─────────────────────────────────────────────────────────

describe("DeepSeek V4 Flash reasoning effort", () => {
  it("hides the effort selector in Normal mode", () => {
    render(<ModelSelector {...baseProps({ currentThinkingMode: "normal", currentReasoningEffort: "" })} />);
    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    expect(within(flashRow).queryByLabelText(/reasoning effort/i)).not.toBeInTheDocument();
  });

  it("shows only Low / High / Max options in Thinking mode", () => {
    render(<ModelSelector {...baseProps({ currentThinkingMode: "thinking" })} />);
    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const effortSelect = within(flashRow).getByLabelText(/reasoning effort/i);
    const options = Array.from(effortSelect.querySelectorAll("option")).map((o) => o.textContent);
    expect(options).toEqual(["Low", "High", "Max"]);
    expect(options).not.toContain("Medium");
    expect(options).not.toContain("XHigh");
    expect(options).not.toContain("Not set");
  });

  it("saves reasoning_effort=\"max\" when Max is selected", async () => {
    render(<ModelSelector {...baseProps({ currentThinkingMode: "thinking" })} />);
    await setReasoningEffortSelect("Max");
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "set_model_upstream",
        expect.objectContaining({ reasoningEffort: "max" }),
      );
    });
  });

  it("saves reasoning_effort=\"low\" when Low is selected", async () => {
    render(<ModelSelector {...baseProps({ currentThinkingMode: "thinking" })} />);
    await setReasoningEffortSelect("Low");
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "set_model_upstream",
        expect.objectContaining({ reasoningEffort: "low" }),
      );
    });
  });

  it("clears the effort to null when switching from Thinking to Normal", async () => {
    render(<ModelSelector {...baseProps({ currentThinkingMode: "thinking" })} />);
    // Start with Max selected.
    await setReasoningEffortSelect("Max");
    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBe("max");
    });

    // Switch Mode → Normal (mode select is the second combobox in the row).
    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const modeSelect = within(flashRow).getAllByRole("combobox")[1] as HTMLElement;
    await act(async () => {
      await userEvent.selectOptions(modeSelect, "normal");
    });

    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBeNull();
      expect(lastUpstreamSave()?.thinkingMode).toBe("normal");
    });
  });

  it("defaults to High when enabling Thinking with no prior effort", async () => {
    render(<ModelSelector {...baseProps({ currentThinkingMode: "normal" })} />);
    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const modeSelect = within(flashRow).getAllByRole("combobox")[1] as HTMLElement;
    await act(async () => {
      await userEvent.selectOptions(modeSelect, "thinking");
    });
    const effortSelect = within(flashRow).getByLabelText(/reasoning effort/i);
    expect((effortSelect as HTMLSelectElement).value).toBe("high");
  });

  it("normalizes a legacy medium to high when switching from Pro to Flash", async () => {
    render(
      <ModelSelector
        {...baseProps({
          currentUpstream: "deepseek-v4-pro",
          currentThinkingMode: "thinking",
          currentReasoningEffort: "medium",
        })}
      />,
    );
    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const modelSelect = within(flashRow).getAllByRole("combobox")[0];
    await act(async () => {
      await userEvent.selectOptions(modelSelect, "deepseek-flash");
    });
    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBe("high");
    });
  });

  it("preserves low when switching from Pro to Flash", async () => {
    render(
      <ModelSelector
        {...baseProps({
          currentUpstream: "deepseek-v4-pro",
          currentThinkingMode: "thinking",
          currentReasoningEffort: "low",
        })}
      />,
    );
    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const modelSelect = within(flashRow).getAllByRole("combobox")[0];
    await act(async () => {
      await userEvent.selectOptions(modelSelect, "deepseek-flash");
    });
    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBe("low");
    });
  });
});

describe("DeepSeek V4 Pro reasoning effort", () => {
  const proProps = (overrides: Partial<ModelSelectorProps> = {}) =>
    baseProps({
      currentUpstream: "deepseek-v4-pro",
      thinkingModePolicy: "toggleable",
      ...overrides,
    });

  function getEffortSelect() {
    const proRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    return within(proRow).getByLabelText(/reasoning effort/i) as HTMLSelectElement;
  }

  it("hides the effort selector in Normal mode", () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "normal" })} />);
    const proRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    expect(queryOptions(within(proRow).queryByLabelText(/reasoning effort/i))).toBeNull();
  });

  it("shows Low / High / Max options in Thinking mode", () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking" })} />);
    const options = Array.from(getEffortSelect().querySelectorAll("option")).map((o) => o.textContent);
    expect(options).toEqual(["Low", "High", "Max"]);
    expect(options).not.toContain("Medium");
    expect(options).not.toContain("Not set");
  });

  it("saves reasoning_effort=\"high\" when High is selected", async () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking" })} />);
    await act(async () => {
      await userEvent.selectOptions(getEffortSelect(), "high");
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "set_model_upstream",
        expect.objectContaining({ reasoningEffort: "high" }),
      );
    });
  });

  it("saves reasoning_effort=\"max\" when Max is selected", async () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking" })} />);
    await act(async () => {
      await userEvent.selectOptions(getEffortSelect(), "max");
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "set_model_upstream",
        expect.objectContaining({ reasoningEffort: "max" }),
      );
    });
  });

  it("clears the effort to null when switching from Thinking to Normal", async () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking" })} />);
    await act(async () => {
      await userEvent.selectOptions(getEffortSelect(), "max");
    });
    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBe("max");
    });

    const proRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const modeSelect = within(proRow).getAllByRole("combobox")[1] as HTMLElement;
    await act(async () => {
      await userEvent.selectOptions(modeSelect, "normal");
    });

    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBeNull();
      expect(lastUpstreamSave()?.thinkingMode).toBe("normal");
    });
  });

  it("defaults to High when enabling Thinking with no prior effort", async () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "normal" })} />);
    const proRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const modeSelect = within(proRow).getAllByRole("combobox")[1] as HTMLElement;
    await act(async () => {
      await userEvent.selectOptions(modeSelect, "thinking");
    });
    expect(getEffortSelect().value).toBe("high");
  });

  it("normalizes a legacy medium to high on display", () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking", currentReasoningEffort: "medium" })} />);
    expect(getEffortSelect().value).toBe("high");
  });

  it("keeps low as-is on display", () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking", currentReasoningEffort: "low" })} />);
    expect(getEffortSelect().value).toBe("low");
  });

  it("keeps an already-valid high / max value as-is on display", () => {
    const { unmount } = render(
      <ModelSelector {...proProps({ currentThinkingMode: "thinking", currentReasoningEffort: "max" })} />,
    );
    expect(getEffortSelect().value).toBe("max");
    unmount();
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking", currentReasoningEffort: "high" })} />);
    expect(getEffortSelect().value).toBe("high");
  });

  it("normalizes a legacy xhigh to high on display", () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking", currentReasoningEffort: "xhigh" })} />);
    expect(getEffortSelect().value).toBe("high");
  });

  it("saves reasoning_effort=\"low\" when Low is selected", async () => {
    render(<ModelSelector {...proProps({ currentThinkingMode: "thinking" })} />);
    await act(async () => {
      await userEvent.selectOptions(getEffortSelect(), "low");
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "set_model_upstream",
        expect.objectContaining({ reasoningEffort: "low" }),
      );
    });
  });
});

describe("DeepSeek V4.1 Flash reasoning effort", () => {
  const flashProps = (overrides: Partial<ModelSelectorProps> = {}) =>
    baseProps({
      currentUpstream: "deepseek-flash",
      thinkingModePolicy: "toggleable",
      ...overrides,
    });

  function getEffortSelect() {
    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    return within(flashRow).getByLabelText(/reasoning effort/i) as HTMLSelectElement;
  }

  it("renders Low, High, Max options for deepseek-flash", () => {
    render(<ModelSelector {...flashProps({ currentThinkingMode: "thinking" })} />);
    const options = Array.from(getEffortSelect().querySelectorAll("option")).map((o) => o.textContent);
    expect(options).toEqual(["Low", "High", "Max"]);
  });

  it("saves reasoning_effort=\"low\" when Low is selected", async () => {
    render(<ModelSelector {...flashProps({ currentThinkingMode: "thinking" })} />);
    await act(async () => {
      await userEvent.selectOptions(getEffortSelect(), "low");
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "set_model_upstream",
        expect.objectContaining({ reasoningEffort: "low" }),
      );
    });
  });

  it("saves reasoning_effort=\"max\" when Max is selected", async () => {
    render(<ModelSelector {...flashProps({ currentThinkingMode: "thinking" })} />);
    await act(async () => {
      await userEvent.selectOptions(getEffortSelect(), "max");
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "set_model_upstream",
        expect.objectContaining({ reasoningEffort: "max" }),
      );
    });
  });

  it("clears the effort to null when switching from Thinking to Normal", async () => {
    render(<ModelSelector {...flashProps({ currentThinkingMode: "thinking" })} />);
    await act(async () => {
      await userEvent.selectOptions(getEffortSelect(), "max");
    });
    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBe("max");
    });

    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const modeSelect = within(flashRow).getAllByRole("combobox")[1] as HTMLElement;
    await act(async () => {
      await userEvent.selectOptions(modeSelect, "normal");
    });

    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBeNull();
      expect(lastUpstreamSave()?.thinkingMode).toBe("normal");
    });
  });

  it("defaults to High when enabling Thinking with no prior effort", async () => {
    render(<ModelSelector {...flashProps({ currentThinkingMode: "normal", currentReasoningEffort: "" })} />);
    const flashRow = screen.getByText("Sonnet 5 →").closest("div") as HTMLElement;
    const modeSelect = within(flashRow).getAllByRole("combobox")[1] as HTMLElement;
    await act(async () => {
      await userEvent.selectOptions(modeSelect, "thinking");
    });

    await waitFor(() => {
      expect(lastUpstreamSave()?.reasoningEffort).toBe("high");
      expect(lastUpstreamSave()?.thinkingMode).toBe("thinking");
    });
  });

  it("preserves low when switching from Pro to Flash", () => {
    const { rerender } = render(
      <ModelSelector
        {...flashProps({
          currentUpstream: "deepseek-v4-pro",
          currentThinkingMode: "thinking",
          currentReasoningEffort: "low",
        })}
      />,
    );
    expect(getEffortSelect().value).toBe("low");

    rerender(
      <ModelSelector
        {...flashProps({
          currentUpstream: "deepseek-flash",
          currentThinkingMode: "thinking",
          currentReasoningEffort: "low",
        })}
      />,
    );
    expect(getEffortSelect().value).toBe("low");
  });

  it("normalizes a legacy medium to high when switching from Pro to Flash", () => {
    render(
      <ModelSelector
        {...flashProps({
          currentUpstream: "deepseek-flash",
          currentThinkingMode: "thinking",
          currentReasoningEffort: "medium",
        })}
      />,
    );
    expect(getEffortSelect().value).toBe("high");
  });
});

describe("Kimi Code ModelSelector", () => {
  it("renders only kimi-for-coding and kimi-for-coding-highspeed options", () => {
    const props = baseProps({
      providerId: "kimi-code",
      modelKey: "claude-sonnet-5",
      currentUpstream: "kimi-for-coding",
      thinkingModePolicy: "thinking_only",
      currentThinkingMode: "thinking_only",
      currentReasoningEffort: "",
    });
    render(<ModelSelector {...props} />);
    const modelSelect = screen.getByRole("combobox");
    const options = Array.from(modelSelect.querySelectorAll("option")).map((o) => o.value);
    expect(options).toEqual(["kimi-for-coding", "kimi-for-coding-highspeed", "__custom__"]);
  });

  it("displays subscription and quota note in Kimi Code provider panel when expanded", async () => {
    const mockConfig: GatewayConfig = {
      port: 8080,
      host: "127.0.0.1",
      cors_origins: [],
      active_provider: "kimi-code",
      auto_compact_threshold_kb: 50,
      claude_desktop: { enabled: true },
      providers: {
        "kimi-code": {
          display_name: "Kimi Code",
          api_key_env: "KIMI_CODE_API_KEY",
          messages_url: "https://api.kimi.com/coding/v1/messages",
          supports_count_tokens: false,
          models: {
            "claude-opus-5": { upstream_model: "kimi-for-coding", thinking_mode: "thinking_only" },
            "claude-sonnet-5": { upstream_model: "kimi-for-coding", thinking_mode: "thinking_only" },
            "claude-haiku-4-5": { upstream_model: "kimi-for-coding-highspeed", thinking_mode: "thinking_only" },
          },
        },
      },
    } as unknown as GatewayConfig;

    render(
      <ApiKeyPanel
        config={mockConfig}
        refreshConfig={vi.fn().mockResolvedValue(undefined)}
        gatewayRunning={false}
        restartGateway={vi.fn().mockResolvedValue(undefined)}
      />,
    );

    const kimiCodeHeader = screen.getByText("Kimi Code");
    await act(async () => {
      kimiCodeHeader.click();
    });

    expect(screen.getByText(/Kimi Code subscription/i)).toBeInTheDocument();
    expect(screen.getByText(/membership/i)).toBeInTheDocument();
  });

  it("renders providers in the order: DeepSeek, MiniMax, Kimi, Kimi Code, MiMo, OpenRouter regardless of object key order", () => {
    const mockConfig: GatewayConfig = {
      port: 8080,
      host: "127.0.0.1",
      cors_origins: [],
      active_provider: "deepseek",
      auto_compact_threshold_kb: 50,
      claude_desktop: { enabled: true },
      providers: {
        openrouter: { display_name: "OpenRouter", profiles: [] },
        "kimi-code": { display_name: "Kimi Code", models: {} },
        mimo: { display_name: "MiMo", models: {} },
        kimi: { display_name: "Kimi", models: {} },
        minimax: { display_name: "MiniMax", models: {} },
        deepseek: { display_name: "DeepSeek", models: {} },
      },
    } as unknown as GatewayConfig;

    render(
      <ApiKeyPanel
        config={mockConfig}
        refreshConfig={vi.fn().mockResolvedValue(undefined)}
        gatewayRunning={false}
        restartGateway={vi.fn().mockResolvedValue(undefined)}
      />,
    );

    const providerRowButtons = screen.getAllByRole("button").filter((btn) => btn.getAttribute("aria-expanded") !== null);
    const providerNames = providerRowButtons.map((btn) => btn.querySelector("div:nth-child(2)")?.textContent?.trim());
    expect(providerNames).toEqual(["DeepSeek", "MiniMax", "Kimi", "Kimi Code", "MiMo", "OpenRouter"]);
  });
});

// Returns option texts from a select element, or null when absent.
function queryOptions(select: HTMLElement | null): string[] | null {
  if (!select) return null;
  return Array.from(select.querySelectorAll("option")).map((o) => o.textContent ?? "");
}
