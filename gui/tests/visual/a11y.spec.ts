import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import {
  appToResults,
  doneFixtureResult,
  idleFixtureView,
  openApp,
  reviewFixtureView,
  stubEngine,
  tokenColor,
} from "./engineStub";

/**
 * Accessibility in a real browser: axe on every phase (REQ-DCLEAN-021,
 * REQ-DCLEAN-023), the keyboard focus ring on every control
 * (REQ-DCLEAN-014), and the hover / pressed states of each control class
 * (REQ-DCLEAN-013, REQ-DCLEAN-015).
 */
test.describe.configure({ mode: "serial" });
test.setTimeout(180_000);

// ── Phases ───────────────────────────────────────────────────────

type Phase = { name: string; open: (page: Page) => Promise<void> };

const PHASES: Phase[] = [
  {
    name: "idle",
    open: async (page) => {
      await stubEngine(page, idleFixtureView());
      await openApp(page);
      await page.getByTestId("idle").waitFor();
    },
  },
  {
    name: "idle with the below-goal notification",
    open: async (page) => {
      await stubEngine(page, reviewFixtureView());
      await openApp(page);
      await page.getByTestId("goal-notification").waitFor();
    },
  },
  {
    name: "loading",
    open: async (page) => {
      await stubEngine(page, "forever");
      await openApp(page);
      await page.getByTestId("busy").waitFor();
    },
  },
  {
    name: "scanning",
    open: async (page) => {
      await stubEngine(page, idleFixtureView(), true);
      await openApp(page);
      await page.getByTestId("idle").getByRole("button", { name: "Start scan" }).click();
      await page.getByTestId("scanning").waitFor();
    },
  },
  {
    name: "scan error",
    open: async (page) => {
      await stubEngine(page, idleFixtureView());
      await openApp(page);
      await page.getByTestId("idle").getByRole("button", { name: "Start scan" }).click();
      await page.getByTestId("scan-error").waitFor();
    },
  },
  {
    name: "results",
    open: async (page) => {
      await appToResults(page, { ...reviewFixtureView(), skipped_paths: 3 });
      await page.getByRole("button", { name: "Show items in System caches" }).click();
    },
  },
  {
    name: "empty results",
    open: async (page) => {
      await appToResults(page, idleFixtureView());
      await page.getByTestId("results-empty").waitFor();
    },
  },
  {
    name: "confirm dialog",
    open: async (page) => {
      await appToResults(page);
      await page.getByTestId("clean").click();
      await page.getByRole("dialog").waitFor();
    },
  },
  {
    name: "cleaning",
    open: async (page) => {
      await appToResults(page, reviewFixtureView(), "forever");
      await page.getByTestId("clean").click();
      await page.getByRole("dialog").getByRole("button", { name: "Clean now" }).click();
      await page.getByTestId("cleaning").waitFor();
    },
  },
  {
    name: "done",
    open: async (page) => {
      await appToResults(page, reviewFixtureView(), doneFixtureResult());
      await page.getByTestId("clean").click();
      await page.getByRole("dialog").getByRole("button", { name: "Clean now" }).click();
      await page.getByTestId("done").waitFor();
    },
  },
];

// ── REQ-DCLEAN-021 / 023 · axe ───────────────────────────────────

async function everyPhasePassesAxe({ page }: { page: Page }) {
  const report: Record<string, string[]> = {};
  for (const phase of PHASES) {
    await phase.open(page);
    const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
    report[phase.name] = result.violations
      .filter((v) => v.impact === "serious" || v.impact === "critical")
      .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  }
  expect(report).toEqual(Object.fromEntries(PHASES.map((p) => [p.name, []])));
}

test("every phase has zero serious or critical axe violations (REQ-DCLEAN-021, REQ-DCLEAN-023, REQ-DCLEAN-024)", everyPhasePassesAxe);

/** Status changes are announced in a polite live region; focus stays put (REQ-DCLEAN-023). */
async function statusIsAnnouncedPolitelyWithoutStealingFocus({ page }: { page: Page }) {
  await appToResults(page, reviewFixtureView(), doneFixtureResult());
  const live = page.getByTestId("announcer");
  await expect(live).toHaveAttribute("aria-live", "polite");
  await expect(live).toHaveText(/^Found .+ you can clear$/);
  await page.getByTestId("clean").click();
  await page.getByRole("dialog").getByRole("button", { name: "Clean now" }).click();
  await page.getByTestId("done").waitFor();
  await expect(live).toHaveText(/^.+ cleared$/);
  expect(await live.evaluate((el) => el.contains(document.activeElement))).toBe(false);
}

test("status changes are announced politely without taking focus (REQ-DCLEAN-023.A2)", statusIsAnnouncedPolitelyWithoutStealingFocus);

