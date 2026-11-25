import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
const progressListeners: ((ev: { payload: unknown }) => void)[] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_event: string, cb: (ev: { payload: unknown }) => void) => {
    progressListeners.push(cb);
    return () => progressListeners.splice(progressListeners.indexOf(cb), 1);
  },
}));

import { App } from "../../App";
import { clean } from "../../lib/engine";
import { resetDiskState } from "../../lib/diskState";
import { parseCleanupView } from "../../lib/contract";
import { Results } from "../smartClean/Results";
import { INITIAL_STATE } from "../smartClean/phase";

const DAY = 86_400_000;

function engineView(overrides: Record<string, unknown> = {}) {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: null,
    snapshot_taken_at: null,
    is_stale: false,
    preferences: { trash_by_default: true, safe_only: true, notifications: true, assistant: true },
    large_files_count: 3,
    categories: [
      {
        id: "engine_cat_a",
        name: "Engine label A",
        description: "Engine description A",
        icon: "box",
        safety: "safe",
        bytes: 4_200_000_000,
        items: [{ name: "x", path: "/x", bytes: 4_200_000_000 }],
      },
      {
        id: "engine_cat_b",
        name: "Engine label B",
        description: "Engine description B",
        icon: "engine-icon-unknown",
        safety: "review",
        bytes: 600_000_000,
        items: [{ name: "y", path: "/y", bytes: 600_000_000 }],
      },
    ],
    catalogue: [
      { id: "engine_cat_a", name: "Engine label A", icon: "box", safety: "safe" },
      { id: "engine_cat_b", name: "Engine label B", icon: "box", safety: "review" },
      { id: "engine_cat_c", name: "Engine label C", icon: "box", safety: "review" },
    ],
    ...overrides,
  };
}

/** Route each Tauri command to a handler; unknown commands reject. */
function engine(handlers: Record<string, (args: any) => unknown>) {
  invoke.mockImplementation(async (cmd: string, args: unknown) => {
    const h = handlers[cmd];
    if (!h) throw new Error(`unexpected command ${cmd}`);
    return h(args);
  });
}

function renderApp() {
  render(() => <App />);
}

async function idle() {
  return screen.findByTestId("idle");
}

async function startScanAndFinish(scanned: ReturnType<typeof engineView>) {
  let finish!: (v: unknown) => void;
  engine({
    load_cleanup_view: () => engineView(),
    scan: () => new Promise((r) => (finish = r)),
  });
  renderApp();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Start scan" }));
  await screen.findByTestId("scanning");
  finish(scanned);
  return screen.findByTestId("results");
}

beforeEach(() => {
  invoke.mockReset();
  progressListeners.length = 0;
  resetDiskState();
  window.location.hash = "";
});
afterEach(cleanup);

/** The results list's category rows. */
function resultRows() {
  return within(screen.getByTestId("categories")).getAllByRole("listitem");
}

async function rendersExactlyTheEngineCategories() {
  await startScanAndFinish(engineView());
  const rows = resultRows();
  expect(rows.map((r) => r.getAttribute("data-category-id"))).toEqual(["engine_cat_a", "engine_cat_b"]);
  expect(rows[0]).toHaveTextContent("Engine label A");
  expect(rows[0]).toHaveTextContent("Engine description A");
  expect(rows[0]).toHaveTextContent("Safe");
  expect(rows[0]).toHaveTextContent("4.2 GB");
  expect(rows[1]).toHaveTextContent("Review");
  expect(rows[1]).toHaveTextContent("600 MB");
  expect(within(rows[1]).getByTestId("category-size")).toHaveClass("num");
  expect(invoke).toHaveBeenCalledWith("load_cleanup_view", { snapshot: null, goalBytes: null });
}

async function malformedDataShowsTheErrorStateNotAPartialList() {
  const bad = engineView();
  (bad.categories as any[])[1].bytes = -5;
  engine({ load_cleanup_view: () => bad });
  renderApp();
  expect(await screen.findByRole("alert")).toHaveTextContent("view.categories[1].bytes");
  expect(screen.queryByTestId("categories")).toBeNull();
  expect(screen.queryByTestId("idle")).toBeNull();
  expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
}

async function staleSnapshotWarnsBeforeCleaning() {
  await startScanAndFinish(engineView({ is_stale: true }));
  expect(await screen.findByTestId("stale-warning")).toHaveTextContent("more than a day old");
}

async function emptyViewShowsNothingToClean() {
  await startScanAndFinish(engineView({ categories: [] }));
  expect(await screen.findByText("Nothing worth clearing right now.")).toBeInTheDocument();
}

// ── Results phase (REQ-DCLEAN-005) ───────────────────────────────

/** A view whose categories carry several items, one with a long path. */
const LONG_PATH = "/Users/me/Library/Application Support/Some Very Long Vendor Name/Caches/deeply/nested/folder/cache.db";
function itemsView(overrides: Record<string, unknown> = {}) {
  return engineView({
    categories: [
      {
        id: "engine_cat_a",
        name: "Engine label A",
        description: "Engine description A",
        icon: "box",
        safety: "safe",
        bytes: 4_200_000_000,
        items: [
          { name: "one", path: "/a/one", bytes: 1_100_000_000 },
          { name: "two", path: LONG_PATH, bytes: 3_100_000_000 },
        ],
      },
      {
        id: "engine_cat_b",
        name: "Engine label B",
        description: "Engine description B",
        icon: "engine-icon-unknown",
        safety: "review",
        bytes: 600_000_000,
        items: [{ name: "y", path: "/y", bytes: 600_000_000 }],
      },
      {
        id: "engine_cat_c",
        name: "Engine label C",
        description: "Engine description C",
        icon: "box",
        safety: "safe",
        bytes: 0,
        items: [],
      },
    ],
    ...overrides,
  });
}

