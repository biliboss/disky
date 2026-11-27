import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => undefined }));

import { HashRouter } from "@solidjs/router";
import { History } from "../History";
import { diskState, resetDiskState, setFromView } from "../../lib/diskState";
import { EMPTY, headerLine, rowMeta, TAGS } from "../history/historyCopy";

const DAY = 86_400_000;
const ago = (d: number) => new Date(Date.now() - d * DAY - 60_000).toISOString();

/** A History row as the engine's `contract::HistoryRow` serialises it. */
function row(id: string, title: string, days: number, detail: string, bytes: number, state: string, mode = "trash") {
  return { id, title, ts: ago(days), mode, categories: [], bytes, items: [], restored_at: null, detail, state };
}

/** The snapshot's three rows, newest first, plus a delete-mode entry. */
function snapshotRows() {
  return [
    row("e1", "Smart Clean", 7, "App caches, Temporary files, Trash", 6_800_000_000, "restorable"),
    row("e2", "Large files", 22, "a.iso, b.dmg, c.zip", 11_200_000_000, "restorable"),
    row("e3", "Smart Clean", 49, "System caches, Logs", 3_900_000_000, "emptied"),
  ];
}

/** The engine: `list_history` reads `log`, `restore` marks the entry as the engine would. */
function fakeEngine(rows: ReturnType<typeof row>[], report: { missing?: string[]; conflicts?: string[] } = {}) {
  const log = { rows: structuredClone(rows) };
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "list_history") {
      return { total_bytes: log.rows.reduce((a, r) => a + r.bytes, 0), rows: structuredClone(log.rows) };
    }
    if (cmd === "restore") {
      const missing = report.missing ?? [];
      const conflicts = report.conflicts ?? [];
      if (!missing.length && !conflicts.length) {
        const r = log.rows.find((x) => x.id === args.historyId)!;
        r.state = "restored";
      }
      return {
        report: { id: args.historyId, restored: ["/Users/me/a"], conflicts, missing },
        disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 81e9 },
      };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
  return log;
}

function renderHistory() {
  render(() => <HashRouter>{[{ path: "/", component: History }]}</HashRouter>);
  return screen.findByTestId("history");
}

const rowsOf = async (s: HTMLElement) => within(s).findAllByTestId("history-row");

async function rendersOneRowPerEntryNewestFirst() {
  fakeEngine(snapshotRows());
  const s = await renderHistory();
  const rows = await rowsOf(s);
  expect(rows).toHaveLength(3);
  expect(rows.map((r) => r.querySelector(".history-name")!.textContent)).toEqual(["Smart Clean", "Large files", "Smart Clean"]);
  expect(within(s).getByRole("heading", { level: 1 })).toHaveTextContent(/^History$/);
  expect(within(s).getByText(headerLine(21_900_000_000))).toBeInTheDocument();
  expect(headerLine(21_900_000_000)).toBe("21.9 GB cleared · items stay restorable for 30 days");
  expect(invoke).toHaveBeenCalledWith("list_history", undefined);
}

async function rowShowsTitleWhenDetailAndSize() {
  fakeEngine(snapshotRows());
  const s = await renderHistory();
  const [first] = await rowsOf(s);
  expect(first).toHaveTextContent("Smart Clean");
  expect(within(first).getByText("7 days ago · App caches, Temporary files, Trash")).toBeInTheDocument();
  expect(within(first).getByText("6.8 GB")).toHaveClass("num");
  expect(first.querySelector("svg")).not.toBeNull();
  expect(rowMeta(new Date(Date.now() - 1000).toISOString(), "x")).toBe("Just now · x");
}

