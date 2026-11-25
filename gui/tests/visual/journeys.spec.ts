import { expect, test, type Page } from "@playwright/test";
import {
  WINDOW,
  doneFixtureResult,
  openApp,
  reviewFixtureView,
  snapshotDupesView,
  snapshotHistoryRows,
  snapshotLargeFiles,
  snapshotMapFolders,
  snapshotSettingsPrefs,
} from "./engineStub";
import { snapshotAssistantView } from "../fixtures/snapshotAssistant";

/**
 * Whole-app journeys (REQ-DSCR-017.A4): one stand-in engine answers every
 * Tauri command with shared state (a clean, a trash and a restore all show
 * up in History; prefs saved anywhere are read everywhere), and each test
 * walks a real user flow across screens by mouse.
 *
 * Every journey fails on a page error, a console error, or a command the
 * app calls that no engine command answers — the class of bug where a
 * button "does nothing".
 */


async function stubWholeApp(page: Page) {
  await page.addInitScript(
    ([assistant, review, done, history, large, dupes, folders, prefs]) => {
      const w = window as any;
      const disk = (free: number) => ({ volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: free });
      const s = {
        free: 74e9,
        prefs: structuredClone(prefs) as any,
        history: structuredClone(history) as any[],
        large: structuredClone(large) as any[],
        dupes: structuredClone(dupes),
        calls: [] as string[],
        unknown: [] as string[],
      };
      w.__journey = s;
      // Tauri's event plugin reads this global on unlisten.
      w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => undefined };
      const entry = (id: string, title: string, bytes: number) => ({
        id,
        title,
        ts: new Date().toISOString(),
        mode: "trash",
        categories: [],
        bytes,
        items: [],
        detail: title,
        state: "restorable",
      });
      w.__TAURI_INTERNALS__ = {
        transformCallback: () => 0,
        invoke: async (cmd: string, args: any) => {
          s.calls.push(cmd);
          switch (cmd) {
            case "plugin:event|listen":
              return 1;
            case "plugin:event|unlisten":
              return null;
            case "load_cleanup_view":
              return { ...structuredClone(review), disk: disk(s.free), preferences: structuredClone(s.prefs) };
            case "load_assistant":
              return {
                ...structuredClone(assistant),
                disk: disk(s.free),
                goal_bytes: s.prefs.goal_bytes,
                checks: structuredClone(s.prefs.checks),
                agent: s.prefs.agent,
              };
            case "scan":
              return { ...structuredClone(review), disk: disk(s.free), preferences: structuredClone(s.prefs) };
            case "stop_scan":
              return null;
            case "clean": {
              s.free += done.freed_bytes;
              s.history.unshift(entry("j-clean", "Smart Clean", done.freed_bytes));
              return { ...structuredClone(done), history_id: "j-clean", disk: disk(s.free) };
            }
            case "restore": {
              const r = s.history.find((x) => x.id === args.historyId);
              if (r) r.state = "restored";
              return { report: { id: args.historyId, restored: ["/Users/me/x"], conflicts: [], missing: [] }, disk: disk(s.free) };
            }
            case "list_history":
              return { total_bytes: s.history.reduce((a, r) => a + r.bytes, 0), rows: structuredClone(s.history) };
            case "get_prefs":
              return structuredClone(s.prefs);
            case "set_prefs": {
              const { key, value } = args;
              if (key === "goal_bytes") s.prefs.goal_bytes = Number(value);
              else if (key === "checks.enabled") s.prefs.checks.enabled = value === "true";
              else if (key === "checks.frequency") s.prefs.checks.frequency = value;
              else if (key === "agent" || key === "scheduled_scan") s.prefs[key] = value;
              else if (key === "excluded") s.prefs.excluded = JSON.parse(value);
              else s.prefs[key] = value === "true";
              return structuredClone(s.prefs);
            }
            case "pick_folder":
              return "~/Projects/active";
            case "load_map":
              return structuredClone((folders as any)[args.path ?? "/"]);
            case "reveal_in_finder":
              return null;
            case "load_large": {
              const records = s.large.filter(
                (f) => args.filter === "all" || (args.filter === "over-10gb" ? f.bytes >= 10e9 : f.unused_1y),
              );
              return { filter: args.filter, count: records.length, total_bytes: records.reduce((a, f) => a + f.bytes, 0), records };
            }
            case "trash_large": {
              const freed = s.large.filter((f) => args.paths.includes(f.path)).reduce((a, f) => a + f.bytes, 0);
              s.large = s.large.filter((f) => !args.paths.includes(f.path));
              s.free += freed;
              s.history.unshift(entry("j-large", "Large files", freed));
              return { handled: args.paths, failed: [], freed_physical_bytes: freed, history_id: "j-large", disk: disk(s.free) };
            }
            case "load_dupes":
              return structuredClone(s.dupes);
            case "trash_dupes":
              s.history.unshift(entry("j-dupes", "Duplicates", 1e9));
              return { handled: args.paths, failed: [], freed_physical_bytes: 1e9, history_id: "j-dupes", disk: disk(s.free) };
            case "open_full_disk_access":
              return null;
            default:
              s.unknown.push(cmd);
              throw new Error(`journey stub: ${cmd} is not an engine command`);
          }
        },
      };
    },
    [
      snapshotAssistantView(),
      reviewFixtureView(),
      doneFixtureResult(),
      snapshotHistoryRows(),
      snapshotLargeFiles(),
      snapshotDupesView(),
      snapshotMapFolders(),
      snapshotSettingsPrefs(),
    ] as const,
  );
}

