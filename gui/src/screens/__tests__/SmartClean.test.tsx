import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
// The shell loads the Assistant view for its badge (REQ-DSCR-009.A5); these
// tests are about Smart Clean, so that call stays out of `invoke`'s record.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) =>
    args[0] === "load_assistant" ? Promise.reject(new Error("no Assistant here")) : invoke(...args),
}));
const progressListeners: ((ev: { payload: unknown }) => void)[] = [];
/** Only the progress channels these tests drive are recorded. */
const PROGRESS_CHANNELS = ["scan://progress", "clean://progress"];
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (event: string, cb: (ev: { payload: unknown }) => void) => {
    if (!PROGRESS_CHANNELS.includes(event)) return () => undefined;
    progressListeners.push(cb);
    return () => progressListeners.splice(progressListeners.indexOf(cb), 1);
  },
}));

import { HashRouter } from "@solidjs/router";
import { App } from "../../App";
import { SmartClean } from "../SmartClean";
import { clean } from "../../lib/engine";
import { resetDiskState } from "../../lib/diskState";
import { parseCleanupView } from "../../lib/contract";
import { Results } from "../smartClean/Results";
import { INITIAL_STATE } from "../smartClean/phase";
import { resetSmartCleanStore } from "../smartClean/store";

const DAY = 86_400_000;

function engineView(overrides: Record<string, unknown> = {}) {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: null,
    snapshot_taken_at: null,
    is_stale: false,
    preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] },
    large_files_count: 3,
    skipped_paths: 0,
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
  resetSmartCleanStore();
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
  // The goal comes from the engine's preferences store (REQ-DSCR-001.A5), never from the GUI.
  expect(invoke).toHaveBeenCalledWith("load_cleanup_view", { snapshot: null });
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
    itemsView({ preferences: { trash_by_default: true, skip_recently_used: true, safe_only: false, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] } }),
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
    itemsView({ preferences: { trash_by_default: false, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] } }),
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
      engineView({ preferences: { trash_by_default: false, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] } }),
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
  // The real Space map screen (REQ-DSCR-012).
  expect(await within(main).findByTestId("space-map")).toBeInTheDocument();
  expect(within(main).queryByText("Coming soon")).toBeNull();
}

async function shortcutsOpenTheirScreens() {
  engine({ load_cleanup_view: () => engineView() });
  renderApp();
  fireEvent.click(within(await idle()).getByTestId("shortcut-large"));
  // The real Large files screen (REQ-DSCR-013).
  expect(await within(screen.getByRole("main")).findByTestId("large-files")).toBeInTheDocument();
  expect(within(screen.getByRole("main")).queryByText("Coming soon")).toBeNull();
}

