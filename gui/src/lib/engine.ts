/**
 * The GUI's only door to the disky engine (REQ-DCLEAN-001). Each function
 * delegates to one Tauri command in `src-tauri/src/main.rs`, which delegates
 * to the `disky` library crate. Every payload is parsed against the contract
 * (REQ-DCLEAN-027) before a component sees it, and every failure is an
 * {@link EngineError} typed by disky's exit-code slug (REQ-DCLEAN-018).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  parseAssistantView,
  parseCleanProgress,
  parseCleanResult,
  parseCleanupView,
  parseDupesView,
  parseEngineError,
  parseHistoryView,
  parseLargeView,
  parseMapView,
  parsePreferences,
  parseRestoreResult,
  parseScanProgress,
  parseTrashResult,
  type AssistantView,
  type CleanMode,
  type CleanProgress,
  type CleanResult,
  type CleanupView,
  type DupesView,
  type HistoryView,
  type LargeFilter,
  type LargeView,
  type MapView,
  type Preferences,
  type RestoreResult,
  type ScanProgress,
  type TrashResult,
} from "./contract";
import { setDisk, setFromClean } from "./diskState";

/** Event channel the backend streams scan progress on. */
export const SCAN_PROGRESS_EVENT = "scan://progress";

/** Event channel the backend streams clean progress on, one event per item. */
export const CLEAN_PROGRESS_EVENT = "clean://progress";

/** `invoke`, with a rejection turned into an EngineError. */
async function call(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    throw parseEngineError(e);
  }
}

/** The latest snapshot's cleanup view. CLI: `disky cleanup --format json`. */
export async function loadCleanupView(snapshot?: string): Promise<CleanupView> {
  return parseCleanupView(await invoke("load_cleanup_view", { snapshot: snapshot ?? null }));
}

/** Scan `root` into a new snapshot and return its view. CLI: `disky scan`. */
export async function scan(root?: string): Promise<CleanupView> {
  return parseCleanupView(await call("scan", { root: root ?? null }));
}

/** Ask a running scan to stop; the partial snapshot stays queryable. */
export async function stopScan(): Promise<void> {
  await invoke("stop_scan");
}

/** Subscribe to `channel`, parsing each payload. Malformed events are dropped, never rendered. */
function subscribe<T>(
  channel: string,
  parse: (raw: unknown) => T,
  cb: (p: T) => void,
  onError?: (e: unknown) => void,
): Promise<UnlistenFn> {
  return listen(channel, (ev) => {
    let p: T;
    try {
      p = parse(ev.payload);
    } catch (e) {
      onError?.(e);
      return;
    }
    cb(p);
  });
}

/** Subscribe to scan progress. */
export function onScanProgress(cb: (p: ScanProgress) => void, onError?: (e: unknown) => void): Promise<UnlistenFn> {
  return subscribe(SCAN_PROGRESS_EVENT, parseScanProgress, cb, onError);
}

/** Subscribe to clean progress (REQ-DCLEAN-008). */
export function onCleanProgress(cb: (p: CleanProgress) => void, onError?: (e: unknown) => void): Promise<UnlistenFn> {
  return subscribe(CLEAN_PROGRESS_EVENT, parseCleanProgress, cb, onError);
}

/**
 * Clean exactly the reviewed item paths, in the order given. CLI:
 * `disky cleanup --apply [--reversible]` (its envelope carries the same
 * post-clean `disk`); undo with `disky restore <id>`. `openedBy` names the
 * agent that opened the review, which titles the history entry
 * (REQ-DCLEAN-009.A5).
 */
export async function clean(itemPaths: string[], mode: CleanMode, openedBy: string | null = null): Promise<CleanResult> {
  const result = parseCleanResult(await call("clean", { itemPaths, mode, openedBy }));
  // REQ-DCLEAN-002.A4: meter, ring and tag follow the post-clean disk.
  setFromClean(result);
  return result;
}

/** Undo a Trash clean (REQ-DCLEAN-009.A4). CLI: `disky restore <id>`. */
export async function restore(historyId: string): Promise<RestoreResult> {
  const result = parseRestoreResult(await call("restore", { historyId }));
  setDisk(result.disk);
  return result;
}

/** Open System Settings at Full Disk Access (REQ-DCLEAN-018.A2). */
export async function openFullDiskAccess(): Promise<void> {
  await call("open_full_disk_access");
}

/** Every preference, from the shared store (REQ-DSCR-001). CLI: `disky prefs --format json`. */
export async function getPrefs(): Promise<Preferences> {
  return parsePreferences(await call("get_prefs"));
}

/**
 * Save one preference at once and return the store as saved
 * (REQ-DSCR-016). CLI: `disky prefs --set <key>=<value>`. `excluded` takes
 * a JSON array, so folder names may hold commas.
 */
export async function setPrefs(key: string, value: string): Promise<Preferences> {
  return parsePreferences(await call("set_prefs", { key, value }));
}