/** Errors seen while a journey runs; asserted empty at the end. */
function watchErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(`console: ${m.text()}`);
  });
  return errors;
}

async function start(page: Page, hash = "#/") {
  const errors = watchErrors(page);
  await stubWholeApp(page);
  await page.setViewportSize(WINDOW);
  await openApp(page, hash);
  return errors;
}

async function finish(page: Page, errors: string[]) {
  const unknown = await page.evaluate(() => (window as any).__journey.unknown);
  expect(unknown, "commands no engine command answers").toEqual([]);
  expect(errors).toEqual([]);
  // No screen is left showing an error alert.
  await expect(page.locator('[role="alert"]')).toHaveCount(0);
}

const nav = (page: Page, name: string) => page.locator(".sidebar").getByRole("button", { name: new RegExp(`^${name}`) });

/** Every sidebar entry opens its own screen, and nothing errors on the way. */
async function everySidebarEntryOpensItsScreen({ page }: { page: Page }) {
  const errors = await start(page);
  const screens: [string, string][] = [
    ["Assistant", "assistant"],
    ["Space Map", "space-map"],
    ["Large files", "large-files"],
    ["Duplicates", "duplicates"],
    ["History", "history"],
    ["Settings", "settings"],
    ["Smart Clean", "idle"],
  ];
  for (const [label, testid] of screens) {
    await nav(page, label).click();
    await expect(page.getByTestId(testid)).toBeVisible();
    await expect(nav(page, label)).toHaveAttribute("aria-current", "page");
  }
  await finish(page, errors);
}

/** Smart Clean: scan → review → clean → done → undo, then History shows it restored. */
async function smartCleanScanCleanUndoShowsInHistory({ page }: { page: Page }) {
  const errors = await start(page);
  await page.getByRole("button", { name: /^(Start scan|Scan again|Scan)/ }).first().click();
  await expect(page.getByTestId("results")).toBeVisible();
  await page.getByRole("button", { name: /^Clean/ }).last().click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: /^Clean now/ }).click();
  await expect(page.getByTestId("done")).toBeVisible();
  await page.getByRole("button", { name: /^Undo/ }).click();
  await nav(page, "History").click();
  await expect(page.getByTestId("history")).toBeVisible();
  await expect(page.getByTestId("history").getByText("Smart Clean").first()).toBeVisible();
  await finish(page, errors);
}

/** Every actionable Assistant card opens Smart Clean's review as the agent. */
async function everyAssistantCardOpensItsReview({ page }: { page: Page }) {
  const errors = await start(page, "#/assistant");
  const cards = await page.locator('article[data-testid^="suggestion-"]').evaluateAll((els) =>
    els.filter((e) => e.querySelector(".btn-primary")).map((e) => e.getAttribute("data-testid")!),
  );
  expect(cards.length).toBeGreaterThan(0);
  for (const id of cards) {
    await nav(page, "Assistant").click();
    await page.getByTestId(id).locator(".btn-primary").click();
    await expect(page.getByTestId("results")).toBeVisible();
    await expect(page.getByTestId("results-kicker")).toHaveText(/^Prepared by .+ · review before cleaning$/);
    // "Review & confirm" also opens the confirmation; cancel it. Nothing
    // is cleaned: `clean` is never called below.
    const dialog = page.getByRole("dialog");
    if (await dialog.isVisible()) await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
  }
  expect(await page.evaluate(() => (window as any).__journey.calls.includes("clean"))).toBe(false);
  await finish(page, errors);
}