async function rowsAndItemsComeOnlyFromTheScan() {
  const view = itemsView();
  await startScanAndFinish(view);
  const rows = resultRows();
  // A1 + REQ-DCLEAN-019.A2: the scan's non-empty categories, in engine order, nothing else.
  expect(rows.map((r) => r.getAttribute("data-category-id"))).toEqual(["engine_cat_a", "engine_cat_b"]);
  fireEvent.click(within(rows[0]).getByRole("button", { name: "Show items in Engine label A" }));
  const items = within(rows[0]).getAllByRole("row");
  const engineItems = (view.categories as any[])[0].items;
  expect(items).toHaveLength(engineItems.length);
  items.forEach((tr, i) => {
    expect(tr).toHaveTextContent(engineItems[i].name);
    expect(tr.querySelector(".item-path")).toHaveTextContent(engineItems[i].path);
  });
  expect(within(items[0]).getByText("1.1 GB")).toHaveClass("num");
}

async function rowCarriesSelectionIconNameTagDescriptionSizeAndDisclosure() {
  await startScanAndFinish(itemsView());
  const screenEl = screen.getByTestId("results");
  expect(screenEl).toHaveAttribute("data-screen-label", "Smart clean · review");
  expect(within(screenEl).getByTestId("results-kicker")).toHaveTextContent(/^Scan finished · 2 categories$/);
  expect(within(screenEl).getByRole("heading", { level: 1 })).toHaveTextContent(/^Found 4.8 GB you can clear$/);
  const [a, b] = resultRows();
  expect(within(a).getByRole("checkbox", { name: "Engine label A" })).toBeInTheDocument();
  expect(a.querySelector(".result-icon svg")).not.toBeNull();
  expect(a.querySelector(".result-icon")).toHaveAttribute("data-icon", "box");
  // An icon name the GUI does not know still draws (the folder fallback).
  expect(b.querySelector(".result-icon svg")).not.toBeNull();
  expect(within(a).getByTestId("safety-tag")).toHaveClass("tag", "tag-accent-2");
  expect(within(a).getByTestId("safety-tag")).toHaveTextContent(/^Safe$/);
  expect(within(b).getByTestId("safety-tag")).toHaveClass("tag", "tag-accent");
  expect(within(b).getByTestId("safety-tag")).toHaveTextContent(/^Review$/);
  expect(a).toHaveTextContent("Engine description A");
  expect(within(a).getByTestId("category-size")).toHaveTextContent(/^4.2 GB$/);
  const disclose = within(a).getByRole("button", { name: "Show items in Engine label A" });
  expect(disclose).toHaveAttribute("aria-expanded", "false");
  expect(disclose).toHaveAttribute("aria-controls", within(a).getByTestId("category-items").id);
}

async function oneCategoryExpandedAtATime() {
  await startScanAndFinish(itemsView());
  const [a, b] = resultRows();
  const discA = within(a).getByRole("button", { name: "Show items in Engine label A" });
  const discB = within(b).getByRole("button", { name: "Show items in Engine label B" });
  fireEvent.click(discA);
  expect(discA).toHaveAttribute("aria-expanded", "true");
  expect(within(a).getByTestId("category-items")).toBeVisible();
  fireEvent.click(discB);
  expect(discA).toHaveAttribute("aria-expanded", "false");
  expect(within(a).getByTestId("category-items")).not.toBeVisible();
  expect(discB).toHaveAttribute("aria-expanded", "true");
  fireEvent.click(discB);
  expect(discB).toHaveAttribute("aria-expanded", "false");
  expect(screen.getByTestId("results").querySelectorAll('[aria-expanded="true"]')).toHaveLength(0);
}

async function chevronRotatesWithTheMotionToken() {
  await startScanAndFinish(itemsView());
  const disc = within(resultRows()[0]).getByRole("button", { name: "Show items in Engine label A" });
  expect(disc).toHaveAttribute("data-expanded", "false");
  expect(disc.querySelector("svg.chevron")).not.toBeNull();
  fireEvent.click(disc);
  expect(disc).toHaveAttribute("data-expanded", "true");
  const css = readFileSync(resolve(__dirname, "../smartClean/smartClean.css"), "utf8");
  expect(css).toMatch(/\.chevron\s*\{[^}]*transition:\s*var\(--motion-chevron\)/);
  expect(css).toMatch(/\[data-expanded="true"\]\s*\.chevron\s*\{[^}]*transform:/);
}

async function longPathIsEllipsisedWithTooltipAndAccessibleName() {
  await startScanAndFinish(itemsView());
  const a = resultRows()[0];
  fireEvent.click(within(a).getByRole("button", { name: "Show items in Engine label A" }));
  const cell = within(a).getByText(LONG_PATH);
  expect(cell).toHaveClass("item-path");
  expect(cell).toHaveAttribute("title", LONG_PATH);
  expect(cell).toHaveAttribute("aria-label", LONG_PATH);
  const css = readFileSync(resolve(__dirname, "../smartClean/smartClean.css"), "utf8");
  expect(css).toMatch(/\.item-path\s*\{[^}]*text-overflow:\s*ellipsis/);
}

// ── Selection (REQ-DCLEAN-006) ───────────────────────────────────

function checkedIds() {
  return resultRows()
    .filter((r) => within(r).getByRole("checkbox").getAttribute("aria-checked") === "true")
    .map((r) => r.getAttribute("data-category-id"));
}

function cleanButton() {
  return screen.getByTestId("clean");
}

async function safeOnlyPreferencePreselectsSafeCategories() {
  await startScanAndFinish(itemsView());
  expect(checkedIds()).toEqual(["engine_cat_a"]);
  expect(cleanButton()).toHaveTextContent(/^Clean 4.2 GB$/);
}

async function safeOnlyOffPreselectsEverything() {
  await startScanAndFinish(
    itemsView({ preferences: { trash_by_default: true, safe_only: false, notifications: true, assistant: true } }),
  );
  expect(checkedIds()).toEqual(["engine_cat_a", "engine_cat_b"]);
  expect(cleanButton()).toHaveTextContent(/^Clean 4.8 GB$/);
}

