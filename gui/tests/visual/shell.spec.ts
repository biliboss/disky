import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, REGIONS, baselinePath, offset, openSnapshot } from "./snapshot";

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
  };
}

/** Answer the app's Tauri `invoke` calls in the browser build with `view`. */
async function stubEngine(page: Page, view: unknown) {
  await page.addInitScript((v) => {
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === "load_cleanup_view") return v;
        throw new Error(`visual stub: ${cmd} is not available`);
      },
      transformCallback: () => 0,
    };
  }, view);
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

// Full-phase comparisons (REQ-DCLEAN-032.A1). Parked until each Smart Clean
// phase REQ is implemented; un-fixme as the phase lands.
const PHASES = [
  ["scanning", "REQ-DCLEAN-004"],
  ["results", "REQ-DCLEAN-005"],
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
