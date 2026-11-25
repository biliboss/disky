import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { MASK_COLOR, baselinePath, offset, openSnapshot } from "./snapshot";
import { WINDOW, crowdedMapFolder, openApp, sliverMapFolder, stubMap } from "./engineStub";

/**
 * REQ-DSCR-012 in a real browser: the Space map against the snapshot
 * rendered with startScreen = map at 1200 × 780 (within 1%), axe, and the
 * keyboard drilling into a folder. Fixture folders are the snapshot's MAP
 * (root children 212 / 92 / 58 / 34 / 26 / 16 GB on a 512 GB disk).
 * Masked: the traffic-light slot and the meter's live free-space number.
 */
test.describe.configure({ mode: "serial" });

const SNAPSHOT_LIGHTS = '[data-screen-label="App window"] > div:first-child > div:first-child';
const APP_LIGHTS = '[data-testid="traffic-lights"]';
const DIFF = { maxDiffPixelRatio: 0.01, maskColor: MASK_COLOR, animations: "disabled" as const };

async function openMap(page: Page, noSnapshot = false) {
  await stubMap(page, noSnapshot);
  await openApp(page, "#/space-map");
  if (!noSnapshot) await page.getByRole("button", { name: "Users, 212 GB" }).waitFor();
}

async function spaceMapMatchesSnapshot({ page }: { page: Page }) {
  const at = await openSnapshot(page, { startScreen: "map" });
  await page.locator('[data-screen-label="Space map"]').waitFor();
  await page.screenshot({
    path: baselinePath("screen-map.png"),
    clip: offset(WINDOW, at),
    mask: [page.locator(SNAPSHOT_LIGHTS), page.locator('aside span:has-text("GB free")')],
    maskColor: MASK_COLOR,
    animations: "disabled",
  });

  await page.setViewportSize(WINDOW);
  await openMap(page);
  await expect(page).toHaveScreenshot("screen-map.png", {
    ...DIFF,
    clip: WINDOW,
    mask: [page.locator(APP_LIGHTS), page.locator(".meter-free")],
  });
}

/**
 * A crowded folder (bug: blocks spilled past the treemap as pills and
 * labels overflowed): every block stays inside the treemap box, the
 * engine's Other block is drawn last, and no shown label is wider than its
 * block (ellipsis allowed, overflow not).
 */
async function crowdedFolderStaysInsideTheTreemap({ page }: { page: Page }) {
  await stubMap(page, false, crowdedMapFolder());
  await page.setViewportSize(WINDOW);
  await openApp(page, "#/space-map");
  await page.getByRole("button", { name: "Other, 214 smaller items, 22 GB" }).waitFor();
  const report = await page.evaluate(() => {
    const box = document.querySelector(".map-tiles")!.getBoundingClientRect();
    const tiles = [...document.querySelectorAll<HTMLElement>(".map-tile")];
    const outside = tiles
      .filter((t) => {
        const r = t.getBoundingClientRect();
        return r.left < box.left - 0.5 || r.top < box.top - 0.5 || r.right > box.right + 0.5 || r.bottom > box.bottom + 0.5;
      })
      .map((t) => t.getAttribute("aria-label"));
    // A shown label must fit the text area: the body's width minus its padding.
    const room = (el: HTMLElement) => {
      const cs = getComputedStyle(el);
      return el.getBoundingClientRect().width - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    };
    const overflowing = tiles
      .flatMap((t) => [...t.querySelectorAll<HTMLElement>(".map-tile-name, .map-tile-size")])
      .filter((s) => s.offsetParent !== null && s.getBoundingClientRect().width > room(s.parentElement!) + 0.5)
      .map((s) => s.textContent);
    return { count: tiles.length, last: tiles.at(-1)?.getAttribute("aria-label"), outside, overflowing };
  });
  expect(report.count).toBe(20);
  expect(report.last).toBe("Other, 214 smaller items, 22 GB");
  expect(report.outside).toEqual([]);
  expect(report.overflowing).toEqual([]);
  await page.screenshot({ path: test.info().outputPath("crowded-map.png"), clip: WINDOW });
}