/** The native folder picker (REQ-DSCR-016.A3); null when cancelled. Saves nothing. */
export async function pickFolder(): Promise<string | null> {
  const picked = await call("pick_folder");
  if (picked !== null && typeof picked !== "string") throw parseEngineError(new Error("pick_folder: expected a path"));
  return picked;
}

/**
 * The History screen's rows, newest first, with the total cleared
 * (REQ-DSCR-015). CLI: `disky history --format json` (same rows).
 */
export async function listHistory(): Promise<HistoryView> {
  return parseHistoryView(await call("list_history"));
}

/**
 * A folder's immediate children by physical size (REQ-DSCR-003); `path`
 * null maps the scan root. Resolves null when there is no snapshot yet.
 * CLI: `disky map [<path>] --format json`.
 */
export async function loadMap(path: string | null = null): Promise<MapView | null> {
  const raw = await call("load_map", { path });
  return raw === null ? null : parseMapView(raw);
}

/**
 * Reveal `path` in Finder (REQ-DSCR-012.A4). The backend only reveals a
 * path its last map listed. CLI: `open -R <path>`.
 */
export async function revealInFinder(path: string): Promise<void> {
  await call("reveal_in_finder", { path });
}

/**
 * Files of at least 2 GB for `filter`, largest first, with last-opened
 * times (REQ-DSCR-004). The backend remembers the listed paths: they are
 * the only ones `trashLarge` may move (REQ-DSCR-006.A2). Resolves null when
 * there is no snapshot yet. CLI: `disky large --filter <f> --format json`.
 */
export async function loadLarge(filter: LargeFilter): Promise<LargeView | null> {
  const raw = await call("load_large", { filter });
  return raw === null ? null : parseLargeView(raw);
}

/**
 * Move exactly `paths` (from the last `loadLarge`) to the Trash with one
 * "Large files" history entry (REQ-DSCR-006, REQ-DSCR-013.A4). Always the
 * Trash. The meter follows the post-move disk. CLI: `disky trash <path>...
 * --title "Large files"`.
 */
export async function trashLarge(paths: string[]): Promise<TrashResult> {
  const result = parseTrashResult(await call("trash_large", { paths }));
  setDisk(result.disk);
  return result;
}

/**
 * The duplicate groups in the latest snapshot (REQ-DSCR-005), computed on a
 * blocking thread in the backend. The backend remembers the groups: their
 * copies are the only paths `trashDupes` may move, and never every copy of
 * a group (REQ-DSCR-006.A2, A3). Resolves null when there is no snapshot
 * yet. CLI: `disky dupes --format json`.
 */
export async function loadDupes(): Promise<DupesView | null> {
  const raw = await call("load_dupes");
  return raw === null ? null : parseDupesView(raw);
}

/**
 * Move exactly `paths` — the copies not kept — to the Trash with one
 * "Duplicates" history entry (REQ-DSCR-006, REQ-DSCR-014.A4). The meter
 * follows the post-move disk. CLI: `disky trash <path>... --title Duplicates`.
 */
export async function trashDupes(paths: string[]): Promise<TrashResult> {
  const result = parseTrashResult(await call("trash_dupes", { paths }));
  setDisk(result.disk);
  return result;
}

/** Event the backend's proactive checks send a fresh Assistant view on (REQ-DSCR-011.A1). */
export const ASSISTANT_VIEW_EVENT = "assistant://view";

/** Event sent when a scheduled background scan finished (REQ-DSCR-011.A2). */
export const BACKGROUND_SCANNED_EVENT = "assistant://scanned";

/** Event sent when the below-goal notification was clicked (REQ-DSCR-011.A3). */
export const FOCUS_HOME_EVENT = "assistant://focus-home";

/**
 * The Assistant screen's data: the selected agent and its display name, the
 * proactive checks, the goal and disk, the three agents and the engine's
 * suggestions (REQ-DSCR-007 … 010). CLI: `disky suggest`, `disky agents`,
 * `disky prefs`.
 */
export async function loadAssistant(): Promise<AssistantView> {
  return parseAssistantView(await call("load_assistant"));
}

/** Subscribe to the Assistant views the proactive checks reload (REQ-DSCR-011.A1). */
export function onAssistantView(cb: (v: AssistantView) => void, onError?: (e: unknown) => void): Promise<UnlistenFn> {
  return subscribe(ASSISTANT_VIEW_EVENT, parseAssistantView, cb, onError);
}

/** Subscribe to finished background scans (REQ-DSCR-011.A2). */
export function onBackgroundScanned(cb: () => void): Promise<UnlistenFn> {
  return listen(BACKGROUND_SCANNED_EVENT, () => cb());
}

/** Subscribe to clicks on the below-goal notification (REQ-DSCR-011.A3). */
export function onFocusHome(cb: () => void): Promise<UnlistenFn> {
  return listen(FOCUS_HOME_EVENT, () => cb());
}
