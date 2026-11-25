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
