import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { layout, tileColor, TILE_TOKENS } from "../spaceMap/treemap";
import { crumbLine, mapTitle, shareOfDisk, tileLabel } from "../spaceMap/mapCopy";

/** The snapshot's own `layout()` and `TILE`, lifted from snapshot.html. */
function snapshotLayout(): (items: { size: number }[], x: number, y: number, w: number, h: number, out: unknown[]) => any[] {
  const html = readFileSync(resolve(__dirname, "../../../../specs/disk-cleanup/snapshot.html"), "utf8");
  const start = html.indexOf("function layout(items");
  const end = html.indexOf("const css = str", start);
  // eslint-disable-next-line no-new-func
  return new Function(`${html.slice(start, end)}; return layout;`)();
}

function snapshotTiles(): string[] {
  const html = readFileSync(resolve(__dirname, "../../../../specs/disk-cleanup/snapshot.html"), "utf8");
  const line = html.slice(html.indexOf("const TILE = ["), html.indexOf("];", html.indexOf("const TILE = [")));
  return [...line.matchAll(/var\((--color-[\w-]+)\)/g)].map((m) => m[1]);
}

/** The snapshot's MAP root and its Users folder. */
const ROOT = [212, 92, 58, 34, 26, 16];
const USERS = [74, 48, 41, 31, 12, 6];

function matchesTheSnapshotSplitExactly() {
  const theirs = snapshotLayout();
  for (const sizes of [ROOT, USERS, [5], [3, 3], [9, 1, 1, 1], [1, 1, 1, 1, 1, 1, 1, 1, 1]]) {
    const ours = layout(sizes.map((weight) => ({ weight })));
    const ref = theirs(sizes.map((size) => ({ size })), 0, 0, 100, 100, []);
    expect(ours.map(({ x, y, w, h }) => ({ x, y, w, h }))).toEqual(ref.map(({ x, y, w, h }) => ({ x, y, w, h })));
  }
}

function areaIsProportionalToWeight() {
  const total = ROOT.reduce((a, b) => a + b, 0);
  const rects = layout(ROOT.map((weight) => ({ weight })));
  rects.forEach((r, i) => expect((r.w * r.h) / 100).toBeCloseTo((ROOT[i] / total) * 100, 9));
  expect(rects[0]).toMatchObject({ x: 0, y: 0, h: 100 });
  expect(rects[0].w).toBeCloseTo((212 / 438) * 100, 9);
  // Zero weights (gap-fill) still yield finite rectangles.
  expect(layout([{ weight: 0 }, { weight: 0 }]).every((r) => Number.isFinite(r.w))).toBe(true);
}

function tilesCycleTheSnapshotsSevenTokens() {
  expect(TILE_TOKENS).toHaveLength(7);
  expect(tileColor(0)).toBe("var(--tile-0)");
  expect(tileColor(8)).toBe("var(--tile-1)");
  // tokens.css maps --tile-N onto the snapshot's TILE list, in order.
  const tokens = readFileSync(resolve(__dirname, "../../styles/tokens.css"), "utf8");
  const mapped = TILE_TOKENS.map((t) => tokens.match(new RegExp(`${t}:\\s*var\\((--color-[\\w-]+)\\)`))![1]);
  expect(mapped).toEqual(snapshotTiles());
}

function crumbTitleAndShareAreWorded() {
  const root = { path: "/", root: "/", total_bytes: 438e9 };
  expect(crumbLine(root)).toBe("Macintosh HD");
  expect(mapTitle(root)).toBe("Macintosh HD · 438 GB");
  const users = { path: "/Users/me", root: "/", total_bytes: 74e9 };
  expect(crumbLine(users)).toBe("Macintosh HD  /  Users  /  me");
  expect(mapTitle(users)).toBe("me · 74 GB");
  expect(crumbLine({ path: "/Users/me/Projects/a", root: "/Users/me" })).toBe("me  /  Projects  /  a");
  expect(shareOfDisk(212e9, 512e9)).toBe("41.4%");
  expect(tileLabel("Users", 212e9)).toBe("Users, 212 GB");
}

describe("Space map layout and copy (REQ-DSCR-012.A1, A2)", () => {
  it("lays out tiles exactly as the snapshot's split", matchesTheSnapshotSplitExactly);
  it("each tile's area is proportional to its bytes", areaIsProportionalToWeight);
  it("tile colours cycle the snapshot's seven tile tokens", tilesCycleTheSnapshotsSevenTokens);
  it("crumb, title, share and tile names are worded as the snapshot", crumbTitleAndShareAreWorded);
});
