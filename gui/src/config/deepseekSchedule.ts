// DeepSeek peak-valley pricing schedule
// Official: https://api-docs.deepseek.com/quick_start/pricing
// Peak: 01:00–04:00 UTC, 06:00–10:00 UTC  (all billing items ×2)
// Valley: everything else

import { getTimezoneOffsetMinutes as _getTimezoneOffsetMinutes } from "../utils/timezone";
export { _getTimezoneOffsetMinutes as getTimezoneOffsetMinutes };

export type DeepSeekPricingPeriodType = "PEAK" | "VALLEY";

export interface DeepSeekPricingPeriod {
  type: DeepSeekPricingPeriodType;
  startMinuteUTC: number;
  endMinuteUTC: number;
  crossesMidnightUTC: boolean;
}

export interface DeepSeekPricingStatus {
  period: DeepSeekPricingPeriod;
  isEffective: boolean;
}

export interface DeepSeekPricingSchedule {
  /** ISO 8601 UTC string. null = effective date not yet announced. */
  effectiveFromUtc: string | null;
  /** ISO 8601 UTC string for weekend all-day off-peak rule. 2026-08-22T16:00:00Z = 2026-08-23 00:00:00 Beijing Time. */
  weekendAllDayValleyEffectiveFromUtc: string | null;
  peakRangesUtc: ReadonlyArray<{
    startMinuteUTC: number;
    endMinuteUTC: number;
  }>;
}

export const DEEPSEEK_PRICING_SCHEDULE: DeepSeekPricingSchedule = {
  effectiveFromUtc: null,
  weekendAllDayValleyEffectiveFromUtc: "2026-08-22T16:00:00Z",
  peakRangesUtc: [
    { startMinuteUTC: 60, endMinuteUTC: 240 },   // 01:00–04:00 UTC (09:00–12:00 Beijing)
    { startMinuteUTC: 360, endMinuteUTC: 600 },  // 06:00–10:00 UTC (14:00–18:00 Beijing)
  ],
};

/** Get Beijing Time (Asia/Shanghai, UTC+8) date object with safe day rollover. */
export function getBeijingDate(utcDate: Date): Date {
  return new Date(utcDate.getTime() + 8 * 60 * 60 * 1000);
}

/** Check if the given date is weekend (Saturday or Sunday) in Beijing Time. */
export function isWeekendBeijing(utcDate: Date): boolean {
  const beijingDate = getBeijingDate(utcDate);
  const day = beijingDate.getUTCDay();
  return day === 0 || day === 6;
}

/**
 * Determine current DeepSeek pricing period from a UTC Date.
 * Boundary: start inclusive, end exclusive.
 */
export function getDeepSeekPricingStatus(utcDate: Date): DeepSeekPricingStatus {
  const schedule = DEEPSEEK_PRICING_SCHEDULE;

  const isEffective =
    schedule.effectiveFromUtc == null ||
    utcDate >= new Date(schedule.effectiveFromUtc);

  const isWeekendOverride =
    schedule.weekendAllDayValleyEffectiveFromUtc != null &&
    utcDate >= new Date(schedule.weekendAllDayValleyEffectiveFromUtc) &&
    isWeekendBeijing(utcDate);

  if (isWeekendOverride) {
    return {
      period: {
        type: "VALLEY",
        startMinuteUTC: 0,
        endMinuteUTC: 1440,
        crossesMidnightUTC: false,
      },
      isEffective,
    };
  }

  const minutes = utcDate.getUTCHours() * 60 + utcDate.getUTCMinutes();

  for (const range of schedule.peakRangesUtc) {
    if (minutes >= range.startMinuteUTC && minutes < range.endMinuteUTC) {
      const crosses = range.endMinuteUTC <= range.startMinuteUTC;
      return {
        period: {
          type: "PEAK",
          startMinuteUTC: range.startMinuteUTC,
          endMinuteUTC: range.endMinuteUTC,
          crossesMidnightUTC: crosses,
        },
        isEffective,
      };
    }
  }

  // Valley ranges
  let valleyStart: number;
  let valleyEnd: number;
  let valleyCrosses = false;

  if (minutes >= 600) {
    valleyStart = 600;
    valleyEnd = 60;
    valleyCrosses = true;
  } else if (minutes >= 240) {
    valleyStart = 240;
    valleyEnd = 360;
  } else {
    valleyStart = 600;
    valleyEnd = 60;
    valleyCrosses = true;
  }

  return {
    period: {
      type: "VALLEY",
      startMinuteUTC: valleyStart,
      endMinuteUTC: valleyEnd,
      crossesMidnightUTC: valleyCrosses,
    },
    isEffective,
  };
}

