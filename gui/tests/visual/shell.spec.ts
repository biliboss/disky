import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, REGIONS, baselinePath, offset, openSnapshot } from "./snapshot";
import { WINDOW, doneFixtureResult, idleFixtureView, openApp, reviewFixtureView, stubEngine } from "./engineStub";

/**
 * REQ-DCLEAN-032 visual contract. The baseline is generated from
 * specs/disk-cleanup/snapshot.html by the first test; the shell tests compare
 * the built app against it. Masked: the traffic-light slot (native lights in
 * the app, drawn dots in the snapshot).
 */
test.describe.configure({ mode: "serial" });

const SNAPSHOT_LIGHTS = '[data-screen-label="App window"] > div:first-child > div:first-child';
const APP_LIGHTS = '[data-testid="traffic-lights"]';
const SHELL_DIFF = { maxDiffPixelRatio: 0.01, maskColor: MASK_COLOR, animations: "disabled" as const };

async function snapshotBaselineRenders({ page }: { page: Page }) {
  const at = await openSnapshot(page);
  for (const [name, region] of Object.entries(REGIONS)) {
    await page.screenshot({
      path: baselinePath(`${name}.png`),
      clip: offset(region, at),
      mask: [page.locator(SNAPSHOT_LIGHTS)],
      maskColor: MASK_COLOR,
      animations: "disabled",
    });
  }
  await expect(page.locator("aside")).toContainText("disky");
  await expect(page.locator("body")).not.toContainText("Diskee");
}

async function titleBarMatchesSnapshot({ page }: { page: Page }) {
  await openApp(page);
  await expect(page).toHaveScreenshot("titlebar.png", {
    ...SHELL_DIFF,
    clip: REGIONS.titlebar,
    mask: [page.locator(APP_LIGHTS)],
  });
}

async function sidebarMatchesSnapshot({ page }: { page: Page }) {
  await openApp(page);
  await expect(page).toHaveScreenshot("sidebar.png", { ...SHELL_DIFF, clip: REGIONS.sidebar });
}

async function trafficLightSlotSitsWhereSnapshotDrawsDots({ page }: { page: Page }) {
  await openApp(page);
  expect(await page.locator(APP_LIGHTS).boundingBox()).toEqual({ x: 16, y: 14, width: 52, height: 12 });
}

async function everyScreenKeepsTheShell({ page }: { page: Page }) {
  // Every sidebar entry is a real screen now (REQ-DSCR-017.A3): none reads
  // "Coming soon", and each keeps the title bar and sidebar with its own
  // entry current.
  for (const [hash, label] of [
    ["#/assistant", "Assistant"],
    ["#/space-map", "Space Map"],
    ["#/large-files", "Large files"],
    ["#/duplicates", "Duplicates"],
    ["#/history", "History"],
    ["#/settings", "Settings"],
  ]) {
    await openApp(page, hash);
    await expect(page.locator(".sidebar")).toBeVisible();
    await expect(page.locator(APP_LIGHTS)).toBeVisible();
    await expect(page.locator('.nav-item[aria-current="page"] .nav-label')).toHaveText(label);
    await expect(page.getByText("Coming soon")).toHaveCount(0);
  }
}