async function safeOnlyAndSelectAllButtons() {
  await startScanAndFinish(itemsView());
  fireEvent.click(screen.getByRole("button", { name: "Select all" }));
  expect(checkedIds()).toEqual(["engine_cat_a", "engine_cat_b"]);
  fireEvent.click(screen.getByRole("button", { name: "Safe only" }));
  expect(checkedIds()).toEqual(["engine_cat_a"]);
  expect(screen.getByRole("button", { name: "Safe only" })).toHaveClass("btn", "btn-ghost");
}

async function togglingFlipsOnlyThatCategoryAndUpdatesTheTotal() {
  await startScanAndFinish(itemsView());
  const [a, b] = resultRows();
  fireEvent.click(within(b).getByRole("checkbox", { name: "Engine label B" }));
  expect(checkedIds()).toEqual(["engine_cat_a", "engine_cat_b"]);
  expect(cleanButton()).toHaveTextContent(/^Clean 4.8 GB$/);
  fireEvent.click(within(a).getByRole("checkbox", { name: "Engine label A" }));
  expect(checkedIds()).toEqual(["engine_cat_b"]);
  expect(cleanButton()).toHaveTextContent(/^Clean 600 MB$/);
  expect(cleanButton()).toBeEnabled();
}

async function cleanIsDisabledWhenNothingIsSelected() {
  await startScanAndFinish(itemsView());
  fireEvent.click(within(resultRows()[0]).getByRole("checkbox", { name: "Engine label A" }));
  expect(checkedIds()).toEqual([]);
  expect(cleanButton()).toHaveTextContent(/^Clean 0 MB$/);
  expect(cleanButton()).toBeDisabled();
  expect(cleanButton()).toHaveClass("btn-primary");
}

async function footerNoteFollowsTrashByDefault() {
  await startScanAndFinish(itemsView());
  expect(screen.getByTestId("safety-note")).toHaveTextContent(
    /^Items move to the Trash and can be restored from History for 30 days\.$/,
  );
  cleanup();
  resetDiskState();
  await startScanAndFinish(
    itemsView({ preferences: { trash_by_default: false, safe_only: true, notifications: true, assistant: true } }),
  );
  expect(screen.getByTestId("safety-note")).toHaveTextContent(
    /^Trash is off in Settings, so items will be deleted permanently\.$/,
  );
}

async function noSelectNoneAction() {
  await startScanAndFinish(itemsView());
  const results = screen.getByTestId("results");
  expect(within(results).queryByRole("button", { name: /select none|deselect|clear selection/i })).toBeNull();
  const names = within(results)
    .getAllByRole("button")
    .map((b) => b.textContent?.trim())
    .filter(Boolean);
  expect(names).toEqual(["Safe only", "Select all", "Cancel", "Clean 4.2 GB"]);
}

// ── Empty states (REQ-DCLEAN-019) ────────────────────────────────

async function zeroCategoriesShowTheEmptyStateWithBackToHome() {
  const zeroOnly = itemsView();
  (zeroOnly as any).categories = [(zeroOnly.categories as any[])[2]]; // one category, 0 bytes
  await startScanAndFinish(zeroOnly);
  const empty = screen.getByTestId("results-empty");
  expect(within(empty).getByRole("heading", { level: 1 })).toHaveTextContent(/^Nothing worth clearing right now\.$/);
  expect(screen.queryByTestId("clean")).toBeNull();
  expect(screen.queryByRole("button", { name: /^Clean/ })).toBeNull();
  expect(screen.queryByTestId("categories")).toBeNull();
  fireEvent.click(within(empty).getByRole("button", { name: "Back to home" }));
  expect(await idle()).toBeInTheDocument();
}

async function zeroSizeCategoryIsOmitted() {
  await startScanAndFinish(itemsView());
  expect(screen.getByTestId("results").querySelector('[data-category-id="engine_cat_c"]')).toBeNull();
  expect(screen.getByTestId("results-kicker")).toHaveTextContent("2 categories");
}

function expandedCategoryWithNoItemsSaysNoItems() {
  // The strict parser refuses this shape from the engine (bytes ≠ item sum),
  // so the branch is rendered directly.
  const view = parseCleanupView(engineView({ categories: [] }));
  view.categories = [
    { id: "engine_cat_a", name: "Engine label A", description: "d", icon: "box", safety: "safe", bytes: 5, items: [] },
  ];
  render(() => (
    <Results
      view={view}
      state={{ ...INITIAL_STATE, phase: "results", expanded: "engine_cat_a" }}
      onToggleExpand={() => undefined}
      onToggleSelect={() => undefined}
      onSelectSafe={() => undefined}
      onSelectAll={() => undefined}
      onBack={() => undefined}
    />
  ));
  const items = screen.getByTestId("category-items");
  expect(items).toBeVisible();
  expect(items).toHaveTextContent(/^No items$/);
  expect(within(items).queryByRole("table")).toBeNull();
}

async function cancelReturnsHome() {
  await startScanAndFinish(itemsView());
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(await idle()).toBeInTheDocument();
  expect(invoke.mock.calls.map(([cmd]) => cmd)).toEqual(["load_cleanup_view", "scan"]);
}

async function idleShowsHeadingDescriptionActionsAndShortcuts() {
  engine({ load_cleanup_view: () => engineView() });
  renderApp();
  const home = await idle();
  expect(within(home).getByRole("heading", { level: 1 })).toHaveTextContent("Let’s clear some room.");
  expect(within(home).getByTestId("idle-description")).toHaveTextContent(
    "Nothing is removed until you review it, and everything goes to the Trash first.",
  );
  expect(within(home).getByRole("button", { name: "Start scan" })).toHaveClass("btn-primary");
  expect(within(home).getByRole("button", { name: "Explore disk" })).toHaveClass("btn-secondary");
  for (const id of ["assistant", "map", "large"]) expect(within(home).getByTestId(`shortcut-${id}`)).toBeInTheDocument();
  expect(within(home).getByTestId("usage-ring")).toHaveAttribute("data-phase", "idle");
  expect(within(home).getByTestId("goal-tag")).toHaveTextContent("Below your goal");
}

