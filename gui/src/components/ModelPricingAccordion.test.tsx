import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ModelPricingAccordion, { PriceCell, formatPrice } from "./ModelPricingAccordion";
import { MODEL_PRICING } from "../config/modelPricing";
import { PROVIDER_MODELS } from "../modelCapabilities";

describe("ModelPricingAccordion", () => {
  function openPricing() {
    render(<ModelPricingAccordion />);
    fireEvent.click(screen.getByRole("button", { name: /model pricing/i }));
  }

  it("opens with click and keyboard interaction", () => {
    render(<ModelPricingAccordion />);
    const header = screen.getByRole("button", { name: /model pricing/i });

    expect(screen.queryByRole("columnheader", { name: "Input/1M" })).toBeNull();
    fireEvent.keyDown(header, { key: "Enter" });
    expect(screen.getByRole("columnheader", { name: "Input/1M" })).toBeInTheDocument();
    fireEvent.keyDown(header, { key: " " });
    expect(screen.queryByRole("columnheader", { name: "Input/1M" })).toBeNull();
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

  it("renders current and revised standard prices for Terra and Luna", () => {
    openPricing();

    expect(screen.getAllByText("$1.000")).toHaveLength(2);
    expect(screen.getAllByText("$2.000")).toHaveLength(2);
    expect(screen.getAllByText("$0.100")).toHaveLength(2);
    expect(screen.getAllByText("$0.200")).toHaveLength(2);
    expect(screen.getAllByText("$0.1000")).toHaveLength(2);
    expect(screen.getAllByText("$0.2000")).toHaveLength(2);
    expect(document.querySelectorAll("s")).toHaveLength(12);
  });

  it("renders all three regular prices in the Luna Pro row", () => {
    openPricing();

    const lunaProRow = screen
      .getByText("openai/gpt-5.6-luna-pro")
      .closest("tr");

    expect(lunaProRow).not.toBeNull();
    expect(within(lunaProRow!).getByText("$0.100")).toBeInTheDocument();
    expect(within(lunaProRow!).getByText("$0.200")).toBeInTheDocument();
    expect(within(lunaProRow!).getByText("$0.0100")).toBeInTheDocument();
    expect(within(lunaProRow!).getByText("$0.0200")).toBeInTheDocument();
    expect(lunaProRow!.querySelectorAll("s")).toHaveLength(3);
  });

  it("uses the production price catalog for the complete data test", () => {
    const ids = [
      "openai/gpt-5.6-sol",
      "openai/gpt-5.6-sol-pro",
      "openai/gpt-5.6-terra",
      "openai/gpt-5.6-terra-pro",
      "openai/gpt-5.6-luna",
      "openai/gpt-5.6-luna-pro",
    ] as const;

    for (const id of ids) {
      const pricing = MODEL_PRICING[id];
      const regular = [
        pricing.regularInputPerMillionUsd,
        pricing.regularOutputPerMillionUsd,
        pricing.regularCachedInputPerMillionUsd,
      ];

      expect(regular.some((value) => value !== undefined)).toBe(
        regular.every((value) => value !== undefined),
      );
    }
  });

  it("renders Sol once without a duplicate standard price", () => {
    openPricing();

    const solRows = screen.getAllByRole("row").filter((row) =>
      row.textContent?.includes("openai/gpt-5.6-sol"),
    );
    expect(solRows).toHaveLength(2);
    for (const row of solRows) {
      expect(row.querySelectorAll("s")).toHaveLength(0);
      expect(row.textContent).toContain("$5.000");
      expect(row.textContent).toContain("$30.000");
    }
  });

  it("provides localized screen-reader text for discounted prices", () => {
    openPricing();

    const texts = screen.getAllByText(
      "Current price $1.000, revised standard price $2.000",
    );
    expect(texts).toHaveLength(2);
    for (const text of texts) {
      expect(text).toHaveClass("sr-only");
    }
    expect(document.querySelectorAll('[aria-hidden="true"]')).not.toHaveLength(0);
  });

  it("renders the promotion and long-context notes", () => {
    openPricing();

    expect(screen.getAllByText(/Limited-time 50% provider discount/)).toHaveLength(4);
    expect(screen.getAllByText(/272K tokens or more/)).toHaveLength(6);
    expect(screen.getAllByText(/no discount/i)).toHaveLength(2);
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

      const regularValues = [
        standard.regularInputPerMillionUsd,
        standard.regularOutputPerMillionUsd,
        standard.regularCachedInputPerMillionUsd,
      ];
      expect(regularValues.some((value) => value !== undefined)).toBe(
        regularValues.every((value) => value !== undefined),
      );
      expect(pro.regularCachedInputPerMillionUsd).toBe(
        standard.regularCachedInputPerMillionUsd,
      );
    }

    expect(MODEL_PRICING["openai/gpt-5.6-sol"].regularInputPerMillionUsd).toBeUndefined();
    expect(MODEL_PRICING["openai/gpt-5.6-terra"].regularInputPerMillionUsd).toBe(2);
    expect(MODEL_PRICING["openai/gpt-5.6-luna"].regularInputPerMillionUsd).toBe(0.2);
  });
});
