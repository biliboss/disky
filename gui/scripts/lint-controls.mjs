// Control lint (REQ-DCLEAN-012 … 015). Every <button> in src/**/*.tsx uses the
// design-system button classes or a themed exception that resets the browser
// look itself (012.A1); every checkbox is the themed `.result-check` with its
// tokens (012.A2); every control class has :hover and :active rules
// (013 / 015); no component stylesheet removes the focus-visible ring (014).
// The vendored Organic sheets are read for their rules, never linted.
// Usage: node scripts/lint-controls.mjs   (exit 1 on any finding)
import { readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join, relative, sep } from "node:path";
import {
  CHECKBOX_CLASS,
  STATEFUL_CLASSES,
  THEMED_EXCEPTIONS,
  VENDORED,
  buttonClassProblem,
} from "./controlClasses.mjs";

// fileURLToPath, not .pathname: on Windows the pathname is "/D:/a/..." (REQ-XPLAT-010.A3).
const ROOT = fileURLToPath(new URL("..", import.meta.url));
const SRC = join(ROOT, "src");
// `/`-separated, as VENDORED and controlClasses.mjs spell paths, on every OS (REQ-XPLAT-010.A3).
const rel = (p) => relative(ROOT, p).split(sep).join("/");
const isVendored = (p) => VENDORED.includes(rel(p));

function* walk(dir, ext) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name === "__tests__") continue;
      yield* walk(p, ext);
    } else if (p.endsWith(ext) && !/\.test\.tsx?$/.test(p)) {
      yield p;
    }
  }
}

const lineOf = (src, index) => src.slice(0, index).split("\n").length;

/** Opening tags `<tag …>`, skipping `>` inside quotes and `{…}` expressions (arrows, JSX). */
function* openingTags(src, tag) {
  const re = new RegExp(`<${tag}\\b`, "g");
  for (const m of src.matchAll(re)) {
    let depth = 0;
    let quote = null;
    for (let i = m.index + m[0].length; i < src.length; i++) {
      const ch = src[i];
      if (quote) {
        if (ch === quote && src[i - 1] !== "\\") quote = null;
      } else if (ch === '"' || ch === "'" || ch === "`") {
        quote = ch;
      } else if (ch === "{") {
        depth++;
      } else if (ch === "}") {
        depth--;
      } else if (ch === ">" && depth === 0) {
        yield { text: src.slice(m.index, i + 1), index: m.index };
        break;
      }
    }
  }
}

