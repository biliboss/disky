/**
 * WCAG 2.2 contrast over the GUI's token sheets (REQ-DCLEAN-024). Resolves a
 * token to an sRGB colour — hex, `var(--x)` aliases and
 * `color-mix(in srgb, A p%, B)` with `transparent` — and computes the
 * relative-luminance ratio, compositing translucent colours over the
 * background they sit on. Pure: the caller supplies the sheets' text.
 *
 * @implements REQ-DCLEAN-024
 */

export interface Rgba {
  r: number;
  g: number;
  b: number;
  /** 0..1 */
  a: number;
}

/** WCAG 2.2 AA minimums. */
export const AA_TEXT = 4.5;
export const AA_UI = 3;

/** `--name: value;` declarations of `:root` blocks, later sheets overriding earlier ones. */
export function parseTokens(...sheets: string[]): Map<string, string> {
  const out = new Map<string, string>();
  for (const sheet of sheets) {
    const clean = sheet.replace(/\/\*[\s\S]*?\*\//g, "");
    for (const m of clean.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) out.set(m[1], m[2].trim());
  }
  return out;
}

const TRANSPARENT: Rgba = { r: 0, g: 0, b: 0, a: 0 };

/** Split on top-level commas (not inside parentheses). */
function args(s: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const ch of s) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if (ch === "," && depth === 0) {
      out.push(cur.trim());
      cur = "";
    } else cur += ch;
  }
  out.push(cur.trim());
  return out;
}

/** Resolve a CSS colour expression against `tokens`. Throws on anything it does not understand. */
export function resolveColor(expr: string, tokens: Map<string, string>, depth = 0): Rgba {
  if (depth > 20) throw new Error(`contrast: alias loop at ${expr}`);
  const e = expr.trim();
  if (e === "transparent") return TRANSPARENT;
  const hex = e.match(/^#([0-9a-f]{6})$/i);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255, a: 1 };
  }
  const v = e.match(/^var\((--[\w-]+)\)$/);
  if (v) {
    const value = tokens.get(v[1]);
    if (value === undefined) throw new Error(`contrast: unknown token ${v[1]}`);
    return resolveColor(value, tokens, depth + 1);
  }
  const mix = e.match(/^color-mix\(\s*in srgb\s*,([\s\S]*)\)$/);
  if (mix) {
    const [a, b] = args(mix[1]);
    const pa = a.match(/^(.*?)\s+(\d+(?:\.\d+)?)%$/);
    const pb = b.match(/^(.*?)\s+(\d+(?:\.\d+)?)%$/);
    const ca = resolveColor(pa ? pa[1] : a, tokens, depth + 1);
    const cb = resolveColor(pb ? pb[1] : b, tokens, depth + 1);
    const wa = pa ? Number(pa[2]) / 100 : pb ? 1 - Number(pb[2]) / 100 : 0.5;
    // CSS Color 5: premultiplied interpolation.
    const alpha = ca.a * wa + cb.a * (1 - wa);
    if (alpha === 0) return TRANSPARENT;
    const ch = (k: "r" | "g" | "b") => (ca[k] * ca.a * wa + cb[k] * cb.a * (1 - wa)) / alpha;
    return { r: ch("r"), g: ch("g"), b: ch("b"), a: alpha };
  }
  throw new Error(`contrast: cannot resolve ${expr}`);
}

/** `fg` painted over the opaque `bg`. */
export function composite(fg: Rgba, bg: Rgba): Rgba {
  return { r: fg.r * fg.a + bg.r * (1 - fg.a), g: fg.g * fg.a + bg.g * (1 - fg.a), b: fg.b * fg.a + bg.b * (1 - fg.a), a: 1 };
}

const channel = (c: number) => {
  const s = c / 255;
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};

/** WCAG relative luminance of an opaque colour. */
export function luminance(c: Rgba): number {
  return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
}

/** Contrast ratio of two opaque colours, 1..21. */
export function contrastRatio(a: Rgba, b: Rgba): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * Ratio of `fg` on `bg`, where `bg` may be layered: `["--color-surface",
 * "--overlay-hover"]` is the hover overlay over the surface. Translucent
 * foregrounds are composited over the result.
 */
export function pairRatio(fg: string, bg: readonly string[], tokens: Map<string, string>): number {
  const ref = (t: string) => (t.startsWith("--") ? `var(${t})` : t);
  let ground = resolveColor(ref(bg[0]), tokens);
  if (ground.a !== 1) throw new Error(`contrast: base background ${bg[0]} is not opaque`);
  for (const layer of bg.slice(1)) ground = composite(resolveColor(ref(layer), tokens), ground);
  return contrastRatio(composite(resolveColor(ref(fg), tokens), ground), ground);
}
