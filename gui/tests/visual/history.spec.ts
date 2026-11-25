import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, baselinePath, offset, openSnapshot } from "./snapshot";
import { WINDOW, openApp, stubHistory } from "./engineStub";

/**
 * REQ-DSCR-015 in a real browser: the History screen against the snapshot
 * rendered with startScreen = history at 1200 × 780 (within 1%), axe, and
 * the keyboard restoring a row. Fixture rows are the snapshot's three
 * entries (same titles, details and bytes). Masked: the traffic-light slot,
 * the meter's live free-space number, and each row's "<when> · <detail>"
 * line — the snapshot prints fixed dates ("Nov 17"), the app relative ones.
 */
test.describe.configure({ mode: "serial" });

const SNAPSHOT_LIGHTS = '[data-screen-label="App window"] > div:first-child > div:first-child';
const APP_LIGHTS = '[data-testid="traffic-lights"]';
const DIFF = { maxDiffPixelRatio: 0.01, maskColor: MASK_COLOR, animations: "disabled" as const };

async function openHistory(page: Page) {
  await stubHistory(page);
  await openApp(page, "#/history");
  await page.getByTestId("history-row").first().waitFor();
}

async function historyMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page, { startScreen: "history" });
  await page.locator('[data-screen-label="History"]').waitFor();
  await page.screenshot({
    path: baselinePath("screen-history.png"),
    clip: offset(WINDOW, at),
    mask: [
      page.locator(SNAPSHOT_LIGHTS),
      page.locator('aside span:has-text("GB free")'),
      page.locator('[data-screen-label="History"] span:has-text(" · ")'),
    ],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await openHistory(page);
  await expect(page).toHaveScreenshot("screen-history.png", {
    ...DIFF,
    clip: WINDOW,
    mask: [page.locator(APP_LIGHTS), page.locator(".meter-free"), page.locator(".history-meta")],
  });
}

async function historyPassesAxe({ page }: { page: Page }) {
  await openHistory(page);
  const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  const bad = result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  expect(bad).toEqual([]);
}

async function keyboardRestoresARow({ page }: { page: Page }) {
  await openHistory(page);
  const restore = page.getByRole("button", { name: /^Restore Smart Clean/ });
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press("Tab");
    if (await restore.evaluate((el) => el === document.activeElement)) break;
  }
  await expect(restore).toBeFocused();
  await page.keyboard.press("Enter");
  const first = page.getByTestId("history-row").first();
  await expect(first.getByText("Restored")).toBeVisible();
  await expect(first.getByRole("button")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__restored)).toEqual(["visual-e1"]);
}

test("History matches the snapshot within 1% (REQ-DSCR-015.A1, REQ-DSCR-017.A1)", historyMatchesSnapshot);
test("History has zero serious or critical axe violations (REQ-DSCR-017.A2)", historyPassesAxe);
test("the keyboard restores a History row (REQ-DSCR-015.A3, REQ-DSCR-017.A2)", keyboardRestoresARow);
