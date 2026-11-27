import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { loadDupes, trashDupes } from "../lib/engine";
import { parseEngineError, type DupeGroup, type EngineError } from "../lib/contract";
import { useDelayedBusy } from "../lib/busy";
import {
  DUPES_TITLE,
  EMPTY,
  KEEP,
  NO_SCANS,
  NO_SCANS_NOTE,
  REMOVE,
  REMOVE_BUSY,
  START_SCAN,
  eachLabel,
  extraCopies,
  failureMessage,
  footNote,
  freedBytes,
  kicker,
  removeLabel,
} from "./duplicates/dupesCopy";
import "./duplicates/duplicates.css";

/** A group as shown: the engine's group minus copies trashed this visit. */
interface Shown {
  /** Stable key: the engine's first copy path. */
  id: string;
  group: DupeGroup;
}

/**
 * The Duplicates screen (REQ-DSCR-014): byte-identical files from the
 * engine's `load_dupes` (`disky dupes`), one card per group. Each card's
 * copy rows are a radio group — click, Space or the arrow keys choose the
 * kept copy (the first by default); the rest are marked "Remove". "Remove
 * <n> extra copies" sends every non-kept copy to `trash_dupes`
 * (REQ-DSCR-006), which refuses to take every copy of a group. Handled
 * copies vanish, a group left with one copy vanishes, the meter follows
 * the engine's post-move disk, and failed copies stay with their reason.
 *
 * @implements REQ-DSCR-014
 */
export function Duplicates() {
  const navigate = useNavigate();
  const [view] = createResource(loadDupes);
  /** Kept copy per group id; a group not here keeps its first copy. */
  const [kept, setKept] = createSignal<ReadonlyMap<string, string>>(new Map());
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

  const shown = createMemo<Shown[]>(() =>
    (latest()?.records ?? [])
      .map((g) => ({ id: g.copies[0].path, group: { ...g, copies: g.copies.filter((c) => !gone().has(c.path)) } }))
      .filter((s) => s.group.copies.length >= 2),
  );
  const keptOf = (s: Shown) => {
    const k = kept().get(s.id);
    return k !== undefined && s.group.copies.some((c) => c.path === k) ? k : s.group.copies[0].path;
  };
  const extras = createMemo(() => {
    const list = shown();
    return extraCopies(
      list.map((s) => s.group),
      (i) => keptOf(list[i]),
    );
  });

  function keep(s: Shown, path: string) {
    setKept(new Map(kept()).set(s.id, path));
  }

  /** Arrow keys move the kept copy within one group, wrapping (radio group pattern). */
  function onKey(e: KeyboardEvent, s: Shown, index: number) {
    const step = e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : 0;
    if (step === 0) return;
    e.preventDefault();
    const copies = s.group.copies;
    const next = copies[(index + step + copies.length) % copies.length];
    keep(s, next.path);
    const group = (e.currentTarget as HTMLElement).closest('[role="radiogroup"]');
    group?.querySelector<HTMLElement>(`[data-path="${CSS.escape(next.path)}"]`)?.focus();
  }

  async function removeExtras() {
    const paths = extras().map((c) => c.path);
    if (paths.length === 0) return;
    setError(null);
    setTrashing(true);
    try {
      const r = await trashDupes(paths);
      setGone(new Set([...gone(), ...r.handled]));
      const failed = new Map(failures());
      for (const p of r.handled) failed.delete(p);
      for (const f of r.failed) failed.set(f.path, f.reason);
      setFailures(failed);
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
        <section class="screen" data-screen-label="Duplicates" aria-labelledby="dupes-title" data-testid="duplicates">
          <h1 class="screen-title" id="dupes-title">
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
      <section class="dupes" data-screen-label="Duplicates" aria-labelledby="dupes-title" data-testid="duplicates">
        <div class="dupes-head">
          <Show when={latest()}>
            <p class="dupes-kicker" data-testid="dupes-kicker">
              {kicker(shown().length)}
            </p>
          </Show>
          <h1 class="dupes-title" id="dupes-title">
            {DUPES_TITLE}
          </h1>
        </div>
        <div class="dupes-body">
          <Show when={loadError() ?? error()}>
            {(e) => (
              <p class="dupes-error" role="alert">
                {e().detail}
              </p>
            )}
          </Show>
          <Show when={busy()}>
            <div class="busy" role="status" aria-busy="true" data-testid="busy">
              <span class="busy-spinner" aria-hidden="true" />
              Finding duplicates…
            </div>
          </Show>
          <Show when={latest()}>
            <Show when={shown().length > 0} fallback={<p class="dupes-empty">{EMPTY}</p>}>
              <div class="dupes-grid">
                <For each={shown()}>
                  {(s, gi) => {
                    const titleId = () => `dupes-group-${gi()}`;
                    return (
                      <div class="card" data-testid="dupe-group">
                        <div class="dupes-card-head">
                          <span class="card-title dupes-card-title" id={titleId()} title={s.group.name}>
                            {s.group.name}
                          </span>
                          <span class="tag tag-neutral" data-testid="dupe-each">
                            {eachLabel(s.group.bytes)}
                          </span>
                        </div>
                        <div class="dupes-copies" role="radiogroup" aria-labelledby={titleId()}>
                          <For each={s.group.copies}>
                            {(c, ci) => {
                              const on = () => keptOf(s) === c.path;
                              return (
                                <button
                                    type="button"
                                    role="radio"
                                    class="dupe-copy"
                                    data-testid="dupe-copy"
                                    data-path={c.path}
                                    aria-checked={on()}
                                    tabIndex={on() ? 0 : -1}
                                    title={c.path}
                                    onClick={() => keep(s, c.path)}
                                    onKeyDown={(e) => onKey(e, s, ci())}
                                  >
                                    <span class="dupe-dot" aria-hidden="true" />
                                    <span class="dupe-path">{c.location}</span>
                                    <span class={on() ? "tag tag-accent-2" : "tag tag-outline"}>{on() ? KEEP : REMOVE}</span>
                                </button>
                              );
                            }}
                          </For>
                        </div>
                        <For each={s.group.copies.filter((c) => failures().has(c.path))}>
                          {(c) => (
                            <p class="dupe-copy-error" role="alert">
                              {failureMessage(c.location, failures().get(c.path) ?? "")}
                            </p>
                          )}
                        </For>
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>
          </Show>
        </div>
        <div class="dupes-foot">
          <span class="dupes-foot-note" data-testid="dupes-note">
            {footNote(freedBytes(extras()))}
          </span>
          <button
            type="button"
            class="btn btn-primary dupes-remove"
            disabled={extras().length === 0 || trashing()}
            onClick={removeExtras}
          >
            {trashing() ? REMOVE_BUSY : removeLabel(extras().length)}
          </button>
        </div>
      </section>
    </Show>
  );
}
