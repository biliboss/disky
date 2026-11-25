import type { Page } from "@playwright/test";
import { snapshotReviewCategories } from "../fixtures/snapshotReview";
import { snapshotAssistantView } from "../fixtures/snapshotAssistant";

/**
 * The browser build's stand-in for the Tauri engine, and the snapshot
 * fixtures the Playwright specs drive the app with. Shared by shell.spec.ts
 * (visual contract) and the a11y / keyboard / motion / perf / window specs.
 */

/** The window the snapshot is drawn at (REQ-DCLEAN-026). */
export const WINDOW = { x: 0, y: 0, width: 1200, height: 780 };

export async function openApp(page: Page, hash = "#/") {
  // A real load every time: going to the same URL again is only a hash change,
  // which would keep the previous app state and engine stub.
  if (page.url() !== "about:blank") await page.goto("about:blank");
  await page.goto(`/${hash}`);
  await page.locator(".sidebar").waitFor();
  await page.evaluate(() => document.fonts.ready);
}

/**
 * The snapshot prototype's idle state as an engine view: 74 of 512 GB free,
 * 80 GB goal, last scan 7 days ago, 8 files over 2 GB.
 */
export function idleFixtureView() {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: new Date(Date.now() - 7 * 86_400_000 - 60_000).toISOString(),
    snapshot_taken_at: null,
    is_stale: false,
    preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] },
    large_files_count: 8,
    skipped_paths: 0,
    categories: [],
    catalogue: SNAPSHOT_SCAN_ROWS.map((name, i) => ({ id: `visual_cat_${i}`, name, icon: "folder", safety: "safe" })),
  };
}

/** The snapshot prototype's eight scanning rows, as catalogue entries. */
const SNAPSHOT_SCAN_ROWS = [
  "System caches",
  "App caches",
  "Temporary files",
  "Logs & crash reports",
  "Trash",
  "App leftovers",
  "Old downloads",
  "Developer junk",
];

/**
 * Answer the app's Tauri `invoke` calls in the browser build with `view`.
 * `view` "forever" holds the initial load (the busy indicator shows). With `scanForever`, `scan` never resolves and progress subscriptions are
 * accepted, so the app holds the scanning phase. With `scanResult`, `scan`
 * resolves to that view at once, so the app enters the results phase.
 */
export async function stubEngine(
  page: Page,
  view: unknown,
  scanForever = false,
  scanResult: unknown = null,
  cleanResult: unknown = null,
) {
  await page.addInitScript(
    ([v, forever, result, cleaned]) => {
      (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string) => {
          if (cmd === "load_cleanup_view") return v === "forever" ? new Promise(() => undefined) : v;
          if (forever && cmd === "scan") return new Promise(() => undefined);
          if (result && cmd === "scan") return result;
          if (cleaned === "forever" && cmd === "clean") return new Promise(() => undefined);
          if (cleaned && cmd === "clean") return cleaned;
          if ((forever || result) && cmd === "plugin:event|listen") return 1;
          if (result && cmd === "plugin:event|unlisten") return null;
          throw new Error(`visual stub: ${cmd} is not available`);
        },
        transformCallback: () => 0,
      };
    },
    [view, scanForever, scanResult, cleanResult] as const,
  );
}

/** The snapshot's review data as the engine's results view. */
export function reviewFixtureView() {
  return { ...idleFixtureView(), categories: snapshotReviewCategories() };
}

/**
 * The app's clean of the snapshot's default (Safe) selection: the same
 * bytes the prototype frees, so the post-clean disk, ring and meter agree.
 */
export function doneFixtureResult() {
  const safe = snapshotReviewCategories().filter((c) => c.safety === "safe");
  const freed = safe.reduce((a, c) => a + c.bytes, 0);
  return {
    mode: "trash",
    freed_bytes: freed,
    succeeded: safe.flatMap((c) => c.items.map((i) => i.path)),
    failed: [],
    history_id: "visual-entry",
    history_entry: null,
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 + freed },
  };
}


