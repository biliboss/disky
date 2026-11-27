import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { AA_TEXT, AA_UI, pairRatio, parseTokens, resolveColor } from "../lib/contrast";

/**
 * REQ-DCLEAN-024: every foreground/background token pair the GUI paints,
 * measured against WCAG 2.2 AA (4.5:1 text, 3:1 interface). Values come
 * from the sheets as shipped: organic.css, then tokens.css, then
 * gui-tokens.css (index.tsx's load order).
 */
const read = (p: string) => readFileSync(resolve(__dirname, p), "utf8");
const TOKENS = parseTokens(read("organic.css"), read("tokens.css"), read("gui-tokens.css"));

type Pair = { fg: string; bg: string[]; where: string };

/** Text pairs: 4.5:1. `bg` lists layers bottom-up. */
const TEXT: Pair[] = [
  { fg: "--color-text", bg: ["--color-bg"], where: "body text, headings" },
  { fg: "--color-text", bg: ["--color-surface"], where: "rows, sidebar, dialog, footer" },
  { fg: "--color-text", bg: ["--color-neutral-200"], where: "hovered shortcut card" },
  { fg: "--color-text", bg: ["--color-neutral-300"], where: "pressed shortcut card" },
  { fg: "--color-text", bg: ["--color-surface", "--overlay-hover"], where: "hovered nav entry" },
  { fg: "--color-text", bg: ["--color-surface", "--overlay-active"], where: "pressed nav entry" },
  { fg: "--color-text", bg: ["--color-surface", "--overlay-row-hover"], where: "hovered category row" },
  { fg: "--color-text", bg: ["--color-bg", "--overlay-hover"], where: "hovered secondary button" },
  { fg: "--color-text", bg: ["--color-bg", "--overlay-active"], where: "pressed secondary button" },
  { fg: "--color-neutral-700", bg: ["--color-bg"], where: "kickers, ring caption, meter" },
  { fg: "--color-neutral-700", bg: ["--color-surface"], where: "row descriptions, title bar, item paths" },
  { fg: "--color-neutral-800", bg: ["--color-bg"], where: "idle and done paragraphs, notices" },
  { fg: "--color-neutral-800", bg: ["--color-surface"], where: "footer note" },
  { fg: "--color-on-accent", bg: ["--color-accent"], where: "primary label, current nav, badge, selected mode" },
  { fg: "--color-on-accent", bg: ["--color-primary-hover"], where: "hovered primary button" },
  { fg: "--color-on-accent", bg: ["--color-primary-active"], where: "pressed primary button" },
  { fg: "--color-bg", bg: ["--color-danger"], where: "Clean now, delete mode" },
  { fg: "--color-bg", bg: ["--color-danger-hover"], where: "Clean now, delete mode, hovered" },
  { fg: "--color-ghost-text", bg: ["--color-bg"], where: "ghost buttons on the page" },
  { fg: "--color-ghost-text", bg: ["--color-bg", "--overlay-ghost-hover"], where: "hovered ghost button" },
  { fg: "--color-ghost-text", bg: ["--color-bg", "--overlay-ghost-active"], where: "pressed ghost button" },
  { fg: "--color-ghost-text", bg: ["--color-surface"], where: "ghost buttons on a surface" },
  { fg: "--color-accent-700", bg: ["--color-bg"], where: "links, notice link" },
  { fg: "--color-link-hover", bg: ["--color-bg"], where: "hovered link" },
  { fg: "--color-link-active", bg: ["--color-bg"], where: "pressed link" },
  { fg: "--color-danger", bg: ["--color-bg"], where: "error states" },
  { fg: "--color-danger", bg: ["--color-surface"], where: "error states on a surface" },
  { fg: "--color-accent-800", bg: ["--color-accent-100"], where: "tag-accent (Review, Below your goal)" },
  { fg: "--color-accent-800", bg: ["--color-bg"], where: "badge on the current nav entry" },
  { fg: "--color-accent-2-800", bg: ["--color-accent-2-100"], where: "tag-accent-2 (Safe, Done, Goal met)" },
  { fg: "color-mix(in srgb, var(--color-text) 80%, transparent)", bg: ["--color-surface"], where: "shortcut card body (.card-body opacity .8)" },
  { fg: "color-mix(in srgb, var(--color-text) 85%, transparent)", bg: ["--color-surface"], where: "dialog body (.dialog-body opacity .85)" },
];

/** Interface pairs: 3:1 (focus rings, selection-control edges and marks, meaningful icons). */
const UI: Pair[] = [
  { fg: "--focus-ring-color", bg: ["--color-bg"], where: "focus ring on the page" },
  { fg: "--focus-ring-color", bg: ["--color-surface"], where: "focus ring in rows, sidebar, dialog, footer" },
  { fg: "--focus-ring-color", bg: ["--color-neutral-200"], where: "focus ring beside a hovered card" },
  { fg: "--focus-ring-inset-color", bg: ["--color-accent"], where: "ring on the selected mode option" },
  { fg: "--focus-ring-inset-color", bg: ["--color-bg"], where: "ring on an unselected mode option" },
  { fg: "--color-check-border", bg: ["--color-surface"], where: "unchecked checkbox edge" },
  { fg: "--color-check-hover", bg: ["--color-surface"], where: "hovered unchecked checkbox edge" },
  { fg: "--color-check-border-checked", bg: ["--color-surface"], where: "checked checkbox edge" },
  { fg: "--color-bg", bg: ["--color-accent"], where: "check mark" },
  { fg: "--color-bg", bg: ["--color-check-checked-hover"], where: "check mark, hovered" },
  { fg: "--color-bg", bg: ["--color-check-checked-active"], where: "check mark, pressed" },
  { fg: "--color-bg", bg: ["--color-accent-2"], where: "scan row done mark" },
  { fg: "--color-accent-700", bg: ["--color-bg"], where: "shortcut icon" },
  { fg: "--color-accent-2-700", bg: ["--color-bg"], where: "category icon" },
  { fg: "--color-accent-2-700", bg: ["--color-surface"], where: "footer shield icon" },
  { fg: "--color-text", bg: ["--color-surface"], where: "disclosure chevron" },
];

