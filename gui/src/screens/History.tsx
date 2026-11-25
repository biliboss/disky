import { createResource, createSignal, For, Show } from "solid-js";
import Trash from "lucide-solid/icons/trash";
import { listHistory, restore } from "../lib/engine";
import { parseEngineError, type EngineError, type HistoryRow } from "../lib/contract";
import { useDelayedBusy } from "../lib/busy";
import { formatBytes, NUMERIC_CLASS } from "../lib/format";
import {
  EMPTY,
  HISTORY_TITLE,
  RESTORE,
  RESTORE_BUSY,
  TAG_CLASS,
  TAGS,
  headerLine,
  restoreLabel,
  restoreNotices,
  rowMeta,
  type TaggedState,
} from "./history/historyCopy";
import "./history/history.css";

/**
 * The History screen (REQ-DSCR-015): every recorded clean, newest first,
 * from the engine's `list_history` (the rows `disky history` prints). The
 * engine decides each row's detail and state; Restore runs `disky restore`
 * through the backend, which also refreshes the meter, then the log is
 * reloaded. A partial restore (items no longer in the Trash) is reported
 * per item and its row keeps "Restore", as the engine retries the rest.
 *
 * @implements REQ-DSCR-015
 */
export function History() {
  const [view, { refetch }] = createResource(listHistory);
  const [restoring, setRestoring] = createSignal<string | null>(null);
  const [notices, setNotices] = createSignal<string[]>([]);
  const [error, setError] = createSignal<EngineError | null>(null);
  /** The last loaded log; never throws (an errored load reads as none). */
  const latest = () => (view.state === "errored" ? undefined : view.latest);
  const busy = useDelayedBusy(() => view.loading && !latest());

  const loadError = () => (view.state === "errored" ? parseEngineError(view.error) : null);

  async function restoreRow(row: HistoryRow) {
    setError(null);
    setNotices([]);
    setRestoring(row.id);
    try {
      const r = await restore(row.id);
      setNotices(restoreNotices(r.report));
    } catch (e) {
      setError(parseEngineError(e));
    } finally {
      setRestoring(null);
      await refetch();
    }
  }

  return (
    <section class="history" data-screen-label="History" aria-labelledby="history-title" data-testid="history">
      <div class="history-head">
        <Show when={latest()}>{(v) => <p class="history-kicker">{headerLine(v().total_bytes)}</p>}</Show>
        <h1 class="history-title" id="history-title">
          {HISTORY_TITLE}
        </h1>
      </div>
      <Show when={loadError() ?? error()}>
        {(e) => (
          <p class="history-error" role="alert">
            {e().detail}
          </p>
        )}
      </Show>
      <Show when={notices().length > 0}>
        <ul class="history-notices" role="status" data-testid="restore-notices">
          <For each={notices()}>{(n) => <li>{n}</li>}</For>
        </ul>
      </Show>
      <Show when={busy()}>
        <div class="busy" role="status" aria-busy="true" data-testid="busy">
          <span class="busy-spinner" aria-hidden="true" />
          Loading…
        </div>
      </Show>
      <Show when={latest()}>
        {(v) => (
          <Show when={v().rows.length > 0} fallback={<p class="history-empty">{EMPTY}</p>}>
            <ul class="history-list" aria-labelledby="history-title">
              <For each={v().rows}>
                {(row) => (
                  <li class="history-row" data-testid="history-row">
                    <span class="history-icon" aria-hidden="true">
                      <Trash class="history-glyph" />
                    </span>
                    <div class="history-text">
                      <span class="history-name">{row.title}</span>
                      <span class="history-meta">{rowMeta(row.ts, row.detail)}</span>
                    </div>
                    <span class={`history-size ${NUMERIC_CLASS}`}>{formatBytes(row.bytes)}</span>
                    <Show
                      when={row.state !== "restorable" && (row.state as TaggedState)}
                      fallback={
                        <button
                          type="button"
                          class="btn btn-secondary"
                          aria-label={restoreLabel(row.title, row.ts)}
                          disabled={restoring() !== null}
                          onClick={() => restoreRow(row)}
                        >
                          {restoring() === row.id ? RESTORE_BUSY : RESTORE}
                        </button>
                      }
                    >
                      {(st) => <span class={TAG_CLASS[st()]}>{TAGS[st()]}</span>}
                    </Show>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        )}
      </Show>
    </section>
  );
}
