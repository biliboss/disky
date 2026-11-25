import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => undefined }));

import { HashRouter } from "@solidjs/router";
import { Duplicates } from "../Duplicates";
import { diskState, resetDiskState, setFromView } from "../../lib/diskState";
import { parseCleanupView } from "../../lib/contract";
import { EMPTY, NO_SCANS, eachLabel, extraCopies, footNote, freedBytes, kicker, removeLabel } from "../duplicates/dupesCopy";

const GB = 1e9;
const MB = 1e6;

/** The snapshot's DUPES, as `disky dupes` reports them: name, bytes per copy, copy folders. */
const GROUPS: [string, number, string[]][] = [
  ["IMG_2041.HEIC", 6 * MB, ["~/Pictures/Photos Library", "~/Desktop/to sort", "~/Downloads"]],
  ["Brand guidelines v3.pdf", 184 * MB, ["~/Documents/Work", "~/Downloads", "~/Desktop"]],
  ["Wedding 4K master.mov", 18.4 * GB, ["~/Movies/Exports", "~/Documents/Backups/Video"]],
  ["Lecture 07.mp4", 1.3 * GB, ["~/Documents/School", "~/Downloads"]],
];

const pathOf = (location: string, name: string) => `/Users/me/${location.slice(2)}/${name}`;

function dupesView(groups = GROUPS) {
  const records = groups.map(([name, bytes, locs]) => ({
    name,
    bytes,
    copies: locs.map((location) => ({ path: pathOf(location, name), location, mtime: null })),
  }));
  const extra = records.reduce((a, g) => a + g.copies.length - 1, 0);
  return {
    min_size: MB,
    groups: records.length,
    extra_copies: extra,
    reclaimable_bytes: records.reduce((a, g) => a + g.bytes * (g.copies.length - 1), 0),
    records,
  };
}

const DISK_AFTER = { volume: "Macintosh HD", capacity_bytes: 512 * GB, free_bytes: 130 * GB };

function fakeEngine(opts: { snapshot?: boolean; empty?: boolean; fail?: string[]; refuse?: boolean; pending?: boolean } = {}) {
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "load_dupes") {
      if (opts.pending) return new Promise(() => undefined);
      if (opts.snapshot === false) return null;
      return opts.empty ? dupesView([]) : dupesView();
    }
    if (cmd === "trash_dupes") {
      if (opts.refuse) {
        throw { code: "generic", title: "every copy selected", detail: "every copy of a.mov is selected; keep at least one. Nothing was moved", retryable: false };
      }
      const fail = opts.fail ?? [];
      const handled = args.paths.filter((p: string) => !fail.includes(p));
      return {
        handled,
        failed: args.paths.filter((p: string) => fail.includes(p)).map((path: string) => ({ path, reason: "move to Trash failed" })),
        freed_physical_bytes: 0,
        history_id: handled.length ? "d1" : null,
        disk: DISK_AFTER,
      };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
}

function renderDupes() {
  render(() => (
    <HashRouter>{[{ path: "/", component: () => <p data-testid="smart-clean" /> }, { path: "/duplicates", component: Duplicates }]}</HashRouter>
  ));
  window.location.hash = "#/duplicates";
  return screen.findByTestId("duplicates");
}

const cards = (s: HTMLElement) => within(s).getAllByTestId("dupe-group");
const radios = (card: HTMLElement) => within(card).getAllByRole("radio");
const checked = (card: HTMLElement) => radios(card).map((r) => r.getAttribute("aria-checked"));
const removeButton = (s: HTMLElement) => within(s).getByRole("button", { name: /^Remove \d+ extra cop(y|ies)$/ });

