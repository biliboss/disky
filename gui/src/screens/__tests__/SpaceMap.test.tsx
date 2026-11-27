import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => undefined }));

import { HashRouter } from "@solidjs/router";
import { SpaceMap } from "../SpaceMap";
import { HINT, NO_SCANS } from "../spaceMap/mapCopy";

const GB = 1e9;

/** The snapshot's MAP, as `disky map` answers it per folder. */
const FOLDERS: Record<string, { name: string; total: number; kids: [string, number, boolean][] }> = {
  "/": {
    name: "/",
    total: 438 * GB,
    kids: [["Users", 212, true], ["Applications", 92, true], ["System", 58, false], ["Library", 34, false], ["Containers", 26, false], ["Other", 16, false]],
  },
  "/Users/me": {
    name: "me",
    total: 300 * GB,
    kids: [["Library", 120, true], ["dev", 90, true]],
  },
  "/Users": {
    name: "Users",
    total: 212 * GB,
    kids: [["Library", 74, false], ["Documents", 48, false], ["Movies", 41, false], ["Developer", 31, false], ["Pictures", 12, false], ["Music", 6, false]],
  },
};

function mapView(path: string) {
  const f = FOLDERS[path];
  if (!f) throw { code: "not-found", title: "folder not in snapshot", detail: `${path} is not a folder in this snapshot`, retryable: false };
  return {
    path,
    name: f.name,
    root: "/",
    total_bytes: f.total,
    capacity_bytes: 512 * GB,
    records: f.kids.map(([name, gb, has]) => ({ name, path: `${path === "/" ? "" : path}/${name}`, bytes: gb * GB, has_children: has })),
    // The engine folded 214 small children of /Users/me into one block.
    other: path === "/Users/me" ? { count: 214, bytes: 90 * GB } : null,
  };
}

function fakeEngine(snapshot = true) {
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "load_map") return snapshot ? mapView(args.path ?? "/") : null;
    if (cmd === "reveal_in_finder") return null;
    throw new Error(`unexpected command ${cmd}`);
  });
}

function renderMap() {
  render(() => <HashRouter>{[{ path: "/", component: () => <p data-testid="smart-clean" /> }, { path: "/space-map", component: SpaceMap }]}</HashRouter>);
  window.location.hash = "#/space-map";
  return screen.findByTestId("space-map");
}

const tiles = (s: HTMLElement) => within(s).getAllByRole("button", { name: /, \d+(\.\d)? [GM]B$/ });
const card = (s: HTMLElement) => within(s).getByTestId("map-selected");

async function rendersTilesLargestSelectedOnEntry() {
  fakeEngine();
  const s = await renderMap();
  await within(s).findByRole("button", { name: "Users, 212 GB" });
  expect(tiles(s).map((t) => t.getAttribute("aria-label"))).toEqual([
    "Users, 212 GB", "Applications, 92 GB", "System, 58 GB", "Library, 34 GB", "Containers, 26 GB", "Other, 16 GB",
  ]);
  expect(tiles(s)[0]).toHaveAttribute("aria-pressed", "true");
  expect(tiles(s)[0].style.backgroundColor).toBe("var(--tile-0)");
  expect(tiles(s)[1].style.backgroundColor).toBe("var(--tile-1)");
  expect(within(s).getByText("Macintosh HD")).toBeInTheDocument();
  expect(within(s).getByRole("heading", { level: 1 })).toHaveTextContent("Macintosh HD · 438 GB");
  expect(within(s).queryByRole("button", { name: "Up" })).toBeNull();
  expect(within(card(s)).getByText("Users")).toBeInTheDocument();
  expect(within(card(s)).getByText("/Users")).toBeInTheDocument();
  expect(within(card(s)).getByText("41.4%")).toBeInTheDocument();
  expect(within(s).getByText(HINT)).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith("load_map", { path: null });
}

async function clickSelectsAndOpenFolderOnlyWithChildren() {
  fakeEngine();
  const s = await renderMap();
  fireEvent.click(await within(s).findByRole("button", { name: "System, 58 GB" }));
  expect(within(s).getByRole("button", { name: "System, 58 GB" })).toHaveAttribute("aria-pressed", "true");
  expect(tiles(s)[0]).toHaveAttribute("aria-pressed", "false");
  expect(within(card(s)).getByText("/System")).toBeInTheDocument();
  expect(within(card(s)).getByText("11.3%")).toBeInTheDocument();
  expect(within(card(s)).queryByRole("button", { name: "Open folder" })).toBeNull();
  fireEvent.click(within(s).getByRole("button", { name: "Users, 212 GB" }));
  expect(within(card(s)).getByRole("button", { name: "Open folder" })).toHaveClass("btn", "btn-primary");
}