// ── IANA timezone entry ──

export interface TimezoneOption {
  id: string;           // IANA timezone ID, e.g. "Asia/Tokyo"
  groupKey: string;     // Group key, e.g. "major"
  labelKey: string;     // Label key, e.g. "Asia/Tokyo"
  label?: string;       // Optional legacy fallback label
  group?: string;       // Optional legacy fallback group
}

// Fallback abbreviation map for zones where Intl returns "GMT+N" style
const ABBREV_OVERRIDES: Record<string, string> = {
  "Asia/Tokyo": "JST",
  "Asia/Seoul": "KST",
  "Asia/Shanghai": "CST",
  "Asia/Hong_Kong": "HKT",
  "Asia/Taipei": "TST",
  "Asia/Singapore": "SGT",
  "Asia/Bangkok": "ICT",
  "Asia/Jakarta": "WIB",
  "Asia/Manila": "PHT",
  "Asia/Kuala_Lumpur": "MYT",
  "Asia/Kolkata": "IST",
  "Asia/Kathmandu": "NPT",
  "Asia/Karachi": "PKT",
  "Asia/Dhaka": "BST",
  "Asia/Dubai": "GST",
  "Asia/Riyadh": "AST",
  "Europe/Istanbul": "TRT",
  "Asia/Jerusalem": "IDT",
  "Europe/London": "GMT",
  "Europe/Lisbon": "WET",
  "Europe/Paris": "CET",
  "Europe/Berlin": "CET",
  "Europe/Rome": "CET",
  "Europe/Helsinki": "EET",
  "Europe/Athens": "EET",
  "Europe/Moscow": "MSK",
  "America/New_York": "ET",
  "America/Chicago": "CT",
  "America/Denver": "MT",
  "America/Phoenix": "MST",
  "America/Los_Angeles": "PT",
  "America/Anchorage": "AKT",
  "Pacific/Honolulu": "HST",
  "America/Halifax": "AT",
  "America/St_Johns": "NT",
  "America/Toronto": "ET",
  "America/Vancouver": "PT",
  "America/Mexico_City": "CST",
  "America/Sao_Paulo": "BRT",
  "America/Argentina/Buenos_Aires": "ART",
  "America/Santiago": "CLT",
  "America/Bogota": "COT",
  "America/Lima": "PET",
  "Australia/Sydney": "AEST",
  "Australia/Brisbane": "AEST",
  "Australia/Adelaide": "ACST",
  "Australia/Perth": "AWST",
  "Pacific/Auckland": "NZST",
  "Africa/Johannesburg": "SAST",
  "Africa/Nairobi": "EAT",
  "Africa/Lagos": "WAT",
  "Africa/Cairo": "EET",
};

