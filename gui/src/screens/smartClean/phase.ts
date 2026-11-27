/**
 * Smart Clean phase state machine (REQ-DCLEAN-003): idle → scanning →
 * results → cleaning → done. Pure transitions over a small state record, so
 * each rule is testable without rendering. Only the transitions the shipped
 * phases use live here; later phase REQs (005 … 009) add theirs.
 */
import type { CategoryView, CleanMode, CleanProgress, ScanProgress } from "../../lib/contract";
import { goalStatus } from "../../lib/meter";
import { totalBytes } from "./resultsCopy";

export type Phase = "idle" | "scanning" | "results" | "cleaning" | "done";

export interface SmartCleanState {
  phase: Phase;
  /** Scan or clean progress, 0..1. */
  progress: number;
  /** The expanded category id on the results list, if any. */
  expanded: string | null;
  /** The below-goal notification was dismissed this session (REQ-DCLEAN-010). */
  notificationDismissed: boolean;
  /** The path the running scan last reported (REQ-DCLEAN-004). */
  currentPath: string;
  /** Category ids the running scan has finished checking. Only grows. */
  completed: readonly string[];
  /** Physical bytes of each completed category that has hits. */
  completedBytes: Readonly<Record<string, number>>;
  /**
   * The agent that opened the review ("Prepared by <agent>", REQ-DSCR-009.A3),
   * or null for a scan the user ran or the below-goal notification.
   */
  openedBy: string | null;
  /** Category ids selected for cleaning on the results list (REQ-DCLEAN-006). */
  selected: readonly string[];
  /**
   * The confirm-clean dialog is open over the results (REQ-DCLEAN-007). Not
   * a phase: the results stay rendered underneath, and the selection is
   * frozen while it is open.
   */
  confirming: boolean;
  /** The mode of the running or finished clean (REQ-DCLEAN-008.A3), else null. */
  cleanMode: CleanMode | null;
  /** The category id the running clean is on (REQ-DCLEAN-008.A1), else null. */
  cleaningCategory: string | null;
}

/** What selection needs from a category. */
export type Selectable = Pick<CategoryView, "id" | "safety" | "bytes">;

export const INITIAL_STATE: SmartCleanState = {
  phase: "idle",
  progress: 0,
  expanded: null,
  notificationDismissed: false,
  currentPath: "",
  completed: [],
  completedBytes: {},
  openedBy: null,
  selected: [],
  confirming: false,
  cleanMode: null,
  cleaningCategory: null,
};

/** Highest progress a running clean shows: 100% means finished. */
export const CLEANING_PROGRESS_CAP = 0.99;

/** Highest progress a running scan shows: 100% means finished (REQ-DCLEAN-004.A1). */
export const SCANNING_PROGRESS_CAP = 0.99;

/** Phases a scan may start from: never while scanning or cleaning. */
const CAN_SCAN: readonly Phase[] = ["idle", "results", "done"];

const NO_SCAN = { progress: 0, currentPath: "", completed: [], completedBytes: {} } as const;

/**
 * "Start scan" (REQ-DCLEAN-003.A1): enter scanning with progress at 0,
 * collapse any expanded category, and dismiss the goal notification.
 */
export function startScan(s: SmartCleanState): SmartCleanState {
  if (!CAN_SCAN.includes(s.phase)) return s;
  return {
    ...s,
    ...NO_SCAN,
    phase: "scanning",
    expanded: null,
    notificationDismissed: true,
    openedBy: null,
    selected: [],
    confirming: false,
    cleanMode: null,
    cleaningCategory: null,
  };
}

/**
 * A progress event while scanning (REQ-DCLEAN-004). Progress never goes
 * backwards and stays below 100% until {@link scanFinished} (A1); an empty
 * path keeps the last one shown; completed categories only accumulate (A4).
 *
 * @implements REQ-DCLEAN-004
 */
export function scanProgress(s: SmartCleanState, p: ScanProgress): SmartCleanState {
  if (s.phase !== "scanning") return s;
  const fresh = p.completed_category_ids.filter((id) => !s.completed.includes(id));
  return {
    ...s,
    progress: Math.max(s.progress, Math.min(SCANNING_PROGRESS_CAP, p.fraction)),
    currentPath: p.current_path || s.currentPath,
    completed: fresh.length ? [...s.completed, ...fresh] : s.completed,
    completedBytes: { ...s.completedBytes, ...p.completed_category_bytes },
  };
}