async function cleanUpdatesRingTagAndSidebar() {
  engine({
    load_cleanup_view: () => engineView(),
    clean: () => ({
      mode: "trash",
      freed_bytes: 16e9,
      succeeded: ["/x"],
      failed: [],
      history_id: null,
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

const TRASH_OFF = { preferences: { trash_by_default: false, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] } };

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
  // REQ-DCLEAN-008: "Clean now" runs exactly one clean, and nothing before it did.
  expect(invoke.mock.calls.map(([cmd]) => cmd)).toEqual(["load_cleanup_view", "scan", "clean"]);
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

// ── Cleaning, done, undo and errors (REQ-DCLEAN-008, -009, -018) ──

const CLEAN_DISK = { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 78e9 };

function cleanResultFixture(overrides: Record<string, unknown> = {}) {
  return {
    mode: "trash",
    freed_bytes: 3_000_000_000,
    succeeded: ["/a/one", LONG_PATH],
    failed: [],
    history_id: "h1",
    history_entry: null,
    disk: CLEAN_DISK,
    ...overrides,
  };
}

/** Scan to results with extra engine handlers (clean, restore, …). */
async function resultsWith(scanned: unknown, handlers: Record<string, (args: any) => unknown> = {}) {
  let finish!: (v: unknown) => void;
  engine({
    load_cleanup_view: () => engineView(),
    scan: () => new Promise((r) => (finish = r)),
    ...handlers,
  });
  renderApp();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Start scan" }));
  await screen.findByTestId("scanning");
  finish(scanned);
  return screen.findByTestId("results");
}

/** A clean that waits until `finish` is called; records its arguments. */
function deferredClean() {
  const d = { args: null as any, finish: (_v: unknown) => undefined as void };
  const handler = (args: any) => {
    d.args = args;
    return new Promise((r) => (d.finish = r));
  };
  return { d, handler };
}

function emitClean(category_id: string, done: number, total: number) {
  for (const cb of [...progressListeners]) cb({ payload: { category_id, done_items: done, total_items: total, fraction: done / total } });
}

/** Select both categories, open the dialog, pick `mode`, and Clean now. */
async function cleanBoth(mode: "Move to Trash" | "Delete permanently" = "Move to Trash") {
  fireEvent.click(within(resultRows()[1]).getByRole("checkbox", { name: "Engine label B" }));
  const dialog = openDialog();
  fireEvent.click(modeRadio(mode));
  fireEvent.click(within(dialog).getByRole("button", { name: "Clean now" }));
  return screen.findByTestId("cleaning");
}

async function cleaningShowsHeadingStepAndBar() {
  const { d, handler } = deferredClean();
  await resultsWith(itemsView(), { clean: handler });
  const cleaning = await cleanBoth();
  expect(within(cleaning).getByRole("heading", { level: 1 })).toHaveTextContent("Tidying up…");
  // The first chosen category shows before any event arrives.
  expect(within(cleaning).getByTestId("clean-step")).toHaveTextContent(/^Moving Engine label A…$/);
  await waitFor(() => expect(progressListeners.length).toBeGreaterThan(0));
  emitClean("engine_cat_a", 1, 3);
  expect(within(cleaning).getByTestId("clean-step")).toHaveTextContent(/^Moving Engine label A…$/);
  emitClean("engine_cat_b", 2, 3);
  expect(within(cleaning).getByTestId("clean-step")).toHaveTextContent(/^Moving Engine label B…$/);
  expect(within(cleaning).getByTestId("clean-progress")).toHaveTextContent("67%");
  const bar = within(cleaning).getByRole("progressbar", { name: "Clean progress" });
  expect(bar).toHaveAttribute("aria-valuenow", "67");
  expect(within(cleaning).getByTestId("clean-bar-fill")).toHaveClass("clean-bar-fill");
  // A4: no cancel or stop while cleaning.
  expect(within(cleaning).queryAllByRole("button")).toEqual([]);
  // One clean, with the chosen categories' item paths in list order.
  expect(cleanCalls()).toHaveLength(1);
  expect(d.args).toEqual({ itemPaths: ["/a/one", LONG_PATH, "/y"], mode: "trash", openedBy: null });
}

async function permanentCleanSaysDeleting() {
  const { handler } = deferredClean();
  await resultsWith(itemsView(), { clean: handler });
  const cleaning = await cleanBoth("Delete permanently");
  expect(within(cleaning).getByTestId("clean-step")).toHaveTextContent(/^Deleting Engine label A…$/);
  await waitFor(() => expect(progressListeners.length).toBeGreaterThan(0));
  emitClean("engine_cat_b", 3, 3);
  expect(within(cleaning).getByTestId("clean-step")).toHaveTextContent(/^Deleting Engine label B…$/);
  expect(within(cleaning).getByTestId("clean-progress")).toHaveTextContent("99%");
}

function cleanFillUsesAccent2() {
  const css = readFileSync(resolve(__dirname, "../smartClean/smartClean.css"), "utf8");
  expect(css).toMatch(/\.clean-bar-fill\s*\{[^}]*background:\s*var\(--color-accent-2\);/);
  expect(css).toMatch(/\.scan-bar-fill\s*\{[^}]*background:\s*var\(--color-accent\);/);
}

async function finishCleanToDone(result: unknown, handlers: Record<string, (args: any) => unknown> = {}, mode?: "Delete permanently") {
  await resultsWith(itemsView(), { clean: () => result, ...handlers });
  await cleanBoth(mode);
  return screen.findByTestId("done");
}

async function doneReportsReclaimedBytesNoteAndActions() {
  // Estimate was 4.8 GB; the engine reclaimed 3 GB — that is what shows (A1).
  const done = await finishCleanToDone(cleanResultFixture());
  expect(within(done).getByText("Done")).toHaveClass("tag", "tag-accent-2");
  expect(within(done).getByTestId("done-heading")).toHaveTextContent(/^3 GB cleared$/);
  expect(within(done).getByTestId("done-note")).toHaveTextContent(
    "Everything went to the Trash. Changed your mind? Undo now or restore any time in the next 30 days.",
  );
  expect(within(done).getByRole("button", { name: "Back to home" })).toHaveClass("btn-primary");
  expect(within(done).getByRole("button", { name: "Undo" })).toHaveClass("btn-secondary");
  expect(within(done).getByRole("button", { name: "View history" })).toHaveClass("btn-ghost");
  expect(within(done).getByTestId("ring-total")).toHaveTextContent("now free");
  expect(within(done).getByTestId("ring-free")).toHaveTextContent("78 GB");
  expect(within(done).getByTestId("ring-fill")).toHaveClass("ring-fill--done");
  expect(within(done).queryByTestId("done-failed")).toBeNull();
}

async function permanentDoneHasNoUndo() {
  const done = await finishCleanToDone(cleanResultFixture({ mode: "delete" }), {}, "Delete permanently");
  expect(within(done).getByTestId("done-note")).toHaveTextContent(/^Those files were deleted permanently\.$/);
  expect(within(done).queryByRole("button", { name: "Undo" })).toBeNull();
}

async function backHomeAndViewHistoryNavigate() {
  let done = await finishCleanToDone(cleanResultFixture());
  fireEvent.click(within(done).getByRole("button", { name: "Back to home" }));
  expect(await idle()).toBeInTheDocument();
  cleanup();
  window.location.hash = "";
  done = await finishCleanToDone(cleanResultFixture());
  fireEvent.click(within(done).getByRole("button", { name: "View history" }));
  const main = screen.getByRole("main");
  expect(await within(main).findByRole("heading", { level: 1, name: "History" })).toBeInTheDocument();
}

async function undoRestoresUpdatesTheMeterAndReportsMissing() {
  let restoreArgs: unknown = null;
  const done = await finishCleanToDone(cleanResultFixture(), {
    restore: (args) => {
      restoreArgs = args;
      return {
        report: { id: "h1", restored: ["/a/one"], conflicts: [], missing: [LONG_PATH] },
        disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 75e9 },
      };
    },
  });
  expect(screen.getByTestId("disk-meter")).toHaveTextContent("78 GB free");
  fireEvent.click(within(done).getByRole("button", { name: "Undo" }));
  const home = await idle();
  expect(restoreArgs).toEqual({ historyId: "h1" });
  expect(screen.getByTestId("disk-meter")).toHaveTextContent("75 GB free");
  expect(within(home).getByTestId("ring-free")).toHaveTextContent("75 GB");
  expect(screen.getByTestId("undo-notices")).toHaveTextContent(`${LONG_PATH} is no longer in Trash`);
}

async function undoShowsBusyOnlyPastTheThreshold() {
  const done = await finishCleanToDone(cleanResultFixture(), { restore: () => new Promise(() => undefined) });
  const undoBtn = within(done).getByTestId("undo");
  vi.useFakeTimers();
  try {
    fireEvent.click(undoBtn);
    expect(undoBtn).toBeDisabled();
    vi.advanceTimersByTime(199);
    expect(undoBtn).toHaveTextContent(/^Undo$/);
    vi.advanceTimersByTime(1);
    expect(undoBtn).toHaveTextContent("Restoring…");
    expect(undoBtn).toHaveAttribute("aria-busy", "true");
  } finally {
    vi.useRealTimers();
  }
}

async function failedUndoStaysOnDoneWithTheCause() {
  const done = await finishCleanToDone(cleanResultFixture(), {
    restore: () => Promise.reject({ code: "generic", title: "already restored", detail: "entry h1 already restored at x", retryable: false }),
  });
  fireEvent.click(within(done).getByRole("button", { name: "Undo" }));
  expect(await within(done).findByTestId("undo-error")).toHaveTextContent("entry h1 already restored at x");
  expect(within(done).getByTestId("undo-error")).toHaveClass("error-state");
}

async function partialCleanListsFailedPathsAndReclaimedBytes() {
  const done = await finishCleanToDone(
    cleanResultFixture({
      freed_bytes: 1_100_000_000,
      succeeded: ["/a/one"],
      failed: [
        { path: LONG_PATH, reason: "delete failed" },
        { path: "/y", reason: "missing" },
      ],
    }),
  );
  expect(within(done).getByTestId("done-heading")).toHaveTextContent(/^1.1 GB cleared$/);
  const failed = within(done).getByTestId("done-failed");
  expect(failed).toHaveAttribute("role", "alert");
  expect(failed).toHaveClass("error-state");
  expect(failed).toHaveTextContent("2 items couldn't be cleaned");
  const rows = within(failed).getAllByRole("listitem");
  expect(rows.map((r) => r.getAttribute("title"))).toEqual([LONG_PATH, "/y"]);
  expect(rows[0]).toHaveTextContent("couldn't be deleted");
}

async function totalFailureReturnsToResultsWithTheSelection() {
  await resultsWith(itemsView(), {
    clean: () =>
      cleanResultFixture({
        freed_bytes: 0,
        succeeded: [],
        failed: [
          { path: "/a/one", reason: "delete failed" },
          { path: LONG_PATH, reason: "delete failed" },
          { path: "/y", reason: "delete failed" },
        ],
        history_id: null,
      }),
  });
  await cleanBoth();
  const results = await screen.findByTestId("results");
  expect(screen.queryByTestId("done")).toBeNull();
  const err = within(results).getByTestId("clean-error");
  expect(err).toHaveAttribute("role", "alert");
  expect(err).toHaveClass("error-state");
  expect(err).toHaveTextContent("Nothing was cleaned");
  expect(checkedIds()).toEqual(["engine_cat_a", "engine_cat_b"]);
}

async function aRejectedCleanAlsoReturnsToResults() {
  await resultsWith(itemsView(), {
    clean: () => Promise.reject({ code: "io", title: "i/o error", detail: "disk on fire", retryable: true }),
  });
  await cleanBoth();
  const results = await screen.findByTestId("results");
  expect(within(results).getByTestId("clean-error")).toHaveTextContent("Nothing was cleaned: disk on fire");
  expect(checkedIds()).toEqual(["engine_cat_a", "engine_cat_b"]);
}

async function lockHeldScanNamesTheCauseWithTryAgain() {
  let scans = 0;
  engine({
    load_cleanup_view: () => engineView(),
    scan: () => {
      scans++;
      return Promise.reject({ code: "lock-held", title: "snapshot locked", detail: "Could not set lock", retryable: true });
    },
  });
  renderApp();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Start scan" }));
  const err = await screen.findByTestId("scan-error");
  expect(err).toHaveAttribute("role", "alert");
  expect(err).toHaveClass("error-state");
  expect(err).toHaveTextContent(/^Another disky process is using the snapshot\. Try again in a moment\.Try again$/);
  fireEvent.click(within(err).getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(scans).toBe(2));
}

async function otherScanErrorsKeepTheEngineDetail() {
  engine({
    load_cleanup_view: () => engineView(),
    scan: () => Promise.reject({ code: "generic", title: "error", detail: "walker exploded", retryable: false }),
  });
  renderApp();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Start scan" }));
  const err = await screen.findByTestId("scan-error");
  expect(err).toHaveTextContent("The scan didn't finish: walker exploded");
  expect(within(err).queryByRole("button", { name: "Try again" })).toBeNull();
}

async function skippedPathsAreCountedWithAFullDiskAccessLink() {
  let opened = 0;
  const results = await resultsWith(itemsView({ skipped_paths: 3 }), {
    open_full_disk_access: () => {
      opened++;
      return null;
    },
  });
  const note = within(results).getByTestId("skipped-paths");
  expect(note).toHaveTextContent("3 paths skipped");
  // Results for the readable paths are still shown.
  expect(resultRows()).toHaveLength(2);
  fireEvent.click(within(note).getByRole("button", { name: "Open Full Disk Access" }));
  await waitFor(() => expect(opened).toBe(1));
}

async function noSkippedNoteWhenNothingWasSkipped() {
  const results = await resultsWith(itemsView());
  expect(within(results).queryByTestId("skipped-paths")).toBeNull();
}

function errorStatesUseTheDangerToken() {
  const css = readFileSync(resolve(__dirname, "../smartClean/smartClean.css"), "utf8");
  expect(css).toMatch(/\.error-state\s*\{[^}]*color:\s*var\(--color-danger\);/);
}

describe("Smart Clean cleaning phase (REQ-DCLEAN-008)", () => {
  it("shows the heading, the step label in selection order and the bar, with no cancel (REQ-DCLEAN-008.A1, A4)", cleaningShowsHeadingStepAndBar);
  it("a permanent clean reads Deleting (REQ-DCLEAN-008.A3)", permanentCleanSaysDeleting);
  it("the cleaning fill uses accent-2 (REQ-DCLEAN-008.A2)", cleanFillUsesAccent2);
});

describe("Smart Clean done phase (REQ-DCLEAN-009)", () => {
  it("reports the reclaimed bytes, the Trash note and the three actions (REQ-DCLEAN-009.A1, A2)", doneReportsReclaimedBytesNoteAndActions);
  it("a permanent clean has its note and no Undo (REQ-DCLEAN-009.A2, A3)", permanentDoneHasNoUndo);
  it("Back to home returns to idle and View history opens History (REQ-DCLEAN-009.A6)", backHomeAndViewHistoryNavigate);
  it("Undo restores, updates the meter, reports missing items and returns to idle (REQ-DCLEAN-009.A4, A7)", undoRestoresUpdatesTheMeterAndReportsMissing);
  it("Undo shows busy only past the spinner threshold (REQ-DCLEAN-017.A2)", undoShowsBusyOnlyPastTheThreshold);
  it("a failed Undo stays on done and says why (REQ-DCLEAN-009.A7)", failedUndoStaysOnDoneWithTheCause);
});

describe("Smart Clean error states (REQ-DCLEAN-018)", () => {
  it("a lock-held scan names the cause with Try again (REQ-DCLEAN-018.A1)", lockHeldScanNamesTheCauseWithTryAgain);
  it("other scan errors keep the engine's detail (REQ-DCLEAN-018)", otherScanErrorsKeepTheEngineDetail);
  it("skipped paths are counted with a Full Disk Access link (REQ-DCLEAN-018.A2)", skippedPathsAreCountedWithAFullDiskAccessLink);
  it("no skipped note when nothing was skipped (REQ-DCLEAN-018.A2)", noSkippedNoteWhenNothingWasSkipped);
  it("a partial clean lists failed paths and only reclaimed bytes (REQ-DCLEAN-018.A3)", partialCleanListsFailedPathsAndReclaimedBytes);
  it("a total failure returns to results with the selection (REQ-DCLEAN-018.A4)", totalFailureReturnsToResultsWithTheSelection);
  it("a rejected clean also returns to results with the selection (REQ-DCLEAN-018.A4)", aRejectedCleanAlsoReturnsToResults);
  it("error states use --color-danger (REQ-DCLEAN-018.A5)", errorStatesUseTheDangerToken);
});

// ── Below-goal notification (REQ-DCLEAN-010) ─────────────────────

/** An idle view below its goal (74 of 80 GB) with a scan: 4.2 GB Safe, 600 MB Review. */
function belowGoalView(overrides: Record<string, unknown> = {}) {
  return engineView({ last_scan_at: new Date(Date.now() - 2 * DAY).toISOString(), ...overrides });
}

const NOTIFICATION_TEXT =
  "You’re down to 74 GB free, under your 80 GB goal. I can clear 4.2 GB of safe junk once you confirm.";

function notification() {
  return screen.queryByTestId("goal-notification");
}

/** Smart Clean alone under a router, with an explicit `showNotification`. */
function renderSmartClean(showNotification: boolean) {
  render(() => <HashRouter>{[{ path: "/", component: () => <SmartClean showNotification={showNotification} /> }]}</HashRouter>);
}

async function notificationShowsOnIdleBelowGoal() {
  engine({ load_cleanup_view: () => belowGoalView() });
  renderApp();
  await idle();
  const n = notification()!;
  expect(n).toHaveAttribute("role", "status");
  expect(n).toHaveAttribute("aria-live", "polite");
  expect(within(n).getByTestId("goal-notification-text")).toHaveTextContent(new RegExp(`^${NOTIFICATION_TEXT}$`));
  // Snapshot order: Later (secondary), then Review cleanup (primary).
  const buttons = within(n).getAllByRole("button");
  expect(buttons.map((b) => b.textContent)).toEqual(["Later", "Review cleanup"]);
  expect(buttons[0]).toHaveClass("btn", "btn-secondary");
  expect(buttons[1]).toHaveClass("btn", "btn-primary");
}

async function notificationHiddenUnlessEveryConditionHolds() {
  const cases: [string, () => void][] = [
    ["showNotification={false}", () => {
      engine({ load_cleanup_view: () => belowGoalView() });
      renderSmartClean(false);
    }],
    ["notifications preference off", () => {
      engine({ load_cleanup_view: () => belowGoalView({ preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: false, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] } }) });
      renderApp();
    }],
    ["assistant off", () => {
      engine({ load_cleanup_view: () => belowGoalView({ preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: false, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] } }) });
      renderApp();
    }],
    ["free space at the goal", () => {
      engine({ load_cleanup_view: () => belowGoalView({ goal_bytes: 74e9 }) });
      renderApp();
    }],
    ["free space above the goal", () => {
      engine({ load_cleanup_view: () => belowGoalView({ goal_bytes: 50e9 }) });
      renderApp();
    }],
    ["no scan", () => {
      engine({ load_cleanup_view: () => belowGoalView({ last_scan_at: null }) });
      renderApp();
    }],
    ["no Safe bytes", () => {
      engine({ load_cleanup_view: () => belowGoalView({ categories: [engineView().categories[1]] }) });
      renderApp();
    }],
  ];
  for (const [name, setup] of cases) {
    setup();
    await idle();
    expect(notification(), name).toBeNull();
    cleanup();
    resetSmartCleanStore();
    resetDiskState();
    window.location.hash = "";
  }
  // The prop wins over the preference: on with the preference on renders it.
  engine({ load_cleanup_view: () => belowGoalView() });
  renderSmartClean(true);
  await idle();
  expect(notification()).not.toBeNull();
}