/** All timezone options grouped for settings selector. */
export const TIMEZONE_OPTIONS: TimezoneOption[] = [
  // 主要 (Major)
  { id: "Asia/Tokyo",        groupKey: "major", labelKey: "Asia/Tokyo",        label: "JST（日本・東京）",        group: "主要" },
  { id: "Asia/Shanghai",     groupKey: "major", labelKey: "Asia/Shanghai",     label: "CST（中国・上海）",        group: "主要" },
  { id: "Asia/Seoul",        groupKey: "major", labelKey: "Asia/Seoul",        label: "KST（韓国・ソウル）",       group: "主要" },
  { id: "Asia/Singapore",    groupKey: "major", labelKey: "Asia/Singapore",    label: "SGT（シンガポール）",       group: "主要" },
  { id: "Asia/Kolkata",      groupKey: "major", labelKey: "Asia/Kolkata",      label: "IST（インド・コルカタ）",    group: "主要" },
  { id: "Europe/London",     groupKey: "major", labelKey: "Europe/London",     label: "GMT／BST（英国・ロンドン）",  group: "主要" },
  { id: "Europe/Paris",      groupKey: "major", labelKey: "Europe/Paris",      label: "CET／CEST（フランス・パリ）", group: "主要" },
  { id: "America/New_York",  groupKey: "major", labelKey: "America/New_York",  label: "ET（米国東部・ニューヨーク）", group: "主要" },
  { id: "America/Chicago",   groupKey: "major", labelKey: "America/Chicago",   label: "CT（米国中部・シカゴ）",     group: "主要" },
  { id: "America/Los_Angeles", groupKey: "major", labelKey: "America/Los_Angeles", label: "PT（米国西部・ロサンゼルス）", group: "主要" },
  { id: "Australia/Sydney",  groupKey: "major", labelKey: "Australia/Sydney",  label: "AEST／AEDT（豪州・シドニー）", group: "主要" },
  { id: "UTC",               groupKey: "major", labelKey: "UTC",               label: "UTC（協定世界時）",         group: "主要" },
  // 日本・東アジア (East Asia)
  { id: "Asia/Hong_Kong",    groupKey: "eastAsia", labelKey: "Asia/Hong_Kong", label: "HKT（香港）",             group: "日本・東アジア" },
  { id: "Asia/Taipei",       groupKey: "eastAsia", labelKey: "Asia/Taipei",    label: "TST（台湾・台北）",        group: "日本・東アジア" },
  // 東南アジア・南アジア (South & Southeast Asia)
  { id: "Asia/Bangkok",      groupKey: "southEastAsia", labelKey: "Asia/Bangkok",      label: "ICT（タイ・バンコク）",     group: "東南アジア・南アジア" },
  { id: "Asia/Jakarta",      groupKey: "southEastAsia", labelKey: "Asia/Jakarta",      label: "WIB（インドネシア・ジャカルタ）", group: "東南アジア・南アジア" },
  { id: "Asia/Manila",       groupKey: "southEastAsia", labelKey: "Asia/Manila",       label: "PHT（フィリピン・マニラ）",   group: "東南アジア・南アジア" },
  { id: "Asia/Kuala_Lumpur", groupKey: "southEastAsia", labelKey: "Asia/Kuala_Lumpur", label: "MYT（マレーシア・クアラルンプール）", group: "東南アジア・南アジア" },
  { id: "Asia/Kathmandu",    groupKey: "southEastAsia", labelKey: "Asia/Kathmandu",    label: "NPT（ネパール・カトマンズ）", group: "東南アジア・南アジア" },
  { id: "Asia/Karachi",      groupKey: "southEastAsia", labelKey: "Asia/Karachi",      label: "PKT（パキスタン・カラチ）",  group: "東南アジア・南アジア" },
  { id: "Asia/Dhaka",        groupKey: "southEastAsia", labelKey: "Asia/Dhaka",        label: "BST（バングラデシュ・ダッカ）", group: "東南アジア・南アジア" },
  // 中東 (Middle East)
  { id: "Asia/Dubai",        groupKey: "middleEast", labelKey: "Asia/Dubai",        label: "GST（UAE・ドバイ）",       group: "中東" },
  { id: "Asia/Riyadh",       groupKey: "middleEast", labelKey: "Asia/Riyadh",       label: "AST（サウジアラビア・リヤド）", group: "中東" },
  { id: "Europe/Istanbul",   groupKey: "middleEast", labelKey: "Europe/Istanbul",   label: "TRT（トルコ・イスタンブール）", group: "中東" },
  { id: "Asia/Jerusalem",    groupKey: "middleEast", labelKey: "Asia/Jerusalem",    label: "IDT（イスラエル・エルサレム）", group: "中東" },
  // 欧州 (Europe)
  { id: "Europe/Lisbon",     groupKey: "europe", labelKey: "Europe/Lisbon",     label: "WET／WEST（ポルトガル・リスボン）", group: "欧州" },
  { id: "Europe/Berlin",     groupKey: "europe", labelKey: "Europe/Berlin",     label: "CET／CEST（ドイツ・ベルリン）", group: "欧州" },
  { id: "Europe/Rome",       groupKey: "europe", labelKey: "Europe/Rome",       label: "CET／CEST（イタリア・ローマ）", group: "欧州" },
  { id: "Europe/Helsinki",   groupKey: "europe", labelKey: "Europe/Helsinki",   label: "EET／EEST（フィンランド・ヘルシンキ）", group: "欧州" },
  { id: "Europe/Athens",     groupKey: "europe", labelKey: "Europe/Athens",     label: "EET／EEST（ギリシャ・アテネ）", group: "欧州" },
  { id: "Europe/Moscow",     groupKey: "europe", labelKey: "Europe/Moscow",     label: "MSK（ロシア・モスクワ）",    group: "欧州" },
  // 北米 (North America)
  { id: "America/Denver",    groupKey: "northAmerica", labelKey: "America/Denver",    label: "MT（米国山岳部・デンバー）",  group: "北米" },
  { id: "America/Phoenix",   groupKey: "northAmerica", labelKey: "America/Phoenix",   label: "MST（米国・アリゾナ）",     group: "北米" },
  { id: "America/Anchorage", groupKey: "northAmerica", labelKey: "America/Anchorage", label: "AKT（米国・アラスカ）",     group: "北米" },
  { id: "Pacific/Honolulu",  groupKey: "northAmerica", labelKey: "Pacific/Honolulu",  label: "HST（米国・ハワイ）",       group: "北米" },
  { id: "America/Halifax",   groupKey: "northAmerica", labelKey: "America/Halifax",   label: "AT（カナダ大西洋・ハリファックス）", group: "北米" },
  { id: "America/St_Johns",  groupKey: "northAmerica", labelKey: "America/St_Johns",  label: "NST／NDT（カナダ・ニューファンドランド）", group: "北米" },
  { id: "America/Toronto",   groupKey: "northAmerica", labelKey: "America/Toronto",   label: "ET（カナダ・トロント）",     group: "北米" },
  { id: "America/Vancouver", groupKey: "northAmerica", labelKey: "America/Vancouver", label: "PT（カナダ・バンクーバー）",  group: "北米" },
  { id: "America/Mexico_City", groupKey: "northAmerica", labelKey: "America/Mexico_City", label: "CST（メキシコ・メキシコシティ）", group: "北米" },
  // 中南米 (Latin America)
  { id: "America/Sao_Paulo",                groupKey: "latinAmerica", labelKey: "America/Sao_Paulo",                label: "BRT（ブラジル・サンパウロ）",      group: "中南米" },
  { id: "America/Argentina/Buenos_Aires",   groupKey: "latinAmerica", labelKey: "America/Argentina/Buenos_Aires",   label: "ART（アルゼンチン・ブエノスアイレス）", group: "中南米" },
  { id: "America/Santiago",                 groupKey: "latinAmerica", labelKey: "America/Santiago",                 label: "CLT／CLST（チリ・サンティアゴ）",  group: "中南米" },
  { id: "America/Bogota",                   groupKey: "latinAmerica", labelKey: "America/Bogota",                   label: "COT（コロンビア・ボゴタ）",       group: "中南米" },
  { id: "America/Lima",                     groupKey: "latinAmerica", labelKey: "America/Lima",                     label: "PET（ペルー・リマ）",          group: "中南米" },
  // オセアニア (Oceania)
  { id: "Australia/Brisbane",  groupKey: "oceania", labelKey: "Australia/Brisbane",  label: "AEST（豪州・ブリスベン）",          group: "オセアニア" },
  { id: "Australia/Adelaide",  groupKey: "oceania", labelKey: "Australia/Adelaide",  label: "ACST／ACDT（豪州・アデレード）",    group: "オセアニア" },
  { id: "Australia/Perth",     groupKey: "oceania", labelKey: "Australia/Perth",     label: "AWST（豪州・パース）",             group: "オセアニア" },
  { id: "Pacific/Auckland",    groupKey: "oceania", labelKey: "Pacific/Auckland",    label: "NZST／NZDT（ニュージーランド・オークランド）", group: "オセアニア" },
  // アフリカ (Africa)
  { id: "Africa/Johannesburg", groupKey: "africa", labelKey: "Africa/Johannesburg", label: "SAST（南アフリカ・ヨハネスブルグ）", group: "アフリカ" },
  { id: "Africa/Nairobi",      groupKey: "africa", labelKey: "Africa/Nairobi",      label: "EAT（ケニア・ナイロビ）",         group: "アフリカ" },
  { id: "Africa/Lagos",        groupKey: "africa", labelKey: "Africa/Lagos",        label: "WAT（ナイジェリア・ラゴス）",      group: "アフリカ" },
  { id: "Africa/Cairo",        groupKey: "africa", labelKey: "Africa/Cairo",        label: "EET（エジプト・カイロ）",         group: "アフリカ" },
];