async function restorableRowRestoresAndShowsRestoredTag() {
  fakeEngine(snapshotRows());
  setFromView({ disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 }, goal_bytes: 80e9 } as never);
  const s = await renderHistory();
  const [first] = await rowsOf(s);
  const btn = within(first).getByRole("button", { name: /^Restore Smart Clean/ });
  expect(btn).toHaveClass("btn", "btn-secondary");
  expect(btn).toHaveTextContent(/^Restore$/);
  fireEvent.click(btn);
  await waitFor(() => expect(within(s).getAllByTestId("history-row")[0]).toHaveTextContent(TAGS.restored));
  const tag = within(within(s).getAllByTestId("history-row")[0]).getByText("Restored");
  expect(tag).toHaveClass("tag", "tag-accent-2");
  expect(within(within(s).getAllByTestId("history-row")[0]).queryByRole("button")).toBeNull();
  expect(invoke).toHaveBeenCalledWith("restore", { historyId: "e1" });
  expect(diskState()!.disk.free_bytes).toBe(81e9);
}

async function missingItemsAreReported() {
  fakeEngine(snapshotRows(), { missing: ["/Users/me/Downloads/gone.iso"] });
  setFromView({ disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 }, goal_bytes: 80e9 } as never);
  const s = await renderHistory();
  const [, second] = await rowsOf(s);
  fireEvent.click(within(second).getByRole("button", { name: /^Restore Large files/ }));
  const notices = await within(s).findByTestId("restore-notices");
  expect(notices).toHaveTextContent("/Users/me/Downloads/gone.iso is no longer in Trash, so it couldn't be restored.");
  // Partial: the engine retries the rest, so the row keeps Restore.
  await waitFor(() =>
    expect(within(within(s).getAllByTestId("history-row")[1]).getByRole("button", { name: /^Restore/ })).toBeEnabled(),
  );
}

async function emptiedAndDeletedShowTagsNoRestore() {
  fakeEngine([
    row("e3", "Smart Clean", 49, "System caches, Logs", 3_900_000_000, "emptied"),
    row("e4", "Cleanup", 2, "Rust target", 1_000_000_000, "deleted", "delete"),
  ]);
  const s = await renderHistory();
  const [emptied, deleted] = await rowsOf(s);
  expect(within(emptied).getByText("Emptied")).toHaveClass("tag", "tag-neutral");
  expect(within(deleted).getByText("Deleted")).toHaveClass("tag", "tag-neutral");
  expect(within(s).queryByRole("button")).toBeNull();
}

async function emptyLogReadsNothingCleanedYet() {
  fakeEngine([]);
  const s = await renderHistory();
  expect(await within(s).findByText(EMPTY)).toBeInTheDocument();
  expect(EMPTY).toBe("Nothing cleaned yet.");
  expect(within(s).queryAllByTestId("history-row")).toHaveLength(0);
}

async function aFailedRestoreIsReported() {
  fakeEngine(snapshotRows());
  const base = invoke.getMockImplementation()!;
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "restore") throw { code: "generic", title: "already restored", detail: "entry e1 already restored at x", retryable: false };
    return base(cmd, args);
  });
  const s = await renderHistory();
  const [first] = await rowsOf(s);
  fireEvent.click(within(first).getByRole("button", { name: /^Restore/ }));
  expect(await within(s).findByRole("alert")).toHaveTextContent("entry e1 already restored at x");
}

beforeEach(() => {
  invoke.mockReset();
  resetDiskState();
  window.location.hash = "";
});
afterEach(cleanup);

describe("History screen (REQ-DSCR-015)", () => {
  it("renders one row per entry, newest first, under the total (A1)", rendersOneRowPerEntryNewestFirst);
  it("each row shows title, when · detail and size (A2)", rowShowsTitleWhenDetailAndSize);
  it("Restore restores, refreshes the meter and shows Restored (A3)", restorableRowRestoresAndShowsRestoredTag);
  it("items no longer in the Trash are reported (A3, REQ-DCLEAN-034.A4)", missingItemsAreReported);
  it("emptied and deleted entries show tags and no Restore (A4)", emptiedAndDeletedShowTagsNoRestore);
  it("an empty log reads Nothing cleaned yet. (A5)", emptyLogReadsNothingCleanedYet);
  it("a failed restore is reported", aFailedRestoreIsReported);
});