async function laterHidesItForTheSession() {
  engine({ load_cleanup_view: () => belowGoalView() });
  renderApp();
  await idle();
  fireEvent.click(within(notification()!).getByRole("button", { name: "Later" }));
  expect(notification()).toBeNull();
  // Away to another screen and back: still hidden this session.
  fireEvent.click(screen.getByRole("button", { name: /^History/ }));
  await screen.findByTestId("history");
  fireEvent.click(screen.getByRole("button", { name: /^Smart Clean/ }));
  await idle();
  expect(notification()).toBeNull();
}

async function reviewCleanupOpensResultsWithSafeSelectedAndTheDialog() {
  const { d, handler } = deferredClean();
  engine({ load_cleanup_view: () => belowGoalView(), clean: handler });
  renderApp();
  await idle();
  fireEvent.click(within(notification()!).getByRole("button", { name: "Review cleanup" }));
  expect(await screen.findByTestId("results")).toBeInTheDocument();
  expect(checkedIds()).toEqual(["engine_cat_a"]);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("heading")).toHaveTextContent("4.2 GB");
  expect(cleanCalls()).toHaveLength(0);
  expect(notification()).toBeNull();
  fireEvent.click(within(dialog).getByRole("button", { name: "Clean now" }));
  await screen.findByTestId("cleaning");
  expect(cleanCalls()).toHaveLength(1);
  expect(d.args).toEqual({ itemPaths: ["/x"], mode: "trash", openedBy: null });
}

