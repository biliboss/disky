/**
 * The GUI's only door to the disky engine (REQ-DCLEAN-001). Each function
 * delegates to one Tauri command in `src-tauri/src/main.rs`, which delegates
 * to the `disky` library crate. Every payload is parsed against the contract
 * (REQ-DCLEAN-027) before a component sees it.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  parseCleanResult,
  parseCleanupView,
  parseScanProgress,
  type CleanMode,
  type CleanResult,
  type CleanupView,
  type ScanProgress,
} from "./contract";

/** Event channel the backend streams scan progress on. */
export const SCAN_PROGRESS_EVENT = "scan://progress";

/** The latest snapshot's cleanup view. CLI: `disky cleanup --format json`. */
export async function loadCleanupView(snapshot?: string): Promise<CleanupView> {
  return parseCleanupView(await invoke("load_cleanup_view", { snapshot: snapshot ?? null, goalBytes: null }));
}

/** Scan `root` into a new snapshot and return its view. CLI: `disky scan`. */
export async function scan(root?: string): Promise<CleanupView> {
  return parseCleanupView(await invoke("scan", { root: root ?? null }));
}

/** Ask a running scan to stop; the partial snapshot stays queryable. */
export async function stopScan(): Promise<void> {
  await invoke("stop_scan");
}

/** Subscribe to scan progress. Malformed events are dropped, never rendered. */
export function onScanProgress(cb: (p: ScanProgress) => void, onError?: (e: unknown) => void): Promise<UnlistenFn> {
  return listen(SCAN_PROGRESS_EVENT, (ev) => {
    try {
      cb(parseScanProgress(ev.payload));
    } catch (e) {
      onError?.(e);
    }
  });
}

/**
 * Clean exactly the reviewed item paths. CLI:
 * `disky cleanup --apply [--reversible]`; undo with `disky restore <id>`.
 */
export async function clean(itemPaths: string[], mode: CleanMode): Promise<CleanResult> {
  return parseCleanResult(await invoke("clean", { itemPaths, mode }));
}
