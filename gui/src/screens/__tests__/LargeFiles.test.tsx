import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => undefined }));

import { HashRouter } from "@solidjs/router";
import { LargeFiles } from "../LargeFiles";
import { diskState, resetDiskState, setFromView } from "../../lib/diskState";
import { parseCleanupView } from "../../lib/contract";
import { EMPTY, NOTE_NONE, NO_SCANS, kicker, openedLabel, selectionNote, trashLabel } from "../largeFiles/largeCopy";

const GB = 1e9;
const DAY = 86_400_000;
const ago = (days: number) => new Date(Date.now() - days * DAY).toISOString();

/** The snapshot's LARGE rows, as `disky large` lists them (largest first). */
const ROWS: [string, string, number, number, boolean][] = [
  ["Windows 11 VM.utm", "~/Library/Containers/UTM", 38.2, 245, false],
  ["Photo library backup.zip", "~/Documents/Backups", 22.6, 1100, true],
  ["Wedding 4K master.mov", "~/Movies/Exports", 18.4, 800, true],
  ["GoPro dump 2023", "~/Pictures", 14.9, 800, true],
  ["dataset-train.parquet", "~/Developer/ml", 6.8, 14, false],
  ["ubuntu-22.04-desktop.iso", "~/Downloads", 4.7, 400, true],
  ["Xcode_15.dmg", "~/Downloads", 3.2, 400, true],
  ["podcast-raw-ep42.wav", "~/Music/Recordings", 2.1, 155, false],
];

function file([name, location, gb, days, old]: (typeof ROWS)[number]) {
  return {
    name,
    path: `/Users/me/${location.slice(2)}/${name}`,
    location,
    bytes: Math.round(gb * GB),
    last_opened: ago(days),
    unused_1y: old,
  };
}

function largeView(filter: string) {
  const all = ROWS.map(file);
  const records = all.filter((f) => filter === "all" || (filter === "over-10gb" ? f.bytes >= 10 * GB : f.unused_1y));
  return { filter, count: records.length, total_bytes: records.reduce((a, f) => a + f.bytes, 0), records };
}

const DISK_AFTER = { volume: "Macintosh HD", capacity_bytes: 512 * GB, free_bytes: 130 * GB };

function fakeEngine(opts: { snapshot?: boolean; empty?: boolean; fail?: string[] } = {}) {
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "load_large") {
      if (opts.snapshot === false) return null;
      return opts.empty ? { filter: args.filter, count: 0, total_bytes: 0, records: [] } : largeView(args.filter);
    }
    if (cmd === "trash_large") {
      const fail = opts.fail ?? [];
      const all = ROWS.map(file);
      const handled = args.paths.filter((p: string) => !fail.includes(p));
      return {
        handled,
        failed: args.paths.filter((p: string) => fail.includes(p)).map((path: string) => ({ path, reason: "move to Trash failed" })),
        freed_physical_bytes: all.filter((f) => handled.includes(f.path)).reduce((a, f) => a + f.bytes, 0),
        history_id: handled.length ? "e1" : null,
        disk: DISK_AFTER,
      };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
}

function renderLarge() {
  render(() => (
    <HashRouter>{[{ path: "/", component: () => <p data-testid="smart-clean" /> }, { path: "/large-files", component: LargeFiles }]}</HashRouter>
  ));
  window.location.hash = "#/large-files";
  return screen.findByTestId("large-files");
}

const rows = (s: HTMLElement) => within(s).getAllByTestId("large-row");
const names = (s: HTMLElement) => rows(s).map((r) => r.children[1].firstChild?.textContent);
const trashButton = (s: HTMLElement) => within(s).getByRole("button", { name: /^Move .* to Trash$/ });

async function listsTheEnginesRowsWithHeaderAndColumns() {
  fakeEngine();
  const s = await renderLarge();
  await within(s).findByText("Windows 11 VM.utm");
  expect(within(s).getByTestId("large-kicker")).toHaveTextContent("8 files · 111 GB");
  expect(within(s).getByRole("heading", { level: 1 })).toHaveTextContent("Large & old files");
  expect(names(s)).toEqual(ROWS.map((r) => r[0]));
  const headers = within(s).getAllByRole("columnheader").map((h) => h.textContent);
  expect(headers).toEqual(["Select", "Name", "Location", "Last opened", "Size"]);
  const first = rows(s)[0];
  expect(within(first).getByText("~/Library/Containers/UTM")).toBeInTheDocument();
  expect(within(first).getByText("38.2 GB")).toBeInTheDocument();
  expect(within(first).getByTestId("large-opened")).toHaveTextContent("8 months ago");
  expect(within(first).getByTestId("large-opened")).toHaveClass("tag", "tag-neutral");
  expect(within(rows(s)[1]).getByTestId("large-opened")).toHaveClass("tag", "tag-accent");
  expect(within(rows(s)[1]).getByTestId("large-opened")).toHaveTextContent("3 years ago");
  expect(invoke).toHaveBeenCalledWith("load_large", { filter: "all" });
}