async function notificationDoesNotTakeFocus() {
  engine({ load_cleanup_view: () => belowGoalView() });
  renderApp();
  const before = document.activeElement;
  await idle();
  await waitFor(() => expect(notification()).not.toBeNull());
  expect(document.activeElement).toBe(before);
  const n = notification()!;
  expect(n).not.toHaveAttribute("tabindex");
  expect(n.querySelector("[autofocus]")).toBeNull();
}

async function aScanDismissesIt() {
  let finish!: (v: unknown) => void;
  engine({ load_cleanup_view: () => belowGoalView(), scan: () => new Promise((r) => (finish = r)) });
  renderApp();
  await idle();
  expect(notification()).not.toBeNull();
  fireEvent.click(within(await idle()).getByRole("button", { name: "Start scan" }));
  await screen.findByTestId("scanning");
  finish(belowGoalView());
  const results = await screen.findByTestId("results");
  fireEvent.click(within(results).getByRole("button", { name: "Cancel" }));
  await idle();
  expect(notification()).toBeNull();
}

describe("below-goal notification (REQ-DCLEAN-010)", () => {
  it("shows on idle below the goal with the snapshot's text and actions (REQ-DCLEAN-010.A1)", notificationShowsOnIdleBelowGoal);
  it("is hidden unless every condition holds (REQ-DCLEAN-010.A1, A4, A6)", notificationHiddenUnlessEveryConditionHolds);
  it("Later hides it for the rest of the session (REQ-DCLEAN-010.A2)", laterHidesItForTheSession);
  it("Review cleanup opens results with Safe selected and the dialog, cleaning nothing (REQ-DCLEAN-010.A3)", reviewCleanupOpensResultsWithSafeSelectedAndTheDialog);
  it("does not take focus when it appears (REQ-DCLEAN-010.A5)", notificationDoesNotTakeFocus);
  it("a scan dismisses it (REQ-DCLEAN-010)", aScanDismissesIt);
});

