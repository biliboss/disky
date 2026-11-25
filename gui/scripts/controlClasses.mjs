// Control classes the GUI may put on a <button> (REQ-DCLEAN-012.A1). Shared by
// scripts/lint-controls.mjs and src/screens/__tests__/controls.test.tsx, so the
// static lint and the render test cannot drift apart.

/** Design-system button variants; every `.btn` carries exactly one. */
export const BUTTON_VARIANTS = ["btn-primary", "btn-secondary", "btn-ghost"];

/** Modifiers that refine a variant but never stand alone (`btn-icon` sizes, `btn-danger` recolours). */
export const BUTTON_MODIFIERS = ["btn-icon", "btn-danger", "btn-block"];

/**
 * Buttons themed by their own component class instead of `.btn`. Each must
 * reset the browser's border and background in its own rule; `card` lets a
 * class take its background from the design system's `.card`.
 */
export const THEMED_EXCEPTIONS = [
  { cls: "nav-item", css: "src/shell/shell.css" },
  { cls: "notice-link", css: "src/screens/smartClean/smartClean.css" },
  { cls: "result-check", css: "src/screens/smartClean/smartClean.css" },
  { cls: "shortcut", css: "src/screens/smartClean/smartClean.css", backgroundFrom: "card" },
  { cls: "switch", css: "src/screens/settings/settings.css" },
  { cls: "chip-remove", css: "src/screens/settings/settings.css" },
  { cls: "map-tile", css: "src/screens/spaceMap/spaceMap.css" },
  { cls: "dupe-copy", css: "src/screens/duplicates/duplicates.css" },
  { cls: "agent-option", css: "src/screens/assistant/assistant.css" },
];

export const THEMED_EXCEPTION_CLASSES = THEMED_EXCEPTIONS.map((e) => e.cls);

/** The one themed selection control (REQ-DCLEAN-012.A2): Smart Clean results and the Large files rows (REQ-DSCR-013). */
export const CHECKBOX_CLASS = "result-check";

/** Classes that must declare both a :hover and an :active rule (REQ-DCLEAN-013, -015). */
export const STATEFUL_CLASSES = [...BUTTON_VARIANTS, ...THEMED_EXCEPTION_CLASSES, "seg-opt"];

/** Verbatim copies of the Organic design system: read, never linted or edited. */
export const VENDORED = ["src/styles/organic.css", "src/styles/tokens.css"];

/**
 * Why a button's class list breaks REQ-DCLEAN-012.A1, or null when it is
 * themed: `btn` with one variant, or a themed exception.
 */
export function buttonClassProblem(classes) {
  const set = new Set(classes);
  if (THEMED_EXCEPTION_CLASSES.some((c) => set.has(c))) return null;
  if (!set.has("btn")) return `no design-system class (want "btn btn-<variant>" or one of ${THEMED_EXCEPTION_CLASSES.join(", ")})`;
  const variants = BUTTON_VARIANTS.filter((v) => set.has(v));
  if (variants.length !== 1) return `"btn" needs exactly one of ${BUTTON_VARIANTS.join(", ")} (has ${variants.length})`;
  return null;
}
