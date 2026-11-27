/**
 * Loading states that appear only when a wait would otherwise feel stalled
 * (REQ-DCLEAN-017). Short waits — results loading, undo, restore — show a
 * busy indicator only once they pass the spinner threshold (A2). Scanning and
 * cleaning never use this: their phase screens are the loading state (A1).
 */
import { createEffect, createSignal, onCleanup, type Accessor } from "solid-js";
import { SPINNER_THRESHOLD_MS } from "./budgets";

export { SPINNER_THRESHOLD_MS };

/**
 * True once `active` has stayed true for `delayMs`; false again the moment it
 * turns false. Call inside a reactive owner (a component or `createRoot`).
 *
 * @implements REQ-DCLEAN-017
 */
export function useDelayedBusy(active: Accessor<boolean>, delayMs = SPINNER_THRESHOLD_MS): Accessor<boolean> {
  const [busy, setBusy] = createSignal(false);
  createEffect(() => {
    if (!active()) {
      setBusy(false);
      return;
    }
    const t = setTimeout(() => setBusy(true), delayMs);
    onCleanup(() => clearTimeout(t));
  });
  return busy;
}