async function startScanEntersScanningAtZero() {
  engine({ load_cleanup_view: () => engineView(), scan: () => new Promise(() => undefined) });
  renderApp();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Start scan" }));
  expect(await screen.findByTestId("scanning")).toBeInTheDocument();
  expect(screen.getByTestId("scan-progress")).toHaveTextContent(/^0%$/);
  expect(screen.queryByTestId("idle")).toBeNull();
  expect(invoke).toHaveBeenCalledWith("scan", { root: null });
  await waitFor(() => expect(progressListeners).toHaveLength(1));
  emit(0.42, { current_path: "/Users" });
  expect(await screen.findByText("42%")).toBeInTheDocument();
}

/** Push one scan progress event through the mocked `listen`. */
function emit(fraction: number, more: Record<string, unknown> = {}) {
  for (const l of [...progressListeners]) {
    l({ payload: { fraction, current_path: "", completed_category_ids: [], completed_category_bytes: {}, ...more } });
  }
}

/** Start a scan that stays open; returns its resolver. */
async function startOpenScan(view = engineView()) {
  let finish!: (v: unknown) => void;
  engine({
    load_cleanup_view: () => view,
    scan: () => new Promise((r) => (finish = r)),
    stop_scan: () => undefined,
  });
  renderApp();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Start scan" }));
  const scanning = await screen.findByTestId("scanning");
  await waitFor(() => expect(progressListeners).toHaveLength(1));
  return { scanning, finish: (v: unknown) => finish(v) };
}

/** Let promise chains settle without advancing any timer. */
async function settle() {
  for (let i = 0; i < 20; i++) await Promise.resolve();
}

async function scanningShowsHeadingPathAndTruncatedTooltip() {
  const { scanning } = await startOpenScan();
  expect(within(scanning).getByRole("heading", { level: 1 })).toHaveTextContent("Looking around…");
  const long = "/Users/me/Library/Application Support/Some Very Long Vendor Name/Caches/deeply/nested/folder";
  emit(0.1, { current_path: long });
  const path = await within(scanning).findByText(long);
  expect(path).toHaveAttribute("data-testid", "scan-path");
  expect(path).toHaveAttribute("title", long);
  expect(path).toHaveClass("scan-path");
  expect(within(scanning).getByRole("button", { name: "Stop" })).toHaveClass("btn", "btn-secondary");
}

async function progressBarFollowsEventsAndNeverDecreases() {
  const { scanning } = await startOpenScan();
  const bar = within(scanning).getByRole("progressbar");
  const seen: number[] = [];
  for (const f of [0.1, 0.35, 0.2, 0.6, 1]) {
    emit(f);
    await settle();
    seen.push(Number(bar.getAttribute("aria-valuenow")));
  }
  expect(seen).toEqual([10, 35, 35, 60, 99]);
  expect(screen.getByTestId("scan-progress")).toHaveTextContent(/^99%$/);
  expect(screen.getByTestId("scan-bar-fill").style.width).toBe("99%");
}

async function rowsShowSizesOnlyOnceTheirCategoryIsComplete() {
  const { scanning } = await startOpenScan(engineView({ categories: [] }));
  // First scan: the prior view has no hits, yet every catalogued category is listed.
  const rows = within(scanning).getAllByRole("row");
  expect(rows.map((r) => r.getAttribute("data-category-id"))).toEqual(["engine_cat_a", "engine_cat_b", "engine_cat_c"]);
  for (const r of rows) {
    expect(r).toHaveTextContent("Checking…");
    expect(r).not.toHaveTextContent(/\d+(\.\d+)? [KMG]B/);
  }
  emit(0.5, { completed_category_ids: ["engine_cat_a", "engine_cat_c"], completed_category_bytes: { engine_cat_a: 4_200_000_000 } });
  await settle();
  expect(rows[0]).toHaveTextContent("4.2 GB");
  expect(rows[0]).toHaveAttribute("data-status", "found");
  expect(rows[0].querySelector("svg")).not.toBeNull();
  expect(rows[1]).toHaveTextContent("Checking…");
  expect(rows[1].querySelector("svg")).toBeNull();
  expect(rows[2]).toHaveTextContent("Nothing found");
  expect(rows[2]).not.toHaveTextContent("GB");
}

async function completionEntersResultsWithoutAPause() {
  const { finish } = await startOpenScan();
  finish(engineView());
  await settle(); // microtasks only: no timer is allowed to run
  expect(screen.getByTestId("results")).toBeInTheDocument();
  expect(screen.queryByTestId("scanning")).toBeNull();
}

async function stopDiscardsALateResultAndModifiesNothing() {
  const { scanning, finish } = await startOpenScan();
  emit(0.4, { current_path: "/Users", completed_category_ids: ["engine_cat_a"], completed_category_bytes: { engine_cat_a: 1 } });
  fireEvent.click(within(scanning).getByRole("button", { name: "Stop" }));
  expect(await idle()).toBeInTheDocument();
  finish(engineView());
  await settle();
  expect(screen.queryByTestId("results")).toBeNull();
  expect(screen.getByTestId("idle")).toBeInTheDocument();
  expect(invoke.mock.calls.map(([cmd]) => cmd)).toEqual(["load_cleanup_view", "scan", "stop_scan"]);
  // A new scan starts clean: no stale path, progress or completed rows.
  fireEvent.click(within(screen.getByTestId("idle")).getByRole("button", { name: "Start scan" }));
  const again = await screen.findByTestId("scanning");
  expect(within(again).getByTestId("scan-progress")).toHaveTextContent(/^0%$/);
  expect(within(again).getByTestId("scan-path")).toHaveTextContent(/^$/);
  for (const r of within(again).getAllByRole("row")) expect(r).toHaveTextContent("Checking…");
}

