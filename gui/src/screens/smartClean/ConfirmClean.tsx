import { For, createSignal, createUniqueId, onCleanup, onMount } from "solid-js";
import { Portal } from "solid-js/web";
import type { CategoryView, CleanMode } from "../../lib/contract";
import { NUMERIC_CLASS, formatBytes } from "../../lib/format";
import { MODE_LABELS, confirmTitle, initialMode, modeNote } from "./resultsCopy";

export interface ConfirmCleanProps {
  /** The selected categories, in list order. */
  categories: readonly Pick<CategoryView, "id" | "name" | "bytes">[];
  /** The footer's selected total (REQ-DCLEAN-007.A5). */
  total: number;
  trashByDefault: boolean;
  /** Where focus goes back on close when the opener is gone (REQ-DCLEAN-022.A4). */
  fallbackFocus?: () => HTMLElement | null | undefined;
  onCancel: () => void;
  onConfirm: (mode: CleanMode) => void;
}

const MODES: readonly CleanMode[] = ["trash", "delete"]; // lint-boundary-allow: clean mode, not a category

/** Focusable controls inside the dialog, in tab order. Only the checked radio of a group is a tab stop. */
function tabStops(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled])")].filter(
    (el) => !(el instanceof HTMLInputElement && el.type === "radio" && !el.checked),
  );
}

/**
 * The confirm-clean modal (REQ-DCLEAN-007): "Clean <total>?", each selected
 * category with its size, the Trash / Delete mode switch (starting at the
 * trash-by-default preference) with its note, and "Cancel" / "Clean now".
 * Nothing is cleaned until "Clean now"; Cancel, Escape and the backdrop
 * close it. "Delete permanently" gives "Clean now" the danger fill.
 *
 * Focus (REQ-DCLEAN-022): exposed as a modal labelled by its title, opens on
 * "Cancel", traps Tab / Shift+Tab, makes the rest of the page inert, and on
 * close returns focus to the opener (or `fallbackFocus`, e.g. the results
 * heading when opened from the notification). Layout follows snapshot.html's
 * confirm dialog.
 *
 * @implements REQ-DCLEAN-007
 * @implements REQ-DCLEAN-022
 */
export function ConfirmClean(props: ConfirmCleanProps) {
  const [mode, setMode] = createSignal<CleanMode>(initialMode(props.trashByDefault));
  const titleId = createUniqueId();
  const radioName = `clean-mode-${titleId}`;
  // The control that opened the dialog; none when nothing had focus (e.g. the notification path).
  const active = document.activeElement;
  const opener = active instanceof HTMLElement && active !== document.body ? active : null;
  let dialog!: HTMLDivElement;
  let cancel!: HTMLButtonElement;
  const inerted: HTMLElement[] = [];

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      props.onCancel();
      return;
    }
    if (e.key !== "Tab") return;
    const stops = tabStops(dialog);
    if (stops.length === 0) return;
    const first = stops[0];
    const last = stops[stops.length - 1];
    const at = document.activeElement;
    if (e.shiftKey && (at === first || !dialog.contains(at))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (at === last || !dialog.contains(at))) {
      e.preventDefault();
      first.focus();
    }
  }

  /** Focus that lands outside the dialog (e.g. by pointer) is pulled back in. */
  function onFocusIn(e: FocusEvent) {
    if (e.target instanceof Node && !dialog.contains(e.target)) cancel.focus();
  }

  /** Arrow keys move between the mode options (REQ-DCLEAN-020.A3). */
  function onSegKeyDown(e: KeyboardEvent) {
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (step === 0) return;
    e.preventDefault();
    const next = MODES[(MODES.indexOf(mode()) + step + MODES.length) % MODES.length];
    setMode(next);
    dialog.querySelector<HTMLInputElement>(`input[value="${next}"]`)?.focus();
  }

  onMount(() => {
    // Everything outside the dialog's portal is inert while it is open.
    const host = [...document.body.children].find((el) => el.contains(dialog));
    for (const el of document.body.children) {
      if (el !== host && el instanceof HTMLElement && !el.hasAttribute("inert")) {
        el.setAttribute("inert", "");
        inerted.push(el);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("focusin", onFocusIn);
    cancel.focus();
  });

  onCleanup(() => {
    document.removeEventListener("keydown", onKeyDown);
    document.removeEventListener("focusin", onFocusIn);
    for (const el of inerted) el.removeAttribute("inert");
    const back = opener?.isConnected ? opener : props.fallbackFocus?.();
    back?.focus();
  });

  return (
    <Portal>
      <div
        class="dialog-backdrop confirm-backdrop"
        data-testid="confirm-backdrop"
        onClick={(e) => {
          if (e.target === e.currentTarget) props.onCancel();
        }}
      >
        <div
          ref={dialog}
          class="dialog confirm-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          data-testid="confirm-dialog"
        >
          <h2 id={titleId} class="dialog-title confirm-title">
            {confirmTitle(props.total)}
          </h2>
          <ul class="dialog-body confirm-list" role="list" data-testid="confirm-categories">
            <For each={props.categories}>
              {(c) => (
                <li class="confirm-row" data-category-id={c.id}>
                  <span>{c.name}</span>
                  <span class={NUMERIC_CLASS}>{formatBytes(c.bytes)}</span>
                </li>
              )}
            </For>
          </ul>
          <div class="seg confirm-seg" role="radiogroup" aria-label="Clean mode" onKeyDown={onSegKeyDown}>
            <For each={MODES}>
              {(m) => (
                <label class="seg-opt" data-mode={m}>
                  <input
                    type="radio"
                    name={radioName}
                    value={m}
                    checked={mode() === m}
                    onChange={() => setMode(m)}
                  />
                  <span>{MODE_LABELS[m]}</span>
                </label>
              )}
            </For>
          </div>
          <p class="confirm-note" data-testid="mode-note">
            {modeNote(mode())}
          </p>
          <div class="dialog-actions">
            <button ref={cancel} type="button" class="btn btn-secondary" onClick={() => props.onCancel()}>
              Cancel
            </button>
            <button
              type="button"
              class="btn btn-primary"
              classList={{ "btn-danger": mode() === "delete" }}
              data-testid="clean-now"
              onClick={() => props.onConfirm(mode())}
            >
              Clean now
            </button>
          </div>
        </div>
      </div>
    </Portal>
  );
}
