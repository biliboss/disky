/**
 * Duplicates copy (REQ-DSCR-014), from the snapshot's Duplicates markup.
 * Every size is an engine figure (`disky dupes`); this module only words
 * and formats them, and adds up the engine's per-copy bytes of the copies
 * the user chose to remove.
 */
import type { DupeGroup } from "../../lib/contract";
import { formatBytes } from "../../lib/format";

export const DUPES_TITLE = "Duplicates";
export const KEEP = "Keep";
export const REMOVE = "Remove";
/** No groups (gap-fill, REQ-DSCR-014.A5). */
export const EMPTY = "No duplicates found.";
export const NO_SCANS = "No scans yet";
export const NO_SCANS_NOTE = "Run a scan to find duplicate files.";
export const START_SCAN = "Start scan";
export const REMOVE_BUSY = "Moving to Trash…";

/** "<n> groups · one copy of each is kept, you choose which" (REQ-DSCR-014.A1). */
export function kicker(groups: number): string {
  return `${groups} ${groups === 1 ? "group" : "groups"} · one copy of each is kept, you choose which`;
}

/** The card's "<size> each" tag (REQ-DSCR-014.A2). */
export function eachLabel(bytes: number): string {
  return `${formatBytes(bytes)} each`;
}

/** Copies not kept, with each group's engine bytes per copy. */
export function extraCopies(groups: readonly Pick<DupeGroup, "bytes" | "copies">[], kept: (g: number) => string) {
  return groups.flatMap((g, i) => g.copies.filter((c) => c.path !== kept(i)).map((c) => ({ path: c.path, bytes: g.bytes })));
}

/** Engine bytes of the copies to remove, added up — no file is measured here. */
export function freedBytes(extras: readonly { bytes: number }[]): number {
  return extras.reduce((acc, c) => acc + c.bytes, 0); // lint-boundary-allow: adds engine per-copy sizes, measures no file
}

/** "Frees <reclaimable> · removed copies go to the Trash" (REQ-DSCR-014.A4). */
export function footNote(bytes: number): string {
  return `Frees ${formatBytes(bytes)} · removed copies go to the Trash`;
}

/** "Remove <n> extra copies" (REQ-DSCR-014.A4). */
export function removeLabel(n: number): string {
  return `Remove ${n} extra ${n === 1 ? "copy" : "copies"}`;
}

/** A copy that failed to move, e.g. "Couldn't move ~/Downloads to Trash: missing". */
export function failureMessage(location: string, reason: string): string {
  return `Couldn't move ${location} to Trash: ${reason}`;
}