/**
 * The review data with `n` items per category (bytes still sum to the
 * category total, as the contract requires), so the results list overflows.
 */
export function manyItemsView(n = 30) {
  const categories = snapshotReviewCategories().map((c) => {
    const each = Math.floor(c.bytes / n);
    const items = Array.from({ length: n }, (_, i) => ({
      name: `${c.id}-${i}`,
      path: `/Users/me/${c.id}/${i}`,
      bytes: i === n - 1 ? c.bytes - each * (n - 1) : each,
    }));
    return { ...c, items };
  });
  return { ...idleFixtureView(), categories };
}

/** Resolve a token in the page, as a computed background colour. */
export async function tokenColor(page: Page, token: string): Promise<string> {
  return page.evaluate((t) => {
    const probe = document.createElement("div");
    probe.style.background = `var(${t})`;
    document.body.append(probe);
    const c = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return c;
  }, token);
}

/** Open the app on idle and run a scan that resolves at once to `scanned`. */
export async function appToResults(page: Page, scanned: unknown = reviewFixtureView(), cleaned: unknown = null) {
  await stubEngine(page, idleFixtureView(), false, scanned, cleaned);
  await openApp(page);
  await page.getByTestId("idle").getByRole("button", { name: "Start scan" }).click();
  await page.getByTestId("results").waitFor();
}

/**
 * The snapshot's Settings state as the engine's preferences store: every
 * toggle on, scheduled scan Weekly, and its three excluded folders.
 */
export function snapshotSettingsPrefs() {
  return {
    trash_by_default: true,
    skip_recently_used: true,
    safe_only: true,
    warn_below_goal: true,
    goal_bytes: 80e9,
    checks: { enabled: true, frequency: "daily" },
    agent: "claude",
    scheduled_scan: "weekly",
    excluded: ["~/Projects/active", "~/Pictures/RAW", "~/Library/Mobile Documents"],
  };
}

/**
 * Answer the Settings screen's commands from an in-page preferences store
 * (`window.__prefsStore`), as `disky prefs` would: `get_prefs` reads it and
 * `set_prefs` writes one key. Each `set_prefs` call is recorded in
 * `window.__prefsSets`.
 */
export async function stubPrefs(page: Page, prefs: unknown = snapshotSettingsPrefs(), picked: string | null = null) {
  await page.addInitScript(
    ([initial, folder]) => {
      const w = window as any;
      w.__prefsStore = structuredClone(initial);
      w.__prefsSets = [];
      w.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
          const store = w.__prefsStore;
          if (cmd === "get_prefs") return structuredClone(store);
          if (cmd === "pick_folder") return folder;
          if (cmd === "set_prefs") {
            w.__prefsSets.push(args);
            if (args.key === "excluded") store.excluded = JSON.parse(args.value);
            else if (args.key === "scheduled_scan") store.scheduled_scan = args.value;
            else store[args.key] = args.value === "true";
            return structuredClone(store);
          }
          throw new Error(`visual stub: ${cmd} is not available`);
        },
        transformCallback: () => 0,
      };
    },
    [prefs, picked] as const,
  );
}

const DAY_MS = 86_400_000;

/**
 * The snapshot's History state as engine rows: its three entries (7, 22
 * and 49 days old; 6.8, 11.2 and 3.9 GB), newest first. The detail text is
 * the snapshot's own.
 */
export function snapshotHistoryRows(now = Date.now()) {
  const row = (id: string, title: string, days: number, detail: string, bytes: number, state: string) => ({
    id,
    title,
    ts: new Date(now - days * DAY_MS - 60_000).toISOString(),
    mode: "trash",
    categories: [],
    bytes,
    items: [],
    restored_at: null,
    detail,
    state,
  });
  return [
    row("visual-e1", "Smart Clean", 7, "App caches, Temporary files, Trash", 6_800_000_000, "restorable"),
    row("visual-e2", "Large files", 22, "3 files from Downloads", 11_200_000_000, "restorable"),
    row("visual-e3", "Smart Clean", 49, "System caches, Logs", 3_900_000_000, "emptied"),
  ];
}

