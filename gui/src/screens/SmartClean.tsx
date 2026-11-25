import { Match, Show, Switch, createEffect, createResource, createSignal, onCleanup } from "solid-js";
import { useDelayedBusy } from "../lib/busy";
import { loadCleanupView, onScanProgress, scan, stopScan } from "../lib/engine";
import type { CleanupView } from "../lib/contract";
import { diskState, setFromView } from "../lib/diskState";
import { Idle } from "./smartClean/Idle";
import { Results } from "./smartClean/Results";
import { Scanning } from "./smartClean/Scanning";
import {
  INITIAL_STATE,
  backHome,
  scanFinished,
  scanProgress,
  scanStopped,
  selectAll,
  selectSafeOnly,
  startScan,
  toggleExpanded,
  toggleSelected,
  type SmartCleanState,
} from "./smartClean/phase";
import "./smartClean/smartClean.css";

/**
 * Smart Clean. Walks the phase machine in `smartClean/phase.ts`: idle
 * (REQ-DCLEAN-003), scanning (REQ-DCLEAN-004), results, cleaning, done.
 * Every figure comes from the engine's cleanup view (REQ-DCLEAN-027.A4),
 * parsed at the boundary, and disk usage goes through the shared disk state
 * (REQ-DCLEAN-002). Results (REQ-DCLEAN-005) list the engine's categories
 * with the safe-only preselection (REQ-DCLEAN-006), or the empty state
 * (REQ-DCLEAN-019).
 *
 * Loading (REQ-DCLEAN-017): the scanning screen is the scan's loading state;
 * loading the view shows a busy indicator only past the spinner threshold;
 * a finished scan enters results with no pause.
 *
 * @implements REQ-DCLEAN-003
 * @implements REQ-DCLEAN-004
 * @implements REQ-DCLEAN-005
 * @implements REQ-DCLEAN-006
 * @implements REQ-DCLEAN-017
 * @implements REQ-DCLEAN-019
 */
export function SmartClean() {
  const [loaded, { refetch }] = createResource(() => loadCleanupView());
  const [scanned, setScanned] = createSignal<CleanupView | null>(null);
  const [state, setState] = createSignal<SmartCleanState>(INITIAL_STATE);
  const [scanError, setScanError] = createSignal<string | null>(null);
  // Read the resource only once it holds a value: an errored resource throws.
  const ready = () => (loaded.state === "ready" ? loaded() : undefined);
  const view = () => scanned() ?? ready();
  // REQ-DCLEAN-017.A2: no indicator for a load that finishes within 200 ms.
  const loadingBusy = useDelayedBusy(() => loaded.loading);

  createEffect(() => {
    const v = ready();
    if (v) setFromView(v);
  });

  let unlisten: (() => void) | undefined;
  onCleanup(() => unlisten?.());

  async function beginScan() {
    const before = state();
    const next = startScan(before);
    if (next === before) return;
    setScanError(null);
    setState(next);
    try {
      unlisten = await onScanProgress((p) => setState((s) => scanProgress(s, p)));
    } catch {
      unlisten = undefined; // progress is cosmetic; the scan still runs
    }
    try {
      const v = await scan();
      // REQ-DCLEAN-004.A3: a stopped scan's partial snapshot is never shown.
      if (state().phase !== "scanning") return;
      // REQ-DCLEAN-004.A2 / REQ-DCLEAN-017.A3: straight to results, no pause.
      setScanned(v);
      setFromView(v);
      // REQ-DCLEAN-006.A1: a new scan preselects per the safe-only preference.
      setState((s) => scanFinished(s, v.categories, v.preferences.safe_only));
    } catch (e) {
      if (state().phase === "scanning") setScanError(String((e as Error)?.message ?? e));
      setState(scanStopped);
    } finally {
      unlisten?.();
      unlisten = undefined;
    }
  }

  /** "Stop" (REQ-DCLEAN-004.A3): back to idle now; the engine cancels the walk. */
  function endScan() {
    setState(scanStopped);
    void stopScan().catch(() => undefined);
  }

  return (
    <section class="smart-clean" data-screen-label="Smart clean" aria-label="Smart Clean" data-phase={state().phase}>
      <Switch>
        <Match when={loaded.error}>
          {/* REQ-DCLEAN-029.A4: malformed data → error state, never a partial list. */}
          <div class="screen" role="alert" data-testid="view-error">
            <p class="screen-note">Couldn't read the cleanup data: {String(loaded.error?.message ?? loaded.error)}</p>
            <button type="button" class="btn btn-secondary" onClick={() => refetch()}>
              Try again
            </button>
          </div>
        </Match>
        <Match when={state().phase === "scanning"}>
          <Scanning state={state()} catalogue={view()?.catalogue ?? []} onStop={endScan} />
        </Match>
        <Match when={state().phase === "results" && view()}>
          {(v) => (
            <Results
              view={v()}
              state={state()}
              onToggleExpand={(id) => setState((s) => toggleExpanded(s, id))}
              onToggleSelect={(id) => setState((s) => toggleSelected(s, id))}
              onSelectSafe={() => setState((s) => selectSafeOnly(s, v().categories))}
              onSelectAll={() => setState((s) => selectAll(s, v().categories))}
              onBack={() => setState(backHome)}
            />
          )}
        </Match>
        <Match when={view() && diskState()}>
          <Show when={scanError()}>
            <p class="screen-note idle-error" role="alert" data-testid="scan-error">
              The scan didn't finish: {scanError()}
            </p>
          </Show>
          <Idle view={view()!} disk={diskState()!} onStartScan={() => void beginScan()} />
        </Match>
        <Match when={loadingBusy()}>
          <div class="busy" role="status" aria-busy="true" data-testid="busy">
            <span class="busy-spinner" aria-hidden="true" />
            Loading…
          </div>
        </Match>
      </Switch>
    </section>
  );
}

