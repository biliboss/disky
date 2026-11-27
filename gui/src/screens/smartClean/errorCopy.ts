import { OS, skippedHint } from "../../lib/os";
/**
 * Error-state copy (REQ-DCLEAN-018). Failures are told apart by the engine's
 * exit-code slug (`EngineError.code`), never by message text.
 */
import { LOCK_HELD, type EngineError } from "../../lib/contract";

/** A1: the snapshot is locked by another disky process. */
export const LOCK_HELD_MESSAGE = "Another disky process is using the snapshot. Try again in a moment.";

/**
 * The scan error line: the lock-held copy (A1), else the engine's detail.
 *
 * @implements REQ-DCLEAN-018
 */
export function scanErrorMessage(e: EngineError): string {
  return e.code === LOCK_HELD ? LOCK_HELD_MESSAGE : `The scan didn't finish: ${e.detail}`;
}

/** "Try again" is offered for a lock (A1) and any other retryable failure. */
export function canRetry(e: EngineError): boolean {
  return e.code === LOCK_HELD || e.retryable;
}

/** A2: how many paths the scan could not read. */
export function skippedPaths(n: number): string {
  return n === 1 ? "1 path skipped" : `${n} paths skipped`;
}

/** Worded for the OS: only macOS has a Full Disk Access setting (REQ-XPLAT-010.A2). */
export const SKIPPED_HINT = skippedHint(OS);
export const FULL_DISK_ACCESS_LINK = "Open Full Disk Access";
