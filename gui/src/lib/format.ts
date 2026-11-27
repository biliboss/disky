/**
 * Size formatting from exact bytes (REQ-DCLEAN-028): decimal units
 * (1 GB = 10^9, as Finder reports). ≥ 1 GB: one decimal below 100 GB
 * (trailing ".0" dropped, as the snapshot does), none at or above 100 GB.
 * Below 1 GB: whole MB, minimum "1 MB" for any non-zero value.
 */
/** Class for numeric cells: tabular figures, so totals align (REQ-DCLEAN-028.A4). */
export const NUMERIC_CLASS = "num";

const GB = 1e9;
const MB = 1e6;

export function formatBytes(bytes: number): string {
  if (bytes >= GB) {
    const gb = bytes / GB;
    return gb >= 100 ? `${Math.round(gb)} GB` : `${+gb.toFixed(1)} GB`;
  }
  if (bytes <= 0) return "0 MB";
  return `${Math.max(1, Math.round(bytes / MB))} MB`;
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function ago(n: number, unit: string): string {
  return `${n} ${unit}${n === 1 ? "" : "s"} ago`;
}

/**
 * Relative age of an RFC 3339 timestamp at `now` ("7 days ago"), for the idle
 * kicker (REQ-DCLEAN-003.A2). Whole units, rounded down; a future or
 * sub-minute timestamp reads "just now".
 */
export function relativeTime(iso: string, now: number = Date.now()): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "at an unknown time";
  const d = now - t;
  if (d < MINUTE) return "just now";
  if (d < HOUR) return ago(Math.floor(d / MINUTE), "minute");
  if (d < DAY) return ago(Math.floor(d / HOUR), "hour");
  if (d < 30 * DAY) return ago(Math.floor(d / DAY), "day");
  if (d < 365 * DAY) return ago(Math.floor(d / (30 * DAY)), "month");
  return ago(Math.floor(d / (365 * DAY)), "year");
}
