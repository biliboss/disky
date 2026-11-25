import { For, Show } from "solid-js";
import type { CleanResult } from "../../lib/contract";
import type { DiskState } from "../../lib/diskState";
import { GoalTag } from "../../shell/GoalTag";
import { UsageRing } from "../../shell/UsageRing";
import { UNDO_BUSY, canUndo, doneHeading, doneNote, failedHeading, failureReason, undoFailedMessage } from "./doneCopy";

export interface DoneProps {
  result: CleanResult;
  disk: DiskState;
  /** An Undo is running: every action is disabled. */
  undoing: boolean;
  /** The Undo has run past the spinner threshold (REQ-DCLEAN-017.A2). */
  undoBusy: boolean;
  /** Why the last Undo failed, if it did. */
  undoError: string | null;
  onHome: () => void;
  onUndo: () => void;
  onHistory: () => void;
}

/**
 * Done phase (REQ-DCLEAN-009): the "Done" tag, "<freed> cleared" from the
 * bytes actually reclaimed (A1), the mode's note (A2), "Back to home",
 * "Undo" only after a Trash clean (A3) and "View history" (A6), and the ring
 * with "now free". Paths the clean could not handle are listed, untouched
 * (REQ-DCLEAN-018.A3). Layout follows snapshot.html "Smart clean · done".
 *
 * @implements REQ-DCLEAN-009
 * @implements REQ-DCLEAN-018
 */
export function Done(props: DoneProps) {
  const r = () => props.result;
  return (
    <div class="done" data-testid="done" data-screen-label="Smart clean · done">
      <div class="done-main">
        <span class="tag tag-accent-2 done-tag">Done</span>
        <h1 class="done-title" tabIndex={-1} data-phase-heading data-testid="done-heading">
          {doneHeading(r().freed_bytes)}
        </h1>
        <p class="done-note" data-testid="done-note">
          {doneNote(r().mode)}
        </p>
        <Show when={r().failed.length > 0}>
          <div class="done-failed error-state" role="alert" data-testid="done-failed">
            <span>{failedHeading(r().failed.length)}</span>
            <ul class="done-failed-list">
              <For each={r().failed}>
                {(f) => (
                  <li class="done-failed-path" title={f.path}>
                    {f.path} — {failureReason(f)}
                  </li>
                )}
              </For>
            </ul>
          </div>
        </Show>
        <Show when={props.undoError}>
          <p class="error-state" role="alert" data-testid="undo-error">
            {undoFailedMessage(props.undoError!)}
          </p>
        </Show>
        <div class="done-actions">
          <button type="button" class="btn btn-primary done-home" disabled={props.undoing} onClick={() => props.onHome()}>
            Back to home
          </button>
          <Show when={canUndo(r().mode, r().history_id)}>
            <button
              type="button"
              class="btn btn-secondary done-undo"
              data-testid="undo"
              disabled={props.undoing}
              aria-busy={props.undoBusy}
              onClick={() => props.onUndo()}
            >
              {props.undoBusy ? UNDO_BUSY : "Undo"}
            </button>
          </Show>
          <button type="button" class="btn btn-ghost" disabled={props.undoing} onClick={() => props.onHistory()}>
            View history
          </button>
        </div>
      </div>
      <div class="done-rail">
        <UsageRing disk={props.disk.disk} phase="done" />
        <GoalTag freeBytes={props.disk.disk.free_bytes} goalBytes={props.disk.goalBytes} />
      </div>
    </div>
  );
}
