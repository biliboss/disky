import { describe, expect, it } from "vitest";
import {
  INITIAL_STATE,
  scanFinished,
  scanProgress,
  scanStopped,
  startScan,
  type SmartCleanState,
} from "../smartClean/phase";

function startScanResetsProgressCollapsesAndDismisses() {
  const from: SmartCleanState = { phase: "results", progress: 1, expanded: "app_caches", notificationDismissed: false };
  expect(startScan(from)).toEqual({ phase: "scanning", progress: 0, expanded: null, notificationDismissed: true });
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
  s = scanProgress(s, 0.5);
  s = scanProgress(s, 0.3);
  expect(s.progress).toBe(0.5);
  expect(scanProgress(s, 4).progress).toBe(1);
  expect(scanProgress(INITIAL_STATE, 0.5)).toBe(INITIAL_STATE);
}

function stoppedScanIgnoresALateFinish() {
  const stopped = scanStopped(scanProgress(startScan(INITIAL_STATE), 0.7));
  expect(stopped).toMatchObject({ phase: "idle", progress: 0 });
  expect(scanFinished(stopped)).toBe(stopped);
  expect(scanFinished(startScan(INITIAL_STATE))).toMatchObject({ phase: "results", progress: 1 });
}

describe("Smart Clean phase machine", () => {
  it("Start scan enters scanning at 0, collapses and dismisses (REQ-DCLEAN-003.A1)", startScanResetsProgressCollapsesAndDismisses);
  it("Start scan is ignored while scanning or cleaning (REQ-DCLEAN-003.A1)", startScanIsIgnoredWhileBusy);
  it("scan progress never goes backwards (REQ-DCLEAN-003)", scanProgressNeverGoesBackwards);
  it("a stopped scan ignores a late finish (REQ-DCLEAN-003)", stoppedScanIgnoresALateFinish);
});
