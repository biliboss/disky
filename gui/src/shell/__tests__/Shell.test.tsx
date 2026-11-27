import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { App, ROUTES, SCREENS } from "../../App";
import { NAV_ITEMS, type NavId } from "../nav";
import { resetDiskState, setFromClean, setFromView } from "../../lib/diskState";
import { goalPos } from "../../lib/meter";
import type { CleanResult, CleanupView } from "../../lib/contract";

// vitest runs with gui/ as its root.
const guiFile = (rel: string) => resolve(process.cwd(), rel);

/** Every nav entry's real screen, by the screen's test id (REQ-DSCR-017.A3). */
const REAL_SCREENS: Record<NavId, string> = {
  clean: "idle",
  assistant: "assistant",
  map: "space-map",
  large: "large-files",
  dupes: "duplicates",
  history: "history",
  settings: "settings",
};

function renderApp() {
  render(() => <App />);
}

function navButton(label: string) {
  return screen.getByRole("button", { name: new RegExp(`^${label}`) });
}

function brandAndTitleBarReadDisky() {
  renderApp();
  expect(screen.getByTestId("titlebar")).toHaveTextContent(/^disky$/);
  expect(document.querySelector(".brand-word")).toHaveTextContent(/^disky$/);
  expect(screen.getByRole("img", { name: "disky logo" })).toBeInTheDocument();
  expect(document.body.innerHTML).not.toMatch(/diskee/i);
}

function windowTitleIsDisky() {
  const html = readFileSync(guiFile("index.html"), "utf8");
  expect(html).toMatch(/<title>disky<\/title>/);
  const conf = JSON.parse(readFileSync(guiFile("src-tauri/tauri.conf.json"), "utf8"));
  expect(conf.productName).toBe("disky");
  expect(conf.app.windows[0].title).toBe("disky");
  expect(JSON.stringify(conf)).not.toMatch(/diskee/i);
}

async function everyNavRouteRendersARealScreen() {
  // Each entry has its own screen in the route table; none is a placeholder.
  expect(Object.keys(SCREENS).sort()).toEqual(NAV_ITEMS.map((i) => i.id).sort());
  expect(ROUTES.map((r) => r.path)).toEqual(NAV_ITEMS.map((i) => i.route));
  renderApp();
  for (const item of NAV_ITEMS.slice(1)) {
    fireEvent.click(navButton(item.label));
    const main = screen.getByRole("main");
    // Router navigation commits in a transition, so wait for the new screen.
    expect(await within(main).findByTestId(REAL_SCREENS[item.id], {}, { timeout: 3000 })).toBeInTheDocument();
    expect(within(main).queryByText("Coming soon")).toBeNull();
    await waitFor(() => expect(navButton(item.label)).toHaveAttribute("aria-current", "page"));
  }
  fireEvent.click(navButton("Smart Clean"));
  await waitFor(() => expect(navButton("Smart Clean")).toHaveAttribute("aria-current", "page"));
  expect(within(screen.getByRole("main")).queryByText("Coming soon")).toBeNull();
}

function sidebarShowsSevenEntriesAndAssistantBadge() {
  renderApp();
  const nav = screen.getByRole("navigation", { name: "Main" });
  expect(within(nav).getAllByRole("button")).toHaveLength(7);
  expect(navButton("Assistant")).toHaveTextContent("Assistant4");
  expect(navButton("Smart Clean")).toHaveAttribute("aria-current", "page");
}

function meterShowsFixtureFreeSpaceAndGoal() {
  renderApp();
  const meter = screen.getByTestId("disk-meter");
  expect(meter).toHaveTextContent("Macintosh HD");
  expect(meter).toHaveTextContent("74 GB free");
  expect(meter).toHaveTextContent("Goal: keep 80 GB free");
  expect(screen.getByTestId("goal-marker").style.left).toBe(
    `calc(${(432 / 512) * 100}% - var(--meter-marker-nudge))`,
  );
}

const GB = 1e9;

function liveView(disk: CleanupView["disk"], goal: number): CleanupView {
  return {
    disk,
    goal_bytes: goal,
    last_scan_at: null,
    snapshot_taken_at: null,
    is_stale: false,
    preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] },
    large_files_count: 0,
    skipped_paths: 0,
    categories: [],
    catalogue: [],
  };
}

function meterShowsLiveVolumeAndFreeSpace() {
  setFromView(liveView({ volume: "Data", capacity_bytes: 1000 * GB, free_bytes: 250 * GB }, 100 * GB));
  renderApp();
  const meter = screen.getByTestId("disk-meter");
  expect(meter).toHaveTextContent("Data");
  expect(meter).toHaveTextContent("250 GB free");
  expect(meter).toHaveTextContent("Goal: keep 100 GB free");
  expect(meter).not.toHaveTextContent("Macintosh HD");
}

function goalMarkerFollowsLiveGoalPos() {
  setFromView(liveView({ volume: "Data", capacity_bytes: 1000 * GB, free_bytes: 250 * GB }, 100 * GB));
  renderApp();
  expect(goalPos(1000 * GB, 100 * GB)).toBeCloseTo(0.9, 10);
  expect(screen.getByTestId("goal-marker").style.left).toBe(
    `calc(${goalPos(1000 * GB, 100 * GB) * 100}% - var(--meter-marker-nudge))`,
  );
}

function meterFollowsACleanWithoutRemount() {
  setFromView(liveView({ volume: "Data", capacity_bytes: 1000 * GB, free_bytes: 250 * GB }, 100 * GB));
  renderApp();
  const meter = screen.getByTestId("disk-meter");
  const result: CleanResult = {
    mode: "trash",
    freed_bytes: 50 * GB,
    succeeded: ["/x"],
    failed: [],
    history_id: null,
    history_entry: null,
    disk: { volume: "Data", capacity_bytes: 1000 * GB, free_bytes: 300 * GB },
  };
  setFromClean(result);
  expect(screen.getByTestId("disk-meter")).toBe(meter); // same node: no remount
  expect(meter).toHaveTextContent("300 GB free");
  expect(meter).toHaveTextContent("Goal: keep 100 GB free"); // goal kept
}

describe("app shell", () => {
  beforeEach(() => {
    window.location.hash = "";
    resetDiskState();
  });
  afterEach(cleanup);

  it("brand and title bar read disky, never Diskee (REQ-DCLEAN-032.A2)", brandAndTitleBarReadDisky);
  it("window title is disky (REQ-DCLEAN-032.A2)", windowTitleIsDisky);
  it("every sidebar entry renders its real screen, no placeholder (REQ-DSCR-017.A3)", everyNavRouteRendersARealScreen);
  it("sidebar shows seven entries and the assistant badge (REQ-DCLEAN-032)", sidebarShowsSevenEntriesAndAssistantBadge);
  it("meter shows fixture free space and goal (REQ-DCLEAN-032)", meterShowsFixtureFreeSpaceAndGoal);
  it("meter shows the live volume and free space (REQ-DCLEAN-002.A1)", meterShowsLiveVolumeAndFreeSpace);
  it("goal marker sits at the live goal position (REQ-DCLEAN-002.A3)", goalMarkerFollowsLiveGoalPos);
  it("meter follows a clean without a remount (REQ-DCLEAN-002.A4)", meterFollowsACleanWithoutRemount);
});
