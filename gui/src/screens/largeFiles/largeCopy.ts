/**
 * Large files copy (REQ-DSCR-013), from the snapshot's Large files markup.
 * Every size and date is an engine figure (`disky large`); this module only
 * words and formats them, and adds up the engine sizes of listed rows.
 */
import type { LargeFile, LargeFilter } from "../../lib/contract";
import { formatBytes, relativeTime } from "../../lib/format";

export const LARGE_TITLE = "Large & old files";
export const FILTER_GROUP = "Show";
export const FILTER_LABELS: Record<LargeFilter, string> = {
  all: "All",
  "over-10gb": "Over 10 GB",
  "unused-1y": "Unused 1 year+",
};
export const COL_NAME = "Name";
export const COL_LOCATION = "Location";
export const COL_OPENED = "Last opened";
export const COL_SIZE = "Size";
/** The check column's header, for screen readers only. */
export const COL_SELECT = "Select";
export const NOTE_NONE = "Select files to move them to the Trash.";
/** An empty list (gap-fill, REQ-DSCR-013.A5). */
export const EMPTY = "No files over 2 GB.";
export const NO_SCANS = "No scans yet";
export const NO_SCANS_NOTE = "Run a scan to find your largest files.";
export const START_SCAN = "Start scan";
/** A file whose snapshot predates access times (REQ-DSCR-004.A3; gap-fill). */
export const OPENED_UNKNOWN = "Unknown";
export const TRASH_BUSY = "Moving to Trash…";

/** Engine sizes of `rows` added up — no file is measured here. */
export function rowsBytes(rows: readonly Pick<LargeFile, "bytes">[]): number {
  return rows.reduce((acc, f) => acc + f.bytes, 0); // lint-boundary-allow: adds engine file sizes, measures no file
}

/** "<count> files · <total>" (REQ-DSCR-013.A1). */
export function kicker(rows: readonly Pick<LargeFile, "bytes">[]): string {
  const n = rows.length;
  return `${n} ${n === 1 ? "file" : "files"} · ${formatBytes(rowsBytes(rows))}`;
}

/** The footer note (REQ-DSCR-013.A3). */
export function selectionNote(selected: number): string {
  return selected > 0 ? `${selected} selected · restorable from History for 30 days` : NOTE_NONE;
}

/** "Move <selected total> to Trash" (REQ-DSCR-013.A4). */
export function trashLabel(selectedBytes: number): string {
  return `Move ${formatBytes(selectedBytes)} to Trash`;
}

/** "Last opened" as a relative time; null reads "Unknown". */
export function openedLabel(iso: string | null, now: number = Date.now()): string {
  return iso === null ? OPENED_UNKNOWN : relativeTime(iso, now);
}

/** The tag: accent when unused for over a year, neutral otherwise (REQ-DSCR-013.A2). */
export function openedTagClass(unused1y: boolean): string {
  return unused1y ? "tag tag-accent" : "tag tag-neutral";
}

/** A failed path's message, e.g. "Couldn't move to Trash: missing". */
export function failureMessage(reason: string): string {
  return `Couldn't move to Trash: ${reason}`;
}
