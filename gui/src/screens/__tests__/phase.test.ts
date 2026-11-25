import { describe, expect, it } from "vitest";
import type { ScanProgress } from "../../lib/contract";
import {
  INITIAL_STATE,
  SCANNING_PROGRESS_CAP,
  backHome,
  categoryRowStatus,
  listedCategories,
  preselect,
  scanFinished,
  selectAll,
  selectSafeOnly,
  toggleExpanded,
  toggleSelected,
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

const CATS = [
  { id: "a", safety: "safe", bytes: 5 },
  { id: "b", safety: "review", bytes: 3 },
  { id: "c", safety: "safe", bytes: 2 },
  { id: "z", safety: "safe", bytes: 0 },
] as const;

const RESULTS: SmartCleanState = { ...INITIAL_STATE, phase: "results", progress: 1 };

function startScanClearsOpenedByAndSelection() {
  const from: SmartCleanState = { ...RESULTS, openedBy: "claude", selected: ["a", "b"] };
  const next = startScan(from);
  expect(next.openedBy).toBeNull();
  expect(next.selected).toEqual([]);
}

function toggleExpandedOpensSwitchesAndCollapses() {
  const a = toggleExpanded(RESULTS, "a");
  expect(a.expanded).toBe("a");
  const b = toggleExpanded(a, "b");
  expect(b.expanded).toBe("b"); // opening B collapses A: one open row at most
  expect(toggleExpanded(b, "b").expanded).toBeNull();
}

function toggleExpandedIsIgnoredOutsideResults() {
  for (const phase of ["idle", "scanning", "cleaning", "done"] as const) {
    const s: SmartCleanState = { ...INITIAL_STATE, phase };
    expect(toggleExpanded(s, "a")).toBe(s);
  }
}

function safeOnlyPreferencePreselectsExactlyTheSafeCategories() {
  const scanning = startScan(INITIAL_STATE);
  expect(scanFinished(scanning, CATS, true).selected).toEqual(["a", "c"]);
  expect(scanFinished(scanning, CATS, false).selected).toEqual(["a", "b", "c"]);
  // A zero-size category is not listed, so it is never preselected.
  expect(preselect(CATS, false)).not.toContain("z");
  // A late finish after Stop selects nothing.
  const stopped = scanStopped(scanning);
  expect(scanFinished(stopped, CATS, true)).toBe(stopped);
}

function safeOnlyAndSelectAllActions() {
  const mixed: SmartCleanState = { ...RESULTS, selected: ["b"] };
  expect(selectSafeOnly(mixed, CATS).selected).toEqual(["a", "c"]);
  expect(selectAll(mixed, CATS).selected).toEqual(["a", "b", "c"]);
  expect(selectAll(INITIAL_STATE, CATS)).toBe(INITIAL_STATE);
}

function toggleSelectedFlipsOnlyThatCategory() {
  const s: SmartCleanState = { ...RESULTS, selected: ["a", "c"] };
  expect(toggleSelected(s, "a").selected).toEqual(["c"]);
  expect(toggleSelected(s, "b").selected).toEqual(["a", "c", "b"]);
  expect(toggleSelected(toggleSelected(s, "b"), "b").selected).toEqual(["a", "c"]);
  expect(toggleSelected(INITIAL_STATE, "a")).toBe(INITIAL_STATE);
}

function zeroSizeCategoriesAreNotListed() {
  expect(listedCategories(CATS).map((c) => c.id)).toEqual(["a", "b", "c"]);
}

function backHomeReturnsToIdleWithNothingSelected() {
  const s: SmartCleanState = { ...RESULTS, expanded: "a", selected: ["a"] };
  const home = backHome(s);
  expect(home.phase).toBe("idle");
  expect(home.selected).toEqual([]);
  expect(home.expanded).toBeNull();
  expect(backHome(INITIAL_STATE)).toBe(INITIAL_STATE);
}

describe("Smart Clean results transitions (REQ-DCLEAN-005, REQ-DCLEAN-006, REQ-DCLEAN-019)", () => {
  it("Start scan clears openedBy and the selection (REQ-DCLEAN-005)", startScanClearsOpenedByAndSelection);
  it("one row open at most; re-activating collapses it (REQ-DCLEAN-005.A3)", toggleExpandedOpensSwitchesAndCollapses);
  it("expansion is ignored outside results (REQ-DCLEAN-005.A3)", toggleExpandedIsIgnoredOutsideResults);
  it("a new scan preselects per the safe-only preference (REQ-DCLEAN-006.A1)", safeOnlyPreferencePreselectsExactlyTheSafeCategories);
  it("Safe only and Select all (REQ-DCLEAN-006.A2)", safeOnlyAndSelectAllActions);
  it("toggling flips only that category (REQ-DCLEAN-006.A3)", toggleSelectedFlipsOnlyThatCategory);
  it("zero-size categories are omitted (REQ-DCLEAN-019.A2)", zeroSizeCategoriesAreNotListed);
  it("Back to home returns to idle with nothing selected (REQ-DCLEAN-019.A1)", backHomeReturnsToIdleWithNothingSelected);
});

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