/** Every `.map-tile` lies inside `.map-tiles` (half-pixel tolerance). */
async function tilesOutsideTheTreemap(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const box = document.querySelector(".map-tiles")!.getBoundingClientRect();
    return [...document.querySelectorAll<HTMLElement>(".map-tile")]
      .filter((t) => {
        const r = t.getBoundingClientRect();
        return r.left < box.left - 0.5 || r.top < box.top - 0.5 || r.right > box.right + 0.5 || r.bottom > box.bottom + 0.5;
      })
      .map((t) => t.getAttribute("aria-label") ?? "");
  });
}

/**
 * A sliver block (bug: a 1 MB child next to 24.7 GB kept its 14 px padding,
 * grew wider than its slot and slid under the Selected card) stays inside
 * the treemap, and its block is no wider than its share.
 */
async function sliverBlockStaysInsideItsSlot({ page }: { page: Page }) {
  await stubMap(page, false, sliverMapFolder());
  await page.setViewportSize(WINDOW);
  await openApp(page, "#/space-map");
  // At 0.004% of the folder the gutter eats the whole slot: the block has no
  // width (so Playwright calls it hidden), yet it stays in the accessibility
  // tree and the tab order.
  const sliver = page.getByRole("button", { name: "manifests, 1 MB" });
  await sliver.waitFor({ state: "attached" });
  expect(await tilesOutsideTheTreemap(page)).toEqual([]);
  const tiles = await page.locator(".map-tiles").boundingBox();
  const box = await sliver.boundingBox();
  // 1 MB of 24.7 GB: a hairline, never the padding's 28 px.
  expect(box!.width).toBeLessThan(tiles!.width * 0.01 + 1);
}

async function spaceMapPassesAxe({ page }: { page: Page }) {
  await openMap(page);
  const result = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  const bad = result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  expect(bad).toEqual([]);
}

async function keyboardDrillsIntoAFolder({ page }: { page: Page }) {
  await openMap(page);
  const users = page.getByRole("button", { name: "Users, 212 GB" });
  const apps = page.getByRole("button", { name: "Applications, 92 GB" });
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press("Tab");
    if (await apps.evaluate((el) => el === document.activeElement)) break;
  }
  await expect(apps).toBeFocused();
  // Space selects; Enter on the selected tile opens it.
  await page.keyboard.press("Space");
  await expect(apps).toHaveAttribute("aria-pressed", "true");
  await expect(users).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByTestId("map-selected")).toContainText("18.0%");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Xcode, 18 GB" })).toBeVisible();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Applications · 92 GB");
  expect(await page.evaluate(() => (window as any).__mapped)).toEqual([null, "/Applications"]);
  // Up goes back to the root.
  await page.getByRole("button", { name: "Up", exact: true }).press("Enter");
  await expect(users).toBeVisible();
  await expect(page.getByRole("button", { name: "Up", exact: true })).toHaveCount(0);
  // Reveal in Finder names the selected path to the backend.
  await page.getByRole("button", { name: "Reveal in Finder" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__revealed)).toEqual(["/Users"]);
}

async function noSnapshotOffersStartScan({ page }: { page: Page }) {
  await openMap(page, true);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("No scans yet");
  await expect(page.getByRole("button", { name: "Start scan" })).toBeVisible();
}

test("Space map matches the snapshot within 1% (REQ-DSCR-012.A1, REQ-DSCR-017.A1)", spaceMapMatchesSnapshot);
test("Space map has zero serious or critical axe violations (REQ-DSCR-017.A2)", spaceMapPassesAxe);
test("the keyboard selects and drills into a folder (REQ-DSCR-012.A5, REQ-DSCR-017.A2)", keyboardDrillsIntoAFolder);
test("no snapshot shows No scans yet with Start scan (REQ-DSCR-012.A5)", noSnapshotOffersStartScan);
test("a crowded folder stays inside the treemap with an Other block (REQ-DSCR-012.A2)", crowdedFolderStaysInsideTheTreemap);
test("a sliver block stays inside its slot (REQ-DSCR-012.A2)", sliverBlockStaysInsideItsSlot);