/**
 * Answer the History screen's commands from an in-page log
 * (`window.__historyRows`), as `disky history` / `disky restore` would:
 * `list_history` reads it and `restore` marks the entry restored. Each
 * restore id is recorded in `window.__restored`.
 */
export async function stubHistory(page: Page, rows: unknown[] = snapshotHistoryRows()) {
  await page.addInitScript((initial) => {
    const w = window as any;
    w.__historyRows = structuredClone(initial);
    w.__restored = [];
    w.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string, args: any) => {
        if (cmd === "list_history") {
          const rows = w.__historyRows;
          return { total_bytes: rows.reduce((a: number, r: any) => a + r.bytes, 0), rows: structuredClone(rows) };
        }
        if (cmd === "restore") {
          w.__restored.push(args.historyId);
          const r = w.__historyRows.find((x: any) => x.id === args.historyId);
          r.state = "restored";
          return {
            report: { id: args.historyId, restored: ["/Users/me/x"], conflicts: [], missing: [] },
            disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 80.8e9 },
          };
        }
        throw new Error(`visual stub: ${cmd} is not available`);
      },
      transformCallback: () => 0,
    };
  }, rows);
}

const GB = 1e9;

/** The snapshot's MAP (root and its two folders), as `disky map` answers per folder. */
export function snapshotMapFolders() {
  const kids = (parent: string, list: [string, number, boolean?][]) =>
    list.map(([name, gb, has]) => ({ name, path: `${parent === "/" ? "" : parent}/${name}`, bytes: gb * GB, has_children: !!has }));
  const view = (path: string, name: string, records: ReturnType<typeof kids>) => ({
    path,
    name,
    root: "/",
    total_bytes: records.reduce((a, r) => a + r.bytes, 0),
    capacity_bytes: 512 * GB,
    records,
  });
  return {
    "/": view("/", "/", kids("/", [["Users", 212, true], ["Applications", 92, true], ["System", 58], ["Library", 34], ["Containers", 26], ["Other", 16]])),
    "/Users": view("/Users", "Users", kids("/Users", [["Library", 74], ["Documents", 48], ["Movies", 41], ["Developer", 31], ["Pictures", 12], ["Music", 6]])),
    "/Applications": view("/Applications", "Applications", kids("/Applications", [["Adobe Creative Cloud", 22], ["Xcode", 18], ["Logic Pro", 12], ["Final Cut Pro", 9], ["Other apps", 31]])),
  } as Record<string, unknown>;
}

/**
 * A folder with one big child and one 1 MB sliver (the bug report's
 * ~/.ollama/models): the sliver is kept as itself (REQ-DSCR-003.A5), so its
 * block is only a few pixels wide.
 */
export function sliverMapFolder() {
  const records = [
    { name: "blobs", path: "/Users/me/.ollama/models/blobs", bytes: 24_700_000_000, has_children: true },
    { name: "manifests", path: "/Users/me/.ollama/models/manifests", bytes: 1_000_000, has_children: true },
  ];
  return {
    path: "/Users/me/.ollama/models",
    name: "models",
    root: "/Users/me/.ollama/models",
    total_bytes: records.reduce((a, r) => a + r.bytes, 0),
    capacity_bytes: 994 * GB,
    records,
    other: null,
  };
}

/**
 * A crowded home folder (the bug report's shape): 19 children the engine
 * kept, largest first, and the 200+ it folded into `other`, as
 * `MapView.fold(MAX_TILES, MIN_TILE_SHARE)` returns it.
 */