// ── REQ-DCLEAN-014 · focus-visible ring ──────────────────────────

/**
 * Tab through the page once, from the top, and report each focused control
 * whose ring is missing. A hidden radio rings its `.seg-opt` wrapper (A2).
 */
async function ringsOnTab(page: Page): Promise<{ seen: number; missing: string[] }> {
  const ring = await tokenColor(page, "--focus-ring-color");
  const inset = await tokenColor(page, "--focus-ring-inset-color");
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  const missing: string[] = [];
  const visited = new Set<string>();
  for (let i = 0; i < 60; i++) {
    await page.keyboard.press("Tab");
    const r = await page.evaluate(
      ([ringColor, insetColor]) => {
        const el = document.activeElement as HTMLElement | null;
        if (!el || el === document.body) return null;
        const id = `${el.tagName}:${el.getAttribute("aria-label") ?? el.textContent?.trim() ?? ""}:${[...document.querySelectorAll("*")].indexOf(el)}`;
        const radio = el instanceof HTMLInputElement && el.type === "radio";
        const ringed = radio ? (el.closest(".seg-opt") as HTMLElement) : el;
        const s = getComputedStyle(ringed);
        const outline = s.outlineStyle !== "none" && parseFloat(s.outlineWidth) > 0;
        // A1: the ring is the focus token (the inset one on a mode option, A2).
        const coloured = s.outlineColor === (radio ? insetColor : ringColor);
        return { id, ok: el.matches(":focus-visible") && outline && coloured };
      },
      [ring, inset] as const,
    );
    if (!r) continue;
    if (visited.has(r.id)) break; // wrapped
    visited.add(r.id);
    if (!r.ok) missing.push(r.id);
  }
  return { seen: visited.size, missing };
}

async function everyControlShowsTheRingOnTab({ page }: { page: Page }) {
  const report: Record<string, string[]> = {};
  for (const phase of PHASES.filter((p) => !["loading", "cleaning"].includes(p.name))) {
    await phase.open(page);
    const { seen, missing } = await ringsOnTab(page);
    expect(seen, `${phase.name}: focusable controls`).toBeGreaterThan(0);
    report[phase.name] = missing;
  }
  expect(report).toEqual(Object.fromEntries(Object.keys(report).map((k) => [k, []])));
}

test("every control shows the focus ring when reached by keyboard (REQ-DCLEAN-014.A1, A2)", everyControlShowsTheRingOnTab);

async function pointerFocusShowsNoRing({ page }: { page: Page }) {
  await appToResults(page);
  for (const name of ["Safe only", "Select all", "Show items in System caches"]) {
    const b = page.getByRole("button", { name });
    await b.click();
    await expect(b).toBeFocused();
    const s = await b.evaluate((el) => [el.matches(":focus-visible"), getComputedStyle(el).outlineStyle]);
    expect(s).toEqual([false, "none"]); // no design ring, and no browser-default ring (A3)
  }
  const box = page.getByRole("checkbox", { name: "App leftovers" });
  await box.click();
  expect(await box.evaluate((el) => getComputedStyle(el).outlineStyle)).toBe("none");
}

test("pointer focus shows no ring, design or browser default (REQ-DCLEAN-014.A1, A3)", pointerFocusShowsNoRing);

// ── REQ-DCLEAN-013 / 015 · hover and pressed ─────────────────────

type StateCase = {
  what: string;
  target: (page: Page) => ReturnType<Page["locator"]>;
  prop: "backgroundColor" | "color" | "borderTopColor";
  hover: string;
  active: string;
};

/** Hover, then press without releasing over the control, so nothing activates. */
async function hoverAndPressed(page: Page, c: StateCase) {
  const el = c.target(page);
  await el.hover();
  const hovered = await el.evaluate((e, p) => getComputedStyle(e)[p as "color"], c.prop);
  await page.mouse.down();
  const pressed = await el.evaluate((e, p) => getComputedStyle(e)[p as "color"], c.prop);
  // Release off the control: a click fires only when press and release share a target.
  await page.mouse.move(1, 1);
  await page.mouse.up();
  return { hovered, pressed };
}

const IDLE_CASES: StateCase[] = [
  {
    what: "primary button",
    target: (p) => p.getByTestId("idle").getByRole("button", { name: "Start scan" }),
    prop: "backgroundColor",
    hover: "--color-primary-hover",
    active: "--color-primary-active",
  },
  {
    what: "secondary button",
    target: (p) => p.getByRole("button", { name: "Explore disk" }),
    prop: "backgroundColor",
    hover: "--overlay-hover",
    active: "--overlay-active",
  },
  {
    what: "shortcut card",
    target: (p) => p.getByTestId("shortcut-map"),
    prop: "backgroundColor",
    hover: "--color-neutral-200",
    active: "--color-shortcut-active",
  },
  {
    what: "nav entry",
    target: (p) => p.getByRole("button", { name: /^History/ }),
    prop: "backgroundColor",
    hover: "--overlay-hover",
    active: "--overlay-active",
  },
];