/** Static classes of a tag: `class="…"`, string literals in `class={…}`, and `classList` keys. */
function classesOf(tagText) {
  const out = [];
  const plain = tagText.match(/\bclass="([^"]*)"/);
  if (plain) out.push(...plain[1].split(/\s+/));
  const expr = tagText.match(/\bclass=\{([^}]*)\}/);
  if (expr) for (const s of expr[1].matchAll(/["'`]([^"'`$]*)["'`]/g)) out.push(...s[1].split(/\s+/));
  const list = tagText.match(/\bclassList=\{\{([^}]*)\}\}/);
  if (list) for (const k of list[1].matchAll(/["']([\w-]+)["']\s*:/g)) out.push(k[1]);
  return out.filter(Boolean);
}

/** CSS rule blocks `{selector, body, index}` with comments blanked. */
function rules(css) {
  const clean = css.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
  return [...clean.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({ selector: m[1].trim(), body: m[2], index: m.index }));
}

/** Rules whose selector list has `.cls` as a whole selector (no pseudo-class or attribute). */
const baseRules = (all, cls) =>
  all.filter((r) => r.selector.split(",").some((s) => s.trim() === `.${cls}`));

const findings = [];
const add = (file, line, msg) => findings.push(`${rel(file)}:${line}: ${msg}`);

// ── 012.A1 / A2: markup ─────────────────────────────────────────────
const exceptionsUsedWith = new Map(); // cls → Set of companion classes
for (const file of walk(SRC, ".tsx")) {
  const src = readFileSync(file, "utf8");
  for (const tag of openingTags(src, "button")) {
    const cls = classesOf(tag.text);
    const problem = buttonClassProblem(cls);
    if (problem) add(file, lineOf(src, tag.index), `button: ${problem} (REQ-DCLEAN-012.A1)`);
    for (const e of THEMED_EXCEPTIONS) {
      if (cls.includes(e.cls)) {
        const set = exceptionsUsedWith.get(e.cls) ?? new Set();
        cls.forEach((c) => set.add(c));
        exceptionsUsedWith.set(e.cls, set);
      }
    }
    if (/\brole="checkbox"/.test(tag.text) && !cls.includes(CHECKBOX_CLASS)) {
      add(file, lineOf(src, tag.index), `checkbox without .${CHECKBOX_CLASS} (REQ-DCLEAN-012.A2)`);
    }
  }
  for (const tag of openingTags(src, "input")) {
    if (/\btype="checkbox"/.test(tag.text) && !classesOf(tag.text).includes(CHECKBOX_CLASS)) {
      add(file, lineOf(src, tag.index), `native checkbox without .${CHECKBOX_CLASS} (REQ-DCLEAN-012.A2)`);
    }
  }
}

// ── 012.A1: each themed exception resets the browser look ───────────
for (const e of THEMED_EXCEPTIONS) {
  const file = join(ROOT, e.css);
  const own = baseRules(rules(readFileSync(file, "utf8")), e.cls);
  const decl = (prop) => own.some((r) => new RegExp(`(^|[;\\s])${prop}\\s*:`).test(r.body));
  if (own.length === 0) {
    add(file, 1, `.${e.cls} has no base rule (REQ-DCLEAN-012.A1)`);
    continue;
  }
  const at = lineOf(readFileSync(file, "utf8"), own[0].index);
  if (!decl("border")) add(file, at, `.${e.cls} must reset border (REQ-DCLEAN-012.A1)`);
  // The background may come from a design-system class the button always carries alongside.
  const viaCard = Boolean(e.backgroundFrom && exceptionsUsedWith.get(e.cls)?.has(e.backgroundFrom));
  if (!decl("background") && !viaCard) add(file, at, `.${e.cls} must set background (REQ-DCLEAN-012.A1)`);
}

// ── 012.A2: the checkbox's tokens ───────────────────────────────────
{
  const file = join(SRC, "screens/smartClean/smartClean.css");
  const all = rules(readFileSync(file, "utf8"));
  const base = baseRules(all, CHECKBOX_CLASS).map((r) => r.body).join(";");
  const checked = all
    .filter((r) => r.selector === `.${CHECKBOX_CLASS}[aria-checked="true"]`)
    .map((r) => r.body)
    .join(";");
  const want = [
    [base, "var(--checkbox-size)", "size"],
    [base, "var(--color-check-border)", "unchecked border"],
    [base, "color: var(--color-bg)", "check glyph"],
    [checked, "background: var(--color-accent)", "checked fill"],
  ];
  for (const [body, token, what] of want) {
    if (!body.includes(token)) add(file, 1, `.${CHECKBOX_CLASS} ${what} must use ${token} (REQ-DCLEAN-012.A2)`);
  }
}

// ── 013 / 015: hover and pressed rules; 014: the ring stays ─────────
const sheets = [...walk(SRC, ".css")];
const allCss = sheets.map((f) => readFileSync(f, "utf8")).join("\n");
const selectors = rules(allCss).flatMap((r) => r.selector.split(",").map((s) => s.trim()));
for (const cls of STATEFUL_CLASSES) {
  for (const state of ["hover", "active"]) {
    const re = new RegExp(`\\.${cls}(?![\\w-])[^,]*:${state}\\b`);
    if (!selectors.some((s) => re.test(s))) {
      findings.push(`(css): .${cls} has no :${state} rule (REQ-DCLEAN-0${state === "hover" ? 13 : 15})`);
    }
  }
}
for (const file of sheets.filter((f) => !isVendored(f))) {
  const src = readFileSync(file, "utf8");
  for (const r of rules(src)) {
    if (/outline\s*:\s*(none|0)\b/.test(r.body) && !/^\[data-phase-heading\]:focus$/.test(r.selector)) {
      add(file, lineOf(src, r.index), `"${r.selector}" removes the focus ring (REQ-DCLEAN-014)`);
    }
  }
}

if (findings.length) {
  console.error(findings.join("\n"));
  console.error(`\n${findings.length} control finding(s). Use the design-system classes (scripts/controlClasses.mjs).`);
  process.exit(1);
}
console.log("lint:controls — every control is themed, with hover, pressed and focus states.");