export function crowdedMapFolder() {
  const kept: [string, number][] = [
    ["Library", 265], ["dev", 198], [".specship", 26], [".cache", 24.7], [".ollama", 24.7], [".npm", 18.5],
    [".android", 11.1], [".archon", 11.1], [".openclaw", 9.4], ["profiles-and-a-long-folder-name", 8.9], ["go", 7.2],
    [".cargo", 5.2], [".rustup", 4.8], ["Documents", 4.1], ["Movies", 3.9], [".docker", 3.6], ["Pictures", 3.4],
    ["Downloads", 3.3], [".bun", 3.3],
  ];
  const records = kept.map(([name, gb]) => ({ name, path: `/Users/me/${name}`, bytes: Math.round(gb * GB), has_children: true }));
  const other = { count: 214, bytes: 22 * GB };
  return {
    path: "/Users/me",
    name: "me",
    root: "/Users/me",
    total_bytes: records.reduce((a, r) => a + r.bytes, 0) + other.bytes,
    capacity_bytes: 994 * GB,
    records,
    other,
  };
}

/**
 * Answer the Space map's commands from the snapshot's MAP: `load_map`
 * returns the folder (null with `noSnapshot`), `reveal_in_finder` records
 * its path in `window.__revealed`. Every `load_map` path is recorded in
 * `window.__mapped`.
 */
export async function stubMap(page: Page, noSnapshot = false, root?: unknown) {
  const folders = snapshotMapFolders();
  if (root) folders["/"] = root;
  await page.addInitScript(
    ([folders, none]) => {
      const w = window as any;
      w.__mapped = [];
      w.__revealed = [];
      w.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
          if (cmd === "load_map") {
            w.__mapped.push(args.path);
            if (none) return null;
            const f = (folders as any)[args.path ?? "/"];
            if (!f) throw { code: "not-found", title: "folder not in snapshot", detail: `${args.path} is not in this snapshot`, retryable: false };
            return structuredClone(f);
          }
          if (cmd === "reveal_in_finder") {
            w.__revealed.push(args.path);
            return null;
          }
          throw new Error(`visual stub: ${cmd} is not available`);
        },
        transformCallback: () => 0,
      };
    },
    [folders, noSnapshot] as const,
  );
}

/**
 * The snapshot's LARGE rows as `disky large` lists them, largest first:
 * name, location, GB, days since last opened (chosen so the relative time
 * reads like the snapshot's label), unused for over a year.
 */
const SNAPSHOT_LARGE: [string, string, number, number, boolean][] = [
  ["Windows 11 VM.utm", "~/Library/Containers/UTM", 38.2, 245, false],
  ["Photo library backup.zip", "~/Documents/Backups", 22.6, 1100, true],
  ["Wedding 4K master.mov", "~/Movies/Exports", 18.4, 800, true],
  ["GoPro dump 2023", "~/Pictures", 14.9, 800, true],
  ["dataset-train.parquet", "~/Developer/ml", 6.8, 14, false],
  ["ubuntu-22.04-desktop.iso", "~/Downloads", 4.7, 400, true],
  ["Xcode_15.dmg", "~/Downloads", 3.2, 400, true],
  ["podcast-raw-ep42.wav", "~/Music/Recordings", 2.1, 155, false],
];

export function snapshotLargeFiles(now = Date.now()) {
  return SNAPSHOT_LARGE.map(([name, location, gb, days, old]) => ({
    name,
    path: `/Users/me/${location.slice(2)}/${name}`,
    location,
    bytes: Math.round(gb * GB),
    last_opened: new Date(now - days * 86_400_000).toISOString(),
    unused_1y: old,
  }));
}

/**
 * Answer the Large files commands from the snapshot's LARGE rows:
 * `load_large` filters them as the engine would (null with `noSnapshot`);
 * `trash_large` records its paths in `window.__trashed` and reports them
 * handled, freeing their bytes on a 74 GB-free disk.
 */
