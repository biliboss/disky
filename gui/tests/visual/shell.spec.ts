import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, REGIONS, baselinePath, offset, openSnapshot } from "./snapshot";
import { snapshotReviewCategories } from "../fixtures/snapshotReview";

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

async function openApp(page: Page, hash = "#/") {
  await page.goto(`/${hash}`);
  await page.locator(".sidebar").waitFor();
  await page.evaluate(() => document.fonts.ready);
}

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

async function placeholderScreensKeepTheShell({ page }: { page: Page }) {
  for (const [hash, title] of [
    ["#/assistant", "Assistant"],
    ["#/settings", "Settings"],
  ]) {
    await openApp(page, hash);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(title);
    await expect(page.getByText("Coming soon")).toBeVisible();
    await expect(page.locator(".sidebar")).toBeVisible();
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
test("placeholder screens keep the shell (REQ-DCLEAN-032.A3)", placeholderScreensKeepTheShell);
test("renders offline with bundled fonts and icons (REQ-DCLEAN-032.A4)", rendersOfflineWithBundledFontsAndIcons);

const WINDOW = { x: 0, y: 0, width: 1200, height: 780 };

/**
 * The snapshot prototype's idle state as an engine view: 74 of 512 GB free,
 * 80 GB goal, last scan 7 days ago, 8 files over 2 GB.
 */
function idleFixtureView() {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: new Date(Date.now() - 7 * 86_400_000 - 60_000).toISOString(),
    snapshot_taken_at: null,
    is_stale: false,
    preferences: { trash_by_default: true, safe_only: true, notifications: true, assistant: true },
    large_files_count: 8,
    categories: [],
    catalogue: SNAPSHOT_SCAN_ROWS.map((name, i) => ({ id: `visual_cat_${i}`, name, icon: "folder", safety: "safe" })),
  };
}

/** The snapshot prototype's eight scanning rows, as catalogue entries. */
const SNAPSHOT_SCAN_ROWS = [
  "System caches",
  "App caches",
  "Temporary files",
  "Logs & crash reports",
  "Trash",
  "App leftovers",
  "Old downloads",
  "Developer junk",
];

/**
 * Answer the app's Tauri `invoke` calls in the browser build with `view`.
 * With `scanForever`, `scan` never resolves and progress subscriptions are
 * accepted, so the app holds the scanning phase. With `scanResult`, `scan`
 * resolves to that view at once, so the app enters the results phase.
 */
async function stubEngine(page: Page, view: unknown, scanForever = false, scanResult: unknown = null) {
  await page.addInitScript(
    ([v, forever, result]) => {
      (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string) => {
          if (cmd === "load_cleanup_view") return v;
          if (forever && cmd === "scan") return new Promise(() => undefined);
          if (result && cmd === "scan") return result;
          if ((forever || result) && cmd === "plugin:event|listen") return 1;
          if (result && cmd === "plugin:event|unlisten") return null;
          throw new Error(`visual stub: ${cmd} is not available`);
        },
        transformCallback: () => 0,
      };
    },
    [view, scanForever, scanResult] as const,
  );
}

/**
 * Live-data text on the idle screen (numbers and data-driven copy), masked on
 * both sides. The snapshot's Assistant line shows its 4 fixture suggestions;
 * the app shows "Watching your space goal" until the Assistant spec lands.
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

/** The snapshot's review data as the engine's results view. */
function reviewFixtureView() {
  return { ...idleFixtureView(), categories: snapshotReviewCategories() };
}

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

// Full-phase comparisons (REQ-DCLEAN-032.A1). Parked until each Smart Clean
// phase REQ is implemented; un-fixme as the phase lands.
const PHASES = [
  ["cleaning", "REQ-DCLEAN-008"],
  ["done", "REQ-DCLEAN-009"],
] as const;

for (const [phase, req] of PHASES) {
  test.fixme(`Smart Clean ${phase} phase matches the snapshot within 1% (REQ-DCLEAN-032.A1, ${req})`, async ({ page }) => {
    // Drive the snapshot to `phase`, capture the full 1200 × 780 window as the
    // baseline, drive the app to the same phase with fixture data, then:
    await openApp(page);
    await expect(page).toHaveScreenshot(`phase-${phase}.png`, {
      ...SHELL_DIFF,
      clip: WINDOW,
      mask: [page.locator(APP_LIGHTS)],
    });
  });
}