// ── Navigation blocking (REQ-DCLEAN-011) ─────────────────────────

function navEntries() {
  return within(screen.getByRole("navigation", { name: "Main" })).getAllByRole("button");
}

/** Click and key-activate a nav entry the way a browser does (Enter/Space → click). */
function activate(el: HTMLElement) {
  fireEvent.click(el);
  fireEvent.keyDown(el, { key: "Enter" });
  fireEvent.click(el);
  fireEvent.keyDown(el, { key: " " });
  fireEvent.keyUp(el, { key: " " });
  fireEvent.click(el);
}

async function navIsBlockedAndDisabledWhileScanning() {
  await startOpenScan();
  const hash = window.location.hash;
  for (const entry of navEntries()) expect(entry).toHaveAttribute("aria-disabled", "true");
  activate(screen.getByRole("button", { name: /^History/ }));
  activate(screen.getByRole("button", { name: /^Smart Clean/ }));
  expect(window.location.hash).toBe(hash);
  expect(screen.getByTestId("scanning")).toBeInTheDocument();
  expect(screen.queryByText("Coming soon")).toBeNull();
}

async function navIsBlockedAndDisabledWhileCleaning() {
  const { handler } = deferredClean();
  await resultsWith(itemsView(), { clean: handler });
  for (const entry of navEntries()) expect(entry).not.toHaveAttribute("aria-disabled");
  await cleanBoth();
  const hash = window.location.hash;
  for (const entry of navEntries()) expect(entry).toHaveAttribute("aria-disabled", "true");
  activate(screen.getByRole("button", { name: /^Space Map/ }));
  expect(window.location.hash).toBe(hash);
  expect(screen.getByTestId("cleaning")).toBeInTheDocument();
}

