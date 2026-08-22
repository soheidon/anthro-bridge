import { describe, it, expect } from "vitest";
import {
  getDeepSeekPricingStatus,
  getBeijingDate,
  isWeekendBeijing,
} from "./deepseekSchedule";

describe("deepseekSchedule weekend & peak-valley pricing", () => {
  it("computes Beijing date correctly across UTC midnight / day rollovers", () => {
    // 2026-08-22 20:00 UTC -> 2026-08-23 04:00 Beijing (Saturday UTC -> Sunday Beijing)
    const d1 = new Date("2026-08-22T20:00:00Z");
    const bj1 = getBeijingDate(d1);
    expect(bj1.getUTCFullYear()).toBe(2026);
    expect(bj1.getUTCMonth()).toBe(7); // August (0-indexed)
    expect(bj1.getUTCDate()).toBe(23);
    expect(bj1.getUTCHours()).toBe(4);
    expect(isWeekendBeijing(d1)).toBe(true);

    // 2026-08-21 15:59 UTC -> 2026-08-21 23:59 Beijing (Friday)
    const d2 = new Date("2026-08-21T15:59:00Z");
    expect(isWeekendBeijing(d2)).toBe(false);

    // 2026-08-21 16:00 UTC -> 2026-08-22 00:00 Beijing (Saturday)
    const d3 = new Date("2026-08-21T16:00:00Z");
    expect(isWeekendBeijing(d3)).toBe(true);

    // 2026-08-23 15:59 UTC -> 2026-08-23 23:59 Beijing (Sunday)
    const d4 = new Date("2026-08-23T15:59:00Z");
    expect(isWeekendBeijing(d4)).toBe(true);

    // 2026-08-23 16:00 UTC -> 2026-08-24 00:00 Beijing (Monday)
    const d5 = new Date("2026-08-23T16:00:00Z");
    expect(isWeekendBeijing(d5)).toBe(false);
  });

  it("handles weekend off-peak rule effective boundary (2026-08-22 16:00:00 UTC)", () => {
    // 1 second before effective date (2026-08-22 15:59:59 UTC = Beijing Sat 23:59:59)
    // Outside peak window (15:59 UTC is not 01:00-04:00 or 06:00-10:00) -> VALLEY by normal weekday schedule
    const beforeEffective = new Date("2026-08-22T15:59:59Z");
    const statusBefore = getDeepSeekPricingStatus(beforeEffective);
    expect(statusBefore.period.type).toBe("VALLEY");

    // Saturday BEFORE effective date during standard peak window (2026-08-22 02:00:00 UTC = Beijing Sat 10:00:00)
    // Because weekend rule is not yet effective, standard weekday/weekend peak applies -> PEAK
    const saturdayBeforeEffectivePeak = new Date("2026-08-22T02:00:00Z");
    const statusSatBefore = getDeepSeekPricingStatus(saturdayBeforeEffectivePeak);
    expect(statusSatBefore.period.type).toBe("PEAK");
    expect(statusSatBefore.period.startMinuteUTC).toBe(60);
    expect(statusSatBefore.period.endMinuteUTC).toBe(240);

    // Exactly at effective date: 2026-08-22 16:00:00 UTC (Beijing Sun 00:00:00)
    const atEffective = new Date("2026-08-22T16:00:00Z");
    const statusAt = getDeepSeekPricingStatus(atEffective);
    expect(statusAt.period.type).toBe("VALLEY");
    expect(statusAt.period.startMinuteUTC).toBe(0);
    expect(statusAt.period.endMinuteUTC).toBe(1440);
  });

  it("overrides weekday peak windows on weekends after effective date (Sunday and subsequent Saturday)", () => {
    // Sunday 2026-08-23 02:00:00 UTC (Beijing Sun 10:00:00).
    // On standard schedule, 01:00–04:00 UTC is PEAK.
    // On weekends after effective date, it must be VALLEY.
    const sundayPeakTime = new Date("2026-08-23T02:00:00Z");
    const statusSunday = getDeepSeekPricingStatus(sundayPeakTime);
    expect(statusSunday.period.type).toBe("VALLEY");
    expect(statusSunday.period.startMinuteUTC).toBe(0);
    expect(statusSunday.period.endMinuteUTC).toBe(1440);

    // Sunday 2026-08-23 07:00:00 UTC (Beijing Sun 15:00:00).
    // On standard schedule, 06:00–10:00 UTC is PEAK.
    // On weekends after effective date, it must be VALLEY.
    const sundaySecondPeakTime = new Date("2026-08-23T07:00:00Z");
    const statusSunday2 = getDeepSeekPricingStatus(sundaySecondPeakTime);
    expect(statusSunday2.period.type).toBe("VALLEY");

    // Saturday 2026-08-29 02:00:00 UTC (Beijing Sat 10:00:00).
    // First Saturday AFTER the weekend rule took effect. Must be full-day VALLEY.
    const saturdayAfterEffectivePeak = new Date("2026-08-29T02:00:00Z");
    const statusSatAfter = getDeepSeekPricingStatus(saturdayAfterEffectivePeak);
    expect(statusSatAfter.period.type).toBe("VALLEY");
    expect(statusSatAfter.period.startMinuteUTC).toBe(0);
    expect(statusSatAfter.period.endMinuteUTC).toBe(1440);
  });

  it("resumes normal peak/valley schedule on Monday (Beijing Time)", () => {
    // Monday 2026-08-24 00:30:00 UTC (Beijing Mon 08:30:00) -> outside peak (VALLEY)
    const mondayOffPeak = new Date("2026-08-24T00:30:00Z");
    const statusMonOff = getDeepSeekPricingStatus(mondayOffPeak);
    expect(statusMonOff.period.type).toBe("VALLEY");

    // Monday 2026-08-24 02:00:00 UTC (Beijing Mon 10:00:00) -> inside peak (PEAK)
    const mondayPeak1 = new Date("2026-08-24T02:00:00Z");
    const statusMonPeak1 = getDeepSeekPricingStatus(mondayPeak1);
    expect(statusMonPeak1.period.type).toBe("PEAK");
    expect(statusMonPeak1.period.startMinuteUTC).toBe(60);
    expect(statusMonPeak1.period.endMinuteUTC).toBe(240);

    // Monday 2026-08-24 07:00:00 UTC (Beijing Mon 15:00:00) -> inside peak (PEAK)
    const mondayPeak2 = new Date("2026-08-24T07:00:00Z");
    const statusMonPeak2 = getDeepSeekPricingStatus(mondayPeak2);
    expect(statusMonPeak2.period.type).toBe("PEAK");
    expect(statusMonPeak2.period.startMinuteUTC).toBe(360);
    expect(statusMonPeak2.period.endMinuteUTC).toBe(600);
  });
});
