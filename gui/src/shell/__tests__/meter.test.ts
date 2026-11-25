import { describe, expect, it } from "vitest";
import { RING_CIRCUMFERENCE, goalPos, goalStatus, ringDash, usedFraction } from "../../lib/meter";
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

function goalStatusIsBelowOnlyStrictlyUnderTheGoal() {
  expect(goalStatus(80 * GB, 80 * GB)).toBe("met");
  expect(goalStatus(80 * GB - 1, 80 * GB)).toBe("below");
  expect(goalStatus(81 * GB, 80 * GB)).toBe("met");
  expect(goalStatus(0, 0)).toBe("met");
}

function ringDashIsTheUsedShareOfTheCircumference() {
  const c = RING_CIRCUMFERENCE;
  expect(c).toBeCloseTo(2 * Math.PI * 50, 10);
  // snapshot: `${C2 * used / TOTAL} ${C2}` with used 438 of 512
  const [fill, gap] = ringDash(512 * GB, 74 * GB, c).split(" ").map(Number);
  expect(fill).toBeCloseTo((c * 438) / 512, 10);
  expect(gap).toBeCloseTo(c, 10);
  expect(ringDash(0, 0, c)).toBe(`0 ${c}`);
  expect(ringDash(512 * GB, 512 * GB, c)).toBe(`0 ${c}`);
}

describe("disk meter math", () => {
  it("goal status is below only when free < goal (REQ-DCLEAN-002.A2)", goalStatusIsBelowOnlyStrictlyUnderTheGoal);
  it("ring dash is the used share of the circumference (REQ-DCLEAN-002)", ringDashIsTheUsedShareOfTheCircumference);
  it("goal marker sits at (capacity − goal) / capacity (REQ-DCLEAN-002.A3)", goalPosMatchesSnapshotFixture);
  it("goal marker clamps to the bar (REQ-DCLEAN-002.A3)", goalPosClampsToTheBar);
  it("goal marker is 0 with no capacity (REQ-DCLEAN-002.A3)", goalPosIsZeroWithoutCapacity);
  it("used share matches the snapshot fixture (REQ-DCLEAN-032)", usedFractionMatchesSnapshotFixture);
  it("sizes format in decimal units (REQ-DCLEAN-028.A1)", formatBytesFollowsDecimalRules);
});
