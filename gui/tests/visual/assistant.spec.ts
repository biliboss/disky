import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, baselinePath, offset, openSnapshot } from "./snapshot";
import { WINDOW, openApp, stubAssistant } from "./engineStub";

/**
 * REQ-DSCR-009 in a real browser: the Assistant screen against the
 * snapshot rendered with startScreen = agent at 1200 × 780 (within 1%),
 * axe, and the keyboard reaching "Review" and landing on the results with
 * the kicker (engine stubbed). Fixtures are the snapshot's feed and AGENTS.
 * Masked: the traffic-light slot, the live free-space numbers (meter and
 * goal note) and the cards' relative times.
 */
test.describe.configure({ mode: "serial" });

const SNAPSHOT_LIGHTS = '[data-screen-label="App window"] > div:first-child > div:first-child';
const APP_LIGHTS = '[data-testid="traffic-lights"]';
const SNAPSHOT_SCREEN = '[data-screen-label="Assistant"]';
const DIFF = { maxDiffPixelRatio: 0.01, maskColor: MASK_COLOR, animations: "disabled" as const };

async function openAssistant(page: Page) {
  await stubAssistant(page);
  await openApp(page, "#/assistant");
  await page.getByTestId("suggestion-left").waitFor();
}

async function assistantMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page, { startScreen: "agent" });
  await page.locator(SNAPSHOT_SCREEN).waitFor();
  await page.screenshot({
    path: baselinePath("screen-assistant.png"),
    clip: offset(WINDOW, at),
    mask: [
      page.locator(SNAPSHOT_LIGHTS),
      page.locator('aside span:has-text("GB free")'),
      page.locator(`${SNAPSHOT_SCREEN} .card > div:has-text("free now.")`),
      // A card head is the one row whose first span holds the agent icon; its third span is "when".
      page.locator(`${SNAPSHOT_SCREEN} div:has(> span > svg) > span:nth-child(3)`),
    ],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await openAssistant(page);
  await expect(page).toHaveScreenshot("screen-assistant.png", {
    ...DIFF,
    clip: WINDOW,
    mask: [
      page.locator(APP_LIGHTS),
      page.locator(".meter-free"),
      page.getByTestId("assistant-goal-note"),
      page.getByTestId("suggestion-when"),
    ],
  });
}

async function assistantPassesAxe({ page }: { page: Page }) {
  await openAssistant(page);
  const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  const bad = result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  expect(bad).toEqual([]);
}

async function keyboardReviewLandsOnResultsWithTheKicker({ page }: { page: Page }) {
  await openAssistant(page);
  const review = page.getByTestId("suggestion-dev").getByRole("button", { name: "Review", exact: true });
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press("Tab");
    if (await review.evaluate((el) => el === document.activeElement)) break;
  }
  await expect(review).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("results")).toBeVisible();
  await expect(page.getByTestId("results-kicker")).toHaveText("Prepared by Claude Code · review before cleaning");
  await expect(page.getByRole("checkbox", { name: "Developer junk" })).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("dialog")).toHaveCount(0);
}

test("Assistant matches the snapshot within 1% (REQ-DSCR-009.A1)", assistantMatchesSnapshot);
test("Assistant has zero serious or critical axe violations (REQ-DSCR-009)", assistantPassesAxe);
test("Tab to Review and Enter lands on the results with the kicker (REQ-DSCR-009.A3)", keyboardReviewLandsOnResultsWithTheKicker);
