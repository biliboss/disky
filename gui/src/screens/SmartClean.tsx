import { For, Match, Show, Switch, createResource } from "solid-js";
import { loadCleanupView } from "../lib/engine";
import { NUMERIC_CLASS, formatBytes } from "../lib/format";
import type { CleanupView } from "../lib/contract";

/**
 * Smart Clean. The categories, their names, descriptions and safety come from
 * the engine's cleanup view (REQ-DCLEAN-027.A4), parsed at the boundary; this
 * screen only renders them. The full phase flow (idle, scanning, results,
 * cleaning, done) lands with REQ-DCLEAN-003 … 009 on top of this data.
 */
export function SmartClean() {
  const [view, { refetch }] = createResource(() => loadCleanupView());

  return (
    <section class="screen" data-screen-label="Smart clean" aria-label="Smart Clean">
      <Switch>
        <Match when={view.error}>
          {/* REQ-DCLEAN-029.A4: malformed data → error state, never a partial list. */}
          <div role="alert" data-testid="view-error">
            <p class="screen-note">Couldn't read the cleanup data: {String(view.error?.message ?? view.error)}</p>
            <button type="button" class="btn btn-secondary" onClick={() => refetch()}>
              Try again
            </button>
          </div>
        </Match>
        <Match when={view()}>{(v) => <Categories view={v()} />}</Match>
      </Switch>
    </section>
  );
}

function Categories(props: { view: CleanupView }) {
  return (
    <>
      <Show when={props.view.is_stale}>
        {/* REQ-DCLEAN-029.A2: warn before cleaning a snapshot older than 24 h. */}
        <p role="status" data-testid="stale-warning">
          <span class="tag tag-accent">Out of date</span> This scan is more than a day old. Scan again before
          cleaning.
        </p>
      </Show>
      <Show
        when={props.view.categories.length > 0}
        fallback={<p class="screen-note">Nothing to clean right now.</p>}
      >
        <table class="table" data-testid="categories">
          <tbody>
            <For each={props.view.categories}>
              {(c) => (
                <tr data-category-id={c.id}>
                  <td>{c.name}</td>
                  <td class="text-muted">{c.description}</td>
                  <td>
                    <span class={c.safety === "safe" ? "tag tag-accent-2" : "tag tag-outline"}>
                      {c.safety === "safe" ? "Safe" : "Review"}
                    </span>
                  </td>
                  <td class={NUMERIC_CLASS}>{formatBytes(c.bytes)}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </Show>
    </>
  );
}
