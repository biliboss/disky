/**
 * The snapshot's treemap split (snapshot.html `layout()`), ported verbatim
 * as a pure function (REQ-DSCR-012.A2). It lays out weights the engine
 * already computed (each child's physical bytes from `disky map`); it never
 * walks or sizes files. Items keep their order — the engine sends them
 * largest first — and the rectangle is in percent of the treemap box.
 *
 * Split rule: take items from the front while they stay at or under half
 * the total (at least one), give that run its share of the longer side, and
 * recurse on both halves.
 */
export interface TileRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function layout<T extends { weight: number }>(
  items: readonly T[],
  x = 0,
  y = 0,
  w = 100,
  h = 100,
  out: (T & TileRect)[] = [],
): (T & TileRect)[] {
  if (!items.length) return out;
  if (items.length === 1) {
    out.push({ ...items[0], x, y, w, h });
    return out;
  }
  const total = items.reduce((a, b) => a + b.weight, 0);
  let acc = 0;
  let i = 0;
  while (i < items.length - 1 && acc + items[i].weight <= total / 2) acc += items[i++].weight;
  if (i === 0) {
    acc = items[0].weight;
    i = 1;
  }
  // Gap-fill: an all-zero run splits by count (the snapshot never has one).
  const f = total > 0 ? acc / total : i / items.length;
  const A = items.slice(0, i);
  const B = items.slice(i);
  if (w >= h) {
    layout(A, x, y, w * f, h, out);
    layout(B, x + w * f, y, w * (1 - f), h, out);
  } else {
    layout(A, x, y, w, h * f, out);
    layout(B, x, y + h * f, w, h * (1 - f), out);
  }
  return out;
}

/** The snapshot's seven tile colours (`TILE`), as tokens, cycled by index. */
export const TILE_TOKENS = ["--tile-0", "--tile-1", "--tile-2", "--tile-3", "--tile-4", "--tile-5", "--tile-6"] as const;

export function tileColor(i: number): string {
  return `var(${TILE_TOKENS[i % TILE_TOKENS.length]})`;
}
