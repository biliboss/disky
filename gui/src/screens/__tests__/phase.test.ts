import { describe, expect, it } from "vitest";
import type { CleanProgress, ScanProgress } from "../../lib/contract";
import { DELETE_MODE, TRASH_MODE } from "../../lib/contract";
import {
  CLEANING_PROGRESS_CAP,
  INITIAL_STATE,
  SCANNING_PROGRESS_CAP,
  cleanFailed,
  cleanFinished,
  cleanProgress,
  doneHome,
  backHome,
  cancelConfirm,
  confirmClean,
  openConfirm,
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
  dismissNotification,
  leaveViaNav,
  navBlocked,
  notificationVisible,
  reviewFromNotification,
  reviewSuggestion,
  safeTotal,
  type NotificationInputs,
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

// ── Confirm dialog (REQ-DCLEAN-007) ──────────────────────────────

function openConfirmNeedsResultsAndASelection() {
  const s: SmartCleanState = { ...RESULTS, selected: ["a"] };
  expect(openConfirm(s)).toEqual({ ...s, confirming: true });
  const none: SmartCleanState = { ...RESULTS, selected: [] };
  expect(openConfirm(none)).toBe(none);
  for (const phase of ["idle", "scanning", "cleaning", "done"] as const) {
    const other: SmartCleanState = { ...INITIAL_STATE, phase, selected: ["a"] };
    expect(openConfirm(other)).toBe(other);
  }
}

function cancelConfirmKeepsTheSelection() {
  const s: SmartCleanState = { ...RESULTS, selected: ["b", "a"], expanded: "a", confirming: true };
  expect(cancelConfirm(s)).toEqual({ ...s, confirming: false });
  expect(cancelConfirm(s).selected).toEqual(["b", "a"]);
  const closed: SmartCleanState = { ...RESULTS, selected: ["a"] };
  expect(cancelConfirm(closed)).toBe(closed);
}

function confirmCleanEntersCleaningOnlyFromTheDialog() {
  const s: SmartCleanState = { ...RESULTS, selected: ["a"], progress: 1 };
  expect(confirmClean(s, TRASH_MODE, "a")).toBe(s); // dialog not open: refused
  const open = openConfirm(s);
  const cleaning = confirmClean(open, TRASH_MODE, "a");
  expect(cleaning.phase).toBe("cleaning");
  expect(cleaning.progress).toBe(0);
  expect(cleaning.confirming).toBe(false);
  expect(cleaning.selected).toEqual(["a"]);
  const idleOpen: SmartCleanState = { ...INITIAL_STATE, confirming: true };
  expect(confirmClean(idleOpen, TRASH_MODE, "a")).toBe(idleOpen);
}

function selectionIsFrozenWhileConfirming() {
  const cats = [
    { id: "a", safety: "safe", bytes: 1 },
    { id: "b", safety: "review", bytes: 2 },
  ] as const;
  const s: SmartCleanState = { ...RESULTS, selected: ["a"], confirming: true };
  expect(toggleSelected(s, "a")).toBe(s);
  expect(toggleSelected(s, "b")).toBe(s);
  expect(selectAll(s, cats)).toBe(s);
  expect(selectSafeOnly(s, cats)).toBe(s);
  expect(backHome(s)).toBe(s);
}

describe("Smart Clean confirm dialog transitions (REQ-DCLEAN-007)", () => {
  it("opens only on results with something selected (REQ-DCLEAN-007)", openConfirmNeedsResultsAndASelection);
  it("Cancel closes with the selection unchanged (REQ-DCLEAN-007.A4)", cancelConfirmKeepsTheSelection);
  it("Clean now is the only way into cleaning (REQ-DCLEAN-007.A1)", confirmCleanEntersCleaningOnlyFromTheDialog);
  it("the selection cannot change under the dialog (REQ-DCLEAN-007.A5)", selectionIsFrozenWhileConfirming);
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

function cev(category_id: string, done: number, total: number): CleanProgress {
  return { category_id, done_items: done, total_items: total, fraction: done / total };
}

const CONFIRMING: SmartCleanState = { ...INITIAL_STATE, phase: "results", selected: ["a", "b"], confirming: true };

function confirmCleanStoresModeAndFirstCategory() {
  const t = confirmClean(CONFIRMING, TRASH_MODE, "a");
  expect(t.cleanMode).toBe(TRASH_MODE);
  expect(t.cleaningCategory).toBe("a");
  const d = confirmClean(CONFIRMING, DELETE_MODE, "b");
  expect(d.cleanMode).toBe(DELETE_MODE);
  expect(d.cleaningCategory).toBe("b");
}

function cleanProgressNeverGoesBackwardsAndFollowsTheCategory() {
  let s = confirmClean(CONFIRMING, TRASH_MODE, "a");
  s = cleanProgress(s, cev("a", 1, 4));
  expect(s.progress).toBe(0.25);
  expect(s.cleaningCategory).toBe("a");
  s = cleanProgress(s, cev("b", 2, 4));
  expect(s.cleaningCategory).toBe("b");
  const back = cleanProgress(s, cev("b", 1, 4));
  expect(back.progress).toBe(0.5);
  const full = cleanProgress(s, cev("b", 4, 4));
  expect(full.progress).toBe(CLEANING_PROGRESS_CAP);
  expect(full.phase).toBe("cleaning");
}

function cleanProgressIsIgnoredOutsideCleaning() {
  for (const phase of ["idle", "scanning", "results", "done"] as const) {
    const s: SmartCleanState = { ...INITIAL_STATE, phase };
    expect(cleanProgress(s, cev("a", 1, 2))).toBe(s);
  }
}

function cleanFinishedEntersDone() {
  const s = cleanFinished(cleanProgress(confirmClean(CONFIRMING, TRASH_MODE, "a"), cev("a", 1, 2)));
  expect(s.phase).toBe("done");
  expect(s.progress).toBe(1);
  expect(s.cleanMode).toBe(TRASH_MODE);
  expect(cleanFinished(INITIAL_STATE)).toBe(INITIAL_STATE);
}

function cleanFailedReturnsToResultsWithTheSelection() {
  const s = cleanFailed(confirmClean(CONFIRMING, DELETE_MODE, "a"));
  expect(s.phase).toBe("results");
  expect(s.selected).toEqual(["a", "b"]);
  expect(s.confirming).toBe(false);
  expect(s.cleanMode).toBeNull();
  expect(cleanFailed(INITIAL_STATE)).toBe(INITIAL_STATE);
}

function doneHomeReturnsToIdle() {
  const done = cleanFinished(confirmClean(CONFIRMING, TRASH_MODE, "a"));
  const s = doneHome(done);
  expect(s.phase).toBe("idle");
  expect(s.selected).toEqual([]);
  expect(s.cleanMode).toBeNull();
  expect(doneHome(CONFIRMING)).toBe(CONFIRMING);
}

describe("Smart Clean cleaning and done transitions (REQ-DCLEAN-008, REQ-DCLEAN-009, REQ-DCLEAN-018)", () => {
  it("Clean now stores the mode and the first category (REQ-DCLEAN-008.A1, A3)", confirmCleanStoresModeAndFirstCategory);
  it("clean progress never goes backwards and names the current category (REQ-DCLEAN-008.A1)", cleanProgressNeverGoesBackwardsAndFollowsTheCategory);
  it("clean progress is ignored outside cleaning (REQ-DCLEAN-008)", cleanProgressIsIgnoredOutsideCleaning);
  it("a finished clean enters done at 100% (REQ-DCLEAN-009)", cleanFinishedEntersDone);
  it("a failed clean returns to results with the selection (REQ-DCLEAN-018.A4)", cleanFailedReturnsToResultsWithTheSelection);
  it("Back to home on done returns to idle (REQ-DCLEAN-009.A6)", doneHomeReturnsToIdle);
});

const GB = 1e9;
const GOAL_CATS = [
  { id: "a", safety: "safe" as const, bytes: 3 * GB },
  { id: "b", safety: "review" as const, bytes: 5 * GB },
  { id: "c", safety: "safe" as const, bytes: 2 * GB },
  { id: "d", safety: "safe" as const, bytes: 0 },
];

const SHOWN: NotificationInputs = {
  phase: "idle",
  dismissed: false,
  showNotification: true,
  assistant: true,
  free: 74 * GB,
  goal: 80 * GB,
  safeTotal: 5 * GB,
  hasScan: true,
};

function notificationVisibleOnlyWhenEveryConditionHolds() {
  expect(notificationVisible(SHOWN)).toBe(true);
  for (const phase of ["scanning", "results", "cleaning", "done"] as const) {
    expect(notificationVisible({ ...SHOWN, phase })).toBe(false);
  }
  expect(notificationVisible({ ...SHOWN, dismissed: true })).toBe(false);
  expect(notificationVisible({ ...SHOWN, showNotification: false })).toBe(false);
  expect(notificationVisible({ ...SHOWN, assistant: false })).toBe(false);
  expect(notificationVisible({ ...SHOWN, free: 80 * GB })).toBe(false); // at the goal is not below
  expect(notificationVisible({ ...SHOWN, free: 90 * GB })).toBe(false);
}

function notificationHiddenWithoutAScanOrSafeTotal() {
  expect(notificationVisible({ ...SHOWN, hasScan: false })).toBe(false);
  expect(notificationVisible({ ...SHOWN, safeTotal: 0 })).toBe(false);
}

function safeTotalSumsTheListedSafeCategories() {
  expect(safeTotal(GOAL_CATS)).toBe(5 * GB);
  expect(safeTotal([])).toBe(0);
}

function dismissNotificationOnlyActsOnIdle() {
  const s = dismissNotification(INITIAL_STATE);
  expect(s.notificationDismissed).toBe(true);
  expect(s.phase).toBe("idle");
  const results: SmartCleanState = { ...INITIAL_STATE, phase: "results" };
  expect(dismissNotification(results)).toBe(results);
}

function reviewFromNotificationSelectsSafeAndOpensTheDialog() {
  const s = reviewFromNotification(INITIAL_STATE, GOAL_CATS);
  expect(s.phase).toBe("results");
  expect(s.selected).toEqual(["a", "c"]);
  expect(s.confirming).toBe(true);
  expect(s.cleanMode).toBeNull();
  expect(s.cleaningCategory).toBeNull();
  expect(s.openedBy).toBeNull();
  const scanning: SmartCleanState = { ...INITIAL_STATE, phase: "scanning" };
  expect(reviewFromNotification(scanning, GOAL_CATS)).toBe(scanning);
  // Nothing Safe to clean: no dialog over an empty selection.
  expect(reviewFromNotification(INITIAL_STATE, [GOAL_CATS[1]]).confirming).toBe(false);
}

describe("below-goal notification transitions (REQ-DCLEAN-010)", () => {
  it("shows only on idle when every condition holds (REQ-DCLEAN-010.A1)", notificationVisibleOnlyWhenEveryConditionHolds);
  it("hidden with no scan or nothing Safe to clear (REQ-DCLEAN-010.A4, A6)", notificationHiddenWithoutAScanOrSafeTotal);
  it("safe total sums the listed Safe categories (REQ-DCLEAN-010.A4)", safeTotalSumsTheListedSafeCategories);
  it("Later dismisses it, only on idle (REQ-DCLEAN-010.A2)", dismissNotificationOnlyActsOnIdle);
  it("Review cleanup opens results with Safe selected and the dialog open (REQ-DCLEAN-010.A3)", reviewFromNotificationSelectsSafeAndOpensTheDialog);
});

function navIsBlockedWhileScanningOrCleaning() {
  expect(navBlocked("scanning")).toBe(true);
  expect(navBlocked("cleaning")).toBe(true);
  for (const p of ["idle", "results", "done"] as const) expect(navBlocked(p)).toBe(false);
}

function leavingDoneViaNavResetsToIdle() {
  const done = { ...cleanFinished(confirmClean(CONFIRMING, TRASH_MODE, "a")), notificationDismissed: true };
  for (const stay of [true, false]) {
    const s = leaveViaNav(done, stay);
    expect(s.phase).toBe("idle");
    expect(s.selected).toEqual([]);
    expect(s.cleanMode).toBeNull();
    expect(s.notificationDismissed).toBe(true); // the session's dismissal is kept
  }
  // Results survive re-selecting Smart Clean, not leaving it.
  expect(leaveViaNav(CONFIRMING, true)).toBe(CONFIRMING);
  expect(leaveViaNav(CONFIRMING, false).phase).toBe("idle");
  const cleaning = confirmClean(CONFIRMING, TRASH_MODE, "a");
  expect(leaveViaNav(cleaning, false)).toBe(cleaning);
}

describe("navigation and the Smart Clean phase (REQ-DCLEAN-011)", () => {
  it("navigation is blocked while scanning or cleaning (REQ-DCLEAN-011.A1)", navIsBlockedWhileScanningOrCleaning);
  it("leaving done via navigation resets to idle (REQ-DCLEAN-011.A4)", leavingDoneViaNavResetsToIdle);
});

/** The engine's categories as the results list sees them, one empty. */
const ASSISTANT_CATS = [
  { id: "a", safety: "safe" as const, bytes: 10 },
  { id: "b", safety: "review" as const, bytes: 20 },
  { id: "c", safety: "safe" as const, bytes: 5 },
  { id: "d", safety: "review" as const, bytes: 0 },
];

function reviewSuggestionSelectsExactlyItsCategoriesAndExpandsTheFirst() {
  const s = reviewSuggestion(INITIAL_STATE, ASSISTANT_CATS, ["b", "d", "missing"], "Claude Code");
  expect(s.phase).toBe("results");
  // Exactly the suggestion's ids the list shows: empty or unknown ones drop out.
  expect(s.selected).toEqual(["b"]);
  expect(s.expanded).toBe("b");
  expect(s.openedBy).toBe("Claude Code");
  expect(s.confirming).toBe(false); // nothing cleans before "Clean now"
  expect(s.cleanMode).toBeNull();
  const two = reviewSuggestion(INITIAL_STATE, ASSISTANT_CATS, ["c", "a"], "Codex CLI");
  expect(two.selected).toEqual(["a", "c"]); // list order
  expect(two.expanded).toBe("a");
}

function reviewAndConfirmOpensTheDialogPreparedByTheAgent() {
  const s = reviewFromNotification(INITIAL_STATE, ASSISTANT_CATS, "Claude Code");
  expect(s.selected).toEqual(["a", "c"]);
  expect(s.confirming).toBe(true);
  expect(s.openedBy).toBe("Claude Code");
  expect(s.expanded).toBeNull();
  // The notification itself is not the Assistant: no agent.
  expect(reviewFromNotification(INITIAL_STATE, ASSISTANT_CATS).openedBy).toBeNull();
}

function assistantReviewsChangeNothingUnlessIdle() {
  for (const phase of ["scanning", "results", "cleaning", "done"] as const) {
    const from: SmartCleanState = { ...INITIAL_STATE, phase };
    expect(reviewSuggestion(from, ASSISTANT_CATS, ["b"], "Claude Code")).toBe(from);
    expect(reviewFromNotification(from, ASSISTANT_CATS, "Claude Code")).toBe(from);
  }
}

describe("Assistant reviews (REQ-DSCR-009.A3)", () => {
  it("Review selects exactly the card's categories and expands the first", reviewSuggestionSelectsExactlyItsCategoriesAndExpandsTheFirst);
  it("Review & confirm opens the dialog over the Safe categories, prepared by the agent", reviewAndConfirmOpensTheDialogPreparedByTheAgent);
  it("nothing changes unless Smart Clean is idle", assistantReviewsChangeNothingUnlessIdle);
});
