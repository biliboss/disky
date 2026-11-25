import { expect, test, type Page } from "@playwright/test";
import { appToResults, manyItemsView, openApp, stubEngine, idleFixtureView } from "./engineStub";

/** REQ-DCLEAN-026.A2, A3: extra window space goes to the main area; the list scrolls on its own. */

const SIZES = [
  { width: 1200, height: 780 },
  { width: 1600, height: 1000 },
];

async function extraSpaceGoesToTheMainArea({ page }: { page: Page }) {
  for (const size of SIZES) {
    await page.setViewportSize(size);
    await stubEngine(page, idleFixtureView());
    await openApp(page);
    await page.getByTestId("idle").waitFor();
    expect((await page.locator(".sidebar").boundingBox())!.width).toBe(236);
    expect((await page.locator(".idle-rail").boundingBox())!.width).toBe(300);
    expect((await page.locator(".app-main").boundingBox())!.width).toBe(size.width - 236);
  }
}

test("the sidebar and ring rail keep their design widths at any size (REQ-DCLEAN-026.A2)", extraSpaceGoesToTheMainArea);

async function resultsListScrollsIndependently({ page }: { page: Page }) {
  for (const size of SIZES) {
    await page.setViewportSize(size);
    await appToResults(page, manyItemsView(40));
    await page.getByRole("button", { name: "Show items in System caches" }).click();
    const list = page.getByTestId("categories");
    const head = await page.locator(".results-head").boundingBox();
    const foot = await page.locator(".results-foot").boundingBox();
    const scrolled = await list.evaluate((el) => {
      el.scrollTop = 400;
      return el.scrollTop;
    });
    expect(scrolled).toBeGreaterThan(0);
    expect(await page.locator(".results-head").boundingBox()).toEqual(head);
    expect(await page.locator(".results-foot").boundingBox()).toEqual(foot);
    // The footer stays on screen, at the bottom of the window.
    expect(foot!.y + foot!.height).toBe(size.height);
  }
}

test("the results list scrolls between a fixed header and footer at every size (REQ-DCLEAN-026.A3)", resultsListScrollsIndependently);
