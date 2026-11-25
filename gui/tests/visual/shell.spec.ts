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

// Full-phase comparisons (REQ-DCLEAN-032.A1). Parked until each Smart Clean
// phase REQ is implemented; un-fixme as the phase lands.
const PHASES = [
  ["idle", "REQ-DCLEAN-003"],
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
      clip: { x: 0, y: 0, width: 1200, height: 780 },
      mask: [page.locator(APP_LIGHTS)],
    });
  });
}
