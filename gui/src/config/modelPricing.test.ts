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

  it("defines the Luna Pro source and normalized regular prices", () => {
    expect(
      BUILTIN_OPENROUTER_MODELS["openai/gpt-5.6-luna-pro"].pricing,
    ).toMatchObject({
      inputPerMillionUsd: 0.1,
      outputPerMillionUsd: 0.6,
      cacheReadPerMillionUsd: 0.01,
      regularInputPerMillionUsd: 0.2,
      regularOutputPerMillionUsd: 1.2,
      regularCacheReadPerMillionUsd: 0.02,
    });

    expect(MODEL_PRICING["openai/gpt-5.6-luna-pro"]).toMatchObject({
      regularInputPerMillionUsd: 0.2,
      regularOutputPerMillionUsd: 1.2,
      regularCachedInputPerMillionUsd: 0.02,
    });
  });
});

describe("DeepSeek production pricing data", () => {
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

export { GPT56_IDS };

// This file intentionally imports the production modules directly; it must not use the global builtinOpenRouter test mock.
