import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { useNavigate } from "@solidjs/router";
import Check from "lucide-solid/icons/check";
import { loadLarge, trashLarge } from "../lib/engine";
import { LARGE_FILTERS, parseEngineError, type EngineError, type LargeFile, type LargeFilter } from "../lib/contract";
import { useDelayedBusy } from "../lib/busy";
import { formatBytes, NUMERIC_CLASS } from "../lib/format";
import {
  COL_LOCATION,
  COL_NAME,
  COL_OPENED,
  COL_SELECT,
  COL_SIZE,
  EMPTY,
  FILTER_GROUP,
  FILTER_LABELS,
  LARGE_TITLE,
  NO_SCANS,
  NO_SCANS_NOTE,
  START_SCAN,
  TRASH_BUSY,
  failureMessage,
  kicker,
  openedLabel,
  openedTagClass,
  rowsBytes,
  selectionNote,
  trashLabel,
} from "./largeFiles/largeCopy";
import "./largeFiles/largeFiles.css";

/**
 * The Large files screen (REQ-DSCR-013): files of at least 2 GB from the
 * engine's `load_large` (`disky large`), filtered All / Over 10 GB / Unused
 * 1 year+ by the engine. Rows toggle on click, and each row's round check is
 * a `role="checkbox"` button (Space / Enter). "Move <total> to Trash" sends
 * exactly the selected paths to `trash_large` (REQ-DSCR-006) — always the
 * Trash, whatever trash-by-default says. Handled rows leave the list, the
 * meter follows the engine's post-move disk, and failed rows stay with
 * their reason.
 *
 * @implements REQ-DSCR-013
 */
