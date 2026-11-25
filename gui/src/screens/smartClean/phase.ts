/**
 * Smart Clean phase state machine (REQ-DCLEAN-003): idle → scanning →
 * results → cleaning → done. Pure transitions over a small state record, so
 * each rule is testable without rendering. Only the transitions the shipped
 * phases use live here; later phase REQs (004 … 009) add theirs.
 */

export type Phase = "idle" | "scanning" | "results" | "cleaning" | "done";

export interface SmartCleanState {
  phase: Phase;
  /** Scan or clean progress, 0..1. */
  progress: number;
  /** The expanded category id on the results list, if any. */
  expanded: string | null;
  /** The below-goal notification was dismissed this session (REQ-DCLEAN-010). */
  notificationDismissed: boolean;
}

export const INITIAL_STATE: SmartCleanState = {
  phase: "idle",
  progress: 0,
  expanded: null,
  notificationDismissed: false,
};

/** Phases a scan may start from: never while scanning or cleaning. */
const CAN_SCAN: readonly Phase[] = ["idle", "results", "done"];

/**
 * "Start scan" (REQ-DCLEAN-003.A1): enter scanning with progress at 0,
 * collapse any expanded category, and dismiss the goal notification.
 */
export function startScan(s: SmartCleanState): SmartCleanState {
  if (!CAN_SCAN.includes(s.phase)) return s;
  return { ...s, phase: "scanning", progress: 0, expanded: null, notificationDismissed: true };
}

/** A progress event while scanning. Progress never goes backwards. */
export function scanProgress(s: SmartCleanState, fraction: number): SmartCleanState {
  if (s.phase !== "scanning") return s;
  return { ...s, progress: Math.max(s.progress, Math.min(1, fraction)) };
}

/** The scan finished: show its results. Ignored once the scan was stopped. */
export function scanFinished(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "scanning") return s;
  return { ...s, phase: "results", progress: 1 };
}

/** "Stop", or a scan that failed: back to idle, no results. */
export function scanStopped(s: SmartCleanState): SmartCleanState {
  if (s.phase !== "scanning") return s;
  return { ...s, phase: "idle", progress: 0 };
}