function disabledAndCurrentEntriesUseTheirTokens() {
  const css = readFileSync(resolve(__dirname, "../../shell/shell.css"), "utf8");
  expect(css).toMatch(/\.nav-item\[aria-disabled="true"\]\s*\{[^}]*opacity:\s*var\(--opacity-disabled\);[^}]*cursor:\s*not-allowed;/);
  // The label on the accent fill is --color-on-accent (ink), which reaches 4.5:1 (REQ-DCLEAN-024).
  expect(css).toMatch(/\.nav-item\[aria-current="page"\]\s*\{[^}]*color:\s*var\(--color-on-accent\);[^}]*background:\s*var\(--color-accent\);/);
  const tokens = readFileSync(resolve(__dirname, "../../styles/gui-tokens.css"), "utf8");
  expect(tokens).toMatch(/--opacity-disabled:\s*0\.45;/);
  expect(tokens).toMatch(/--color-on-accent:\s*var\(--color-text\);/);
}

async function currentEntryIsExposedAsTheCurrentPage() {
  engine({ load_cleanup_view: () => engineView() });
  renderApp();
  await idle();
  expect(screen.getByRole("button", { name: /^Smart Clean/ })).toHaveAttribute("aria-current", "page");
  fireEvent.click(screen.getByRole("button", { name: /^History/ }));
  await screen.findByTestId("history");
  expect(screen.getByRole("button", { name: /^History/ })).toHaveAttribute("aria-current", "page");
  expect(screen.getByRole("button", { name: /^Smart Clean/ })).not.toHaveAttribute("aria-current");
}

