import { expect, test, type Page } from "@playwright/test";
import { IDLE_INTERACTIVE_BUDGET_MS, IDLE_INTERACTIVE_MARK, INTERACTION_BUDGET_MS } from "../../src/lib/budgets";
import { appToResults, idleFixtureView, openApp, stubEngine } from "./engineStub";

/**
 * REQ-DCLEAN-030 latency budgets, measured with performance marks in the
 * production build. Budgets live in src/lib/budgets.ts.
 */

async function idleIsInteractiveWithinBudget({ page }: { page: Page }) {
  await stubEngine(page, idleFixtureView());
  await openApp(page);
  await page.waitForFunction((m) => performance.getEntriesByName(m).length > 0, IDLE_INTERACTIVE_MARK);
  const at = await page.evaluate((m) => performance.getEntriesByName(m)[0].startTime, IDLE_INTERACTIVE_MARK);
  expect(at).toBeLessThan(IDLE_INTERACTIVE_BUDGET_MS);
  // Using the existing snapshot: no scan ran to get here.
  await expect(page.getByTestId("idle")).toBeVisible();
}

test("idle is interactive within 1 s of launch (REQ-DCLEAN-030.A1)", idleIsInteractiveWithinBudget);

/**
 * Click `selector`, then wait until `done` holds and the next frame has
 * painted; the performance measure between the two marks is the response time.
 */
async function responseMs(page: Page, name: string, selector: string, done: string): Promise<number> {
  return page.evaluate(
    async ([n, sel, check]) => {
      const el = document.querySelector<HTMLElement>(sel)!;
      const ok = new Function(`return (${check})`) as () => boolean;
      performance.mark(`${n}:start`);
      el.click();
      while (!ok()) await new Promise((r) => requestAnimationFrame(r));
      await new Promise((r) => requestAnimationFrame(r));
      performance.mark(`${n}:end`);
      return performance.measure(n, `${n}:start`, `${n}:end`).duration;
    },
    [name, selector, done] as const,
  );
}

async function interactionsRespondWithinBudget({ page }: { page: Page }) {
  await appToResults(page);
  const total = () => page.getByTestId("clean").textContent();
  const before = await total();
  const timings = {
    select: await responseMs(
      page,
      "select",
      '.result[data-category-id] .result-check[aria-checked="false"]',
      `document.querySelector('[data-testid="clean"]').textContent !== ${JSON.stringify(before)}`,
    ),
    expand: await responseMs(
      page,
      "expand",
      ".result-disclose",
      `document.querySelector('.result-disclose').getAttribute('aria-expanded') === 'true'`,
    ),
    collapse: await responseMs(
      page,
      "collapse",
      ".result-disclose",
      `document.querySelector('.result-disclose').getAttribute('aria-expanded') === 'false'`,
    ),
  };
  // A2 budgets the toggles, expand/collapse and the footer total. Screen
  // navigation is outside it: it mounts a screen and loads its data, which
  // on a shared CI runner can take past 100 ms without anything being wrong.
  for (const [what, ms] of Object.entries(timings)) expect(ms, `${what} took ${ms.toFixed(1)} ms`).toBeLessThan(INTERACTION_BUDGET_MS);
  await page.locator(".nav-item:nth-of-type(6)").click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("History");
}

test("select, expand and the footer total respond within 100 ms (REQ-DCLEAN-030.A2)", interactionsRespondWithinBudget);