export function LargeFiles() {
  const navigate = useNavigate();
  const [filter, setFilter] = createSignal<LargeFilter>("all");
  const [view] = createResource(filter, loadLarge);
  const [selected, setSelected] = createSignal<ReadonlySet<string>>(new Set());
  /** Paths moved to the Trash this visit: the snapshot still lists them. */
  const [gone, setGone] = createSignal<ReadonlySet<string>>(new Set());
  /** Failed paths, with the engine's reason. */
  const [failures, setFailures] = createSignal<ReadonlyMap<string, string>>(new Map());
  const [error, setError] = createSignal<EngineError | null>(null);
  const [trashing, setTrashing] = createSignal(false);

  const latest = () => (view.state === "errored" ? undefined : view.latest);
  const busy = useDelayedBusy(() => view.loading && !latest());
  const loadError = () => (view.state === "errored" ? parseEngineError(view.error) : null);
  const noSnapshot = () => view.state === "ready" && view() === null;

  const rows = createMemo<LargeFile[]>(() => (latest()?.records ?? []).filter((f) => !gone().has(f.path)));
  const picked = createMemo(() => rows().filter((f) => selected().has(f.path)));

  function toggle(path: string) {
    const next = new Set(selected());
    if (next.has(path)) next.delete(path);
    else next.add(path);
    setSelected(next);
  }

  function pickFilter(f: LargeFilter) {
    setError(null);
    setFilter(f);
  }

  async function moveToTrash() {
    const paths = picked().map((f) => f.path);
    if (paths.length === 0) return;
    setError(null);
    setTrashing(true);
    try {
      const r = await trashLarge(paths);
      setGone(new Set([...gone(), ...r.handled]));
      const failed = new Map(failures());
      for (const p of r.handled) failed.delete(p);
      for (const f of r.failed) failed.set(f.path, f.reason);
      setFailures(failed);
      setSelected(new Set([...selected()].filter((p) => !r.handled.includes(p))));
    } catch (e) {
      setError(parseEngineError(e));
    } finally {
      setTrashing(false);
    }
  }

  return (
    <Show
      when={!noSnapshot()}
      fallback={
        <section class="screen" data-screen-label="Large files" aria-labelledby="large-title" data-testid="large-files">
          <h1 class="screen-title" id="large-title">
            {NO_SCANS}
          </h1>
          <p class="screen-note">{NO_SCANS_NOTE}</p>
          <div>
            <button type="button" class="btn btn-primary" onClick={() => navigate("/")}>
              {START_SCAN}
            </button>
          </div>
        </section>
      }
    >
      <section class="large" data-screen-label="Large files" aria-labelledby="large-title" data-testid="large-files">
        <div class="large-head">
          <div class="large-heading">
            <Show when={latest()}>
              <p class="large-kicker" data-testid="large-kicker">
                {kicker(rows())}
              </p>
            </Show>
            <h1 class="large-title" id="large-title">
              {LARGE_TITLE}
            </h1>
          </div>
          <div class="seg" role="radiogroup" aria-label={FILTER_GROUP}>
            <For each={LARGE_FILTERS}>
              {(f) => (
                <label class="seg-opt">
                  <input
                    type="radio"
                    name="large-filter"
                    value={f}
                    checked={filter() === f}
                    onChange={() => pickFilter(f)}
                  />
                  <span>{FILTER_LABELS[f]}</span>
                </label>
              )}
            </For>
          </div>
        </div>
        <div class="large-body">
          <Show when={loadError() ?? error()}>
            {(e) => (
              <p class="large-error" role="alert">
                {e().detail}
              </p>
            )}
          </Show>
          <Show when={busy()}>
            <div class="busy" role="status" aria-busy="true" data-testid="busy">
              <span class="busy-spinner" aria-hidden="true" />
              Loading…
            </div>
          </Show>
          <Show when={latest()}>
            <Show when={rows().length > 0} fallback={<p class="large-empty">{EMPTY}</p>}>
              <table class="table" aria-labelledby="large-title">
                <thead>
                  <tr>
                    <th class="large-check-col">
                      <span class="sr-only">{COL_SELECT}</span>
                    </th>
                    <th>{COL_NAME}</th>
                    <th>{COL_LOCATION}</th>
                    <th>{COL_OPENED}</th>
                    <th class="large-size">{COL_SIZE}</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={rows()}>
                    {(f) => {
                      const on = () => selected().has(f.path);
                      const failure = () => failures().get(f.path);
                      return (
                        <tr
                          class="large-row"
                          data-testid="large-row"
                          data-selected={String(on())}
                          onClick={() => toggle(f.path)}
                        >
                          <td>
                            <button
                              type="button"
                              role="checkbox"
                              class="result-check"
                              aria-checked={on()}
                              aria-label={f.name}
                              onClick={(e) => {
                                e.stopPropagation();
                                toggle(f.path);
                              }}
                            >
                              <Show when={on()}>
                                <Check class="result-check-glyph" aria-hidden="true" />
                              </Show>
                            </button>
                          </td>
                          <td class="large-name">
                            {f.name}
                            <Show when={failure()}>
                              {(reason) => (
                                <span class="large-row-error" role="alert">
                                  {failureMessage(reason())}
                                </span>
                              )}
                            </Show>
                          </td>
                          <td class="large-location">{f.location}</td>
                          <td>
                            <span class={openedTagClass(f.unused_1y)} data-testid="large-opened">
                              {openedLabel(f.last_opened)}
                            </span>
                          </td>
                          <td class={`large-size ${NUMERIC_CLASS}`}>{formatBytes(f.bytes)}</td>
                        </tr>
                      );
                    }}
                  </For>
                </tbody>
              </table>
            </Show>
          </Show>
        </div>
        <div class="large-foot">
          <span class="large-foot-note" data-testid="large-note">
            {selectionNote(picked().length)}
          </span>
          <button
            type="button"
            class="btn btn-primary large-trash"
            disabled={picked().length === 0 || trashing()}
            onClick={moveToTrash}
          >
            {trashing() ? TRASH_BUSY : trashLabel(rowsBytes(picked()))}
          </button>
        </div>
      </section>
    </Show>
  );
}
