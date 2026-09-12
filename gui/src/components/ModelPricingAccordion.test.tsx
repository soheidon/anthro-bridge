import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ModelPricingAccordion, { PriceCell, formatPrice } from "./ModelPricingAccordion";
import { MODEL_PRICING } from "../config/modelPricing";
import { PROVIDER_MODELS } from "../modelCapabilities";

describe("ModelPricingAccordion", () => {
  function openPricing() {
    render(<ModelPricingAccordion />);
  }

  it("renders pricing table unconditionally without accordion toggle", () => {
    render(<ModelPricingAccordion />);
    expect(screen.getByRole("columnheader", { name: "Input/1M" })).toBeInTheDocument();
  });

  it("keeps OpenRouter IDs unique and orders all GPT-5.6 variants", () => {
    const openRouterIds = PROVIDER_MODELS.openrouter;
    expect(new Set(openRouterIds).size).toBe(openRouterIds.length);
    expect(openRouterIds.filter((id) => id.startsWith("openai/gpt-5.6-"))).toEqual([
      "openai/gpt-5.6-sol",
      "openai/gpt-5.6-sol-pro",
      "openai/gpt-5.6-terra",
      "openai/gpt-5.6-terra-pro",
      "openai/gpt-5.6-luna",
      "openai/gpt-5.6-luna-pro",
    ]);

    openPricing();
    const modelCells = screen.getAllByRole("cell").map((cell) => cell.textContent ?? "");
    const gpt56Ids = modelCells.filter((text) => text.startsWith("openai/gpt-5.6-"));
    expect(gpt56Ids).toHaveLength(6);
  });

  it("renders current and revised standard prices for Sol, Laguna, Step, and Gemini Flash", () => {
    openPricing();

    // Sol / Sol Pro ($2.000 current, $4.000 regular)
    expect(screen.getAllByText("$2.000")).toHaveLength(5);
    expect(screen.getAllByText("$4.000")).toHaveLength(3);

    // Gemini 3.7 / 3.8 Flash ($0.750 current, $1.500 regular)
    expect(screen.getAllByText("$0.750")).toHaveLength(2);
    expect(screen.getAllByText("$1.500")).toHaveLength(2);
    expect(screen.getAllByText("$3.750")).toHaveLength(2);
    expect(screen.getAllByText("$7.500")).toHaveLength(2);

    // Sol (3x2) + Laguna S 2.1 (3x1) + Gemini 3.7 (3x1) + Gemini 3.8 (3x1) + Step 3.7 (2x1) = 17 strikethrough elements
    expect(document.querySelectorAll("s")).toHaveLength(17);
  });

  it("renders all three regular prices in the Sol Pro row", () => {
    openPricing();

    const solProRow = screen
      .getByText("openai/gpt-5.6-sol-pro")
      .closest("tr");

    expect(solProRow).not.toBeNull();
    expect(within(solProRow!).getByText("$2.000")).toBeInTheDocument();
    expect(within(solProRow!).getByText("$4.000")).toBeInTheDocument();
    expect(within(solProRow!).getByText("$10.000")).toBeInTheDocument();
    expect(within(solProRow!).getByText("$20.000")).toBeInTheDocument();
    expect(within(solProRow!).getByText("$0.2000")).toBeInTheDocument();
    expect(within(solProRow!).getByText("$0.4000")).toBeInTheDocument();
    expect(solProRow!.querySelectorAll("s")).toHaveLength(3);
  });

  it("uses the production price catalog for the complete data test", () => {
    const promoIds = [
      "openai/gpt-5.6-sol",
      "openai/gpt-5.6-sol-pro",
      "google/gemini-3.7-flash",
      "google/gemini-3.8-flash",
      "poolside/laguna-s-2.1",
    ] as const;

    for (const id of promoIds) {
      const pricing = MODEL_PRICING[id];
      const regular = [
        pricing.regularInputPerMillionUsd,
        pricing.regularOutputPerMillionUsd,
        pricing.regularCachedInputPerMillionUsd,
      ];

      expect(regular.some((value) => value !== undefined)).toBe(true);
    }
  });

  it("renders Sol with promotional discount and regular prices", () => {
    openPricing();

    const solRows = screen.getAllByRole("row").filter((row) =>
      row.textContent?.includes("openai/gpt-5.6-sol"),
    );
    expect(solRows).toHaveLength(2);
    for (const row of solRows) {
      expect(row.querySelectorAll("s")).toHaveLength(3);
      expect(row.textContent).toContain("$2.000");
      expect(row.textContent).toContain("$4.000");
      expect(row.textContent).toContain("$10.000");
      expect(row.textContent).toContain("$20.000");
    }
  });

  it("provides localized screen-reader text for discounted prices", () => {
    openPricing();

    const texts = screen.getAllByText(
      "Current price $2.000, revised standard price $4.000",
    );
    expect(texts).toHaveLength(2);
    for (const text of texts) {
      expect(text).toHaveClass("sr-only");
    }
    expect(document.querySelectorAll('[aria-hidden="true"]')).not.toHaveLength(0);
  });

  it("renders the promotion and long-context notes", () => {
    openPricing();

    // Sol (1), Sol Pro (1), Laguna S (1), Gemini 3.7 (1), Gemini 3.8 (1), Step 3.7 (1) = 6 models with promotion
    expect(
      screen.getAllByText(/Limited-time promotional pricing is currently active on OpenRouter/),
    ).toHaveLength(6);
    expect(
      screen.queryByText(/50% provider discount/),
    ).not.toBeInTheDocument();
    expect(screen.getAllByText(/272K tokens or more/)).toHaveLength(6);
  });

  it("keeps existing models as single-price rows", () => {
    openPricing();

    const existingRow = Array.from(document.querySelectorAll("tbody tr")).find((row) =>
      row.textContent?.includes("deepseek-v4-pro"),
    ) ?? null;
    expect(existingRow).not.toBeNull();
    expect(existingRow?.querySelectorAll("s")).toHaveLength(0);
  });

  it("preserves zero as a valid price", () => {
    expect(formatPrice(0, 3)).toBe("$0.000");
    render(<PriceCell current={0} decimals={3} />);
    expect(screen.getByText("$0.000")).toBeInTheDocument();
  });

  it("keeps standard and pro data aligned and complete", () => {
    const families = ["sol", "terra", "luna"] as const;
    for (const family of families) {
      const standard = MODEL_PRICING[`openai/gpt-5.6-${family}`];
      const pro = MODEL_PRICING[`openai/gpt-5.6-${family}-pro`];
      expect(pro).toMatchObject(standard);
    }

    expect(MODEL_PRICING["openai/gpt-5.6-sol"].regularInputPerMillionUsd).toBe(4);
    expect(MODEL_PRICING["openai/gpt-5.6-terra"].regularInputPerMillionUsd).toBeUndefined();
    expect(MODEL_PRICING["openai/gpt-5.6-luna"].regularInputPerMillionUsd).toBeUndefined();
  });

  it("does not render kimi-code row in pricing accordion and does not display $0.000 for Kimi Code", () => {
    openPricing();

    const allCellTexts = screen.getAllByRole("cell").map((cell) => cell.textContent ?? "");
    expect(allCellTexts.some((text) => text.includes("kimi-for-coding"))).toBe(false);
    expect(allCellTexts.some((text) => text.includes("kimi-for-coding-highspeed"))).toBe(false);

    const rows = screen.getAllByRole("row");
    const kimiCodeRows = rows.filter(
      (row) => row.textContent?.includes("Kimi Code") || row.textContent?.includes("kimi-for-coding"),
    );
    expect(kimiCodeRows).toHaveLength(0);

    expect(MODEL_PRICING["kimi-for-coding"]).toBeUndefined();
    expect(MODEL_PRICING["kimi-for-coding-highspeed"]).toBeUndefined();
  });

  it("renders GPT-6 Astra, GPT-6 Astra Pro, and GPT Astra Latest with $10/$50 pricing", () => {
    openPricing();

    const astraModels = ["openai/gpt-6-astra", "openai/gpt-6-astra-pro", "openai/gpt-astra-latest"];
    for (const id of astraModels) {
      const pricing = MODEL_PRICING[id];
      expect(pricing).toBeDefined();
      expect(pricing.inputPerMillionUsd).toBe(10.0);
      expect(pricing.outputPerMillionUsd).toBe(50.0);
      expect(pricing.cachedInputPerMillionUsd).toBeUndefined();

      const row = screen.getByText(id).closest("tr");
      expect(row).not.toBeNull();
      expect(within(row!).getByText("$10.000")).toBeInTheDocument();
      expect(within(row!).getByText("$50.000")).toBeInTheDocument();
    }
  });
});
