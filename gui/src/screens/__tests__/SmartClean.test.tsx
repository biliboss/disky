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
        safety: "safe",
        bytes: 4_200_000_000,
        items: [{ name: "x", path: "/x", bytes: 4_200_000_000 }],
      },
      {
        id: "engine_cat_b",
        name: "Engine label B",
        description: "Engine description B",
        safety: "review",
        bytes: 600_000_000,
        items: [{ name: "y", path: "/y", bytes: 600_000_000 }],
      },
    ],
    catalogue: [
      { id: "engine_cat_a", name: "Engine label A", safety: "safe" },
      { id: "engine_cat_b", name: "Engine label B", safety: "review" },
      { id: "engine_cat_c", name: "Engine label C", safety: "review" },
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

async function rendersExactlyTheEngineCategories() {
  await startScanAndFinish(engineView());
  const rows = await screen.findAllByRole("row");
  expect(rows.map((r) => r.getAttribute("data-category-id"))).toEqual(["engine_cat_a", "engine_cat_b"]);
  expect(rows[0]).toHaveTextContent("Engine label A");
  expect(rows[0]).toHaveTextContent("Engine description A");
  expect(rows[0]).toHaveTextContent("Safe");
  expect(rows[0]).toHaveTextContent("4.2 GB");
  expect(rows[1]).toHaveTextContent("Review");
  expect(rows[1]).toHaveTextContent("600 MB");
  expect(rows[1].lastElementChild).toHaveClass("num");
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
  expect(await screen.findByText("Nothing to clean right now.")).toBeInTheDocument();
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

describe("Smart Clean consumes the engine view", () => {
  it("renders exactly the engine's categories (REQ-DCLEAN-027.A4)", rendersExactlyTheEngineCategories);
  it("malformed data renders the error state, not a partial list (REQ-DCLEAN-029.A4)", malformedDataShowsTheErrorStateNotAPartialList);
  it("a stale snapshot warns before cleaning (REQ-DCLEAN-029.A2)", staleSnapshotWarnsBeforeCleaning);
  it("an empty view says nothing to clean (REQ-DCLEAN-027)", emptyViewShowsNothingToClean);
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
