/**
 * Shared disk state (REQ-DCLEAN-002): the one place the meter, the usage
 * ring and the goal tag read disk usage from. Fed only by engine payloads —
 * a loaded or scanned view, then each clean's post-clean `disk`
 * (REQ-DCLEAN-002.A4) — so every surface updates together without a manual
 * refresh. `null` until the first view arrives.
 */
import { createSignal } from "solid-js";
import type { CleanResult, CleanupView, Disk } from "./contract";

export interface DiskState {
  disk: Disk;
  goalBytes: number;
}

const [state, setState] = createSignal<DiskState | null>(null);

/** Current disk state, or `null` before any view has loaded. */
export const diskState = state;

/** Take disk usage and the goal from a loaded or scanned view. */
export function setFromView(view: CleanupView): void {
  setState({ disk: view.disk, goalBytes: view.goal_bytes });
}

/** Replace disk usage with a fresh engine reading; the goal is kept. */
export function setDisk(disk: Disk): void {
  setState((s) => (s ? { ...s, disk } : s));
}

/** Replace disk usage with a clean's post-clean reading; the goal is kept. */
export function setFromClean(result: CleanResult): void {
  setDisk(result.disk);
}

/** Forget the state (tests). */
export function resetDiskState(): void {
  setState(null);
}