async function slowLoadShowsBusyOnlyAfterTheThreshold() {
  vi.useFakeTimers();
  try {
    engine({ load_cleanup_view: () => new Promise(() => undefined) });
    renderApp();
    vi.advanceTimersByTime(199);
    expect(screen.queryByTestId("busy")).toBeNull();
    vi.advanceTimersByTime(1);
    expect(screen.getByTestId("busy")).toHaveAttribute("role", "status");
  } finally {
    vi.useRealTimers();
  }
}

async function fastLoadNeverShowsBusy() {
  vi.useFakeTimers();
  try {
    engine({ load_cleanup_view: () => engineView() });
    renderApp();
    vi.advanceTimersByTime(50);
    await settle();
    vi.advanceTimersByTime(1_000);
    expect(screen.getByTestId("idle")).toBeInTheDocument();
    expect(screen.queryByTestId("busy")).toBeNull();
  } finally {
    vi.useRealTimers();
  }
}

async function stopReturnsToIdleWithoutResults() {
  let finish!: (v: unknown) => void;
  engine({
    load_cleanup_view: () => engineView(),
    scan: () => new Promise((r) => (finish = r)),
    stop_scan: () => undefined,
  });
  renderApp();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Start scan" }));
  fireEvent.click(within(await screen.findByTestId("scanning")).getByRole("button", { name: "Stop" }));
  expect(await idle()).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith("stop_scan");
  finish(engineView()); // the partial snapshot's view arrives late
  await Promise.resolve();
  expect(screen.queryByTestId("results")).toBeNull();
  expect(screen.getByTestId("idle")).toBeInTheDocument();
}

async function kickerShowsNoScansYetWithoutASnapshot() {
  engine({ load_cleanup_view: () => engineView({ last_scan_at: null, categories: [] }) });
  renderApp();
  expect(within(await idle()).getByTestId("last-scan")).toHaveTextContent(/^No scans yet$/);
}

async function kickerShowsTheSnapshotAge() {
  engine({ load_cleanup_view: () => engineView({ last_scan_at: new Date(Date.now() - 7 * DAY - 1000).toISOString() }) });
  renderApp();
  expect(within(await idle()).getByTestId("last-scan")).toHaveTextContent(/^Last scan 7 days ago$/);
}

async function assistantShortcutWatchesTheGoalWithNoSuggestions() {
  engine({ load_cleanup_view: () => engineView() });
  renderApp();
  expect(within(await idle()).getByTestId("shortcut-assistant-desc")).toHaveTextContent(/^Watching your space goal$/);
}

async function largeFilesCountComesFromTheEngine() {
  engine({ load_cleanup_view: () => engineView({ large_files_count: 3 }) });
  renderApp();
  const home = await idle();
  expect(within(home).getByTestId("shortcut-large-desc")).toHaveTextContent(/^3 files over 2 GB$/);
  expect(home).not.toHaveTextContent("8 files over 2 GB");
}

async function descriptionIsTruthfulWhenTrashIsOff() {
  engine({
    load_cleanup_view: () =>
      engineView({ preferences: { trash_by_default: false, safe_only: true, notifications: true, assistant: true } }),
  });
  renderApp();
  const desc = within(await idle()).getByTestId("idle-description");
  expect(desc).toHaveTextContent(/Nothing is removed until you review it\.$/);
  expect(desc).not.toHaveTextContent("Trash");
}

async function exploreDiskOpensSpaceMap() {
  engine({ load_cleanup_view: () => engineView() });
  renderApp();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Explore disk" }));
  const main = screen.getByRole("main");
  expect(await within(main).findByRole("heading", { level: 1, name: "Space Map" })).toBeInTheDocument();
  expect(within(main).getByText("Coming soon")).toBeInTheDocument();
}

async function shortcutsOpenTheirScreens() {
  engine({ load_cleanup_view: () => engineView() });
  renderApp();
  fireEvent.click(within(await idle()).getByTestId("shortcut-large"));
  expect(await within(screen.getByRole("main")).findByRole("heading", { level: 1, name: "Large files" })).toBeInTheDocument();
}

async function cleanUpdatesRingTagAndSidebar() {
  engine({
    load_cleanup_view: () => engineView(),
    clean: () => ({
      mode: "trash",
      freed_bytes: 16e9,
      succeeded: ["/x"],
      failed: [],
      history_entry: null,
      disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 90e9 },
    }),
  });
  renderApp();
  const home = await idle();
  const ring = within(home).getByTestId("usage-ring");
  expect(within(ring).getByTestId("ring-free")).toHaveTextContent("74 GB");
  expect(within(home).getByTestId("goal-tag")).toHaveTextContent("Below your goal");
  expect(screen.getByTestId("disk-meter")).toHaveTextContent("74 GB free");

  await clean(["/x"], "trash");

  expect(within(home).getByTestId("usage-ring")).toBe(ring); // no remount, no refetch
  expect(within(ring).getByTestId("ring-free")).toHaveTextContent("90 GB");
  expect(within(home).getByTestId("goal-tag")).toHaveTextContent("Goal met");
  expect(within(home).getByTestId("goal-tag")).toHaveClass("tag-accent-2");
  expect(screen.getByTestId("disk-meter")).toHaveTextContent("90 GB free");
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "load_cleanup_view")).toHaveLength(1);
}

// ── Confirm dialog (REQ-DCLEAN-007, REQ-DCLEAN-022) ──────────────

const TRASH_OFF = { preferences: { trash_by_default: false, safe_only: true, notifications: true, assistant: true } };

