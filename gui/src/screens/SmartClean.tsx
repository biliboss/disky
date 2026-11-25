import { For, Match, Show, Switch, createEffect, createResource, createSignal, onCleanup } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { useDelayedBusy } from "../lib/busy";
import {
  clean,
  loadCleanupView,
  onCleanProgress,
  onScanProgress,
  openFullDiskAccess,
  restore,
  scan,
  stopScan,
} from "../lib/engine";
import { parseEngineError, type CleanMode, type CleanResult, type CleanupView, type EngineError } from "../lib/contract";
import { diskState, setFromView } from "../lib/diskState";
import { Cleaning } from "./smartClean/Cleaning";
import { ConfirmClean } from "./smartClean/ConfirmClean";
import { Done } from "./smartClean/Done";
import { Idle } from "./smartClean/Idle";
import { Results } from "./smartClean/Results";
import { Scanning } from "./smartClean/Scanning";
import { cleanFailedMessage, undoNotices } from "./smartClean/doneCopy";
import { canRetry, scanErrorMessage } from "./smartClean/errorCopy";
import {
  INITIAL_STATE,
  backHome,
  cancelConfirm,
  cleanFailed,
  cleanFinished,
  cleanProgress,
  confirmClean,
  doneHome,
  listedCategories,
  openConfirm,
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
import { selectedBytes } from "./smartClean/resultsCopy";
import "./smartClean/smartClean.css";

/**
 * Smart Clean. Walks the phase machine in `smartClean/phase.ts`: idle
 * (REQ-DCLEAN-003), scanning (REQ-DCLEAN-004), results, cleaning, done.
 * Every figure comes from the engine's cleanup view (REQ-DCLEAN-027.A4),
 * parsed at the boundary, and disk usage goes through the shared disk state
 * (REQ-DCLEAN-002). Results (REQ-DCLEAN-005) list the engine's categories
 * with the safe-only preselection (REQ-DCLEAN-006), or the empty state
 * (REQ-DCLEAN-019). "Clean <total>" opens the confirm dialog
 * (REQ-DCLEAN-007, focus per REQ-DCLEAN-022) over the results, fed the same
 * selected categories and total as the footer; only "Clean now" enters the
 * cleaning phase.
 *
 * Cleaning (REQ-DCLEAN-008): one `clean` call with the chosen categories'
 * reviewed item paths in list order; the engine's per-item progress events
 * drive the step label and bar. Done (REQ-DCLEAN-009) shows the engine's
 * reclaimed bytes, Undo after a Trash clean (restore through the engine),
 * and the paths that failed (REQ-DCLEAN-018.A3). A clean that reclaimed
 * nothing returns to results with the selection kept (REQ-DCLEAN-018.A4).
 * Scan failures name their cause by the engine's error code (A1).
 *
 * Loading (REQ-DCLEAN-017): the scanning screen is the scan's loading state;
 * loading the view shows a busy indicator only past the spinner threshold;
 * a finished scan enters results with no pause.
 *
 * @implements REQ-DCLEAN-003
 * @implements REQ-DCLEAN-004
 * @implements REQ-DCLEAN-005
 * @implements REQ-DCLEAN-006
 * @implements REQ-DCLEAN-007
 * @implements REQ-DCLEAN-008
 * @implements REQ-DCLEAN-009
 * @implements REQ-DCLEAN-017
 * @implements REQ-DCLEAN-018
 * @implements REQ-DCLEAN-019
 */
export function SmartClean() {
  const navigate = useNavigate();
  const [loaded, { refetch }] = createResource(() => loadCleanupView());
  const [scanned, setScanned] = createSignal<CleanupView | null>(null);
  const [state, setState] = createSignal<SmartCleanState>(INITIAL_STATE);
  const [scanError, setScanError] = createSignal<EngineError | null>(null);
  const [cleanResult, setCleanResult] = createSignal<CleanResult | null>(null);
  const [cleanError, setCleanError] = createSignal<string | null>(null);
  const [undoing, setUndoing] = createSignal(false);
  const [undoError, setUndoError] = createSignal<string | null>(null);
  const [notices, setNotices] = createSignal<string[]>([]);
  // REQ-DCLEAN-017.A2: the undo wait shows busy only past the spinner threshold.
  const undoBusy = useDelayedBusy(undoing);
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
  let unlistenClean: (() => void) | undefined;
  onCleanup(() => {
    unlisten?.();
    unlistenClean?.();
  });

  async function beginScan() {
    const before = state();
    const next = startScan(before);
    if (next === before) return;
    setScanError(null);
    setCleanError(null);
    setNotices([]);
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
      // REQ-DCLEAN-018.A1: the cause comes from the engine's error code.
      if (state().phase === "scanning") setScanError(parseEngineError(e));
      setState(scanStopped);
    } finally {
      unlisten?.();
      unlisten = undefined;
    }
  }

  /** The categories the confirm dialog lists: selected, in list order. */
  const chosen = () => {
    const v = view();
    return v ? listedCategories(v.categories).filter((c) => state().selected.includes(c.id)) : [];
  };

  /**
   * "Clean now" (REQ-DCLEAN-007.A1): the only path into cleaning. One clean
   * call with the chosen categories' reviewed item paths in list order, so
   * the step label follows selection order (REQ-DCLEAN-008.A1). The engine
   * refuses any path that was not in the reviewed view.
   */
  async function runClean(mode: CleanMode) {
    const cats = chosen();
    if (cats.length === 0) return;
    const before = state();
    const next = confirmClean(before, mode, cats[0].id);
    if (next === before) return;
    const paths = cats.flatMap((c) => c.items.map((it) => it.path));
    setCleanError(null);
    setCleanResult(null);
    setUndoError(null);
    setState(next);
    try {
      unlistenClean = await onCleanProgress((p) => setState((s) => cleanProgress(s, p)));
    } catch {
      unlistenClean = undefined; // progress is cosmetic; the clean still runs
    }
    try {
      const r = await clean(paths, mode, before.openedBy);
      if (r.succeeded.length === 0) {
        // REQ-DCLEAN-018.A4: nothing reclaimed → no done phase.
        setCleanError(cleanFailedMessage(r.failed));
        setState(cleanFailed);
        return;
      }
      setCleanResult(r);
      setState(cleanFinished);
    } catch (e) {
      setCleanError(cleanFailedMessage([], parseEngineError(e).detail));
      setState(cleanFailed);
    } finally {
      unlistenClean?.();
      unlistenClean = undefined;
    }
  }

  /** "Undo" (REQ-DCLEAN-009.A4): restore the clean's history entry, then idle. */
  async function undo() {
    const id = cleanResult()?.history_id;
    if (!id || undoing()) return;
    setUndoing(true);
    setUndoError(null);
    try {
      const r = await restore(id);
      // A7: every item that did not come back is reported, never silently dropped.
      setNotices(undoNotices(r.report));
      setCleanResult(null);
      setState(doneHome);
    } catch (e) {
      setUndoError(parseEngineError(e).detail);
    } finally {
      setUndoing(false);
    }
  }

  /** The display name of the category the clean is on. */
  const cleaningName = () =>
    view()?.categories.find((c) => c.id === state().cleaningCategory)?.name ?? "files";

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
              onClean={() => setState(openConfirm)}
              cleanError={cleanError()}
              onOpenFullDiskAccess={() => void openFullDiskAccess().catch(() => undefined)}
            />
          )}
        </Match>
        <Match when={state().phase === "cleaning" && state().cleanMode}>
          {(mode) => <Cleaning state={state()} mode={mode()} categoryName={cleaningName()} />}
        </Match>
        <Match when={state().phase === "done" && cleanResult() && diskState()}>
          <Done
            result={cleanResult()!}
            disk={diskState()!}
            undoing={undoing()}
            undoBusy={undoBusy()}
            undoError={undoError()}
            onHome={() => {
              setCleanResult(null);
              setState(doneHome);
            }}
            onUndo={() => void undo()}
            onHistory={() => navigate("/history")}
          />
        </Match>
        <Match when={view() && diskState()}>
          <Show when={scanError()}>
            {(e) => (
              <div class="error-state idle-error" role="alert" data-testid="scan-error">
                <span>{scanErrorMessage(e())}</span>
                <Show when={canRetry(e())}>
                  <button type="button" class="btn btn-secondary" onClick={() => void beginScan()}>
                    Try again
                  </button>
                </Show>
              </div>
            )}
          </Show>
          <Show when={notices().length > 0}>
            <ul class="notice undo-notice" role="status" data-testid="undo-notices">
              <For each={notices()}>{(n) => <li>{n}</li>}</For>
            </ul>
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
      <Show when={state().phase === "results" && state().confirming && view()}>
        {(v) => {
          return (
            <ConfirmClean
              categories={chosen()}
              total={selectedBytes(listedCategories(v().categories), state().selected)}
              trashByDefault={v().preferences.trash_by_default}
              fallbackFocus={() => document.querySelector<HTMLElement>('[data-testid="results-heading"]')}
              onCancel={() => setState(cancelConfirm)}
              onConfirm={(mode) => void runClean(mode)}
            />
          );
        }}
      </Show>
    </section>
  );
}

