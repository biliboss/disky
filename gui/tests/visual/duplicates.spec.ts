import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, baselinePath, offset, openSnapshot } from "./snapshot";
import { WINDOW, openApp, stubDupes } from "./engineStub";

/**
 * REQ-DSCR-014 in a real browser: the Duplicates screen against the
 * snapshot rendered with startScreen = dupes at 1200 × 780 (within 1%),
 * axe, and the keyboard choosing a different copy with the arrow keys then
 * removing the extras (engine stubbed). Fixture groups are the snapshot's
 * DUPES (4 groups, 6 extra copies, 20.1 GB). Masked: the traffic-light slot
 * and the meter's live free-space number.
 */
test.describe.configure({ mode: "serial" });

const SNAPSHOT_LIGHTS = '[data-screen-label="App window"] > div:first-child > div:first-child';
const APP_LIGHTS = '[data-testid="traffic-lights"]';
const DIFF = { maxDiffPixelRatio: 0.01, maskColor: MASK_COLOR, animations: "disabled" as const };

async function openDupes(page: Page, noSnapshot = false) {
  await stubDupes(page, noSnapshot);
  await openApp(page, "#/duplicates");
  if (!noSnapshot) await page.getByRole("radiogroup", { name: "IMG_2041.HEIC" }).waitFor();
}

async function duplicatesMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page, { startScreen: "dupes" });
  await page.locator('[data-screen-label="Duplicates"]').waitFor();
  await page.screenshot({
    path: baselinePath("screen-dupes.png"),
    clip: offset(WINDOW, at),
    mask: [page.locator(SNAPSHOT_LIGHTS), page.locator('aside span:has-text("GB free")')],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await openDupes(page);
  await expect(page).toHaveScreenshot("screen-dupes.png", {
    ...DIFF,
    clip: WINDOW,
    mask: [page.locator(APP_LIGHTS), page.locator(".meter-free")],
  });
}

async function duplicatesPassesAxe({ page }: { page: Page }) {
  await openDupes(page);
  const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  const bad = result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  expect(bad).toEqual([]);
}

async function keyboardChoosesACopyThenRemovesExtras({ page }: { page: Page }) {
  await openDupes(page);
  const group = page.getByRole("radiogroup", { name: "Wedding 4K master.mov" });
  const copies = group.getByRole("radio");
  await expect(copies.nth(0)).toHaveAttribute("aria-checked", "true");
  // Tab reaches only the kept copy of each group (roving tabindex).
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press("Tab");
    if (await copies.nth(0).evaluate((el) => el === document.activeElement)) break;
  }
  await expect(copies.nth(0)).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(copies.nth(1)).toBeFocused();
  await expect(copies.nth(1)).toHaveAttribute("aria-checked", "true");
  await expect(copies.nth(0)).toHaveAttribute("aria-checked", "false");
  await expect(copies.nth(0).locator(".tag")).toHaveText("Remove");
  await expect(copies.nth(1).locator(".tag")).toHaveText("Keep");

  const remove = page.getByRole("button", { name: "Remove 6 extra copies" });
  await remove.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("dupe-group")).toHaveCount(0);
  const trashed: string[][] = await page.evaluate(() => (window as any).__trashed);
  expect(trashed).toHaveLength(1);
  expect(trashed[0]).toContain("/Users/me/Movies/Exports/Wedding 4K master.mov");
  expect(trashed[0]).not.toContain("/Users/me/Documents/Backups/Video/Wedding 4K master.mov");
  expect(trashed[0]).toHaveLength(6);
  await expect(page.getByText("No duplicates found.")).toBeVisible();
  await expect(page.getByTestId("dupes-kicker")).toHaveText("0 groups · one copy of each is kept, you choose which");
}

async function noSnapshotOffersStartScan({ page }: { page: Page }) {
  await openDupes(page, true);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("No scans yet");
  await expect(page.getByRole("button", { name: "Start scan" })).toBeVisible();
}

test("Duplicates matches the snapshot within 1% (REQ-DSCR-014.A1)", duplicatesMatchesSnapshot);
test("Duplicates has zero serious or critical axe violations (REQ-DSCR-014)", duplicatesPassesAxe);
test("the arrow keys choose a different copy, then Remove trashes the extras (REQ-DSCR-014.A3, A4)", keyboardChoosesACopyThenRemovesExtras);
test("no snapshot shows No scans yet with Start scan (REQ-DSCR-014.A5)", noSnapshotOffersStartScan);
