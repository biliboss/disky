import type { CleanMode } from "../../lib/contract";
import { NUMERIC_CLASS } from "../../lib/format";
import { CLEANING_HEADING, stepLabel } from "./cleaningCopy";
import type { SmartCleanState } from "./phase";

export interface CleaningProps {
  state: SmartCleanState;
  mode: CleanMode;
  /** Display name of the category being processed (from the engine view). */
  categoryName: string;
}

/**
 * Cleaning phase (REQ-DCLEAN-008): "Tidying up…", the step label naming the
 * category in progress in selection order (A1) with the mode's verb (A3),
 * the percentage and an accent-2 progress bar (A2). No cancel is offered
 * (A4). This screen is the clean's loading state (REQ-DCLEAN-017.A1).
 * Layout follows snapshot.html "Smart clean · cleaning".
 *
 * @implements REQ-DCLEAN-008
 * @implements REQ-DCLEAN-017
 */
export function Cleaning(props: CleaningProps) {
  const pct = () => Math.round(props.state.progress * 100);
  return (
    <div class="cleaning" data-testid="cleaning" data-screen-label="Smart clean · cleaning">
      <h1 class="cleaning-title" tabIndex={-1} data-phase-heading>{CLEANING_HEADING}</h1>
      <div class="scan-meta" data-testid="clean-meta">
        <span class="scan-path" data-testid="clean-step">
          {stepLabel(props.mode, props.categoryName)}
        </span>
        <span class={NUMERIC_CLASS} data-testid="clean-progress">
          {pct()}%
        </span>
      </div>
      <div
        class="scan-bar"
        role="progressbar"
        aria-label="Clean progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct()}
      >
        <div class="clean-bar-fill" data-testid="clean-bar-fill" style={{ width: `${props.state.progress * 100}%` }} />
      </div>
    </div>
  );
}
