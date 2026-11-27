import { For, Match, Switch } from "solid-js";
import Check from "lucide-solid/icons/check";
import type { CatalogueEntry } from "../../lib/contract";
import { NUMERIC_CLASS, formatBytes } from "../../lib/format";
import { categoryRowStatus, type SmartCleanState } from "./phase";

export interface ScanningProps {
  state: SmartCleanState;
  /** Every category the engine checks (`CleanupView.catalogue`). */
  catalogue: readonly CatalogueEntry[];
  onStop: () => void;
}

/**
 * Scanning phase (REQ-DCLEAN-004): "Looking around…", the path being
 * examined (ellipsised, full path in the tooltip), the percentage and its
 * bar, one row per catalogued category, and "Stop". A row shows its size
 * only once its category is complete (A4); a complete category with no hits
 * reads "Nothing found". This screen is the scan's loading state
 * (REQ-DCLEAN-017.A1). Layout follows snapshot.html "Smart clean · scanning".
 *
 * @implements REQ-DCLEAN-004
 * @implements REQ-DCLEAN-017
 */
export function Scanning(props: ScanningProps) {
  const pct = () => Math.round(props.state.progress * 100);
  return (
    <div class="scanning" data-testid="scanning" data-screen-label="Smart clean · scanning">
      <h1 class="scanning-title" tabIndex={-1} data-phase-heading>Looking around…</h1>
      <div class="scan-meta">
        <span class="scan-path" data-testid="scan-path" title={props.state.currentPath}>
          {props.state.currentPath}
        </span>
        <span class={NUMERIC_CLASS} data-testid="scan-progress">
          {pct()}%
        </span>
      </div>
      <div
        class="scan-bar"
        role="progressbar"
        aria-label="Scan progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct()}
      >
        <div class="scan-bar-fill" data-testid="scan-bar-fill" style={{ width: `${props.state.progress * 100}%` }} />
      </div>
      <table class="scan-rows" data-testid="scan-rows">
        <tbody>
          <For each={props.catalogue}>
            {(c) => {
              const row = () => categoryRowStatus(c.id, props.state);
              return (
                <tr class="scan-row" data-category-id={c.id} data-status={row().status}>
                  <td class="scan-dot-cell">
                    <span class="scan-dot" classList={{ "scan-dot-done": row().status === "found" || row().status === "empty" }}>
                      {row().status === "found" || row().status === "empty" ? (
                        <Check class="scan-check" aria-hidden="true" />
                      ) : null}
                    </span>
                  </td>
                  <td class="scan-name">{c.name}</td>
                  <td class={`scan-size ${NUMERIC_CLASS}`} data-testid="scan-row-status">
                    <Switch>
                      <Match when={row().bytes !== null}>{formatBytes(row().bytes!)}</Match>
                      <Match when={row().status === "empty"}>Nothing found</Match>
                      <Match when={row().status === "checking"}>Checking…</Match>
                    </Switch>
                  </td>
                </tr>
              );
            }}
          </For>
        </tbody>
      </table>
      <div>
        <button type="button" class="btn btn-secondary" onClick={() => props.onStop()}>
          Stop
        </button>
      </div>
    </div>
  );
}
