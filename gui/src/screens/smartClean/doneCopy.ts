/**
 * Done-phase copy (REQ-DCLEAN-009) and the clean / undo outcomes it reports
 * (REQ-DCLEAN-018.A3, REQ-DCLEAN-009.A7).
 */
import { DELETE_MODE, type CleanMode, type Failure, type RestoreReport } from "../../lib/contract";
import { formatBytes } from "../../lib/format";
import { restoreNotices } from "../../lib/restoreNotices";

/** "<freed> cleared": the physical bytes actually reclaimed (A1). */
export function doneHeading(freedBytes: number): string {
  return `${formatBytes(freedBytes)} cleared`;
}

/**
 * The note under the heading (A2).
 *
 * @implements REQ-DCLEAN-009
 */
export function doneNote(mode: CleanMode): string {
  return mode === DELETE_MODE
    ? "Those files were deleted permanently."
    : "Everything went to the Trash. Changed your mind? Undo now or restore any time in the next 30 days.";
}

/** Undo only after a reversible clean that recorded an entry (A3). */
export function canUndo(mode: CleanMode, historyId: string | null): boolean {
  return mode !== DELETE_MODE && historyId !== null;
}

/** Heading over the paths a clean left untouched (REQ-DCLEAN-018.A3). */
export function failedHeading(n: number): string {
  return n === 1 ? "1 item couldn't be cleaned and was left as it was:" : `${n} items couldn't be cleaned and were left as they were:`;
}

/** Engine failure reasons, in words. Unknown reasons are shown as given. */
const REASONS: Record<string, string> = {
  missing: "no longer there",
  "not reviewed": "not part of this review",
  "already in Trash": "already in the Trash",
  "move to Trash failed": "couldn't be moved to the Trash",
  "delete failed": "couldn't be deleted",
};

export function failureReason(f: Failure): string {
  return REASONS[f.reason] ?? f.reason;
}

/**
 * A total failure (REQ-DCLEAN-018.A4): nothing was reclaimed, so there is
 * no done phase. `detail` is the engine's message when the call itself failed.
 */
export function cleanFailedMessage(failed: readonly Failure[], detail?: string): string {
  if (detail) return `Nothing was cleaned: ${detail}`;
  const n = failed.length;
  return `Nothing was cleaned. ${n === 1 ? "The item" : `All ${n} items`} couldn't be cleaned and ${n === 1 ? "was" : "were"} left as ${n === 1 ? "it was" : "they were"}.`;
}

/** Undo's outcome lines (A7): every item that did not come back, and why. */
export const undoNotices: (r: RestoreReport) => string[] = restoreNotices;

/** Undo that failed outright, shown on the done phase. */
export function undoFailedMessage(detail: string): string {
  return `Undo didn't finish: ${detail}`;
}

export const UNDO_BUSY = "Restoring…";
