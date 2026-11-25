import { expect, test, type Page } from "@playwright/test";
import { appToResults, idleFixtureView, openApp, stubEngine } from "./engineStub";

/** REQ-DCLEAN-025: motion tokens, and none of it under reduced motion. */

/** Computed transition / animation of every element matching `sel`. */
const motionOf = (page: Page, sel: string) =>
  page.locator(sel).first().evaluate((el) => {
    const s = getComputedStyle(el);
    return { transition: s.transition, duration: s.transitionDuration, animation: s.animationName };
  });

async function transitionsUseTheMotionTokens({ page }: { page: Page }) {
  await stubEngine(page, idleFixtureView());
  await openApp(page);
  await page.getByTestId("idle").waitFor();
  expect((await motionOf(page, ".ring-fill")).transition).toBe("stroke-dasharray 0.8s");
  expect((await motionOf(page, ".meter-fill")).transition).toBe("width 0.8s");
  await appToResults(page);
  expect((await motionOf(page, ".chevron")).transition).toBe("transform 0.2s");
}

test("ring, bar and chevron transitions use their motion tokens (REQ-DCLEAN-025.A1)", transitionsUseTheMotionTokens);

test.describe("with reduced motion requested", () => {
  test.use({ reducedMotion: "reduce" });

  async function reducedMotionDisablesTransitionsAndAnimations({ page }: { page: Page }) {
    await stubEngine(page, idleFixtureView(), true);
    await openApp(page);
    await page.getByTestId("idle").waitFor();
    for (const sel of [".ring-fill", ".meter-fill"]) expect((await motionOf(page, sel)).duration).toBe("0s");
    await page.getByTestId("idle").getByRole("button", { name: "Start scan" }).click();
    await page.getByTestId("scanning").waitFor();
    expect((await motionOf(page, ".scan-bar-fill")).duration).toBe("0s");

    await appToResults(page);
    expect((await motionOf(page, ".chevron")).duration).toBe("0s");
    // A2: the state change applies at once — the chevron is flipped the moment it is expanded.
    await page.getByRole("button", { name: "Show items in System caches" }).click();
    const flipped = await page.locator(".chevron").first().evaluate((el) => getComputedStyle(el).transform);
    expect(flipped).not.toBe("none");

    // The busy spinner does not spin.
    await stubEngine(page, "forever");
    await openApp(page);
    await page.getByTestId("busy").waitFor();
    expect((await motionOf(page, ".busy-spinner")).animation).toBe("none");
  }

  test("transitions and animations are off (REQ-DCLEAN-025.A2)", reducedMotionDisablesTransitionsAndAnimations);
});
