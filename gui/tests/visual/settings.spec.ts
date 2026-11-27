import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, baselinePath, offset, openSnapshot } from "./snapshot";
import { WINDOW, openApp, stubPrefs } from "./engineStub";

/**
 * REQ-DSCR-016 in a real browser: the Settings screen against the snapshot
 * rendered with startScreen = settings at 1200 × 780 (within 1%), axe, and
 * the keyboard toggling a switch. Fixture prefs are the snapshot's state:
 * every toggle on, Weekly, its three excluded folders. Masked: the
 * traffic-light slot and the meter's live free-space number.
 */
test.describe.configure({ mode: "serial" });

const SNAPSHOT_LIGHTS = '[data-screen-label="App window"] > div:first-child > div:first-child';
const APP_LIGHTS = '[data-testid="traffic-lights"]';
const DIFF = { maxDiffPixelRatio: 0.01, maskColor: MASK_COLOR, animations: "disabled" as const };

async function openSettings(page: Page, picked: string | null = null) {
  await stubPrefs(page, undefined, picked);
  await openApp(page, "#/settings");
  await page.getByRole("switch").first().waitFor();
}

const sets = (page: Page) => page.evaluate(() => (window as any).__prefsSets as { key: string; value: string }[]);

async function settingsMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page, { startScreen: "settings" });
  await page.locator('[data-screen-label="Settings"]').waitFor();
  await page.screenshot({
    path: baselinePath("screen-settings.png"),
    clip: offset(WINDOW, at),
    mask: [page.locator(SNAPSHOT_LIGHTS), page.locator('aside span:has-text("GB free")')],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await openSettings(page);
  await expect(page).toHaveScreenshot("screen-settings.png", {
    ...DIFF,
    clip: WINDOW,
    mask: [page.locator(APP_LIGHTS), page.locator(".meter-free")],
  });
}

async function settingsPassesAxe({ page }: { page: Page }) {
  await openSettings(page);
  const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  const bad = result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  expect(bad).toEqual([]);
}

async function keyboardTogglesASwitch({ page }: { page: Page }) {
  await openSettings(page);
  const recent = page.getByRole("switch", { name: "Skip recently used files" });
  await expect(recent).toHaveAttribute("aria-checked", "true");
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press("Tab");
    if (await recent.evaluate((el) => el === document.activeElement)) break;
  }
  await expect(recent).toBeFocused();
  await page.keyboard.press("Space");
  await expect(recent).toHaveAttribute("aria-checked", "false");
  await page.keyboard.press("Enter");
  await expect(recent).toHaveAttribute("aria-checked", "true");
  expect(await sets(page)).toEqual([
    { key: "skip_recently_used", value: "false" },
    { key: "skip_recently_used", value: "true" },
  ]);
  // The schedule group is reachable and arrow keys pick a period.
  const weekly = page.getByRole("radio", { name: "Weekly" });
  await weekly.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("radio", { name: "Monthly" })).toBeChecked();
  expect((await sets(page)).at(-1)).toEqual({ key: "scheduled_scan", value: "monthly" });
}

async function foldersAreAddedAndRemovedThroughTheStore({ page }: { page: Page }) {
  await openSettings(page, "~/Movies/Edits");
  await page.getByRole("button", { name: "Add folder" }).click();
  await expect(page.getByRole("listitem").filter({ hasText: "~/Movies/Edits" })).toBeVisible();
  await page.getByRole("button", { name: "Remove ~/Pictures/RAW" }).click();
  await expect(page.getByText("~/Pictures/RAW")).toHaveCount(0);
  const store = await page.evaluate(() => (window as any).__prefsStore.excluded);
  expect(store).toEqual(["~/Projects/active", "~/Library/Mobile Documents", "~/Movies/Edits"]);
}

test("Settings matches the snapshot within 1% (REQ-DSCR-016.A1, REQ-DSCR-017.A1)", settingsMatchesSnapshot);
test("Settings has zero serious or critical axe violations (REQ-DSCR-017.A2)", settingsPassesAxe);
test("the keyboard toggles a Settings switch (REQ-DSCR-016.A2, REQ-DSCR-017.A2)", keyboardTogglesASwitch);
test("folders are added and removed through the store (REQ-DSCR-016.A3)", foldersAreAddedAndRemovedThroughTheStore);