/**
 * The scan finished: show its results, with the preselection the
 * safe-only preference asks for (REQ-DCLEAN-006.A1). Ignored once the scan
 * was stopped.
 *
 * @implements REQ-DCLEAN-006
 */
export function scanFinished(
  s: SmartCleanState,
  categories: readonly Selectable[] = [],
  safeOnly = true,
): SmartCleanState {
  if (s.phase !== "scanning") return s;
  return { ...s, phase: "results", progress: 1, expanded: null, selected: preselect(categories, safeOnly) };
}

/**
 * The categories the results list shows: every engine category except the
 * empty ones (REQ-DCLEAN-019.A2), in engine order (REQ-DCLEAN-005.A1).
 *
 * @implements REQ-DCLEAN-019
 */
export function listedCategories<C extends Selectable>(categories: readonly C[]): C[] {
  return categories.filter((c) => c.bytes > 0);
}

/**
 * A new scan's selection (REQ-DCLEAN-006.A1): exactly the Safe categories
 * when the safe-only preference is on, otherwise every listed category.
 *
 * @implements REQ-DCLEAN-006
 */
export function preselect(categories: readonly Selectable[], safeOnly: boolean): string[] {
  return listedCategories(categories)
    .filter((c) => !safeOnly || c.safety === "safe")
    .map((c) => c.id);
}

/**
 * The disclosure control (REQ-DCLEAN-005.A3): open `id`, which collapses any
 * other open row, or collapse `id` when it is already open.
 *
 * @implements REQ-DCLEAN-005
 */
export function toggleExpanded(s: SmartCleanState, id: string): SmartCleanState {
  if (s.phase !== "results") return s;
  return { ...s, expanded: s.expanded === id ? null : id };
}

/** Selection changes only on the results list, never under the confirm dialog. */
function canSelect(s: SmartCleanState): boolean {
  return s.phase === "results" && !s.confirming;
}

/**
 * A category's selection control (REQ-DCLEAN-006.A3): flips `id` and
 * nothing else.
 *
 * @implements REQ-DCLEAN-006
 */
export function toggleSelected(s: SmartCleanState, id: string): SmartCleanState {
  if (!canSelect(s)) return s;
  const selected = s.selected.includes(id) ? s.selected.filter((x) => x !== id) : [...s.selected, id];
  return { ...s, selected };
}

/**
 * "Safe only" (REQ-DCLEAN-006.A2): select exactly the Safe categories.
 *
 * @implements REQ-DCLEAN-006
 */
export function selectSafeOnly(s: SmartCleanState, categories: readonly Selectable[]): SmartCleanState {
  if (!canSelect(s)) return s;
  return { ...s, selected: preselect(categories, true) };
}

/**
 * "Select all" (REQ-DCLEAN-006.A2): select every listed category.
 *
 * @implements REQ-DCLEAN-006
 */
export function selectAll(s: SmartCleanState, categories: readonly Selectable[]): SmartCleanState {
  if (!canSelect(s)) return s;
  return { ...s, selected: preselect(categories, false) };
}

/**
 * "Back to home" / "Cancel" on the results phase (REQ-DCLEAN-019.A1): return
 * to idle. Nothing is cleaned.
 *
 * @implements REQ-DCLEAN-019
 */
export function backHome(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "results") return s;
  if (s.confirming) return s;
  return { ...s, ...NO_SCAN, phase: "idle", expanded: null, selected: [], openedBy: null, confirming: false };
}

/**
 * "Clean <total>" (REQ-DCLEAN-007): open the confirm dialog. Only on the
 * results list with something selected; nothing is cleaned yet (A1).
 *
 * @implements REQ-DCLEAN-007
 */
export function openConfirm(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "results" || s.confirming || s.selected.length === 0) return s;
  return { ...s, confirming: true };
}

/**
 * "Cancel", Escape or the backdrop (REQ-DCLEAN-007.A4): close the dialog
 * with the selection exactly as it was.
 *
 * @implements REQ-DCLEAN-007
 */
export function cancelConfirm(s: SmartCleanState): SmartCleanState {
  if (!s.confirming) return s;
  return { ...s, confirming: false };
}

