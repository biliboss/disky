import { describe, expect, it } from "vitest";
import { CLEAN_STARTED, SCAN_STARTED, SCAN_STOPPED, announcement, type AnnounceContext } from "../smartClean/announce";
import { INITIAL_STATE, type SmartCleanState } from "../smartClean/phase";

const ctx: AnnounceContext = {
  categoryName: (id) => `Name of ${id}`,
  found: () => "Found 4.8 GB you can clear",
  cleared: () => "4.2 GB cleared",
  step: () => "Moving Name of b…",
};
const at = (over: Partial<SmartCleanState>): SmartCleanState => ({ ...INITIAL_STATE, ...over });

function scanAnnouncesStartAndEachFinishedCategoryOnly() {
  const scanning = at({ phase: "scanning" });
  expect(announcement(INITIAL_STATE, scanning, ctx)).toBe(SCAN_STARTED);
  // A progress tick with no newly finished category says nothing (A1).
  expect(announcement(scanning, { ...scanning, progress: 0.3, currentPath: "/x" }, ctx)).toBeNull();
  expect(announcement(scanning, { ...scanning, completed: ["a"] }, ctx)).toBe("Name of a checked");
  expect(announcement(at({ phase: "scanning" }), INITIAL_STATE, ctx)).toBe(SCAN_STOPPED);
}

function completionAnnouncesThePhaseHeading() {
  expect(announcement(at({ phase: "scanning" }), at({ phase: "results" }), ctx)).toBe("Found 4.8 GB you can clear");
  expect(announcement(at({ phase: "cleaning" }), at({ phase: "done" }), ctx)).toBe("4.2 GB cleared");
}

function cleanAnnouncesStartAndEachCategory() {
  const cleaning = at({ phase: "cleaning", cleanMode: "trash", cleaningCategory: "a" });
  expect(announcement(at({ phase: "results", confirming: true }), cleaning, ctx)).toBe(CLEAN_STARTED);
  expect(announcement(cleaning, { ...cleaning, progress: 0.5 }, ctx)).toBeNull();
  expect(announcement(cleaning, { ...cleaning, cleaningCategory: "b" }, ctx)).toBe("Moving Name of b…");
}

function selectionAndDialogChangesAreSilent() {
  const results = at({ phase: "results" });
  expect(announcement(results, { ...results, selected: ["a"] }, ctx)).toBeNull();
  expect(announcement(results, { ...results, confirming: true }, ctx)).toBeNull();
}

describe("announcement (REQ-DCLEAN-023)", () => {
  it("a scan announces its start and each finished category, not every tick (A1)", scanAnnouncesStartAndEachFinishedCategoryOnly);
  it("phase completion announces the new heading (A2)", completionAnnouncesThePhaseHeading);
  it("a clean announces its start and each category it moves to (A1)", cleanAnnouncesStartAndEachCategory);
  it("selection and dialog changes are not announced", selectionAndDialogChangesAreSilent);
});
