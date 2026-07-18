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
  peakRangesUtc: ReadonlyArray<{
    startMinuteUTC: number;
    endMinuteUTC: number;
  }>;
}

export const DEEPSEEK_PRICING_SCHEDULE: DeepSeekPricingSchedule = {
  effectiveFromUtc: null,
  peakRangesUtc: [
    { startMinuteUTC: 60, endMinuteUTC: 240 },   // 01:00–04:00
    { startMinuteUTC: 360, endMinuteUTC: 600 },   // 06:00–10:00
  ],
};

/**
 * Determine current DeepSeek pricing period from a UTC Date.
 * Boundary: start inclusive, end exclusive.
 */
export function getDeepSeekPricingStatus(utcDate: Date): DeepSeekPricingStatus {
  const minutes = utcDate.getUTCHours() * 60 + utcDate.getUTCMinutes();
  const schedule = DEEPSEEK_PRICING_SCHEDULE;

  const isEffective =
    schedule.effectiveFromUtc == null ||
    utcDate >= new Date(schedule.effectiveFromUtc);

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
  label: string;        // Display label, e.g. "JST（日本・東京）"
  group: string;        // Group heading, e.g. "日本・東アジア"
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
  // 主要
  { id: "Asia/Tokyo",        label: "JST（日本・東京）",        group: "主要" },
  { id: "Asia/Shanghai",     label: "CST（中国・上海）",        group: "主要" },
  { id: "Asia/Seoul",        label: "KST（韓国・ソウル）",       group: "主要" },
  { id: "Asia/Singapore",    label: "SGT（シンガポール）",       group: "主要" },
  { id: "Asia/Kolkata",      label: "IST（インド・コルカタ）",    group: "主要" },
  { id: "Europe/London",     label: "GMT／BST（英国・ロンドン）",  group: "主要" },
  { id: "Europe/Paris",      label: "CET／CEST（フランス・パリ）", group: "主要" },
  { id: "America/New_York",  label: "ET（米国東部・ニューヨーク）", group: "主要" },
  { id: "America/Chicago",   label: "CT（米国中部・シカゴ）",     group: "主要" },
  { id: "America/Los_Angeles", label: "PT（米国西部・ロサンゼルス）", group: "主要" },
  { id: "Australia/Sydney",  label: "AEST／AEDT（豪州・シドニー）", group: "主要" },
  { id: "UTC",               label: "UTC（協定世界時）",         group: "主要" },
  // 日本・東アジア
  { id: "Asia/Hong_Kong",    label: "HKT（香港）",             group: "日本・東アジア" },
  { id: "Asia/Taipei",       label: "TST（台湾・台北）",        group: "日本・東アジア" },
  // 東南アジア・南アジア
  { id: "Asia/Bangkok",      label: "ICT（タイ・バンコク）",     group: "東南アジア・南アジア" },
  { id: "Asia/Jakarta",      label: "WIB（インドネシア・ジャカルタ）", group: "東南アジア・南アジア" },
  { id: "Asia/Manila",       label: "PHT（フィリピン・マニラ）",   group: "東南アジア・南アジア" },
  { id: "Asia/Kuala_Lumpur", label: "MYT（マレーシア・クアラルンプール）", group: "東南アジア・南アジア" },
  { id: "Asia/Kathmandu",    label: "NPT（ネパール・カトマンズ）", group: "東南アジア・南アジア" },
  { id: "Asia/Karachi",      label: "PKT（パキスタン・カラチ）",  group: "東南アジア・南アジア" },
  { id: "Asia/Dhaka",        label: "BST（バングラデシュ・ダッカ）", group: "東南アジア・南アジア" },
  // 中東
  { id: "Asia/Dubai",        label: "GST（UAE・ドバイ）",       group: "中東" },
  { id: "Asia/Riyadh",       label: "AST（サウジアラビア・リヤド）", group: "中東" },
  { id: "Europe/Istanbul",   label: "TRT（トルコ・イスタンブール）", group: "中東" },
  { id: "Asia/Jerusalem",    label: "IDT（イスラエル・エルサレム）", group: "中東" },
  // 欧州
  { id: "Europe/Lisbon",     label: "WET／WEST（ポルトガル・リスボン）", group: "欧州" },
  { id: "Europe/Berlin",     label: "CET／CEST（ドイツ・ベルリン）", group: "欧州" },
  { id: "Europe/Rome",       label: "CET／CEST（イタリア・ローマ）", group: "欧州" },
  { id: "Europe/Helsinki",   label: "EET／EEST（フィンランド・ヘルシンキ）", group: "欧州" },
  { id: "Europe/Athens",     label: "EET／EEST（ギリシャ・アテネ）", group: "欧州" },
  { id: "Europe/Moscow",     label: "MSK（ロシア・モスクワ）",    group: "欧州" },
  // 北米
  { id: "America/Denver",    label: "MT（米国山岳部・デンバー）",  group: "北米" },
  { id: "America/Phoenix",   label: "MST（米国・アリゾナ）",     group: "北米" },
  { id: "America/Anchorage", label: "AKT（米国・アラスカ）",     group: "北米" },
  { id: "Pacific/Honolulu",  label: "HST（米国・ハワイ）",       group: "北米" },
  { id: "America/Halifax",   label: "AT（カナダ大西洋・ハリファックス）", group: "北米" },
  { id: "America/St_Johns",  label: "NST／NDT（カナダ・ニューファンドランド）", group: "北米" },
  { id: "America/Toronto",   label: "ET（カナダ・トロント）",     group: "北米" },
  { id: "America/Vancouver", label: "PT（カナダ・バンクーバー）",  group: "北米" },
  { id: "America/Mexico_City", label: "CST（メキシコ・メキシコシティ）", group: "北米" },
  // 中南米
  { id: "America/Sao_Paulo",                label: "BRT（ブラジル・サンパウロ）",      group: "中南米" },
  { id: "America/Argentina/Buenos_Aires",   label: "ART（アルゼンチン・ブエノスアイレス）", group: "中南米" },
  { id: "America/Santiago",                 label: "CLT／CLST（チリ・サンティアゴ）",  group: "中南米" },
  { id: "America/Bogota",                   label: "COT（コロンビア・ボゴタ）",       group: "中南米" },
  { id: "America/Lima",                     label: "PET（ペルー・リマ）",          group: "中南米" },
  // オセアニア
  { id: "Australia/Brisbane",  label: "AEST（豪州・ブリスベン）",          group: "オセアニア" },
  { id: "Australia/Adelaide",  label: "ACST／ACDT（豪州・アデレード）",    group: "オセアニア" },
  { id: "Australia/Perth",     label: "AWST（豪州・パース）",             group: "オセアニア" },
  { id: "Pacific/Auckland",    label: "NZST／NZDT（ニュージーランド・オークランド）", group: "オセアニア" },
  // アフリカ
  { id: "Africa/Johannesburg", label: "SAST（南アフリカ・ヨハネスブルグ）", group: "アフリカ" },
  { id: "Africa/Nairobi",      label: "EAT（ケニア・ナイロビ）",         group: "アフリカ" },
  { id: "Africa/Lagos",        label: "WAT（ナイジェリア・ラゴス）",      group: "アフリカ" },
  { id: "Africa/Cairo",        label: "EET（エジプト・カイロ）",         group: "アフリカ" },
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

/** Get default timezone from browser/OS. */
export function getLocalTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}