/**
 * "Clean now" (REQ-DCLEAN-007.A1): the only way into the cleaning phase.
 * Refused unless the dialog is open. Stores the mode (REQ-DCLEAN-008.A3)
 * and the first chosen category, so the step label names it before any
 * progress event arrives (REQ-DCLEAN-008.A1).
 *
 * @implements REQ-DCLEAN-007
 * @implements REQ-DCLEAN-008
 */
export function confirmClean(s: SmartCleanState, mode: CleanMode, firstCategoryId: string | null): SmartCleanState {
  if (s.phase !== "results" || !s.confirming) return s;
  return { ...s, phase: "cleaning", progress: 0, confirming: false, cleanMode: mode, cleaningCategory: firstCategoryId };
}

/**
 * A clean progress event (REQ-DCLEAN-008): progress never goes backwards
 * and stays below 100% until {@link cleanFinished}; the step label follows
 * the category the engine is on (A1). Ignored outside cleaning.
 *
 * @implements REQ-DCLEAN-008
 */
export function cleanProgress(s: SmartCleanState, p: CleanProgress): SmartCleanState {
  if (s.phase !== "cleaning") return s;
  return {
    ...s,
    progress: Math.max(s.progress, Math.min(CLEANING_PROGRESS_CAP, p.fraction)),
    cleaningCategory: p.category_id || s.cleaningCategory,
  };
}

/**
 * The clean finished with something reclaimed: enter done at 100%
 * (REQ-DCLEAN-009).
 *
 * @implements REQ-DCLEAN-009
 */
export function cleanFinished(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "cleaning") return s;
  return { ...s, phase: "done", progress: 1, cleaningCategory: null };
}

/**
 * The clean reclaimed nothing, or failed outright (REQ-DCLEAN-018.A4): no
 * done phase — back to results with the selection exactly as it was.
 *
 * @implements REQ-DCLEAN-018
 */
export function cleanFailed(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "cleaning") return s;
  return { ...s, phase: "results", progress: 0, confirming: false, cleanMode: null, cleaningCategory: null };
}

/**
 * "Back to home" on done, or a finished Undo (REQ-DCLEAN-009.A4, A6):
 * return to idle with nothing selected.
 *
 * @implements REQ-DCLEAN-009
 */
export function doneHome(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "done") return s;
  return {
    ...s,
    ...NO_SCAN,
    phase: "idle",
    expanded: null,
    selected: [],
    openedBy: null,
    confirming: false,
    cleanMode: null,
    cleaningCategory: null,
  };
}

/** "Stop", or a scan that failed: back to idle, no results (REQ-DCLEAN-004.A3). */
export function scanStopped(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "scanning") return s;
  return { ...s, ...NO_SCAN, phase: "idle" };
}

/**
 * A category row while scanning (REQ-DCLEAN-004.A4): `found` (complete, with
 * its bytes), `empty` (complete, nothing found), `checking` (the scan is still
 * on it) or `pending` (no scan running). Bytes appear only on `found`.
 * Categories are classified after the walk, so every unfinished row is
 * `checking` until the scan's final event.
 *
 * @implements REQ-DCLEAN-004
 */
export function categoryRowStatus(
  id: string,
  s: SmartCleanState,
): { status: "found" | "empty" | "checking" | "pending"; bytes: number | null } {
  if (s.completed.includes(id)) {
    const bytes = s.completedBytes[id];
    return bytes === undefined ? { status: "empty", bytes: null } : { status: "found", bytes };
  }
  return { status: s.phase === "scanning" ? "checking" : "pending", bytes: null };
}

/**
 * Physical bytes of the latest scan's Safe categories (REQ-DCLEAN-010.A4):
 * the "<safe total>" the below-goal notification offers to clear.
 *
 * @implements REQ-DCLEAN-010
 */
export function safeTotal(categories: readonly Selectable[]): number {
  return totalBytes(listedCategories(categories).filter((c) => c.safety === "safe"));
}

/** What the below-goal notification's visibility depends on (REQ-DCLEAN-010). */
export interface NotificationInputs {
  phase: Phase;
  /** "Later" (or a scan) dismissed it this session. */
  dismissed: boolean;
  /** Notifications are enabled (prop `showNotification`). */
  showNotification: boolean;
  /** The assistant is on. */
  assistant: boolean;
  free: number;
  goal: number;
  /** Physical bytes of the latest scan's Safe categories. */
  safeTotal: number;
  /** A scan is available to compute the safe total from. */
  hasScan: boolean;
}

