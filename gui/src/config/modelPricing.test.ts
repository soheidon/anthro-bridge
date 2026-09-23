import { describe, expect, it, vi } from "vitest";

vi.unmock("./builtinOpenRouter");

import { BUILTIN_OPENROUTER_MODELS } from "./builtinOpenRouter";
import { MODEL_PRICING } from "./modelPricing";

const GPT56_IDS = [
  "openai/gpt-5.6-sol",
  "openai/gpt-5.6-sol-pro",
  "openai/gpt-5.6-terra",
  "openai/gpt-5.6-terra-pro",
  "openai/gpt-5.6-luna",
  "openai/gpt-5.6-luna-pro",
] as const;

describe("GPT-5.6 production pricing data", () => {
  it("keeps every source regular-price tier complete", () => {
    for (const id of GPT56_IDS) {
      const pricing = BUILTIN_OPENROUTER_MODELS[id].pricing!;
      const regular = [
        pricing.regularInputPerMillionUsd,
        pricing.regularOutputPerMillionUsd,
        pricing.regularCacheReadPerMillionUsd,
      ];

      expect(regular.some((value) => value !== undefined)).toBe(
        regular.every((value) => value !== undefined),
      );
    }
  });

  it("defines the Sol Pro promotional prices and regular prices", () => {
    expect(
      BUILTIN_OPENROUTER_MODELS["openai/gpt-5.6-sol-pro"].pricing,
    ).toMatchObject({
      inputPerMillionUsd: 2.0,
      outputPerMillionUsd: 10.0,
      cacheReadPerMillionUsd: 0.2,
      regularInputPerMillionUsd: 4.0,
      regularOutputPerMillionUsd: 20.0,
      regularCacheReadPerMillionUsd: 0.4,
    });

    expect(MODEL_PRICING["openai/gpt-5.6-sol-pro"]).toMatchObject({
      inputPerMillionUsd: 2.0,
      outputPerMillionUsd: 10.0,
      cachedInputPerMillionUsd: 0.2,
      regularInputPerMillionUsd: 4.0,
      regularOutputPerMillionUsd: 20.0,
      regularCachedInputPerMillionUsd: 0.4,
    });
  });

  it("defines single-tier headline prices for Terra Pro and Luna Pro without promotional distortion", () => {
    expect(
      BUILTIN_OPENROUTER_MODELS["openai/gpt-5.6-terra-pro"].pricing,
    ).toEqual({
      inputPerMillionUsd: 2.0,
      outputPerMillionUsd: 12.0,
      cacheReadPerMillionUsd: 0.2,
    });
    expect(
      BUILTIN_OPENROUTER_MODELS["openai/gpt-5.6-luna-pro"].pricing,
    ).toEqual({
      inputPerMillionUsd: 0.2,
      outputPerMillionUsd: 1.2,
      cacheReadPerMillionUsd: 0.02,
    });
  });
});

