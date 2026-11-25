/**
 * Screen-reader announcements for Smart Clean status changes
 * (REQ-DCLEAN-023). Pure: given the state before and after a transition, the
 * one polite message to announce, or null. Coarse by design (A1): a scan
 * announces each category it finishes, a clean each category it moves to,
 * never every progress tick. Phase completion (A2) announces the same
 * heading the new phase shows. Errors render in their own `role="alert"`
 * regions, and the below-goal notification is its own polite status (A3).
 */
import type { SmartCleanState } from "./phase";

export const SCAN_STARTED = "Scan started";
export const SCAN_STOPPED = "Scan stopped";
export const CLEAN_STARTED = "Cleaning started";

export interface AnnounceContext {
  /** Display name of a category id, from the engine view. */
  categoryName: (id: string) => string;
  /** "Found <total> you can clear" for the finished scan. */
  found: () => string;
  /** "<freed> cleared" for the finished clean, once its result is known. */
  cleared: () => string | null;
  /** The running clean's step label ("Moving <category>…"). */
  step: () => string;
}

/**
 * The message for the transition `prev` → `next`, or null when there is
 * nothing worth announcing.
 *
 * @implements REQ-DCLEAN-023
 */
export function announcement(prev: SmartCleanState, next: SmartCleanState, ctx: AnnounceContext): string | null {
  if (next.phase === "scanning") {
    if (prev.phase !== "scanning") return SCAN_STARTED;
    if (next.completed.length > prev.completed.length) {
      return `${ctx.categoryName(next.completed[next.completed.length - 1])} checked`;
    }
    return null;
  }
  if (prev.phase === "scanning" && next.phase === "results") return ctx.found();
  if (prev.phase === "scanning" && next.phase === "idle") return SCAN_STOPPED;
  if (next.phase === "cleaning") {
    if (prev.phase !== "cleaning") return CLEAN_STARTED;
    if (next.cleaningCategory !== prev.cleaningCategory && next.cleaningCategory !== null) return ctx.step();
    return null;
  }
  if (prev.phase === "cleaning" && next.phase === "done") return ctx.cleared();
  return null;
}