/**
 * The below-goal notification shows only on idle, and only when every
 * condition holds (REQ-DCLEAN-010.A1): notifications on, assistant on, free
 * space strictly below the goal, not dismissed this session. With no scan,
 * or nothing Safe to clear, there is nothing to offer and it stays hidden
 * (A4, A6).
 *
 * @implements REQ-DCLEAN-010
 */
export function notificationVisible(n: NotificationInputs): boolean {
  return (
    n.phase === "idle" &&
    !n.dismissed &&
    n.showNotification &&
    n.assistant &&
    goalStatus(n.free, n.goal) === "below" &&
    n.hasScan &&
    n.safeTotal > 0
  );
}

/**
 * "Later" on the below-goal notification (REQ-DCLEAN-010.A2): hide it for
 * the rest of the session. The flag lives in memory only, so a restart
 * clears it.
 *
 * @implements REQ-DCLEAN-010
 */
export function dismissNotification(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "idle" || s.notificationDismissed) return s;
  return { ...s, notificationDismissed: true };
}

/**
 * "Review cleanup" on the below-goal notification (REQ-DCLEAN-010.A3), or
 * "Review & confirm" on the Assistant's goal card (REQ-DSCR-009.A3): open
 * the results with exactly the Safe categories selected and the confirm
 * dialog open. `openedBy` names the agent when the Assistant opened it, so
 * the kicker reads "Prepared by <agent>" and the history title is
 * "Assistant cleanup"; the notification passes null. Nothing is cleaned:
 * only {@link confirmClean} enters cleaning.
 *
 * @implements REQ-DCLEAN-010
 * @implements REQ-DSCR-009
 */
export function reviewFromNotification(
  s: SmartCleanState,
  categories: readonly Selectable[],
  openedBy: string | null = null,
): SmartCleanState {
  if (s.phase !== "idle") return s;
  const selected = preselect(categories, true);
  return {
    ...s,
    ...NO_SCAN,
    phase: "results",
    progress: 1,
    expanded: null,
    notificationDismissed: true,
    openedBy,
    selected,
    confirming: selected.length > 0,
    cleanMode: null,
    cleaningCategory: null,
  };
}

/**
 * "Review" on an Assistant suggestion (REQ-DSCR-009.A3): open the results
 * with exactly that suggestion's categories selected (those the list shows)
 * and the first one expanded, prepared by `agent`. The confirm dialog stays
 * closed: nothing is cleaned until "Clean now". Only from idle.
 *
 * @implements REQ-DSCR-009
 */
export function reviewSuggestion(
  s: SmartCleanState,
  categories: readonly Selectable[],
  ids: readonly string[],
  agent: string,
): SmartCleanState {
  if (s.phase !== "idle") return s;
  const selected = listedCategories(categories)
    .filter((c) => ids.includes(c.id))
    .map((c) => c.id);
  return {
    ...s,
    ...NO_SCAN,
    phase: "results",
    progress: 1,
    expanded: selected[0] ?? null,
    notificationDismissed: true,
    openedBy: agent,
    selected,
    confirming: false,
    cleanMode: null,
    cleaningCategory: null,
  };
}

/**
 * Navigation is blocked while a scan or a clean runs (REQ-DCLEAN-011.A1).
 *
 * @implements REQ-DCLEAN-011
 */
export function navBlocked(phase: Phase): boolean {
  return phase === "scanning" || phase === "cleaning";
}

/**
 * Smart Clean when the user navigates (REQ-DCLEAN-011.A4): leaving the done
 * phase resets it to idle, and so does leaving Smart Clean for another
 * screen from the results (the scan's view is not kept across screens).
 * Re-selecting Smart Clean on the results keeps them. Blocked phases are
 * returned unchanged; the session's notification dismissal is kept.
 *
 * @implements REQ-DCLEAN-011
 */
export function leaveViaNav(s: SmartCleanState, stayOnSmartClean: boolean): SmartCleanState {
  if (navBlocked(s.phase)) return s;
  if (s.phase === "done" || (s.phase === "results" && !stayOnSmartClean)) {
    return {
      ...s,
      ...NO_SCAN,
      phase: "idle",
      expanded: null,
      selected: [],
      openedBy: null,
      confirming: false,
      cleanMode: null,
      cleaningCategory: null,
    };
  }
  return s;
}