const RESULTS_CASES: StateCase[] = [
  {
    what: "ghost button",
    target: (p) => p.getByRole("button", { name: "Select all" }),
    prop: "backgroundColor",
    hover: "--overlay-ghost-hover",
    active: "--overlay-ghost-active",
  },
  {
    what: "icon button",
    target: (p) => p.getByRole("button", { name: "Show items in System caches" }),
    prop: "backgroundColor",
    hover: "--overlay-ghost-hover",
    active: "--overlay-ghost-active",
  },
  {
    what: "unchecked checkbox",
    target: (p) => p.getByRole("checkbox", { name: "App leftovers" }),
    prop: "borderTopColor",
    hover: "--color-check-hover",
    active: "--color-check-hover",
  },
  {
    what: "notice link",
    target: (p) => p.getByRole("button", { name: /Full Disk Access/ }),
    prop: "color",
    hover: "--color-link-hover",
    active: "--color-link-active",
  },
];

async function expectStates(page: Page, cases: StateCase[]) {
  for (const c of cases) {
    const got = await hoverAndPressed(page, c);
    expect({ what: c.what, ...got }).toEqual({
      what: c.what,
      hovered: await tokenColor(page, c.hover),
      pressed: await tokenColor(page, c.active),
    });
  }
}

async function controlsShowHoverAndPressedStates({ page }: { page: Page }) {
  await stubEngine(page, idleFixtureView());
  await openApp(page);
  await page.getByTestId("idle").waitFor();
  await expectStates(page, IDLE_CASES);

  await appToResults(page, { ...reviewFixtureView(), skipped_paths: 3 });
  await expectStates(page, RESULTS_CASES);
  // REQ-DCLEAN-013.A4: the category row hover.
  const row = page.locator(".result-row").first();
  await row.locator(".result-desc").hover();
  expect(await row.evaluate((e) => getComputedStyle(e).backgroundColor)).toBe(await tokenColor(page, "--overlay-row-hover"));

  // The mode switch: an unselected option's hover and pressed; the selected one's fill (015.A3).
  await page.getByTestId("clean").click();
  const del = page.locator('.seg-opt[data-mode="delete"]');
  await del.hover();
  expect(await del.evaluate((e) => getComputedStyle(e).backgroundColor)).toBe(await tokenColor(page, "--overlay-hover"));
  await page.mouse.down();
  expect(await del.evaluate((e) => getComputedStyle(e).backgroundColor)).toBe(await tokenColor(page, "--overlay-active"));
  // Release on the dialog's title: off the option, and not on the backdrop (which closes it).
  const title = (await page.locator(".confirm-title").boundingBox())!;
  await page.mouse.move(title.x + 4, title.y + 4);
  await page.mouse.up();
  const trash = page.locator('.seg-opt[data-mode="trash"]');
  const selected = await trash.evaluate((e) => [getComputedStyle(e).backgroundColor, getComputedStyle(e).color]);
  expect(selected).toEqual([await tokenColor(page, "--color-accent"), await tokenColor(page, "--color-on-accent")]);
}

test("each control class shows its hover and pressed state from tokens (REQ-DCLEAN-013, REQ-DCLEAN-015)", controlsShowHoverAndPressedStates);

/** REQ-DCLEAN-016: a disabled control ignores pointer and keyboard and keeps its fill. */
async function disabledCleanIgnoresPointerAndKeyboard({ page }: { page: Page }) {
  await appToResults(page);
  await page.getByRole("button", { name: "Select all" }).click();
  for (const box of await page.getByRole("checkbox").all()) await box.click(); // deselect everything
  const clean = page.getByTestId("clean");
  await expect(clean).toBeDisabled();
  const style = await clean.evaluate((el) => [getComputedStyle(el).opacity, getComputedStyle(el).cursor]);
  expect(style).toEqual(["0.45", "not-allowed"]);
  await clean.click({ force: true });
  await clean.evaluate((el) => el.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await clean.hover({ force: true });
  expect(await clean.evaluate((el) => getComputedStyle(el).backgroundColor)).toBe(await tokenColor(page, "--color-accent"));
}

test("a disabled Clean ignores pointer and keyboard (REQ-DCLEAN-016.A1, A2)", disabledCleanIgnoresPointerAndKeyboard);