/** Focus "Clean <total>" (as a real click would) and activate it. */
function openDialog() {
  cleanButton().focus();
  fireEvent.click(cleanButton());
  return screen.getByRole("dialog");
}

function modeRadio(name: "Move to Trash" | "Delete permanently") {
  return within(screen.getByRole("radiogroup", { name: "Clean mode" })).getByRole("radio", { name });
}

function cleanCalls() {
  return invoke.mock.calls.filter(([cmd]) => cmd === "clean");
}

async function cleanOpensTheDialogTitledWithTheTotal() {
  await startScanAndFinish(itemsView());
  expect(screen.queryByRole("dialog")).toBeNull();
  const dialog = openDialog();
  expect(dialog).toHaveAttribute("aria-modal", "true");
  expect(within(dialog).getByRole("heading", { level: 2 })).toHaveTextContent(/^Clean 4.2 GB\?$/);
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Clean now" })).toBeInTheDocument();
}

async function dialogListsExactlyTheSelectedCategories() {
  await startScanAndFinish(itemsView());
  fireEvent.click(within(resultRows()[1]).getByRole("checkbox", { name: "Engine label B" }));
  const footer = cleanButton().textContent;
  const dialog = openDialog();
  const rows = within(within(dialog).getByTestId("confirm-categories")).getAllByRole("listitem");
  expect(rows.map((r) => r.getAttribute("data-category-id"))).toEqual(["engine_cat_a", "engine_cat_b"]);
  expect(rows[0]).toHaveTextContent("Engine label A4.2 GB");
  expect(rows[1]).toHaveTextContent("Engine label B600 MB");
  expect(within(dialog).getByRole("heading", { level: 2 }).textContent).toBe(`${footer}?`);
}

async function modeStartsAtTheTrashPreference() {
  await startScanAndFinish(itemsView());
  openDialog();
  expect(modeRadio("Move to Trash")).toBeChecked();
  expect(modeRadio("Delete permanently")).not.toBeChecked();
  cleanup();
  resetDiskState();
  await startScanAndFinish(itemsView(TRASH_OFF));
  openDialog();
  expect(modeRadio("Delete permanently")).toBeChecked();
  expect(modeRadio("Move to Trash")).not.toBeChecked();
}

