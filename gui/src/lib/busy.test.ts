import { createRoot, createSignal } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SPINNER_THRESHOLD_MS, useDelayedBusy } from "./busy";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** Effects run once the root's body returns, so drive the helper from outside it. */
function setup() {
  return createRoot((dispose) => {
    const [active, setActive] = createSignal(true);
    return { busy: useDelayedBusy(active), setActive, dispose };
  });
}

function busyAppearsOnlyAfterTheThreshold() {
  const { busy, setActive, dispose } = setup();
  expect(SPINNER_THRESHOLD_MS).toBe(200);
  vi.advanceTimersByTime(SPINNER_THRESHOLD_MS - 1);
  expect(busy()).toBe(false);
  vi.advanceTimersByTime(1);
  expect(busy()).toBe(true);
  setActive(false);
  expect(busy()).toBe(false);
  dispose();
}

function aShortWaitNeverShowsBusy() {
  const { busy, setActive, dispose } = setup();
  vi.advanceTimersByTime(150);
  setActive(false);
  vi.advanceTimersByTime(1_000);
  expect(busy()).toBe(false);
  // A new wait starts its own threshold.
  setActive(true);
  vi.advanceTimersByTime(150);
  expect(busy()).toBe(false);
  dispose();
}

describe("delayed busy indicator (REQ-DCLEAN-017.A2)", () => {
  it("turns on only after the 200 ms spinner threshold", busyAppearsOnlyAfterTheThreshold);
  it("a wait shorter than the threshold never shows busy", aShortWaitNeverShowsBusy);
});