/** Get short timezone abbreviation from Intl, with fallback to ABBREV_OVERRIDES. */
export function getTimezoneAbbrev(date: Date, timeZone: string, locale: string): string {
  try {
    const parts = new Intl.DateTimeFormat(locale, {
      timeZone,
      timeZoneName: "short",
    }).formatToParts(date);
    const tzPart = parts.find((p) => p.type === "timeZoneName");
    const value = tzPart?.value ?? timeZone;
    // If we have an override, always use it (avoids Intl returning full names in some locales)
    if (ABBREV_OVERRIDES[timeZone]) {
      return ABBREV_OVERRIDES[timeZone];
    }
    // If Intl returns "GMT+N" style and no override, return as-is
    return value;
  } catch {
    return ABBREV_OVERRIDES[timeZone] ?? timeZone;
  }
}


/** Format a UTC minute-of-day into HH:MM for a given timezone offset. */
export function formatMinute(minuteOfDay: number, offsetMinutes: number): string {
  const adjusted = ((minuteOfDay + offsetMinutes) % 1440 + 1440) % 1440;
  const hh = Math.floor(adjusted / 60).toString().padStart(2, "0");
  const mm = (adjusted % 60).toString().padStart(2, "0");
  return `${hh}:${mm}`;
}

export interface FormattedPricingRange {
  startLabel: string;
  endLabel: string;
  crossesMidnight: boolean;
  tzAbbrev: string;
}