async function listsGroupsAsCardsWithKickerAndEachTag() {
  fakeEngine();
  const s = await renderDupes();
  await within(s).findByText("IMG_2041.HEIC");
  expect(within(s).getByRole("heading", { level: 1 })).toHaveTextContent("Duplicates");
  expect(within(s).getByTestId("dupes-kicker")).toHaveTextContent("4 groups · one copy of each is kept, you choose which");
  expect(cards(s).map((c) => c.querySelector(".card-title")?.textContent)).toEqual(GROUPS.map((g) => g[0]));
  const each = cards(s).map((c) => within(c).getByTestId("dupe-each"));
  expect(each.map((t) => t.textContent)).toEqual(["6 MB each", "184 MB each", "18.4 GB each", "1.3 GB each"]);
  expect(each[0]).toHaveClass("tag", "tag-neutral");
  const first = radios(cards(s)[0]);
  expect(first.map((r) => r.querySelector(".dupe-path")?.textContent)).toEqual(GROUPS[0][2]);
  expect(first[0]).toHaveAttribute("title", pathOf("~/Pictures/Photos Library", "IMG_2041.HEIC"));
  expect(within(s).getByTestId("dupes-note")).toHaveTextContent("Frees 20.1 GB · removed copies go to the Trash");
  expect(removeButton(s)).toHaveTextContent("Remove 6 extra copies");
  expect(invoke).toHaveBeenCalledWith("load_dupes", undefined);
}

async function firstCopyIsKeptAndClickingChoosesAnother() {
  fakeEngine();
  const s = await renderDupes();
  await within(s).findByText("IMG_2041.HEIC");
  const card = cards(s)[0];
  const group = within(card).getByRole("radiogroup", { name: "IMG_2041.HEIC" });
  expect(group).toBeInTheDocument();
  expect(checked(card)).toEqual(["true", "false", "false"]);
  const tags = () => radios(card).map((r) => r.querySelector(".tag"));
  expect(tags()[0]).toHaveTextContent("Keep");
  expect(tags()[0]).toHaveClass("tag-accent-2");
  expect(tags()[1]).toHaveTextContent("Remove");
  expect(tags()[1]).toHaveClass("tag-outline");
  expect(radios(card).map((r) => r.tabIndex)).toEqual([0, -1, -1]);
  fireEvent.click(radios(card)[2]);
  expect(checked(card)).toEqual(["false", "false", "true"]);
  expect(tags()[2]).toHaveTextContent("Keep");
  expect(radios(card).map((r) => r.tabIndex)).toEqual([-1, -1, 0]);
  // Exactly one kept per group: the other groups are untouched.
  expect(checked(cards(s)[1])).toEqual(["true", "false", "false"]);
}

async function arrowKeysMoveTheKeptCopyWithinAGroup() {
  fakeEngine();
  const s = await renderDupes();
  await within(s).findByText("IMG_2041.HEIC");
  const card = cards(s)[0];
  radios(card)[0].focus();
  fireEvent.keyDown(radios(card)[0], { key: "ArrowDown" });
  expect(checked(card)).toEqual(["false", "true", "false"]);
  expect(document.activeElement).toBe(radios(card)[1]);
  fireEvent.keyDown(radios(card)[1], { key: "ArrowRight" });
  expect(checked(card)).toEqual(["false", "false", "true"]);
  fireEvent.keyDown(radios(card)[2], { key: "ArrowDown" });
  expect(checked(card)).toEqual(["true", "false", "false"]); // wraps to the first
  fireEvent.keyDown(radios(card)[0], { key: "ArrowUp" });
  expect(checked(card)).toEqual(["false", "false", "true"]); // wraps to the last
  expect(document.activeElement).toBe(radios(card)[2]);
}