async function filtersReQueryTheEngine() {
  fakeEngine();
  const s = await renderLarge();
  await within(s).findByText("Windows 11 VM.utm");
  const group = within(s).getByRole("radiogroup", { name: "Show" });
  const opts = within(group).getAllByRole("radio");
  expect(opts.map((o) => o.closest("label")?.textContent)).toEqual(["All", "Over 10 GB", "Unused 1 year+"]);
  expect(opts[0]).toBeChecked();
  fireEvent.click(within(group).getByLabelText("Over 10 GB"));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("load_large", { filter: "over-10gb" }));
  await waitFor(() => expect(rows(s)).toHaveLength(4));
  expect(within(s).getByTestId("large-kicker")).toHaveTextContent("4 files · 94.1 GB");
  fireEvent.click(within(group).getByLabelText("Unused 1 year+"));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("load_large", { filter: "unused-1y" }));
  await waitFor(() => expect(rows(s)).toHaveLength(5));
  expect(rows(s).every((r) => within(r).getByTestId("large-opened").classList.contains("tag-accent"))).toBe(true);
}

async function rowOrCheckTogglesSelectionAndFooter() {
  fakeEngine();
  const s = await renderLarge();
  await within(s).findByText("Windows 11 VM.utm");
  expect(within(s).getByTestId("large-note")).toHaveTextContent(NOTE_NONE);
  expect(trashButton(s)).toBeDisabled();
  fireEvent.click(rows(s)[0]);
  const check0 = within(rows(s)[0]).getByRole("checkbox", { name: "Windows 11 VM.utm" });
  expect(check0).toHaveAttribute("aria-checked", "true");
  fireEvent.click(within(rows(s)[2]).getByRole("checkbox"));
  expect(within(s).getByTestId("large-note")).toHaveTextContent("2 selected · restorable from History for 30 days");
  expect(trashButton(s)).toBeEnabled();
  expect(trashButton(s)).toHaveTextContent("Move 56.6 GB to Trash");
  // The check toggles once, not twice (no bubbling to the row).
  fireEvent.click(check0);
  expect(check0).toHaveAttribute("aria-checked", "false");
  expect(within(s).getByTestId("large-note")).toHaveTextContent("1 selected");
}

async function trashSendsExactlyTheSelectionAndRemovesHandledRows() {
  const failing = file(ROWS[2]).path;
  fakeEngine({ fail: [failing] });
  resetDiskState();
  setFromView(parseCleanupView({
    disk: { volume: "Macintosh HD", capacity_bytes: 512 * GB, free_bytes: 74 * GB },
    goal_bytes: 80 * GB, last_scan_at: null, snapshot_taken_at: null, is_stale: true,
    preferences: { trash_by_default: false, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80 * GB, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] },
    large_files_count: 8, skipped_paths: 0, categories: [], catalogue: [],
  }));
  const s = await renderLarge();
  await within(s).findByText("Windows 11 VM.utm");
  fireEvent.click(rows(s)[0]);
  fireEvent.click(rows(s)[2]);
  fireEvent.click(trashButton(s));
  await waitFor(() => expect(rows(s)).toHaveLength(7));
  // trash_by_default is off, yet only the Trash command exists for this screen.
  expect(invoke).toHaveBeenCalledWith("trash_large", { paths: [file(ROWS[0]).path, failing] });
  expect(names(s)).not.toContain("Windows 11 VM.utm");
  expect(names(s)).toContain("Wedding 4K master.mov");
  const err = within(s).getByText("Couldn't move to Trash: move to Trash failed");
  expect(err).toHaveClass("large-row-error");
  expect(diskState()?.disk.free_bytes).toBe(130 * GB);
  expect(within(s).getByTestId("large-kicker")).toHaveTextContent("7 files · 72.7 GB");
  expect(within(s).getByTestId("large-note")).toHaveTextContent("1 selected");
}

async function emptyListAndNoSnapshotStates() {
  fakeEngine({ empty: true });
  const s = await renderLarge();
  expect(await within(s).findByText(EMPTY)).toBeInTheDocument();
  cleanup();
  fakeEngine({ snapshot: false });
  await renderLarge();
  await waitFor(() => expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(NO_SCANS));
  fireEvent.click(screen.getByRole("button", { name: "Start scan" }));
  expect(await screen.findByTestId("smart-clean")).toBeInTheDocument();
}

function copyIsWordedFromEngineFigures() {
  expect(kicker([{ bytes: 2.5 * GB }])).toBe("1 file · 2.5 GB");
  expect(selectionNote(0)).toBe(NOTE_NONE);
  expect(selectionNote(3)).toBe("3 selected · restorable from History for 30 days");
  expect(trashLabel(18.4 * GB)).toBe("Move 18.4 GB to Trash");
  expect(openedLabel(null)).toBe("Unknown");
  expect(openedLabel(ago(800))).toBe("2 years ago");
}

beforeEach(() => {
  invoke.mockReset();
  window.location.hash = "";
});
afterEach(cleanup);

describe("Large files screen (REQ-DSCR-013)", () => {
  it("lists the engine's rows under the header and columns (A1, A2)", listsTheEnginesRowsWithHeaderAndColumns);
  it("the filter radio group re-queries the engine (A1, A2)", filtersReQueryTheEngine);
  it("row or check toggles selection; footer follows (A3, A4)", rowOrCheckTogglesSelectionAndFooter);
  it("Move to Trash sends exactly the selection; handled rows vanish, failures stay (A4)", trashSendsExactlyTheSelectionAndRemovesHandledRows);
  it("empty list and no snapshot (A5)", emptyListAndNoSnapshotStates);
  it("copy words engine figures (A1–A4)", copyIsWordedFromEngineFigures);
});
