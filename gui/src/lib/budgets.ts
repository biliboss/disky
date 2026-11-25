/**
 * Render and interaction budgets (REQ-DCLEAN-030), in one place. The app
 * reads the item cap and the spinner threshold; the Playwright latency test
 * (tests/visual/perf.spec.ts) reads the time budgets and the mark name.
 *
 * @implements REQ-DCLEAN-030
 */

/** A1: the idle phase is interactive within 1 s of launch. */
export const IDLE_INTERACTIVE_BUDGET_MS = 1000;

/** A2: selection toggles, expand/collapse and the footer total update within 100 ms. */
export const INTERACTION_BUDGET_MS = 100;

/** AGENTS.md § Design Feel: a spinner appears only when work crosses 200 ms (REQ-DCLEAN-017.A2). */
export const SPINNER_THRESHOLD_MS = 200;

/** A3: a category lists at most its 200 largest items, then "+N more". */
export const RESULTS_ITEM_CAP = 200;

/** The performance mark set when the idle phase first becomes interactive (A1). */
export const IDLE_INTERACTIVE_MARK = "disky:idle-interactive";