async function removeTrashesEveryNonKeptCopyThenHandledVanish() {
  const failing = pathOf("~/Downloads", "Brand guidelines v3.pdf");
  fakeEngine({ fail: [failing] });
  resetDiskState();
  setFromView(parseCleanupView({
    disk: { volume: "Macintosh HD", capacity_bytes: 512 * GB, free_bytes: 74 * GB },
    goal_bytes: 80 * GB, last_scan_at: null, snapshot_taken_at: null, is_stale: true,
    preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80 * GB, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] },
    large_files_count: 0, skipped_paths: 0, categories: [], catalogue: [],
  }));
  const s = await renderDupes();
  await within(s).findByText("IMG_2041.HEIC");
  fireEvent.click(radios(cards(s)[0])[1]); // keep "~/Desktop/to sort"
  fireEvent.click(removeButton(s));
  await waitFor(() => expect(cards(s)).toHaveLength(1));
  expect(invoke).toHaveBeenCalledWith("trash_dupes", {
    paths: [
      pathOf("~/Pictures/Photos Library", "IMG_2041.HEIC"),
      pathOf("~/Downloads", "IMG_2041.HEIC"),
      pathOf("~/Downloads", "Brand guidelines v3.pdf"),
      pathOf("~/Desktop", "Brand guidelines v3.pdf"),
      pathOf("~/Documents/Backups/Video", "Wedding 4K master.mov"),
      pathOf("~/Downloads", "Lecture 07.mp4"),
    ],
  });
  // Groups left with one copy vanished; the failed copy keeps its group alive.
  const left = cards(s)[0];
  expect(left.querySelector(".card-title")).toHaveTextContent("Brand guidelines v3.pdf");
  expect(radios(left).map((r) => r.querySelector(".dupe-path")?.textContent)).toEqual(["~/Documents/Work", "~/Downloads"]);
  const err = within(left).getByRole("alert");
  expect(err).toHaveTextContent("Couldn't move ~/Downloads to Trash: move to Trash failed");
  expect(err).toHaveClass("dupe-copy-error");
  expect(diskState()?.disk.free_bytes).toBe(130 * GB);
  expect(within(s).getByTestId("dupes-kicker")).toHaveTextContent("1 group · one copy");
  expect(removeButton(s)).toHaveTextContent("Remove 1 extra copy");
  expect(within(s).getByTestId("dupes-note")).toHaveTextContent("Frees 184 MB");
}

async function aRefusalIsShownAndNothingVanishes() {
  fakeEngine({ refuse: true });
  const s = await renderDupes();
  await within(s).findByText("IMG_2041.HEIC");
  fireEvent.click(removeButton(s));
  expect(await within(s).findByRole("alert")).toHaveTextContent("keep at least one");
  expect(cards(s)).toHaveLength(4);
}

async function emptyNoSnapshotAndBusyStates() {
  fakeEngine({ pending: true });
  const s = await renderDupes();
  expect(within(s).queryByTestId("busy")).toBeNull();
  expect(await within(s).findByTestId("busy")).toHaveAttribute("aria-busy", "true");
  cleanup();
  fakeEngine({ empty: true });
  const e = await renderDupes();
  expect(await within(e).findByText(EMPTY)).toBeInTheDocument();
  expect(removeButton(e)).toBeDisabled();
  cleanup();
  fakeEngine({ snapshot: false });
  await renderDupes();
  await waitFor(() => expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(NO_SCANS));
  fireEvent.click(screen.getByRole("button", { name: "Start scan" }));
  expect(await screen.findByTestId("smart-clean")).toBeInTheDocument();
}

function copyIsWordedFromEngineFigures() {
  expect(kicker(1)).toBe("1 group · one copy of each is kept, you choose which");
  expect(eachLabel(18.4 * GB)).toBe("18.4 GB each");
  expect(footNote(20.08 * GB)).toBe("Frees 20.1 GB · removed copies go to the Trash");
  expect(removeLabel(6)).toBe("Remove 6 extra copies");
  const groups = [{ bytes: 5, copies: [{ path: "/a", location: "~", mtime: null }, { path: "/b", location: "~", mtime: null }] }];
  const extras = extraCopies(groups, () => "/b");
  expect(extras).toEqual([{ path: "/a", bytes: 5 }]);
  expect(freedBytes(extras)).toBe(5);
}

beforeEach(() => {
  invoke.mockReset();
  window.location.hash = "";
});
afterEach(cleanup);

describe("Duplicates screen (REQ-DSCR-014)", () => {
  it("lists groups as cards under the kicker, with '<size> each' tags (A1, A2)", listsGroupsAsCardsWithKickerAndEachTag);
  it("the first copy is kept; clicking another keeps it instead (A2, A3)", firstCopyIsKeptAndClickingChoosesAnother);
  it("arrow keys move the kept copy within a group (A3)", arrowKeysMoveTheKeptCopyWithinAGroup);
  it("Remove trashes every non-kept copy; handled copies and one-copy groups vanish (A4)", removeTrashesEveryNonKeptCopyThenHandledVanish);
  it("a refusal is shown and nothing vanishes (REQ-DSCR-006.A3)", aRefusalIsShownAndNothingVanishes);
  it("busy, empty and no-snapshot states (A5)", emptyNoSnapshotAndBusyStates);
  it("copy words engine figures (A1, A2, A4)", copyIsWordedFromEngineFigures);
});
