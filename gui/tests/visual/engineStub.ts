import type { Page } from "@playwright/test";
import { snapshotReviewCategories } from "../fixtures/snapshotReview";

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
    preferences: { trash_by_default: true, safe_only: true, notifications: true, assistant: true },
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
