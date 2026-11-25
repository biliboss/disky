import { expect, test, type Page } from "@playwright/test";
import { snapshotReviewCategories } from "../fixtures/snapshotReview";
import { doneFixtureResult, idleFixtureView, openApp, reviewFixtureView, stubEngine } from "./engineStub";

/**
 * REQ-DCLEAN-020: the whole Smart Clean flow by keyboard alone, engine
 * stubbed: idle → scan → results → confirm → done → idle. No pointer event
 * is sent.
 */

/** The accessible name of the focused element. */
const focusedName = (page: Page) =>
  page.evaluate(() => {
    const el = document.activeElement as HTMLElement | null;
    if (!el || el === document.body) return "";
    return (el.getAttribute("aria-label") ?? el.textContent ?? "").replace(/\s+/g, " ").trim();
  });

/**
 * Tab `n` times from the current focus, collecting each focused name. The
 * step where focus leaves the page (wrapping round) is dropped, and a nav
 * entry's badge count is trimmed off its name.
 */
async function tabSequence(page: Page, n: number): Promise<string[]> {
  const names: string[] = [];
  for (let i = 0; i < n; i++) {
    await page.keyboard.press("Tab");
    const name = await focusedName(page);
    if (name) names.push(name.replace(/^(Assistant)\d+$/, "$1"));
  }
  return names;
}

const NAV = ["Smart Clean", "Assistant", "Space Map", "Large files", "Duplicates", "History", "Settings"];

/** Press Tab until `name` has focus (at most `max` presses). */
async function tabTo(page: Page, name: string | RegExp, max = 40) {
  for (let i = 0; i < max; i++) {
    const at = await focusedName(page);
    if (typeof name === "string" ? at === name : name.test(at)) return;
    await page.keyboard.press("Tab");
  }
  throw new Error(`Tab never reached ${name}`);
}

async function fullFlowByKeyboardOnly({ page }: { page: Page }) {
  await stubEngine(page, idleFixtureView(), false, reviewFixtureView(), doneFixtureResult());
  await openApp(page);
  await page.getByTestId("idle").waitFor();

  // A1 on idle: navigation, then the phase's actions, then the shortcuts.
  const idleOrder = await tabSequence(page, NAV.length + 5);
  expect(idleOrder.slice(0, NAV.length)).toEqual(NAV);
  expect(idleOrder.slice(NAV.length, NAV.length + 2)).toEqual(["Start scan", "Explore disk"]);

  // A2: Enter starts the scan; A4: focus lands on the results heading.
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await tabTo(page, "Start scan");
  await page.keyboard.press("Enter");
  await page.getByTestId("results").waitFor();
  await expect(page.getByTestId("results-heading")).toBeFocused();

  // A1 on results, read as a cycle from the focused heading: the heading
  // region's actions, each row (selection, then disclosure), the footer, then
  // round to the navigation, which comes first in the page.
  const cats = snapshotReviewCategories();
  const rows = cats.flatMap((c) => [c.name, `Show items in ${c.name}`]);
  const order = await tabSequence(page, 2 + rows.length + 2 + 1 + NAV.length);
  expect(order).toEqual(["Safe only", "Select all", ...rows, "Cancel", expect.stringMatching(/^Clean /), ...NAV]);

  // A2: Space toggles a selection control; Enter and Space toggle a disclosure.
  const review = cats.find((c) => c.safety === "review")!;
  const box = page.getByRole("checkbox", { name: review.name });
  await box.focus();
  await expect(box).toHaveAttribute("aria-checked", "false");
  await page.keyboard.press("Space");
  await expect(box).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("Space");
  await expect(box).toHaveAttribute("aria-checked", "false");
  await page.keyboard.press("Tab");
  const disclose = page.getByRole("button", { name: `Show items in ${review.name}` });
  await expect(disclose).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(disclose).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("Space");
  await expect(disclose).toHaveAttribute("aria-expanded", "false");

  // Confirm: Enter on Clean opens the dialog on Cancel; Escape closes it back to Clean.
  await tabTo(page, /^Clean /);
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.getByTestId("clean")).toBeFocused();
  await page.keyboard.press("Space");
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();

  // A3: arrow keys move between the mode options.
  await page.keyboard.press("Shift+Tab");
  const trash = dialog.getByRole("radio", { name: "Move to Trash" });
  const del = dialog.getByRole("radio", { name: "Delete permanently" });
  await expect(trash).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(del).toBeChecked();
  await page.keyboard.press("ArrowLeft");
  await expect(trash).toBeChecked();
  await expect(trash).toBeFocused();

  // Clean now by keyboard → done, focus on its heading (A4).
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  await expect(dialog.getByRole("button", { name: "Clean now" })).toBeFocused();
  await page.keyboard.press("Enter");
  await page.getByTestId("done").waitFor();
  await expect(page.getByTestId("done-heading")).toBeFocused();

  // Done → idle by keyboard; focus on the idle heading.
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Back to home" })).toBeFocused();
  await page.keyboard.press("Enter");
  await page.getByTestId("idle").waitFor();
  await expect(page.getByTestId("idle").getByRole("heading", { level: 1 })).toBeFocused();
}

test("the whole flow is operable by keyboard alone, in visual order (REQ-DCLEAN-020.A1–A4)", fullFlowByKeyboardOnly);
