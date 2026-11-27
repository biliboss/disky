/**
 * Shared Smart Clean phase state (REQ-DCLEAN-011). The one place the phase
 * record lives: Smart Clean writes it, and the shell's navigation reads it
 * to block itself while a scan or clean runs. Owned by its own root, so it
 * outlives the Smart Clean route: the session's notification dismissal
 * (REQ-DCLEAN-010.A2) survives a trip to another screen.
 */
import { createRoot, createSignal } from "solid-js";
import { INITIAL_STATE, navBlocked, type SmartCleanState } from "./phase";

const [state, setState] = createRoot(() => createSignal<SmartCleanState>(INITIAL_STATE));

/** The current Smart Clean state. */
export const smartCleanState = state;

/** Replace the Smart Clean state, or apply a transition to it. */
export const setSmartCleanState = setState;

/**
 * Navigation is blocked now (REQ-DCLEAN-011.A1).
 *
 * @implements REQ-DCLEAN-011
 */
export function isNavBlocked(): boolean {
  return navBlocked(state().phase);
}

/** Back to the initial state (tests). */
export function resetSmartCleanStore(): void {
  setState(INITIAL_STATE);
}