/**
 * Not measured, with the WCAG reason:
 * - ring, meter, scan and clean fills — the same value is printed beside
 *   them as text, so the graphic is not required to understand it (1.4.11);
 * - button and card boundaries — the text label identifies the control (1.4.11);
 * - disabled controls — inactive components are exempt (1.4.3, 1.4.11);
 * - ring and spinner tracks (`--color-surface` strokes) — as the fills;
 * - divider rules and the brand logo — decorative.
 */
const EXEMPT_FOREGROUNDS = ["--color-accent", "--color-accent-2", "--color-surface", "--color-divider"];

function ratiosMeetAA(pairs: Pair[], min: number) {
  const failing = pairs
    .map((p) => ({ ...p, ratio: Math.round(pairRatio(p.fg, p.bg, TOKENS) * 100) / 100 }))
    .filter((p) => p.ratio < min);
  expect(failing).toEqual([]);
}

const textPairsMeetAA = () => ratiosMeetAA(TEXT, AA_TEXT);
const uiPairsMeetAA = () => ratiosMeetAA(UI, AA_UI);

/** A1: body and secondary text on both grounds. */
function bodyAndSecondaryTextOnBothGrounds() {
  for (const fg of ["--color-text", "--color-neutral-800", "--color-neutral-700"]) {
    for (const bg of ["--color-bg", "--color-surface"]) {
      expect({ fg, bg, ok: pairRatio(fg, [bg], TOKENS) >= AA_TEXT }).toEqual({ fg, bg, ok: true });
    }
  }
}

/** A2: accent text at body size is a deep ramp step, never the accent itself. */
function accentTextUsesADeepStep() {
  expect(TOKENS.get("--color-ghost-text")).toBe("var(--color-accent-700)");
  expect(pairRatio("--color-accent", ["--color-bg"], TOKENS)).toBeLessThan(AA_TEXT); // why the accent is not text
  const sheets = [read("../shell/shell.css"), read("../screens/smartClean/smartClean.css"), read("app.css")].join("\n");
  expect(sheets).not.toMatch(/(^|[^-])color:\s*var\(--color-accent\)/m);
}

/** A3: --text-muted carries no required text: nothing in the GUI uses it. */
function mutedTextIsUnused() {
  const sheets = [read("../shell/shell.css"), read("../screens/smartClean/smartClean.css"), read("app.css")].join("\n");
  expect(sheets).not.toMatch(/--text-muted|\btext-muted\b/);
}

/** A5: new state tokens are aliases of Organic ramp steps; no literal colour. */
function newColourTokensAliasOrganicSteps() {
  const gui = parseTokens(read("gui-tokens.css"));
  const isColour = (value: string) => {
    try {
      resolveColor(value, TOKENS);
      return true;
    } catch {
      return false;
    }
  };
  const colours = [...gui].filter(([, value]) => isColour(value));
  expect(colours.length).toBeGreaterThan(10);
  for (const [name, value] of colours) {
    expect({ name, literal: /#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(|oklch\(/i.test(value) }).toEqual({ name, literal: false });
  }
  // The danger token resolves to an existing ramp step.
  expect(resolveColor("var(--color-danger)", TOKENS)).toEqual(resolveColor("var(--color-accent-800)", TOKENS));
}

/** Every colour token a component stylesheet paints text or icons with is measured above. */
function everyForegroundInUseIsMeasured() {
  const sheets = [read("../shell/shell.css"), read("../screens/smartClean/smartClean.css"), read("app.css")].join("\n");
  const used = new Set(
    [...sheets.matchAll(/(?:^|[\s;{])(?:color|stroke|outline(?:-color)?|border(?:-color)?)\s*:[^;]*?var\((--[\w-]+)\)/gm)]
      .map((m) => m[1])
      .filter((t) => !/^--(check-border|busy-stroke|item-rule|focus-ring-width)$/.test(t)),
  );
  const measured = new Set([...TEXT, ...UI].map((p) => p.fg));
  const unmeasured = [...used].filter((t) => !measured.has(t) && !EXEMPT_FOREGROUNDS.includes(t));
  expect(unmeasured).toEqual([]);
}

describe("WCAG 2.2 AA contrast from tokens (REQ-DCLEAN-024)", () => {
  it("every text pair in use reaches 4.5:1", textPairsMeetAA);
  it("every interface pair in use reaches 3:1 (A4)", uiPairsMeetAA);
  it("body and secondary text reach 4.5:1 on bg and surface (A1)", bodyAndSecondaryTextOnBothGrounds);
  it("accent text at body size uses accent-700 or deeper (A2)", accentTextUsesADeepStep);
  it("--text-muted carries no required text (A3)", mutedTextIsUnused);
  it("state tokens alias Organic ramp steps, no literal colour (A5)", newColourTokensAliasOrganicSteps);
  it("every foreground token the stylesheets use is measured", everyForegroundInUseIsMeasured);
});