async function openFolderDrillsInAndUpGoesBack() {
  fakeEngine();
  const s = await renderMap();
  fireEvent.click(await within(s).findByRole("button", { name: "Open folder" }));
  await within(s).findByRole("button", { name: "Library, 74 GB" });
  expect(invoke).toHaveBeenLastCalledWith("load_map", { path: "/Users" });
  expect(s.querySelector(".map-crumb")!.textContent).toBe("Macintosh HD  /  Users");
  expect(within(s).getByRole("heading", { level: 1 })).toHaveTextContent("Users · 212 GB");
  expect(tiles(s)[0]).toHaveAttribute("aria-pressed", "true");
  fireEvent.click(within(s).getByRole("button", { name: "Up" }));
  await within(s).findByRole("button", { name: "Users, 212 GB" });
  expect(within(s).queryByRole("button", { name: "Up" })).toBeNull();
  // Double-click drills in too.
  fireEvent.dblClick(within(s).getByRole("button", { name: "Users, 212 GB" }));
  await within(s).findByRole("button", { name: "Documents, 48 GB" });
}

async function enterOnTheSelectedTileOpensIt() {
  fakeEngine();
  const s = await renderMap();
  const apps = await within(s).findByRole("button", { name: "Applications, 92 GB" });
  // Enter on a tile that isn't selected leaves it to the native click (select).
  fireEvent.keyDown(apps, { key: "Enter" });
  expect(invoke).toHaveBeenCalledTimes(1);
  fireEvent.keyDown(within(s).getByRole("button", { name: "Users, 212 GB" }), { key: "Enter" });
  await within(s).findByRole("button", { name: "Library, 74 GB" });
  // A file never opens.
  fireEvent.click(within(s).getByRole("button", { name: "Library, 74 GB" }));
  fireEvent.keyDown(within(s).getByRole("button", { name: "Library, 74 GB" }), { key: "Enter" });
  expect(invoke).toHaveBeenCalledTimes(2);
}

async function revealAsksTheBackendForTheSelectedPath() {
  fakeEngine();
  const s = await renderMap();
  await within(s).findByRole("button", { name: "Users, 212 GB" });
  const reveal = within(card(s)).getByRole("button", { name: "Reveal in Finder" });
  expect(reveal).toHaveClass("btn", "btn-secondary");
  fireEvent.click(reveal);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("reveal_in_finder", { path: "/Users" }));
}

async function foldedFolderEndsWithAnOtherBlock() {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "load_map") return mapView("/Users/me");
    throw new Error(`unexpected command ${cmd}`);
  });
  const s = await renderMap();
  const other = await within(s).findByRole("button", { name: "Other, 214 smaller items, 90 GB" });
  expect(other).toHaveClass("map-tile-other");
  // The engine's remainder is the last block, after the kept children.
  const all = within(s).getAllByRole("button").filter((b) => b.classList.contains("map-tile"));
  expect(all.at(-1)).toBe(other);
  fireEvent.click(other);
  expect(within(card(s)).getByText("Other")).toBeInTheDocument();
  expect(within(card(s)).getByText("214 smaller items")).toBeInTheDocument();
  // Not a folder on disk: no Open folder, no Reveal in Finder.
  expect(within(card(s)).queryByRole("button", { name: "Open folder" })).toBeNull();
  expect(within(card(s)).queryByRole("button", { name: "Reveal in Finder" })).toBeNull();
  fireEvent.dblClick(other);
  expect(invoke).toHaveBeenCalledTimes(1);
}

async function noSnapshotOffersStartScan() {
  fakeEngine(false);
  await renderMap();
  await waitFor(() => expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(NO_SCANS));
  fireEvent.click(screen.getByRole("button", { name: "Start scan" }));
  expect(await screen.findByTestId("smart-clean")).toBeInTheDocument();
}

beforeEach(() => {
  invoke.mockReset();
  window.location.hash = "";
});
afterEach(cleanup);

describe("Space map screen (REQ-DSCR-012)", () => {
  it("renders the folder's tiles with the largest selected (A1–A3)", rendersTilesLargestSelectedOnEntry);
  it("click selects; Open folder only with children (A3, A4)", clickSelectsAndOpenFolderOnlyWithChildren);
  it("Open folder / double-click drill in and Up goes back (A3)", openFolderDrillsInAndUpGoesBack);
  it("Enter on the selected tile opens it (A5)", enterOnTheSelectedTileOpensIt);
  it("Reveal in Finder goes through the backend (A4)", revealAsksTheBackendForTheSelectedPath);
  it("no snapshot shows No scans yet with Start scan (A5)", noSnapshotOffersStartScan);
  it("a folded folder ends with the engine's Other block, which cannot be opened (A2)", foldedFolderEndsWithAnOtherBlock);
});
