/** Get current UTC offset in minutes for a timezone at a given date. */
export function getTimezoneOffsetMinutes(date: Date, timeZone: string): number {
  try {
    const parts = new Intl.DateTimeFormat("en-US", {
      timeZone,
      year: "numeric", month: "2-digit", day: "2-digit",
      hour: "2-digit", minute: "2-digit", second: "2-digit",
      hour12: false,
    }).formatToParts(date);

    const get = (type: string) => Number(parts.find((p) => p.type === type)?.value ?? 0);
    const y = get("year");
    const mo = get("month") - 1;
    const d = get("day");
    const h = get("hour") === 24 ? 0 : get("hour");
    const mi = get("minute");

    const localAsUTC = Date.UTC(y, mo, d, h, mi, date.getUTCSeconds());
    return Math.round((localAsUTC - date.getTime()) / 60000);
  } catch {
    return 0;
  }
}

/** Format offsetMinutes into "UTC+HH:MM" form. */
export function formatUtcOffset(offsetMinutes: number): string {
  const sign = offsetMinutes >= 0 ? "+" : "-";
  const abs = Math.abs(offsetMinutes);
  const h = Math.floor(abs / 60);
  const m = abs % 60;
  return `UTC${sign}${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}
