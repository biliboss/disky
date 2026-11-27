/**
 * History copy (REQ-DSCR-015), from the snapshot's History markup. The
 * engine supplies each row's detail and state; the screen only formats the
 * relative "when" and the sizes.
 */
import type { HistoryState } from "../../lib/contract";
import { formatBytes, relativeTime } from "../../lib/format";

export { restoreNotices } from "../../lib/restoreNotices";

export const HISTORY_TITLE = "History";

/** "<total> cleared · items stay restorable for 30 days" (A1). */
export function headerLine(totalBytes: number): string {
  return `${formatBytes(totalBytes)} cleared · items stay restorable for 30 days`;
}

/** No entries (A5, gap-fill). */
export const EMPTY = "Nothing cleaned yet.";

export const RESTORE = "Restore";
export const RESTORE_BUSY = "Restoring…";

/** States shown as a tag instead of the Restore button. */
export type TaggedState = Exclude<HistoryState, "restorable">;

/** The tag a non-restorable row shows (A3, A4); "Deleted" is a gap-fill. */
export const TAGS: Record<TaggedState, string> = {
  restored: "Restored",
  emptied: "Emptied",
  deleted: "Deleted",
};

/** Organic tag class per state: Restored is accent-2, the rest neutral (A3, A4). */
export const TAG_CLASS: Record<TaggedState, string> = {
  restored: "tag tag-accent-2",
  emptied: "tag tag-neutral",
  deleted: "tag tag-neutral",
};

/** "<when> · <detail>" (A2); "when" is relative, sentence-cased ("Just now"). */
export function rowMeta(ts: string, detail: string, now: number = Date.now()): string {
  const when = relativeTime(ts, now);
  return `${when.charAt(0).toUpperCase()}${when.slice(1)} · ${detail}`;
}

/** Accessible name of a row's Restore button. */
export function restoreLabel(title: string, ts: string, now: number = Date.now()): string {
  return `Restore ${title}, ${relativeTime(ts, now)}`;
}