async function noteFollowsTheChosenMode() {
  await startScanAndFinish(itemsView());
  openDialog();
  expect(screen.getByTestId("mode-note")).toHaveTextContent(/^You can restore everything from History for 30 days\.$/);
  fireEvent.click(modeRadio("Delete permanently"));
  expect(screen.getByTestId("mode-note")).toHaveTextContent(/^This can't be undone\.$/);
  fireEvent.click(modeRadio("Move to Trash"));
  expect(screen.getByTestId("mode-note")).toHaveTextContent(/^You can restore everything from History for 30 days\.$/);
}

async function cancelEscapeAndBackdropCloseWithTheSelectionUnchanged() {
  await startScanAndFinish(itemsView());
  fireEvent.click(within(resultRows()[1]).getByRole("checkbox", { name: "Engine label B" }));
  const close: [string, () => void][] = [
    ["Cancel", () => fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }))],
    ["Escape", () => fireEvent.keyDown(document, { key: "Escape" })],
    ["backdrop", () => fireEvent.click(screen.getByTestId("confirm-backdrop"))],
  ];
  for (const [how, act] of close) {
    openDialog();
    // A click inside the dialog panel is not a backdrop click.
    fireEvent.click(screen.getByRole("dialog"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    act();
    expect(screen.queryByRole("dialog"), how).toBeNull();
    expect(screen.getByTestId("results")).toBeInTheDocument();
    expect(checkedIds(), how).toEqual(["engine_cat_a", "engine_cat_b"]);
    expect(cleanButton()).toHaveTextContent(/^Clean 4.8 GB$/);
  }
  expect(cleanCalls()).toEqual([]);
}

async function nothingIsCleanedBeforeCleanNow() {
  await startScanAndFinish(itemsView());
  const dialog = openDialog();
  fireEvent.click(modeRadio("Delete permanently"));
  fireEvent.click(modeRadio("Move to Trash"));
  expect(cleanCalls()).toEqual([]);
  fireEvent.click(within(dialog).getByRole("button", { name: "Clean now" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(await screen.findByTestId("cleaning")).toBeInTheDocument();
  // REQ-DCLEAN-008 runs the clean; this change stops at the cleaning phase.
  expect(cleanCalls()).toEqual([]);
  expect(invoke.mock.calls.map(([cmd]) => cmd)).toEqual(["load_cleanup_view", "scan"]);
}

async function deleteModeGivesCleanNowTheDangerFill() {
  await startScanAndFinish(itemsView());
  const now = within(openDialog()).getByRole("button", { name: "Clean now" });
  expect(now).toHaveClass("btn", "btn-primary");
  expect(now).not.toHaveClass("btn-danger");
  fireEvent.click(modeRadio("Delete permanently"));
  expect(now).toHaveClass("btn", "btn-primary", "btn-danger");
  fireEvent.click(modeRadio("Move to Trash"));
  expect(now).not.toHaveClass("btn-danger");
}

function dangerAndBackdropUseTheirTokens() {
  const tokens = readFileSync(resolve(__dirname, "../../styles/gui-tokens.css"), "utf8");
  expect(tokens).toMatch(/--color-danger:\s*var\(--color-accent-800\);/);
  const verbatim = readFileSync(resolve(__dirname, "../../styles/tokens.css"), "utf8");
  expect(verbatim).not.toMatch(/--color-danger/); // tokens.css stays a verbatim copy
  const css = readFileSync(resolve(__dirname, "../smartClean/smartClean.css"), "utf8");
  expect(css).toMatch(/\.confirm-backdrop\s*\{[^}]*background:\s*var\(--overlay-backdrop\);/);
  expect(css).toMatch(/\.btn\.btn-danger\s*\{[^}]*background:\s*var\(--color-danger\);/);
}

async function modeSwitchIsARadiogroupWithArrowKeys() {
  await startScanAndFinish(itemsView());
  openDialog();
  const group = screen.getByRole("radiogroup", { name: "Clean mode" });
  expect(group).toHaveClass("seg");
  const opts = group.querySelectorAll(".seg-opt");
  expect(opts).toHaveLength(2);
  expect([...opts].map((o) => o.textContent)).toEqual(["Move to Trash", "Delete permanently"]);
  fireEvent.keyDown(modeRadio("Move to Trash"), { key: "ArrowRight" });
  expect(modeRadio("Delete permanently")).toBeChecked();
  expect(modeRadio("Delete permanently")).toHaveFocus();
  fireEvent.keyDown(modeRadio("Delete permanently"), { key: "ArrowRight" });
  expect(modeRadio("Move to Trash")).toBeChecked();
  fireEvent.keyDown(modeRadio("Move to Trash"), { key: "ArrowLeft" });
  expect(modeRadio("Delete permanently")).toBeChecked();
  expect(screen.getByTestId("mode-note")).toHaveTextContent("This can't be undone.");
}

// REQ-DCLEAN-022

async function dialogIsModalAndLabelledByItsTitle() {
  await startScanAndFinish(itemsView());
  const dialog = openDialog();
  expect(dialog).toHaveAttribute("aria-modal", "true");
  const title = within(dialog).getByRole("heading", { level: 2 });
  expect(dialog).toHaveAttribute("aria-labelledby", title.id);
  expect(screen.getByRole("dialog", { name: "Clean 4.2 GB?" })).toBe(dialog);
}

async function focusOpensOnCancel() {
  await startScanAndFinish(itemsView());
  const dialog = openDialog();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
}

async function focusIsTrappedInsideTheDialog() {
  await startScanAndFinish(itemsView());
  const dialog = openDialog();
  const cancel = within(dialog).getByRole("button", { name: "Cancel" });
  const now = within(dialog).getByRole("button", { name: "Clean now" });
  const radio = modeRadio("Move to Trash");
  // Tab from the last stop wraps to the first; Shift+Tab from the first wraps to the last.
  now.focus();
  fireEvent.keyDown(document.activeElement!, { key: "Tab" });
  expect(radio).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey: true });
  expect(now).toHaveFocus();
  // Focus that escapes is pulled back in, and the page behind is inert.
  cleanButton().focus();
  expect(dialog.contains(document.activeElement)).toBe(true);
  expect(cancel).toHaveFocus();
  expect(screen.getByTestId("results").closest("[inert]")).not.toBeNull();
  expect(dialog.closest("[inert]")).toBeNull();
}

async function focusReturnsToTheOpener() {
  await startScanAndFinish(itemsView());
  for (const close of [
    () => fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" })),
    () => fireEvent.keyDown(document, { key: "Escape" }),
    () => fireEvent.click(screen.getByTestId("confirm-backdrop")),
  ]) {
    openDialog();
    close();
    expect(cleanButton()).toHaveFocus();
    expect(screen.getByTestId("results").closest("[inert]")).toBeNull();
  }
}

async function focusFallsBackToTheResultsHeadingWithoutAnOpener() {
  await startScanAndFinish(itemsView());
  // The notification path opens the dialog without a control on the results screen.
  (document.activeElement as HTMLElement | null)?.blur();
  fireEvent.click(cleanButton());
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.getByTestId("results-heading")).toHaveFocus();
}

describe("Smart Clean confirm dialog (REQ-DCLEAN-007)", () => {
  it("Clean opens a dialog titled Clean <total>? (REQ-DCLEAN-007)", cleanOpensTheDialogTitledWithTheTotal);
  it("nothing is cleaned before Clean now (REQ-DCLEAN-007.A1)", nothingIsCleanedBeforeCleanNow);
  it("the mode starts at trash-by-default (REQ-DCLEAN-007.A2)", modeStartsAtTheTrashPreference);
  it("the note follows the chosen mode (REQ-DCLEAN-007.A3)", noteFollowsTheChosenMode);
  it("Cancel, Escape and the backdrop close it with the selection unchanged (REQ-DCLEAN-007.A4)", cancelEscapeAndBackdropCloseWithTheSelectionUnchanged);
  it("lists exactly the selected categories; title total equals the footer (REQ-DCLEAN-007.A5)", dialogListsExactlyTheSelectedCategories);
  it("Delete permanently gives Clean now the danger fill (REQ-DCLEAN-007.A6)", deleteModeGivesCleanNowTheDangerFill);
  it("--color-danger aliases --color-accent-800 and the backdrop uses --overlay-backdrop (REQ-DCLEAN-007.A6, A7)", dangerAndBackdropUseTheirTokens);
  it("the mode switch is a .seg radiogroup driven by arrow keys (REQ-DCLEAN-020.A3)", modeSwitchIsARadiogroupWithArrowKeys);
});

describe("confirm dialog focus (REQ-DCLEAN-022)", () => {
  it("is a modal labelled by its title (REQ-DCLEAN-022.A1)", dialogIsModalAndLabelledByItsTitle);
  it("opens with focus on Cancel (REQ-DCLEAN-022.A2)", focusOpensOnCancel);
  it("traps focus and makes the page inert (REQ-DCLEAN-022.A3)", focusIsTrappedInsideTheDialog);
  it("returns focus to the opener on close (REQ-DCLEAN-022.A4)", focusReturnsToTheOpener);
  it("falls back to the results heading without an opener (REQ-DCLEAN-022.A4)", focusFallsBackToTheResultsHeadingWithoutAnOpener);
});

describe("Smart Clean consumes the engine view", () => {
  it("renders exactly the engine's categories (REQ-DCLEAN-027.A4)", rendersExactlyTheEngineCategories);
  it("malformed data renders the error state, not a partial list (REQ-DCLEAN-029.A4)", malformedDataShowsTheErrorStateNotAPartialList);
  it("a stale snapshot warns before cleaning (REQ-DCLEAN-029.A2)", staleSnapshotWarnsBeforeCleaning);
  it("an empty view says nothing to clean (REQ-DCLEAN-027)", emptyViewShowsNothingToClean);
});

describe("Smart Clean results phase (REQ-DCLEAN-005)", () => {
  it("rows and items come only from the scan (REQ-DCLEAN-005.A1)", rowsAndItemsComeOnlyFromTheScan);
  it("each row has selection, icon, name, tag, description, size and disclosure (REQ-DCLEAN-005.A2)", rowCarriesSelectionIconNameTagDescriptionSizeAndDisclosure);
  it("one category is expanded at a time (REQ-DCLEAN-005.A3)", oneCategoryExpandedAtATime);
  it("the chevron rotates using --motion-chevron (REQ-DCLEAN-005.A4)", chevronRotatesWithTheMotionToken);
  it("long paths are ellipsised with a tooltip and accessible name (REQ-DCLEAN-005.A5)", longPathIsEllipsisedWithTooltipAndAccessibleName);
});

describe("Smart Clean selection (REQ-DCLEAN-006)", () => {
  it("safe-only on preselects exactly the Safe categories (REQ-DCLEAN-006.A1)", safeOnlyPreferencePreselectsSafeCategories);
  it("safe-only off preselects every category (REQ-DCLEAN-006.A1)", safeOnlyOffPreselectsEverything);
  it("Safe only and Select all (REQ-DCLEAN-006.A2)", safeOnlyAndSelectAllButtons);
  it("toggling flips one category and updates the Clean total (REQ-DCLEAN-006.A3, A4)", togglingFlipsOnlyThatCategoryAndUpdatesTheTotal);
  it("the footer note follows trash-by-default (REQ-DCLEAN-006.A5)", footerNoteFollowsTrashByDefault);
  it("no Select none action is offered (REQ-DCLEAN-006.A6)", noSelectNoneAction);
  it("Clean is disabled with nothing selected (REQ-DCLEAN-016.A1)", cleanIsDisabledWhenNothingIsSelected);
  it("Cancel returns home and cleans nothing (REQ-DCLEAN-006)", cancelReturnsHome);
});

describe("Smart Clean empty results (REQ-DCLEAN-019)", () => {
  it("zero categories show the empty state with Back to home and no Clean (REQ-DCLEAN-019.A1)", zeroCategoriesShowTheEmptyStateWithBackToHome);
  it("a zero-size category is omitted (REQ-DCLEAN-019.A2)", zeroSizeCategoryIsOmitted);
  it("an expanded category with no items says No items (REQ-DCLEAN-019.A3)", expandedCategoryWithNoItemsSaysNoItems);
});

describe("Smart Clean idle phase (REQ-DCLEAN-003)", () => {
  it("shows heading, description, both actions, shortcuts and the ring (REQ-DCLEAN-003)", idleShowsHeadingDescriptionActionsAndShortcuts);
  it("Start scan enters scanning with progress at 0 (REQ-DCLEAN-003.A1)", startScanEntersScanningAtZero);
  it("Stop returns to idle and shows no results (REQ-DCLEAN-003.A1)", stopReturnsToIdleWithoutResults);
  it("kicker reads No scans yet without a snapshot (REQ-DCLEAN-003.A2)", kickerShowsNoScansYetWithoutASnapshot);
  it("kicker shows the newest snapshot's age (REQ-DCLEAN-003.A2)", kickerShowsTheSnapshotAge);
  it("Assistant shortcut watches the goal with no suggestions (REQ-DCLEAN-003.A3)", assistantShortcutWatchesTheGoalWithNoSuggestions);
  it("Large files count comes from the engine (REQ-DCLEAN-003.A4)", largeFilesCountComesFromTheEngine);
  it("description is truthful when trash-by-default is off (REQ-DCLEAN-003.A5)", descriptionIsTruthfulWhenTrashIsOff);
  it("Explore disk opens Space Map (REQ-DCLEAN-003)", exploreDiskOpensSpaceMap);
  it("shortcut cards open their screens (REQ-DCLEAN-003)", shortcutsOpenTheirScreens);
});

describe("Smart Clean scanning phase (REQ-DCLEAN-004)", () => {
  it("shows the heading, the ellipsised path with its tooltip, and Stop (REQ-DCLEAN-004)", scanningShowsHeadingPathAndTruncatedTooltip);
  it("the progress bar follows events and never decreases (REQ-DCLEAN-004.A1)", progressBarFollowsEventsAndNeverDecreases);
  it("rows show a size only once their category is complete (REQ-DCLEAN-004.A4)", rowsShowSizesOnlyOnceTheirCategoryIsComplete);
  it("completion enters results automatically (REQ-DCLEAN-004.A2)", completionEntersResultsWithoutAPause);
  it("Stop returns to idle, discards a late result and modifies nothing (REQ-DCLEAN-004.A3)", stopDiscardsALateResultAndModifiesNothing);
});

describe("loading states (REQ-DCLEAN-017)", () => {
  it("a slow load shows the busy indicator only after 200 ms (REQ-DCLEAN-017.A2)", slowLoadShowsBusyOnlyAfterTheThreshold);
  it("a fast load never shows the busy indicator (REQ-DCLEAN-017.A2)", fastLoadNeverShowsBusy);
  it("scanning shows its phase screen as the loading state (REQ-DCLEAN-017.A1)", startScanEntersScanningAtZero);
  it("results follow scan completion with no pause (REQ-DCLEAN-017.A3)", completionEntersResultsWithoutAPause);
});

describe("disk state after a clean (REQ-DCLEAN-002.A4)", () => {
  it("ring, goal tag and sidebar meter follow the post-clean disk", cleanUpdatesRingTagAndSidebar);
});
