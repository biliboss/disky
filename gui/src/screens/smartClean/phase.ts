/**
 * Smart Clean phase state machine (REQ-DCLEAN-003): idle → scanning →
 * results → cleaning → done. Pure transitions over a small state record, so
 * each rule is testable without rendering. Only the transitions the shipped
 * phases use live here; later phase REQs (005 … 009) add theirs.
 */
import type { ScanProgress } from "../../lib/contract";

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
}

export const INITIAL_STATE: SmartCleanState = {
  phase: "idle",
  progress: 0,
  expanded: null,
  notificationDismissed: false,
  currentPath: "",
  completed: [],
  completedBytes: {},
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
  return { ...s, ...NO_SCAN, phase: "scanning", expanded: null, notificationDismissed: true };
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

/** The scan finished: show its results. Ignored once the scan was stopped. */
export function scanFinished(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "scanning") return s;
  return { ...s, phase: "results", progress: 1 };
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
