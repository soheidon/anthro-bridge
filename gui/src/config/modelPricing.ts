export interface ModelPricing {
  inputPerMillionUsd: number;
  outputPerMillionUsd: number;
  cachedInputPerMillionUsd?: number;
  pricingNoteKey?: string;
  verifiedAt: string;
  sourceUrl: string;
}

export const MODEL_PRICING: Record<string, ModelPricing> = {
  // ── DeepSeek ──
  "deepseek-v4-pro": {
    inputPerMillionUsd: 0.435,
    outputPerMillionUsd: 0.87,
    cachedInputPerMillionUsd: 0.003625,
    pricingNoteKey: "modelPricing.notes.deepseekPeakValley",
    verifiedAt: "2026-07-17",
    sourceUrl: "https://api-docs.deepseek.com/quick_start/pricing",
  },
  "deepseek-v4-flash": {
    inputPerMillionUsd: 0.14,
    outputPerMillionUsd: 0.28,
    cachedInputPerMillionUsd: 0.0028,
    pricingNoteKey: "modelPricing.notes.deepseekPeakValley",
    verifiedAt: "2026-07-17",
    sourceUrl: "https://api-docs.deepseek.com/quick_start/pricing",
  },

  // ── MiMo ──
  "mimo-v2.5-pro": {
    inputPerMillionUsd: 0.435,
    outputPerMillionUsd: 0.87,
    cachedInputPerMillionUsd: 0.0036,
    verifiedAt: "2026-07-17",
    sourceUrl: "https://dev.mi.com/mimo/api/pricing",
  },
  "mimo-v2.5-pro-ultraspeed": {
    inputPerMillionUsd: 1.305,
    outputPerMillionUsd: 2.61,
    cachedInputPerMillionUsd: 0.0108,
    pricingNoteKey: "modelPricing.notes.mimoUltraSpeed",
    verifiedAt: "2026-07-18",
    sourceUrl: "https://mimo.mi.com/models/en-US/mimo-v2.5-pro-ultraspeed",
  },
  "mimo-v2.5": {
    inputPerMillionUsd: 0.14,
    outputPerMillionUsd: 0.28,
    cachedInputPerMillionUsd: 0.0028,
    verifiedAt: "2026-07-17",
    sourceUrl: "https://dev.mi.com/mimo/api/pricing",
  },

  // ── MiniMax ──
  "MiniMax-M3": {
    inputPerMillionUsd: 0.30,
    outputPerMillionUsd: 1.20,
    cachedInputPerMillionUsd: 0.06,
    pricingNoteKey: "modelPricing.notes.minimaxLongCtx",
    verifiedAt: "2026-07-17",
    sourceUrl: "https://www.minimaxi.com/document/price",
  },
  "MiniMax-M2.7": {
    inputPerMillionUsd: 0.30,
    outputPerMillionUsd: 1.20,
    cachedInputPerMillionUsd: 0.06,
    pricingNoteKey: "modelPricing.notes.minimaxLongCtx",
    verifiedAt: "2026-07-17",
    sourceUrl: "https://www.minimaxi.com/document/price",
  },
  "MiniMax-M2.7-highspeed": {
    inputPerMillionUsd: 0.60,
    outputPerMillionUsd: 2.40,
    cachedInputPerMillionUsd: 0.06,
    verifiedAt: "2026-07-17",
    sourceUrl: "https://www.minimaxi.com/document/price",
  },

  // ── Kimi / Moonshot ──
  "kimi-k3": {
    inputPerMillionUsd: 3.00,
    outputPerMillionUsd: 15.00,
    cachedInputPerMillionUsd: 0.30,
    pricingNoteKey: "modelPricing.notes.kimiK3",
    verifiedAt: "2026-07-17",
    sourceUrl: "https://platform.moonshot.cn/docs/pricing",
  },
  "kimi-k2.7-code": {
    inputPerMillionUsd: 0.95,
    outputPerMillionUsd: 4.00,
    cachedInputPerMillionUsd: 0.19,
    verifiedAt: "2026-07-17",
    sourceUrl: "https://platform.moonshot.cn/docs/pricing",
  },
  "kimi-k2.7-code-highspeed": {
    inputPerMillionUsd: 1.90,
    outputPerMillionUsd: 8.00,
    cachedInputPerMillionUsd: 0.38,
    pricingNoteKey: "modelPricing.notes.kimiK27HighSpeed",
    verifiedAt: "2026-07-17",
    sourceUrl: "https://platform.moonshot.cn/docs/pricing",
  },
  "kimi-k2.6": {
    inputPerMillionUsd: 0.95,
    outputPerMillionUsd: 4.00,
    cachedInputPerMillionUsd: 0.16,
    verifiedAt: "2026-07-17",
    sourceUrl: "https://platform.moonshot.cn/docs/pricing",
  },
};

export const PROVIDER_PRICE_ORDER = ["deepseek", "mimo", "minimax", "kimi"];