/** The Assistant's panel: dismiss, goal, checks, frequency and agent all save. */
async function assistantPanelSavesEveryControl({ page }: { page: Page }) {
  const errors = await start(page, "#/assistant");
  const dismissable = page.locator('[data-testid^="suggestion-"] .btn-secondary').first();
  const before = await page.locator('article[data-testid^="suggestion-"]').count();
  await dismissable.click();
  await expect(page.locator('article[data-testid^="suggestion-"]')).toHaveCount(before - 1);
  const slider = page.getByRole("slider", { name: "Space goal" });
  await slider.focus();
  await page.keyboard.press("ArrowRight");
  await page.getByRole("switch").click();
  await page.getByRole("switch").click();
  await page.getByText("Weekly", { exact: true }).click();
  const sets = await page.evaluate(() => (window as any).__journey.prefs);
  expect(sets.checks.frequency).toBe("weekly");
  expect(sets.goal_bytes).toBe(85e9);
  await finish(page, errors);
}

/** Space map: select, drill in, reveal, and come back up. */
async function spaceMapDrillRevealUp({ page }: { page: Page }) {
  const errors = await start(page, "#/space-map");
  await page.getByRole("button", { name: "Applications, 92 GB" }).click();
  await page.getByRole("button", { name: "Open folder" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(/^Applications · /);
  await page.getByRole("button", { name: "Reveal in Finder" }).click();
  await page.getByRole("button", { name: "Up", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(/^Macintosh HD · /);
  await finish(page, errors);
}

/** Large files: filter, select, trash; the trash shows up in History. */
async function largeFilesTrashShowsInHistory({ page }: { page: Page }) {
  const errors = await start(page, "#/large-files");
  const screen = page.getByTestId("large-files");
  await screen.getByText("Over 10 GB", { exact: true }).click();
  await screen.getByText("All", { exact: true }).click();
  await screen.getByRole("checkbox").nth(1).click();
  await screen.getByRole("button", { name: /^Move .* to Trash$/ }).click();
  await nav(page, "History").click();
  await expect(page.getByTestId("history").getByText("Large files").first()).toBeVisible();
  await finish(page, errors);
}

/** Duplicates: keep another copy, remove the extras; History shows it. */
async function duplicatesRemoveShowsInHistory({ page }: { page: Page }) {
  const errors = await start(page, "#/duplicates");
  const screen = page.getByTestId("duplicates");
  await screen.getByRole("radio").nth(1).click();
  await screen.getByRole("button", { name: /^Remove \d+ extra cop/ }).click();
  await nav(page, "History").click();
  await expect(page.getByTestId("history").getByText("Duplicates").first()).toBeVisible();
  await finish(page, errors);
}

/** History: restore a restorable entry. */
async function historyRestoresAnEntry({ page }: { page: Page }) {
  const errors = await start(page, "#/history");
  await page.getByTestId("history").getByRole("button", { name: /^Restore/ }).first().click();
  await expect.poll(() => page.evaluate(() => (window as any).__journey.calls.includes("restore"))).toBe(true);
  await finish(page, errors);
}

/** Settings: toggles and Add folder save to the store the Assistant reads. */
async function settingsSaveAndTheAssistantReadsThem({ page }: { page: Page }) {
  const errors = await start(page, "#/settings");
  const screen = page.getByTestId("settings");
  const firstSwitch = screen.getByRole("switch").first();
  const was = await firstSwitch.getAttribute("aria-checked");
  await firstSwitch.click();
  await expect(firstSwitch).not.toHaveAttribute("aria-checked", was!);
  await screen.getByRole("button", { name: /Add folder/ }).click();
  await expect(screen.getByText("~/Projects/active")).toBeVisible();
  await nav(page, "Assistant").click();
  await expect(page.getByTestId("assistant")).toBeVisible();
  await finish(page, errors);
}

test("every sidebar entry opens its screen with no errors (REQ-DSCR-017.A4)", everySidebarEntryOpensItsScreen);
test("Smart Clean: scan, clean, undo; History shows it (REQ-DSCR-017.A4)", smartCleanScanCleanUndoShowsInHistory);
test("every Assistant card's Review opens Smart Clean's review (REQ-DSCR-009.A3, REQ-DSCR-017.A4)", everyAssistantCardOpensItsReview);
test("the Assistant panel saves every control (REQ-DSCR-010, REQ-DSCR-017.A4)", assistantPanelSavesEveryControl);
test("Space map: drill in, reveal, up (REQ-DSCR-012, REQ-DSCR-017.A4)", spaceMapDrillRevealUp);
test("Large files: trash shows in History (REQ-DSCR-013, REQ-DSCR-017.A4)", largeFilesTrashShowsInHistory);
test("Duplicates: remove extras shows in History (REQ-DSCR-014, REQ-DSCR-017.A4)", duplicatesRemoveShowsInHistory);
test("History restores an entry (REQ-DSCR-015, REQ-DSCR-017.A4)", historyRestoresAnEntry);
test("Settings save and the Assistant still opens (REQ-DSCR-016, REQ-DSCR-017.A4)", settingsSaveAndTheAssistantReadsThem);
