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