async function rendersOfflineWithBundledFontsAndIcons({ page, context }: { page: Page; context: import("@playwright/test").BrowserContext }) {
  const blocked: string[] = [];
  await context.route("**/*", (route) => {
    const url = new URL(route.request().url());
    const local = url.protocol === "data:" || ["localhost", "127.0.0.1"].includes(url.hostname);
    if (local) return route.continue();
    blocked.push(url.href);
    return route.abort();
  });
  await openApp(page);

  const fonts = await page.evaluate(async () => {
    await document.fonts.load("16px Caprasimo");
    await document.fonts.load("16px Figtree");
    await document.fonts.ready;
    const loaded = (family: string) =>
      [...document.fonts].some((f) => f.family.replace(/["']/g, "") === family && f.status === "loaded");
    return {
      caprasimoCheck: document.fonts.check("16px Caprasimo"),
      figtreeCheck: document.fonts.check("16px Figtree"),
      caprasimoLoaded: loaded("Caprasimo"),
      figtreeLoaded: loaded("Figtree"),
    };
  });
  expect(fonts).toEqual({ caprasimoCheck: true, figtreeCheck: true, caprasimoLoaded: true, figtreeLoaded: true });
  await expect(page.locator(".nav-item svg.nav-icon")).toHaveCount(7);
  expect(blocked).toEqual([]);
}

test("snapshot baseline renders with the disky brand (REQ-DCLEAN-032.A1)", snapshotBaselineRenders);
test("title bar matches the snapshot within 1% (REQ-DCLEAN-032.A1)", titleBarMatchesSnapshot);
test("sidebar matches the snapshot within 1% (REQ-DCLEAN-032.A1, A3)", sidebarMatchesSnapshot);
test("traffic-light slot sits where the snapshot draws its dots (REQ-DCLEAN-032.A1)", trafficLightSlotSitsWhereSnapshotDrawsDots);
test("every screen keeps the shell, none is a placeholder (REQ-DCLEAN-032.A3, REQ-DSCR-017.A3)", everyScreenKeepsTheShell);
test("renders offline with bundled fonts and icons (REQ-DCLEAN-032.A4)", rendersOfflineWithBundledFontsAndIcons);

/**
 * Live-data text on the idle screen (numbers and data-driven copy), masked on
 * both sides. The Assistant line counts the engine's actionable suggestions
 * (REQ-DSCR-009.A5), which the idle stub does not load.
 */
const SNAPSHOT_IDLE_LIVE = (page: Page) => {
  // The prototype renders each `{{ value }}` in an inner inline wrapper; mask
  // its enclosing element so both sides mask the same boxes.
  const outer = (sel: string) => page.locator(sel).locator("xpath=..");
  return [
    page.locator('main div:text-is("Last scan 7 days ago")'),
    outer('main span:text-is("74 GB")'),
    page.locator('main span:text-is("free of 512 GB")'),
    outer('main span:text-is("Below your goal")'),
    outer('main span:text-is("4 suggestions waiting")'),
    outer('main span:text-is("8 files over 2 GB")'),
    page.locator('aside span:has-text("GB free")'),
  ];
};
const APP_IDLE_LIVE = [
  '[data-testid="last-scan"]',
  '[data-testid="ring-free"]',
  '[data-testid="ring-total"]',
  '[data-testid="goal-tag"]',
  '[data-testid="shortcut-assistant-desc"]',
  '[data-testid="shortcut-large-desc"]',
  ".meter-free",
];

async function idlePhaseMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page);
  // The below-goal notification is REQ-DCLEAN-010's; dismiss it the way the
  // prototype does so the idle screen itself is compared.
  await page.getByRole("button", { name: "Later" }).click();
  await page.locator('[data-screen-label="Smart clean · home"]').waitFor();
  await page.screenshot({
    path: baselinePath("phase-idle.png"),
    clip: offset(WINDOW, at),
    mask: [page.locator(SNAPSHOT_LIGHTS), ...SNAPSHOT_IDLE_LIVE(page)],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await stubEngine(page, idleFixtureView());
  await openApp(page);
  await page.getByTestId("idle").waitFor();
  await expect(page).toHaveScreenshot("phase-idle.png", {
    ...SHELL_DIFF,
    clip: WINDOW,
    mask: [page.locator(APP_LIGHTS), ...APP_IDLE_LIVE.map((s) => page.locator(s))],
  });
}

test("Smart Clean idle phase matches the snapshot within 1% (REQ-DCLEAN-032.A1, REQ-DCLEAN-003)", idlePhaseMatchesSnapshot);

/** The snapshot's scanning screen and its live parts (path line, rows, fill). */
const SNAPSHOT_SCANNING = '[data-screen-label="Smart clean · scanning"]';
const SNAPSHOT_SCANNING_LIVE = (page: Page) => {
  const scr = page.locator(SNAPSHOT_SCANNING);
  return [
    scr.locator("> div").nth(0), // path line + percentage
    scr.locator("> div").nth(1).locator("> div"), // progress fill
    scr.locator("> div").nth(2), // category rows
  ];
};
const APP_SCANNING_LIVE = ['[data-testid="scan-path"]', ".scan-meta", '[data-testid="scan-bar-fill"]', '[data-testid="scan-rows"]'];

async function scanningPhaseMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page);
  await page.getByRole("button", { name: "Later" }).click();
  await page.locator('[data-screen-label="Smart clean · home"]').waitFor();
  // Start the prototype's scan and freeze its simulated timer at 0%, so the
  // screen holds still (the live parts are masked either way).
  await page.evaluate(() => {
    const start = [...document.querySelectorAll("main button")].find((b) => b.textContent?.trim() === "Start scan");
    (start as HTMLButtonElement).click();
    for (let id = 0; id < 10_000; id++) clearInterval(id);
  });
  await page.locator(SNAPSHOT_SCANNING).waitFor();
  await page.screenshot({
    path: baselinePath("phase-scanning.png"),
    clip: offset(WINDOW, at),
    mask: [page.locator(SNAPSHOT_LIGHTS), ...SNAPSHOT_SCANNING_LIVE(page), page.locator('aside span:has-text("GB free")')],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await stubEngine(page, idleFixtureView(), true);
  await openApp(page);
  await page.getByTestId("idle").getByRole("button", { name: "Start scan" }).click();
  await page.getByTestId("scanning").waitFor();
  await expect(page).toHaveScreenshot("phase-scanning.png", {
    ...SHELL_DIFF,
    clip: WINDOW,
    mask: [page.locator(APP_LIGHTS), ...APP_SCANNING_LIVE.map((s) => page.locator(s)), page.locator(".meter-free")],
  });

  // REQ-DCLEAN-004.A5: accent fill over a surface track (the fill is masked above).
  const colours = await page.evaluate(() => {
    const resolve = (token: string) => {
      const probe = document.createElement("div");
      probe.style.background = `var(${token})`;
      document.body.append(probe);
      const c = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return c;
    };
    return {
      fill: getComputedStyle(document.querySelector(".scan-bar-fill")!).backgroundColor,
      track: getComputedStyle(document.querySelector(".scan-bar")!).backgroundColor,
      accent: resolve("--color-accent"),
      surface: resolve("--color-surface"),
    };
  });
  expect(colours.fill).toBe(colours.accent);
  expect(colours.track).toBe(colours.surface);
}

test("Smart Clean scanning phase matches the snapshot within 1% (REQ-DCLEAN-032.A1, REQ-DCLEAN-004)", scanningPhaseMatchesSnapshot);

/** The snapshot's review screen. */
const SNAPSHOT_REVIEW = '[data-screen-label="Smart clean · review"]';

/**
 * Category sizes on the review screen, masked on both sides: the prototype
 * formats MB with 1024 (0.6 GB → "614 MB") where the app uses decimal units
 * (REQ-DCLEAN-028), so the figures differ by design.
 */
const SNAPSHOT_REVIEW_SIZES = ["4.2 GB", "3.1 GB", "1.8 GB", "614 MB", "2.4 GB", "2.7 GB", "5.3 GB", "7.9 GB"];

async function resultsPhaseMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page);
  await page.getByRole("button", { name: "Later" }).click();
  await page.locator('[data-screen-label="Smart clean · home"]').waitFor();
  // Run the prototype's simulated scan to completion (≈ 3.6 s + 350 ms).
  await page.locator("main").getByRole("button", { name: "Start scan" }).click();
  await page.locator(SNAPSHOT_REVIEW).waitFor({ timeout: 15_000 });
  const review = page.locator(SNAPSHOT_REVIEW);
  await page.screenshot({
    path: baselinePath("phase-results.png"),
    clip: offset(WINDOW, at),
    mask: [
      page.locator(SNAPSHOT_LIGHTS),
      ...SNAPSHOT_REVIEW_SIZES.map((t) => review.locator(`span:text-is("${t}")`).first().locator("xpath=..")),
      page.locator('aside span:has-text("GB free")'),
    ],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await stubEngine(page, idleFixtureView(), false, reviewFixtureView());
  await openApp(page);
  await page.getByTestId("idle").getByRole("button", { name: "Start scan" }).click();
  await page.getByTestId("results").waitFor();
  await expect(page.getByTestId("categories").getByRole("listitem")).toHaveCount(8);
  await expect(page).toHaveScreenshot("phase-results.png", {
    ...SHELL_DIFF,
    clip: WINDOW,
    mask: [page.locator(APP_LIGHTS), page.locator('[data-testid="category-size"]'), page.locator(".meter-free")],
  });
}

test("Smart Clean results phase matches the snapshot within 1% (REQ-DCLEAN-032.A1, REQ-DCLEAN-005)", resultsPhaseMatchesSnapshot);

/**
 * Live numbers on the confirm dialog, masked on both sides: the title total
 * and each listed category's size (the prototype formats MB with 1024).
 */
const SNAPSHOT_CONFIRM_LIVE = (page: Page) => {
  const dialog = page.locator('[role="dialog"]');
  return [dialog.locator(".dialog-title"), dialog.locator(".dialog-body > div > span:last-child")];
};
const APP_CONFIRM_LIVE = [".confirm-title", ".confirm-row > span:last-child"];

async function confirmDialogMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page);
  await page.getByRole("button", { name: "Later" }).click();
  await page.locator('[data-screen-label="Smart clean · home"]').waitFor();
  await page.locator("main").getByRole("button", { name: "Start scan" }).click();
  const review = page.locator(SNAPSHOT_REVIEW);
  await review.waitFor({ timeout: 15_000 });
  await page.locator("main").getByRole("button", { name: /^Clean / }).click();
  await page.locator('[role="dialog"]').waitFor();
  await page.screenshot({
    path: baselinePath("confirm-dialog.png"),
    clip: offset(WINDOW, at),
    mask: [
      page.locator(SNAPSHOT_LIGHTS),
      ...SNAPSHOT_REVIEW_SIZES.map((t) => review.locator(`span:text-is("${t}")`).first().locator("xpath=..")),
      ...SNAPSHOT_CONFIRM_LIVE(page),
      page.locator('aside span:has-text("GB free")'),
    ],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await stubEngine(page, idleFixtureView(), false, reviewFixtureView());
  await openApp(page);
  await page.getByTestId("idle").getByRole("button", { name: "Start scan" }).click();
  await page.getByTestId("results").waitFor();
  await page.getByTestId("clean").click();
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  // REQ-DCLEAN-022.A2: focus opens on Cancel. Blur it for the pixel compare so
  // neither side shows a focus ring the other lacks.
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await expect(page).toHaveScreenshot("confirm-dialog.png", {
    ...SHELL_DIFF,
    clip: WINDOW,
    mask: [
      page.locator(APP_LIGHTS),
      page.locator('[data-testid="category-size"]'),
      ...APP_CONFIRM_LIVE.map((s) => page.locator(s)),
      page.locator(".meter-free"),
    ],
  });

  // REQ-DCLEAN-020.A3 in a real browser: arrow keys move between the modes.
  await dialog.getByRole("radio", { name: "Move to Trash" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(dialog.getByRole("radio", { name: "Delete permanently" })).toBeChecked();
  await expect(dialog.getByRole("radio", { name: "Delete permanently" })).toBeFocused();
  await expect(dialog.getByRole("button", { name: "Clean now" })).toHaveClass(/btn-danger/);
  // REQ-DCLEAN-007.A6: the danger fill resolves to --color-accent-800.
  const fills = await page.evaluate(() => {
    const probe = document.createElement("div");
    probe.style.background = "var(--color-accent-800)";
    document.body.append(probe);
    const accent800 = getComputedStyle(probe).backgroundColor;
    probe.remove();
    const now = [...document.querySelectorAll("[role=dialog] button")].find((b) => b.textContent === "Clean now")!;
    return { accent800, now: getComputedStyle(now).backgroundColor };
  });
  expect(fills.now).toBe(fills.accent800);
  // REQ-DCLEAN-022.A3: Tab stays inside the dialog.
  for (let i = 0; i < 4; i++) {
    await page.keyboard.press("Tab");
    expect(await dialog.evaluate((d) => d.contains(document.activeElement))).toBe(true);
  }
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.getByTestId("clean")).toBeFocused();
}

test("confirm dialog matches the snapshot within 1% (REQ-DCLEAN-032.A1, REQ-DCLEAN-007, REQ-DCLEAN-022)", confirmDialogMatchesSnapshot);

/** Drive the snapshot prototype from idle to its review screen. */
async function snapshotToReview(page: Page) {
  const at = await openSnapshot(page);
  await page.getByRole("button", { name: "Later" }).click();
  await page.locator('[data-screen-label="Smart clean · home"]').waitFor();
  await page.locator("main").getByRole("button", { name: "Start scan" }).click();
  await page.locator(SNAPSHOT_REVIEW).waitFor({ timeout: 15_000 });
  return at;
}

/** Drive the app from idle through results and the dialog to "Clean now". */
async function appCleanNow(page: Page) {
  await openApp(page);
  await page.getByTestId("idle").getByRole("button", { name: "Start scan" }).click();
  await page.getByTestId("results").waitFor();
  await page.getByTestId("clean").click();
  await page.getByRole("dialog").getByRole("button", { name: "Clean now" }).click();
}

const SNAPSHOT_CLEANING = '[data-screen-label="Smart clean · cleaning"]';

async function cleaningPhaseMatchesSnapshot({ page }: { page: Page }) {
  const at = await snapshotToReview(page);
  await page.locator("main").getByRole("button", { name: /^Clean / }).click();
  await page.locator('[role="dialog"]').getByRole("button", { name: "Clean now" }).click();
  // Freeze the prototype's simulated clean so the screen holds still.
  await page.evaluate(() => {
    for (let id = 0; id < 10_000; id++) clearInterval(id);
  });
  const scr = page.locator(SNAPSHOT_CLEANING);
  await scr.waitFor();
  await page.screenshot({
    path: baselinePath("phase-cleaning.png"),
    clip: offset(WINDOW, at),
    mask: [
      page.locator(SNAPSHOT_LIGHTS),
      scr.locator("> div").nth(0), // step label + percentage
      scr.locator("> div").nth(1).locator("> div"), // progress fill
      page.locator('aside span:has-text("GB free")'),
    ],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await stubEngine(page, idleFixtureView(), false, reviewFixtureView(), "forever");
  await appCleanNow(page);
  await page.getByTestId("cleaning").waitFor();
  await expect(page.getByTestId("clean-step")).toHaveText("Moving System caches…");
  await expect(page).toHaveScreenshot("phase-cleaning.png", {
    ...SHELL_DIFF,
    clip: WINDOW,
    mask: [
      page.locator(APP_LIGHTS),
      page.getByTestId("clean-meta"),
      page.getByTestId("clean-bar-fill"),
      page.locator(".meter-free"),
    ],
  });

  // REQ-DCLEAN-008.A2: accent-2 fill (masked above) over the surface track.
  const colours = await page.evaluate(() => {
    const probe = document.createElement("div");
    probe.style.background = "var(--color-accent-2)";
    document.body.append(probe);
    const accent2 = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return { fill: getComputedStyle(document.querySelector(".clean-bar-fill")!).backgroundColor, accent2 };
  });
  expect(colours.fill).toBe(colours.accent2);
}

test("Smart Clean cleaning phase matches the snapshot within 1% (REQ-DCLEAN-032.A1, REQ-DCLEAN-008)", cleaningPhaseMatchesSnapshot);

const SNAPSHOT_DONE = '[data-screen-label="Smart clean · done"]';

async function donePhaseMatchesSnapshot({ page }: { page: Page }) {
  const at = await snapshotToReview(page);
  await page.locator("main").getByRole("button", { name: /^Clean / }).click();
  await page.locator('[role="dialog"]').getByRole("button", { name: "Clean now" }).click();
  // The prototype's simulated clean takes ≈ 2 s.
  const done = page.locator(SNAPSHOT_DONE);
  await done.waitFor({ timeout: 15_000 });
  await page.waitForTimeout(1_000); // let the ring's .8 s transition settle
  await page.screenshot({
    path: baselinePath("phase-done.png"),
    clip: offset(WINDOW, at),
    mask: [
      page.locator(SNAPSHOT_LIGHTS),
      done.locator("h1"),
      done.locator('span:text-is("now free")').locator("xpath=preceding-sibling::span"),
      page.locator('aside span:has-text("GB free")'),
    ],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await stubEngine(page, idleFixtureView(), false, reviewFixtureView(), doneFixtureResult());
  await appCleanNow(page);
  await page.getByTestId("done").waitFor();
  // REQ-DCLEAN-009.A3: a Trash clean offers Undo, as the snapshot shows.
  await expect(page.getByTestId("undo")).toBeVisible();
  await page.waitForTimeout(1_000);
  await expect(page).toHaveScreenshot("phase-done.png", {
    ...SHELL_DIFF,
    clip: WINDOW,
    mask: [
      page.locator(APP_LIGHTS),
      page.getByTestId("done-heading"),
      page.locator(".done-rail [data-testid=ring-free]"),
      page.locator(".meter-free"),
    ],
  });
}

test("Smart Clean done phase matches the snapshot within 1% (REQ-DCLEAN-032.A1, REQ-DCLEAN-009)", donePhaseMatchesSnapshot);

/**
 * The below-goal notification over the idle screen (REQ-DCLEAN-010), compared
 * with the snapshot's toast as it opens. Masked on both sides: the toast text
 * (its free / goal / safe figures) and the idle screen's live figures.
 */
const SNAPSHOT_TOAST = 'main [role="status"]';
async function idleNotificationMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page);
  await page.locator(SNAPSHOT_TOAST).waitFor();
  await page.screenshot({
    path: baselinePath("phase-idle-notification.png"),
    clip: offset(WINDOW, at),
    mask: [
      page.locator(SNAPSHOT_LIGHTS),
      ...SNAPSHOT_IDLE_LIVE(page),
      page.locator(SNAPSHOT_TOAST).locator("> div").nth(1),
    ],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await stubEngine(page, reviewFixtureView());
  await openApp(page);
  await page.getByTestId("goal-notification").waitFor();
  await expect(page).toHaveScreenshot("phase-idle-notification.png", {
    ...SHELL_DIFF,
    clip: WINDOW,
    mask: [
      page.locator(APP_LIGHTS),
      ...APP_IDLE_LIVE.map((s) => page.locator(s)),
      page.getByTestId("goal-notification-text"),
    ],
  });
  // It paints above the ring it overlaps, as the snapshot's toast does.
  const onTop = await page.getByTestId("goal-notification-text").evaluate((el) => {
    const r = el.getBoundingClientRect();
    return el.contains(document.elementFromPoint(r.left + r.width / 2, r.bottom - 4));
  });
  expect(onTop).toBe(true);
  // A5: it appeared without taking focus.
  expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
}

test("idle with the below-goal notification matches the snapshot within 1% (REQ-DCLEAN-010)", idleNotificationMatchesSnapshot);

/** Keyboard activation of a blocked nav entry changes nothing (REQ-DCLEAN-011.A1). */
async function blockedNavIgnoresTheKeyboard({ page }: { page: Page }) {
  await stubEngine(page, idleFixtureView(), true);
  await openApp(page);
  await page.getByRole("button", { name: "Start scan" }).click();
  await page.getByTestId("scanning").waitFor();
  const url = page.url();
  const history = page.getByRole("button", { name: /^History/ });
  await expect(history).toHaveAttribute("aria-disabled", "true");
  await history.focus();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Space");
  // Playwright refuses to click an aria-disabled element; force the pointer event.
  await history.click({ force: true });
  expect(page.url()).toBe(url);
  await expect(page.getByTestId("scanning")).toBeVisible();
  // Disabled treatment: the design system's 45% opacity and a not-allowed cursor.
  const style = await history.evaluate((el) => [getComputedStyle(el).opacity, getComputedStyle(el).cursor]);
  expect(style).toEqual(["0.45", "not-allowed"]);
}

test("blocked navigation ignores pointer and keyboard while scanning (REQ-DCLEAN-011.A1, A2)", blockedNavIgnoresTheKeyboard);
