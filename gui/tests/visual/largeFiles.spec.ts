import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, baselinePath, offset, openSnapshot } from "./snapshot";
import { WINDOW, openApp, stubLarge } from "./engineStub";

/**
 * REQ-DSCR-013 in a real browser: the Large files screen against the
 * snapshot rendered with startScreen = large at 1200 × 780 (within 1%),
 * axe, and the keyboard selecting two rows and trashing them (engine
 * stubbed). Fixture rows are the snapshot's LARGE (8 files, 111 GB).
 * Masked: the traffic-light slot, the meter's live free-space number, the
 * relative "Last opened" tags (the app computes them from real dates) and
 * the footer button's live selected total.
 */
test.describe.configure({ mode: "serial" });

const SNAPSHOT_LIGHTS = '[data-screen-label="App window"] > div:first-child > div:first-child';
const APP_LIGHTS = '[data-testid="traffic-lights"]';
const DIFF = { maxDiffPixelRatio: 0.01, maskColor: MASK_COLOR, animations: "disabled" as const };

async function openLarge(page: Page, noSnapshot = false) {
  await stubLarge(page, noSnapshot);
  await openApp(page, "#/large-files");
  if (!noSnapshot) await page.getByRole("checkbox", { name: "Windows 11 VM.utm" }).waitFor();
}

async function largeFilesMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page, { startScreen: "large" });
  const shot = page.locator('[data-screen-label="Large files"]');
  await shot.waitFor();
  await page.screenshot({
    path: baselinePath("screen-large.png"),
    clip: offset(WINDOW, at),
    mask: [
      page.locator(SNAPSHOT_LIGHTS),
      page.locator('aside span:has-text("GB free")'),
      shot.locator("tbody td:nth-child(4) span"),
      shot.locator("button.btn-primary"),
    ],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await openLarge(page);
  await expect(page).toHaveScreenshot("screen-large.png", {
    ...DIFF,
    clip: WINDOW,
    mask: [
      page.locator(APP_LIGHTS),
      page.locator(".meter-free"),
      page.getByTestId("large-opened"),
      page.locator(".large-trash"),
    ],
  });
}

async function largeFilesPassesAxe({ page }: { page: Page }) {
  await openLarge(page);
  const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  const bad = result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  expect(bad).toEqual([]);
}

async function keyboardSelectsTwoRowsAndTrashesThem({ page }: { page: Page }) {
  await openLarge(page);
  const vm = page.getByRole("checkbox", { name: "Windows 11 VM.utm" });
  const wedding = page.getByRole("checkbox", { name: "Wedding 4K master.mov" });
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press("Tab");
    if (await vm.evaluate((el) => el === document.activeElement)) break;
  }
  await expect(vm).toBeFocused();
  await page.keyboard.press("Space");
  await expect(vm).toHaveAttribute("aria-checked", "true");
  // Two Tabs on: Photo library backup, then Wedding 4K master.
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  await expect(wedding).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(wedding).toHaveAttribute("aria-checked", "true");
  await expect(page.getByTestId("large-note")).toHaveText("2 selected · restorable from History for 30 days");

  const trash = page.getByRole("button", { name: "Move 56.6 GB to Trash" });
  await trash.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("large-row")).toHaveCount(6);
  await expect(vm).toHaveCount(0);
  await expect(wedding).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__trashed)).toEqual([
    ["/Users/me/Library/Containers/UTM/Windows 11 VM.utm", "/Users/me/Movies/Exports/Wedding 4K master.mov"],
  ]);
  await expect(page.getByTestId("large-kicker")).toHaveText("6 files · 54.3 GB");
  await expect(page.getByTestId("large-note")).toHaveText("Select files to move them to the Trash.");
  await expect(page.locator(".large-trash")).toBeDisabled();
}

async function noSnapshotOffersStartScan({ page }: { page: Page }) {
  await openLarge(page, true);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("No scans yet");
  await expect(page.getByRole("button", { name: "Start scan" })).toBeVisible();
}

test("Large files matches the snapshot within 1% (REQ-DSCR-013.A1)", largeFilesMatchesSnapshot);
test("Large files has zero serious or critical axe violations (REQ-DSCR-013)", largeFilesPassesAxe);
test("the keyboard selects two rows and trashes them (REQ-DSCR-013.A3, A4)", keyboardSelectsTwoRowsAndTrashesThem);
test("no snapshot shows No scans yet with Start scan (REQ-DSCR-013.A5)", noSnapshotOffersStartScan);
