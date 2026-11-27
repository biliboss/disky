import { describe, expect, it } from "vitest";
import { DELETE_MODE, TRASH_MODE } from "../../lib/contract";
import { CLEANING_HEADING, stepLabel } from "../smartClean/cleaningCopy";
import { canUndo, cleanFailedMessage, doneHeading, doneNote, failureReason, undoNotices } from "../smartClean/doneCopy";
import { LOCK_HELD_MESSAGE, canRetry, scanErrorMessage, skippedPaths } from "../smartClean/errorCopy";
import { EngineError } from "../../lib/contract";

function stepVerbFollowsTheMode() {
  expect(CLEANING_HEADING).toBe("Tidying up…");
  expect(stepLabel(TRASH_MODE, "App caches")).toBe("Moving App caches…");
  expect(stepLabel(DELETE_MODE, "App caches")).toBe("Deleting App caches…");
}

function doneCopyFollowsTheMode() {
  expect(doneHeading(3_000_000_000)).toBe("3 GB cleared");
  expect(doneNote(TRASH_MODE)).toBe(
    "Everything went to the Trash. Changed your mind? Undo now or restore any time in the next 30 days.",
  );
  expect(doneNote(DELETE_MODE)).toBe("Those files were deleted permanently.");
  expect(canUndo(TRASH_MODE, "h1")).toBe(true);
  expect(canUndo(DELETE_MODE, "h1")).toBe(false);
  expect(canUndo(TRASH_MODE, null)).toBe(false);
}

function undoAndFailureCopy() {
  expect(undoNotices({ id: "h", restored: ["/a"], conflicts: ["/c"], missing: ["/m"] })).toEqual([
    "/m is no longer in Trash, so it couldn't be restored.",
    "/c wasn't restored: something else is already there. It's still in the Trash.",
  ]);
  expect(failureReason({ path: "/x", reason: "missing" })).toBe("no longer there");
  expect(failureReason({ path: "/x", reason: "brand new reason" })).toBe("brand new reason");
  expect(cleanFailedMessage([{ path: "/x", reason: "missing" }])).toMatch(/^Nothing was cleaned\./);
  expect(cleanFailedMessage([], "boom")).toBe("Nothing was cleaned: boom");
}

function errorCopyDispatchesOnTheCode() {
  const held = new EngineError("lock-held", "Could not set lock", true);
  expect(scanErrorMessage(held)).toBe("Another disky process is using the snapshot. Try again in a moment.");
  expect(LOCK_HELD_MESSAGE).toBe(scanErrorMessage(held));
  expect(canRetry(held)).toBe(true);
  // The message text alone never makes it a lock error.
  const lookalike = new EngineError("generic", "Could not set lock", false);
  expect(scanErrorMessage(lookalike)).toBe("The scan didn't finish: Could not set lock");
  expect(canRetry(lookalike)).toBe(false);
  expect(skippedPaths(1)).toBe("1 path skipped");
  expect(skippedPaths(12)).toBe("12 paths skipped");
}

describe("cleaning, done and error copy (REQ-DCLEAN-008, REQ-DCLEAN-009, REQ-DCLEAN-018)", () => {
  it("the step verb follows the mode (REQ-DCLEAN-008.A3)", stepVerbFollowsTheMode);
  it("the done heading, note and Undo follow the mode (REQ-DCLEAN-009.A1, A2, A3)", doneCopyFollowsTheMode);
  it("undo outcomes and failure reasons are spelled out (REQ-DCLEAN-009.A7, REQ-DCLEAN-018.A3)", undoAndFailureCopy);
  it("error copy dispatches on the engine code (REQ-DCLEAN-018.A1, A2)", errorCopyDispatchesOnTheCode);
});
