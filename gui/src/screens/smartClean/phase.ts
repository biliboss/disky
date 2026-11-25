/**
 * Smart Clean phase state machine (REQ-DCLEAN-003): idle → scanning →
 * results → cleaning → done. Pure transitions over a small state record, so
 * each rule is testable without rendering. Only the transitions the shipped
 * phases use live here; later phase REQs (005 … 009) add theirs.
 */
import type { CategoryView, ScanProgress } from "../../lib/contract";

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
   * The agent that opened the review ("Prepared by <agent>"), or null for a
   * scan the user ran. Nothing sets it until the Assistant spec.
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
};

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
 * Refused unless the dialog is open.
 *
 * @implements REQ-DCLEAN-007
 */
export function confirmClean(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "results" || !s.confirming) return s;
  return { ...s, phase: "cleaning", progress: 0, confirming: false };
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
