import { describe, expect, it } from "vitest";
import type { ScanProgress } from "../../lib/contract";
import {
  INITIAL_STATE,
  SCANNING_PROGRESS_CAP,
  categoryRowStatus,
  scanFinished,
  scanProgress,
  scanStopped,
  startScan,
  type SmartCleanState,
} from "../smartClean/phase";

function ev(fraction: number, more: Partial<ScanProgress> = {}): ScanProgress {
  return { fraction, current_path: "", completed_category_ids: [], completed_category_bytes: {}, ...more };
}

function startScanResetsProgressCollapsesAndDismisses() {
  const from: SmartCleanState = {
    ...INITIAL_STATE,
    phase: "results",
    progress: 1,
    expanded: "app_caches",
    currentPath: "/old",
    completed: ["a"],
    completedBytes: { a: 1 },
  };
  expect(startScan(from)).toEqual({ ...INITIAL_STATE, phase: "scanning", notificationDismissed: true });
  expect(startScan(INITIAL_STATE).phase).toBe("scanning");
  expect(startScan({ ...INITIAL_STATE, phase: "done" }).phase).toBe("scanning");
}

function startScanIsIgnoredWhileBusy() {
  const scanning = startScan(INITIAL_STATE);
  expect(startScan(scanning)).toBe(scanning);
  const cleaning: SmartCleanState = { ...INITIAL_STATE, phase: "cleaning", progress: 0.4 };
  expect(startScan(cleaning)).toBe(cleaning);
}

function scanProgressNeverGoesBackwards() {
  let s = startScan(INITIAL_STATE);
  s = scanProgress(s, ev(0.5));
  s = scanProgress(s, ev(0.3));
  expect(s.progress).toBe(0.5);
  expect(scanProgress(INITIAL_STATE, ev(0.5))).toBe(INITIAL_STATE);
}

function progressReachesOneOnlyWhenTheScanFinishes() {
  let s = startScan(INITIAL_STATE);
  s = scanProgress(s, ev(1));
  expect(s.progress).toBe(SCANNING_PROGRESS_CAP);
  expect(s.progress).toBeLessThan(1);
  s = scanProgress(s, ev(0.2));
  expect(s.progress).toBe(SCANNING_PROGRESS_CAP);
  expect(scanFinished(s).progress).toBe(1);
}

function completedCategoriesOnlyGrow() {
  let s = startScan(INITIAL_STATE);
  s = scanProgress(s, ev(0.1, { completed_category_ids: ["a"], completed_category_bytes: { a: 10 } }));
  s = scanProgress(s, ev(0.2, { completed_category_ids: ["b"] }));
  s = scanProgress(s, ev(0.3, { completed_category_ids: ["a"] }));
  expect(s.completed).toEqual(["a", "b"]);
  expect(s.completedBytes).toEqual({ a: 10 });
}

function currentPathSurvivesAnEmptyPath() {
  let s = startScan(INITIAL_STATE);
  s = scanProgress(s, ev(0.1, { current_path: "/Users/me/Library" }));
  s = scanProgress(s, ev(0.2, { current_path: "" }));
  expect(s.currentPath).toBe("/Users/me/Library");
}

function stoppedScanIgnoresALateFinish() {
  const running = scanProgress(
    startScan(INITIAL_STATE),
    ev(0.7, { current_path: "/x", completed_category_ids: ["a"], completed_category_bytes: { a: 1 } }),
  );
  const stopped = scanStopped(running);
  expect(stopped).toMatchObject({ phase: "idle", progress: 0, currentPath: "", completed: [], completedBytes: {} });
  expect(scanFinished(stopped)).toBe(stopped);
  expect(scanProgress(stopped, ev(0.9))).toBe(stopped);
  expect(scanFinished(startScan(INITIAL_STATE))).toMatchObject({ phase: "results", progress: 1 });
}

function categoryRowStatusCoversEveryState() {
  let s = startScan(INITIAL_STATE);
  s = scanProgress(s, ev(0.5, { completed_category_ids: ["a", "b"], completed_category_bytes: { a: 42 } }));
  expect(categoryRowStatus("a", s)).toEqual({ status: "found", bytes: 42 });
  expect(categoryRowStatus("b", s)).toEqual({ status: "empty", bytes: null });
  expect(categoryRowStatus("c", s)).toEqual({ status: "checking", bytes: null });
  expect(categoryRowStatus("c", INITIAL_STATE)).toEqual({ status: "pending", bytes: null });
}

describe("Smart Clean phase machine", () => {
  it("Start scan enters scanning at 0, collapses and dismisses (REQ-DCLEAN-003.A1)", startScanResetsProgressCollapsesAndDismisses);
  it("Start scan is ignored while scanning or cleaning (REQ-DCLEAN-003.A1)", startScanIsIgnoredWhileBusy);
  it("scan progress never goes backwards (REQ-DCLEAN-004.A1)", scanProgressNeverGoesBackwards);
  it("progress reaches 100% only when the scan finishes (REQ-DCLEAN-004.A1)", progressReachesOneOnlyWhenTheScanFinishes);
  it("completed categories only grow (REQ-DCLEAN-004.A4)", completedCategoriesOnlyGrow);
  it("the current path survives an event without one (REQ-DCLEAN-004)", currentPathSurvivesAnEmptyPath);
  it("a stopped scan clears its progress and ignores a late finish (REQ-DCLEAN-004.A3)", stoppedScanIgnoresALateFinish);
  it("each category row is found, empty, checking or pending (REQ-DCLEAN-004.A4)", categoryRowStatusCoversEveryState);
});