async function navigatingFromDoneShowsIdle() {
  await finishCleanToDone(cleanResultFixture());
  for (const entry of navEntries()) expect(entry).not.toHaveAttribute("aria-disabled");
  fireEvent.click(screen.getByRole("button", { name: /^Smart Clean/ }));
  expect(await idle()).toBeInTheDocument();
  expect(screen.queryByTestId("done")).toBeNull();
  cleanup();
  resetSmartCleanStore();
  window.location.hash = "";
  await finishCleanToDone(cleanResultFixture());
  fireEvent.click(screen.getByRole("button", { name: /^History/ }));
  await screen.findByTestId("history");
  fireEvent.click(screen.getByRole("button", { name: /^Smart Clean/ }));
  expect(await idle()).toBeInTheDocument();
  expect(screen.queryByTestId("done")).toBeNull();
}

describe("navigation during Smart Clean (REQ-DCLEAN-011)", () => {
  it("is blocked and aria-disabled while scanning (REQ-DCLEAN-011.A1, A2)", navIsBlockedAndDisabledWhileScanning);
  it("is blocked and aria-disabled while cleaning (REQ-DCLEAN-011.A1, A2)", navIsBlockedAndDisabledWhileCleaning);
  it("blocked and current entries use their tokens (REQ-DCLEAN-011.A2, A3)", disabledAndCurrentEntriesUseTheirTokens);
  it("the current entry is exposed as the current page (REQ-DCLEAN-011.A3)", currentEntryIsExposedAsTheCurrentPage);
  it("leaving done via navigation resets Smart Clean to idle (REQ-DCLEAN-011.A4)", navigatingFromDoneShowsIdle);
});
