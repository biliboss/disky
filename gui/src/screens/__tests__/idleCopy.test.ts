import { describe, expect, it } from "vitest";
import {
  IDLE_HEADING,
  assistantShortcut,
  idleDescription,
  largeFilesShortcut,
  lastScanKicker,
  NOTIFICATION_LATER,
  NOTIFICATION_REVIEW,
  notificationSender,
  notificationText,
} from "../smartClean/idleCopy";
import { relativeTime } from "../../lib/format";

const NOW = Date.parse("2026-11-25T12:00:00Z");
const DAY = 86_400_000;

function descriptionFollowsTrashMode() {
  expect(idleDescription(true)).toContain("Nothing is removed until you review it, and everything goes to the Trash first.");
  const off = idleDescription(false);
  expect(off).toMatch(/Nothing is removed until you review it\.$/);
  expect(off).not.toMatch(/Trash/);
}

function kickerReadsNoScansYetWithoutASnapshot() {
  expect(lastScanKicker(null, NOW)).toBe("No scans yet");
}

function kickerUsesTheSnapshotTimestamp() {
  expect(lastScanKicker(new Date(NOW - 7 * DAY).toISOString(), NOW)).toBe("Last scan 7 days ago");
  expect(lastScanKicker(new Date(NOW - 3 * 3_600_000).toISOString(), NOW)).toBe("Last scan 3 hours ago");
}

function relativeTimeUnits() {
  const at = (ms: number) => relativeTime(new Date(NOW - ms).toISOString(), NOW);
  expect(at(10_000)).toBe("just now");
  expect(at(-60_000)).toBe("just now");
  expect(at(60_000)).toBe("1 minute ago");
  expect(at(DAY)).toBe("1 day ago");
  expect(at(60 * DAY)).toBe("2 months ago");
  expect(at(800 * DAY)).toBe("2 years ago");
  expect(relativeTime("not a date", NOW)).toBe("at an unknown time");
}

function assistantShortcutCopy() {
  expect(assistantShortcut(0)).toBe("Watching your space goal");
  expect(assistantShortcut(4)).toBe("4 suggestions waiting");
}

function largeFilesCopyComesFromTheCount() {
  expect(largeFilesShortcut(8)).toBe("8 files over 2 GB");
  expect(largeFilesShortcut(1)).toBe("1 file over 2 GB");
  expect(largeFilesShortcut(0)).toBe("No files over 2 GB");
}

function headingMatchesTheSnapshot() {
  expect(IDLE_HEADING).toBe("Let’s clear some room.");
}

describe("idle copy", () => {
  it("description is truthful to trash-by-default (REQ-DCLEAN-003.A5)", descriptionFollowsTrashMode);
  it("kicker reads No scans yet without a snapshot (REQ-DCLEAN-003.A2)", kickerReadsNoScansYetWithoutASnapshot);
  it("kicker uses the newest snapshot's timestamp (REQ-DCLEAN-003.A2)", kickerUsesTheSnapshotTimestamp);
  it("relative time uses whole units (REQ-DCLEAN-003.A2)", relativeTimeUnits);
  it("Assistant shortcut copy follows the suggestion count (REQ-DCLEAN-003.A3)", assistantShortcutCopy);
  it("Large files copy comes from the count (REQ-DCLEAN-003.A4)", largeFilesCopyComesFromTheCount);
  it("heading is the snapshot's (REQ-DCLEAN-003)", headingMatchesTheSnapshot);
});

function notificationCopyMatchesTheSnapshot() {
  expect(notificationText(74e9, 80e9, 6.4e9)).toBe(
    "You’re down to 74 GB free, under your 80 GB goal. I can clear 6.4 GB of safe junk once you confirm.",
  );
  expect(NOTIFICATION_LATER).toBe("Later");
  expect(NOTIFICATION_REVIEW).toBe("Review cleanup");
  expect(notificationSender(null)).toBe("Assistant");
  expect(notificationSender("Claude Code")).toBe("Assistant · Claude Code");
}

describe("below-goal notification copy", () => {
  it("text and actions are the snapshot's (REQ-DCLEAN-010)", notificationCopyMatchesTheSnapshot);
});