export async function stubLarge(page: Page, noSnapshot = false) {
  await page.addInitScript(
    ([files, none]) => {
      const w = window as any;
      w.__trashed = [];
      w.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
          if (cmd === "load_large") {
            if (none) return null;
            const records = (files as any[]).filter(
              (f) => args.filter === "all" || (args.filter === "over-10gb" ? f.bytes >= 10e9 : f.unused_1y),
            );
            return { filter: args.filter, count: records.length, total_bytes: records.reduce((a, f) => a + f.bytes, 0), records };
          }
          if (cmd === "trash_large") {
            w.__trashed.push(args.paths);
            const freed = (files as any[]).filter((f) => args.paths.includes(f.path)).reduce((a, f) => a + f.bytes, 0);
            return {
              handled: args.paths,
              failed: [],
              freed_physical_bytes: freed,
              history_id: "visual-large",
              disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 + freed },
            };
          }
          throw new Error(`visual stub: ${cmd} is not available`);
        },
        transformCallback: () => 0,
      };
    },
    [snapshotLargeFiles(), noSnapshot] as const,
  );
}

/** The snapshot's DUPES as `disky dupes` reports them: name, bytes per copy, copy folders. */
const SNAPSHOT_DUPES: [string, number, string[]][] = [
  ["IMG_2041.HEIC", 6e6, ["~/Pictures/Photos Library", "~/Desktop/to sort", "~/Downloads"]],
  ["Brand guidelines v3.pdf", 184e6, ["~/Documents/Work", "~/Downloads", "~/Desktop"]],
  ["Wedding 4K master.mov", 18.4 * GB, ["~/Movies/Exports", "~/Documents/Backups/Video"]],
  ["Lecture 07.mp4", 1.3 * GB, ["~/Documents/School", "~/Downloads"]],
];

/** The snapshot's DUPES as a `DupesView`, in the snapshot's order. */
export function snapshotDupesView() {
  const records = SNAPSHOT_DUPES.map(([name, bytes, locs]) => ({
    name,
    bytes,
    copies: locs.map((location) => ({ path: `/Users/me/${location.slice(2)}/${name}`, location, mtime: null })),
  }));
  return {
    min_size: 1e6,
    groups: records.length,
    extra_copies: records.reduce((a, g) => a + g.copies.length - 1, 0),
    reclaimable_bytes: records.reduce((a, g) => a + g.bytes * (g.copies.length - 1), 0),
    records,
  };
}

/**
 * Answer the Duplicates commands from the snapshot's DUPES: `load_dupes`
 * returns them (null with `noSnapshot`); `trash_dupes` records its paths in
 * `window.__trashed` and reports them handled on a 74 GB-free disk.
 */
export async function stubDupes(page: Page, noSnapshot = false) {
  await page.addInitScript(
    ([view, none]) => {
      const w = window as any;
      w.__trashed = [];
      w.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
          if (cmd === "load_dupes") return none ? null : view;
          if (cmd === "trash_dupes") {
            w.__trashed.push(args.paths);
            return {
              handled: args.paths,
              failed: [],
              freed_physical_bytes: 0,
              history_id: "visual-dupes",
              disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 94e9 },
            };
          }
          throw new Error(`visual stub: ${cmd} is not available`);
        },
        transformCallback: () => 0,
      };
    },
    [snapshotDupesView(), noSnapshot] as const,
  );
}

/**
 * Answer the Assistant commands from the snapshot's feed and AGENTS:
 * `load_assistant` returns `snapshotAssistantView`, `load_cleanup_view`
 * the snapshot's review rows (so "Review" lands on real results),
 * `set_prefs` is recorded in `window.__prefsSet`, and event subscriptions
 * are accepted.
 */
export async function stubAssistant(page: Page) {
  await page.addInitScript(
    ([assistant, review]) => {
      const w = window as any;
      w.__prefsSet = [];
      w.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
          if (cmd === "load_assistant") return assistant;
          if (cmd === "load_cleanup_view") return review;
          if (cmd === "set_prefs") {
            w.__prefsSet.push(args);
            return review.preferences;
          }
          if (cmd === "plugin:event|listen") return 1;
          if (cmd === "plugin:event|unlisten") return null;
          throw new Error(`visual stub: ${cmd} is not available`);
        },
        transformCallback: () => 0,
      };
    },
    [snapshotAssistantView(), reviewFixtureView()] as const,
  );
}
