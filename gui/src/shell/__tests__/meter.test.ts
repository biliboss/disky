import { describe, expect, it } from "vitest";
import { goalPos, usedFraction } from "../../lib/meter";
import { formatBytes } from "../../lib/format";

const GB = 1e9;

function goalPosMatchesSnapshotFixture() {
  // snapshot: TOTAL 512, goal 80 → (512 − 80) / 512
  expect(goalPos(512 * GB, 80 * GB)).toBeCloseTo(432 / 512, 10);
}

function goalPosClampsToTheBar() {
  expect(goalPos(512 * GB, 0)).toBe(1);
  expect(goalPos(512 * GB, 600 * GB)).toBe(0);
  expect(goalPos(512 * GB, -5 * GB)).toBe(1);
}

function goalPosIsZeroWithoutCapacity() {
  expect(goalPos(0, 80 * GB)).toBe(0);
  expect(goalPos(-1, 80 * GB)).toBe(0);
  expect(goalPos(Number.NaN, 80 * GB)).toBe(0);
}

function usedFractionMatchesSnapshotFixture() {
  expect(usedFraction(512 * GB, 74 * GB)).toBeCloseTo(438 / 512, 10);
  expect(usedFraction(0, 0)).toBe(0);
}

function formatBytesFollowsDecimalRules() {
  expect(formatBytes(74 * GB)).toBe("74 GB");
  expect(formatBytes(80 * GB)).toBe("80 GB");
  expect(formatBytes(4.25 * GB)).toBe("4.3 GB");
  expect(formatBytes(512 * GB)).toBe("512 GB");
  expect(formatBytes(1)).toBe("1 MB");
  expect(formatBytes(600e6)).toBe("600 MB");
}

describe("disk meter math", () => {
  it("goal marker sits at (capacity − goal) / capacity (REQ-DCLEAN-002.A3)", goalPosMatchesSnapshotFixture);
  it("goal marker clamps to the bar (REQ-DCLEAN-002.A3)", goalPosClampsToTheBar);
  it("goal marker is 0 with no capacity (REQ-DCLEAN-002.A3)", goalPosIsZeroWithoutCapacity);
  it("used share matches the snapshot fixture (REQ-DCLEAN-032)", usedFractionMatchesSnapshotFixture);
  it("sizes format in decimal units (REQ-DCLEAN-028.A1)", formatBytesFollowsDecimalRules);
});
