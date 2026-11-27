/**
 * Disk-meter geometry (REQ-DCLEAN-002). Pure functions over exact bytes;
 * percentages are produced only here, strings only at render time.
 */

const clamp01 = (x: number) => Math.min(1, Math.max(0, x));

/**
 * Goal-marker position along the bar, as a 0..1 fraction:
 * (capacity − goal) / capacity (REQ-DCLEAN-002.A3). Clamped to the bar;
 * a non-positive capacity yields 0 (no bar to place a marker on).
 */
export function goalPos(capacityBytes: number, goalBytes: number): number {
  if (!(capacityBytes > 0)) return 0;
  return clamp01((capacityBytes - goalBytes) / capacityBytes);
}

/** Used share of the volume, as a 0..1 fraction. */
export function usedFraction(capacityBytes: number, freeBytes: number): number {
  if (!(capacityBytes > 0)) return 0;
  return clamp01((capacityBytes - freeBytes) / capacityBytes);
}

/** Goal tag state: "below" only when free < goal, strictly (REQ-DCLEAN-002.A2). */
export type GoalStatus = "below" | "met";

export function goalStatus(freeBytes: number, goalBytes: number): GoalStatus {
  return freeBytes < goalBytes ? "below" : "met";
}

/** Circumference of the usage ring: r = 50 in its 120-unit viewBox (snapshot `C2`). */
export const RING_CIRCUMFERENCE = 2 * Math.PI * 50;

/**
 * SVG `stroke-dasharray` for the usage ring: the used share of the
 * circumference, then a full-length gap (snapshot `ringDash`). No capacity
 * draws no fill.
 */
export function ringDash(capacityBytes: number, freeBytes: number, circumference: number): string {
  return `${usedFraction(capacityBytes, freeBytes) * circumference} ${circumference}`;
}
