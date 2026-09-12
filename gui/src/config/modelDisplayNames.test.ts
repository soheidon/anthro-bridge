import { describe, it, expect } from "vitest";
import { getModelDisplayName } from "./modelDisplayNames";

describe("getModelDisplayName", () => {
  // ── Direct DeepSeek ──
  it("maps deepseek-flash to DeepSeek V4.1 Flash for provider deepseek", () => {
    expect(getModelDisplayName("deepseek-flash", "deepseek")).toBe("DeepSeek V4.1 Flash");
  });

  it("maps deepseek-v4-pro to DeepSeek V4 Pro 0813 for provider deepseek", () => {
    expect(getModelDisplayName("deepseek-v4-pro", "deepseek")).toBe("DeepSeek V4 Pro 0813");
  });

  it("returns unknown deepseek model ID verbatim", () => {
    expect(getModelDisplayName("deepseek-v4-flash", "deepseek")).toBe("deepseek-v4-flash");
  });

  // ── OpenRouter DeepSeek — must NOT be remapped ──
  it("returns OpenRouter V4.1 Flash ID verbatim", () => {
    expect(getModelDisplayName("deepseek/deepseek-v4.1-flash", "openrouter")).toBe(
      "deepseek/deepseek-v4.1-flash",
    );
  });

  it("returns OpenRouter V4 Flash 0731 ID verbatim", () => {
    expect(getModelDisplayName("deepseek/deepseek-v4-flash-0731", "openrouter")).toBe(
      "deepseek/deepseek-v4-flash-0731",
    );
  });

  it("returns OpenRouter V4 Pro 0813 ID verbatim", () => {
    expect(getModelDisplayName("deepseek/deepseek-v4-pro-0813", "openrouter")).toBe(
      "deepseek/deepseek-v4-pro-0813",
    );
  });

  // ── Other providers — all verbatim ──
  it("returns Kimi model ID verbatim", () => {
    expect(getModelDisplayName("kimi-k3", "kimi")).toBe("kimi-k3");
  });

  it("returns MiMo model ID verbatim", () => {
    expect(getModelDisplayName("mimo-v2.5-pro", "mimo")).toBe("mimo-v2.5-pro");
  });

  it("returns MiniMax model ID verbatim", () => {
    expect(getModelDisplayName("MiniMax-M3", "minimax")).toBe("MiniMax-M3");
  });

  // ── Custom model IDs — verbatim regardless of provider ──
  it("returns custom model ID verbatim for deepseek provider", () => {
    const customId = "my-custom-model-v1";
    expect(getModelDisplayName(customId, "deepseek")).toBe(customId);
  });

  it("returns custom model ID verbatim for unknown provider", () => {
    const customId = "custom/some-model";
    expect(getModelDisplayName(customId, "unknown-provider")).toBe(customId);
  });

  // ── No provider ──
  it("returns model ID verbatim when provider is undefined", () => {
    expect(getModelDisplayName("deepseek-flash", undefined)).toBe("deepseek-flash");
  });

  it("returns model ID verbatim when provider is empty string", () => {
    expect(getModelDisplayName("deepseek-flash", "")).toBe("deepseek-flash");
  });
});