describe("DeepSeek production pricing data", () => {
  it("defines Direct DeepSeek pricing for deepseek-flash", () => {
    const flash = MODEL_PRICING["deepseek-flash"];
    expect(flash).toBeDefined();
    expect(flash.inputPerMillionUsd).toBe(0.15);
    expect(flash.outputPerMillionUsd).toBe(0.60);
    expect(flash.cachedInputPerMillionUsd).toBe(0.003);
    expect(flash.pricingNoteKey).toBe("modelPricing.notes.deepseekPeakValley");
  });

  it("defines OpenRouter DeepSeek models in BUILTIN_OPENROUTER_MODELS and MODEL_PRICING", () => {
    const v41 = BUILTIN_OPENROUTER_MODELS["deepseek/deepseek-v4.1-flash"];
    expect(v41).toBeDefined();
    expect(v41.displayName).toBe("DeepSeek V4.1 Flash");
    expect(v41.pricing).toEqual({
      inputPerMillionUsd: 0.15,
      outputPerMillionUsd: 0.60,
    });
    expect(v41.capabilities.supports_vision).toBe(true);
    expect(v41.capabilities.reasoningEffortOptions).toEqual(["low", "high", "max"]);

    const v4Flash0731 = BUILTIN_OPENROUTER_MODELS["deepseek/deepseek-v4-flash-0731"];
    expect(v4Flash0731).toBeDefined();
    expect(v4Flash0731.displayName).toBe("DeepSeek V4 Flash 0731");
    expect(v4Flash0731.pricing).toEqual({
      inputPerMillionUsd: 0.05,
      outputPerMillionUsd: 0.16,
      cacheReadPerMillionUsd: 0.013,
    });
    expect(v4Flash0731.capabilities.supports_vision).toBe(false);
    expect(v4Flash0731.capabilities.reasoningEffortOptions).toEqual(["high", "max"]);

    const v4Pro0813 = BUILTIN_OPENROUTER_MODELS["deepseek/deepseek-v4-pro-0813"];
    expect(v4Pro0813).toBeDefined();
    expect(v4Pro0813.displayName).toBe("DeepSeek V4 Pro 0813");
    expect(v4Pro0813.pricing).toEqual({
      inputPerMillionUsd: 0.66,
      outputPerMillionUsd: 1.98,
    });
    expect(v4Pro0813.pricingNoteKeys).toEqual([
      "modelPricing.notes.openrouterPricing",
      "modelPricing.notes.deepseekV4ProGoingAway",
    ]);
    expect(v4Pro0813.capabilities.supports_vision).toBe(false);
    expect(v4Pro0813.capabilities.reasoningEffortOptions).toEqual(["high", "max"]);
  });

  it("defines identical pricing for deepseek-v4-flash and deepseek-v4-flash-vision-exp", () => {
    const flash = MODEL_PRICING["deepseek-v4-flash"];
    const vision = MODEL_PRICING["deepseek-v4-flash-vision-exp"];

    expect(flash).toBeDefined();
    expect(vision).toBeDefined();

    expect(vision.inputPerMillionUsd).toBe(flash.inputPerMillionUsd);
    expect(vision.outputPerMillionUsd).toBe(flash.outputPerMillionUsd);
    expect(vision.cachedInputPerMillionUsd).toBe(flash.cachedInputPerMillionUsd);
    expect(vision.pricingNoteKey).toBe(flash.pricingNoteKey);

    expect(vision.inputPerMillionUsd).toBe(0.22);
    expect(vision.outputPerMillionUsd).toBe(0.66);
    expect(vision.cachedInputPerMillionUsd).toBe(0.007);
  });
});

describe("Gemini 3.8 Flash production pricing data", () => {
  it("defines promotional pricing with regular prices", () => {
    const gemini38 = BUILTIN_OPENROUTER_MODELS["google/gemini-3.8-flash"];
    expect(gemini38).toBeDefined();
    expect(gemini38.displayName).toBe("Gemini 3.8 Flash");
    expect(gemini38.pricing).toEqual({
      inputPerMillionUsd: 0.75,
      outputPerMillionUsd: 3.75,
      cacheReadPerMillionUsd: 0.075,
      regularInputPerMillionUsd: 1.50,
      regularOutputPerMillionUsd: 7.50,
      regularCacheReadPerMillionUsd: 0.15,
    });
    expect(gemini38.capabilities.forcedThinkingOptions).toEqual([
      "low",
      "medium",
      "high",
    ]);
  });
});

describe("MiMo V2.6 production pricing data", () => {
  it("defines Direct MiMo V2.6 pricing for flash, pro, and pro-ultraspeed", () => {
    const flash = MODEL_PRICING["mimo-v2.6-flash"];
    expect(flash).toBeDefined();
    expect(flash.inputPerMillionUsd).toBe(0.14);
    expect(flash.outputPerMillionUsd).toBe(0.28);
    expect(flash.cachedInputPerMillionUsd).toBe(0.0028);
    expect(flash.verifiedAt).toBe("2026-09-22");

    const pro = MODEL_PRICING["mimo-v2.6-pro"];
    expect(pro).toBeDefined();
    expect(pro.inputPerMillionUsd).toBe(0.435);
    expect(pro.outputPerMillionUsd).toBe(0.87);
    expect(pro.cachedInputPerMillionUsd).toBe(0.0036);
    expect(pro.verifiedAt).toBe("2026-09-22");

    const ultra = MODEL_PRICING["mimo-v2.6-pro-ultraspeed"];
    expect(ultra).toBeDefined();
    expect(ultra.inputPerMillionUsd).toBe(4.35);
    expect(ultra.outputPerMillionUsd).toBe(8.70);
    expect(ultra.cachedInputPerMillionUsd).toBe(0.036);
    expect(ultra.pricingNoteKey).toBe("modelPricing.notes.mimoUltraSpeed");
    expect(ultra.verifiedAt).toBe("2026-09-22");
  });

  it("retains legacy MiMo V2.5 pricing for backward compatibility", () => {
    expect(MODEL_PRICING["mimo-v2.5"]).toBeDefined();
    expect(MODEL_PRICING["mimo-v2.5-pro"]).toBeDefined();
    expect(MODEL_PRICING["mimo-v2.5-pro-ultraspeed"]).toBeDefined();
  });
});

export { GPT56_IDS };

// This file intentionally imports the production modules directly; it must not use the global builtinOpenRouter test mock.
