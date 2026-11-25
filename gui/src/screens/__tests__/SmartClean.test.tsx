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
  progressListeners[0]({ payload: { fraction: 0.42, current_path: "/Users", completed_category_ids: [] } });
  expect(await screen.findByText("42%")).toBeInTheDocument();
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

describe("disk state after a clean (REQ-DCLEAN-002.A4)", () => {
  it("ring, goal tag and sidebar meter follow the post-clean disk", cleanUpdatesRingTagAndSidebar);
});