export function formatDeepSeekPricingRange(
  period: DeepSeekPricingPeriod,
  date: Date,
  timeZone: string,
  locale: string,
): FormattedPricingRange {
  const offsetMin = _getTimezoneOffsetMinutes(date, timeZone);
  const startLabel = formatMinute(period.startMinuteUTC, offsetMin);
  const endLabel = formatMinute(period.endMinuteUTC, offsetMin);

  const startAbs = (period.startMinuteUTC + offsetMin + 2880) % 1440;
  const endAbs = (period.endMinuteUTC + offsetMin + 2880) % 1440;
  const displayCrossesMidnight = period.crossesMidnightUTC || endAbs <= startAbs;

  const tzAbbrev = getTimezoneAbbrev(date, timeZone, locale);

  return { startLabel, endLabel, crossesMidnight: displayCrossesMidnight, tzAbbrev };
}

/**
 * Build a user-locale peak-hours note string using the schedule's peak ranges
 * and the configured display timezone. Returns e.g.
 * "10:00～13:00、15:00～19:00（JST・UTC+09:00）"
 */
export function formatDeepSeekPeakHoursLabel(
  date: Date,
  timeZone: string,
  locale: string,
): string {
  const schedule = DEEPSEEK_PRICING_SCHEDULE;
  const tzAbbrev = getTimezoneAbbrev(date, timeZone, locale);
  const offsetMin = _getTimezoneOffsetMinutes(date, timeZone);
  const offsetStr = formatTimezoneOffset(offsetMin);

  const parts = schedule.peakRangesUtc.map((range) => {
    const s = formatMinute(range.startMinuteUTC, offsetMin);
    const e = formatMinute(range.endMinuteUTC, offsetMin);
    return `${s}～${e}`;
  });

  return `${parts.join("、")}（${tzAbbrev}・${offsetStr}）`;
}

/** Format a UTC offset in minutes as "UTC+09:00" style. */
function formatTimezoneOffset(offsetMinutes: number): string {
  const sign = offsetMinutes >= 0 ? "+" : "-";
  const abs = Math.abs(offsetMinutes);
  const hh = Math.floor(abs / 60).toString().padStart(2, "0");
  const mm = (abs % 60).toString().padStart(2, "0");
  return `UTC${sign}${hh}:${mm}`;
}

/** Get default timezone from browser/OS. */
export function getLocalTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}
