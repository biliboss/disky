import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
const listeners: ((ev: { payload: unknown }) => void)[] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_event: string, cb: (ev: { payload: unknown }) => void) => {
    listeners.push(cb);
    return () => listeners.splice(listeners.indexOf(cb), 1);
  },
}));

import { App } from "../../App";
import { resetDiskState } from "../../lib/diskState";
import { parseCleanupView, type CleanResult } from "../../lib/contract";
import { RESULTS_ITEM_CAP } from "../../lib/budgets";
import { Results } from "../smartClean/Results";
import { Done } from "../smartClean/Done";
import { INITIAL_STATE } from "../smartClean/phase";
import { resetSmartCleanStore } from "../smartClean/store";
import { CLEAN_STARTED, SCAN_STARTED } from "../smartClean/announce";
import { THEMED_EXCEPTION_CLASSES, buttonClassProblem } from "../../../scripts/controlClasses.mjs";

/**
 * Control states and assistive-tech behaviour across Smart Clean:
 * themed defaults (REQ-DCLEAN-012), disabled actions (REQ-DCLEAN-016),
 * heading focus per phase (REQ-DCLEAN-020.A4), roles / names / states
 * (REQ-DCLEAN-021), announcements (REQ-DCLEAN-023) and the item cap
 * (REQ-DCLEAN-030.A3).
 */

const css = (p: string) => readFileSync(resolve(__dirname, p), "utf8");

function view(overrides: Record<string, unknown> = {}) {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: new Date(Date.now() - 86_400_000).toISOString(),
    snapshot_taken_at: null,
    is_stale: false,
    preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] },
    large_files_count: 3,
    skipped_paths: 0,
    categories: [
      {
        id: "cat_a",
        name: "Category A",
        description: "Description A",
        icon: "box",
        safety: "safe",
        bytes: 4_200_000_000,
        items: [{ name: "a", path: "/a", bytes: 4_200_000_000 }],
      },
      {
        id: "cat_b",
        name: "Category B",
        description: "Description B",
        icon: "box",
        safety: "review",
        bytes: 600_000_000,
        items: [{ name: "b", path: "/b", bytes: 600_000_000 }],
      },
    ],
    catalogue: [
      { id: "cat_a", name: "Category A", icon: "box", safety: "safe" },
      { id: "cat_b", name: "Category B", icon: "box", safety: "review" },
    ],
    ...overrides,
  };
}

const CLEANED = {
  mode: "trash",
  freed_bytes: 4_200_000_000,
  succeeded: ["/a"],
  failed: [],
  history_id: "h1",
  history_entry: null,
  disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 78.2e9 },
};

function engine(handlers: Record<string, (args: any) => unknown>) {
  invoke.mockImplementation(async (cmd: string, args: unknown) => {
    const h = handlers[cmd];
    if (!h) throw new Error(`unexpected command ${cmd}`);
    return h(args);
  });
}

function emitScan(fraction: number, completed: string[]) {
  for (const l of [...listeners]) {
    l({ payload: { fraction, current_path: "/Users", completed_category_ids: completed, completed_category_bytes: {} } });
  }
}

/** Idle → (held) scanning; returns a function that finishes the scan. */
async function startScan(scanned = view(), extra: Record<string, (args: any) => unknown> = {}) {
  let finish!: (v: unknown) => void;
  engine({ load_cleanup_view: () => view(), scan: () => new Promise((r) => (finish = r)), ...extra });
  render(() => <App />);
  fireEvent.click(within(await screen.findByTestId("idle")).getByRole("button", { name: "Start scan" }));
  await screen.findByTestId("scanning");
  return () => finish(scanned);
}

async function toResults(scanned = view(), extra: Record<string, (args: any) => unknown> = {}) {
  const finish = await startScan(scanned, extra);
  finish();
  return screen.findByTestId("results");
}

const announcer = () => screen.getByTestId("announcer");
const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  invoke.mockReset();
  listeners.length = 0;
  resetDiskState();
  resetSmartCleanStore();
  window.location.hash = "";
});
afterEach(cleanup);

// ── REQ-DCLEAN-012 · themed defaults ─────────────────────────────

/** Every button on screen is `btn btn-<variant>` or a themed exception. */
function expectThemedButtons() {
  const buttons = [...document.querySelectorAll("button")];
  expect(buttons.length).toBeGreaterThan(0);
  for (const b of buttons) {
    expect({ button: b.textContent || b.getAttribute("aria-label"), problem: buttonClassProblem([...b.classList]) }).toEqual({
      button: b.textContent || b.getAttribute("aria-label"),
      problem: null,
    });
  }
}

async function everyButtonUsesDesignSystemClasses() {
  // Idle with the notification, results with the skipped-paths link, the dialog, then done.
  await toResults(view({ skipped_paths: 3 }), { clean: () => CLEANED });
  expect(screen.getByRole("button", { name: /Full Disk Access/ })).toHaveClass("notice-link");
  expectThemedButtons();
  fireEvent.click(screen.getByTestId("clean"));
  await screen.findByRole("dialog");
  expectThemedButtons();
  fireEvent.click(screen.getByRole("button", { name: "Clean now" }));
  await screen.findByTestId("done");
  expectThemedButtons();
  fireEvent.click(screen.getByRole("button", { name: "Back to home" }));
  await screen.findByTestId("idle");
  expectThemedButtons();
  // The exceptions seen are exactly the shared list's.
  const seen = new Set([...document.querySelectorAll("button")].flatMap((b) => [...b.classList]));
  expect(THEMED_EXCEPTION_CLASSES.filter((c) => seen.has(c))).toEqual(["nav-item", "shortcut"]);
}

async function checkboxIsThemedAndFlipsChecked() {
  const results = await toResults();
  const [a, b] = within(results).getAllByRole("checkbox");
  expect(a).toHaveClass("result-check");
  expect(a).toHaveAttribute("aria-checked", "true"); // safe → preselected
  expect(b).toHaveAttribute("aria-checked", "false");
  fireEvent.click(b);
  expect(b).toHaveAttribute("aria-checked", "true");
  fireEvent.click(b);
  expect(b).toHaveAttribute("aria-checked", "false");
  const sheet = css("../smartClean/smartClean.css");
  expect(sheet).toMatch(/\.result-check\s*\{[^}]*width:\s*var\(--checkbox-size\);[^}]*height:\s*var\(--checkbox-size\);/);
  expect(sheet).toMatch(/\.result-check\s*\{[^}]*border:\s*var\(--check-border\) solid var\(--color-check-border\);[^}]*color:\s*var\(--color-bg\);/);
  expect(sheet).toMatch(/\.result-check\[aria-checked="true"\]\s*\{[^}]*background:\s*var\(--color-accent\);/);
}

async function rowsAndCardsUseSurfaceTokens() {
  const results = await toResults();
  const row = within(results).getAllByRole("listitem")[0];
  expect(row).toHaveClass("result");
  expect(within(row).getByRole("button", { name: "Show items in Category A" })).toHaveClass("btn", "btn-icon", "btn-ghost");
  expect(css("../smartClean/smartClean.css")).toMatch(
    /\.result\s*\{[^}]*background:\s*var\(--color-surface\);[^}]*border-radius:\s*var\(--radius-md\);/,
  );
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  const idle = await screen.findByTestId("idle");
  for (const card of within(idle).getAllByRole("button").filter((b) => b.classList.contains("shortcut"))) {
    expect(card).toHaveClass("card");
  }
  // The design system's .card is surface on the page, rounded to --radius-card's value.
  const organic = css("../../styles/organic.css");
  expect(organic).toMatch(/\.card\s*\{[^}]*background:\s*var\(--color-surface\);/);
  expect(organic).toMatch(/\.card, \.dialog\s*\{\s*border-radius:\s*calc\(var\(--radius-lg\) \* 1\.15\);/);
  expect(css("../../styles/tokens.css")).toMatch(/--radius-card:\s*calc\(var\(--radius-lg\) \* 1\.15\);/);
}

describe("themed default states (REQ-DCLEAN-012)", () => {
  it("every button uses the design-system classes (A1)", everyButtonUsesDesignSystemClasses);
  it("the selection control is themed and flips its checked state (A2)", checkboxIsThemedAndFlipsChecked);
  it("rows and cards use surface on the page with token radii (A3)", rowsAndCardsUseSurfaceTokens);
});

// ── REQ-DCLEAN-016 · disabled actions ────────────────────────────

function cleanIsDisabledAtZeroAndNeverFires() {
  const onClean = vi.fn();
  render(() => (
    <Results
      view={parseCleanupView(view())}
      state={{ ...INITIAL_STATE, phase: "results", selected: [] }}
      onToggleExpand={() => undefined}
      onToggleSelect={() => undefined}
      onSelectSafe={() => undefined}
      onSelectAll={() => undefined}
      onBack={() => undefined}
      onClean={onClean}
    />
  ));
  const clean = screen.getByTestId("clean");
  expect(clean).toBeDisabled(); // exposed as disabled (A3) …
  expect(clean).toBeVisible(); // … and still in the reading order
  fireEvent.click(clean);
  fireEvent.keyDown(clean, { key: "Enter" });
  fireEvent.keyDown(clean, { key: " " });
  expect(onClean).not.toHaveBeenCalled();
}

function doneActionsAreDisabledWhileUndoing() {
  const onHome = vi.fn();
  const onUndo = vi.fn();
  const onHistory = vi.fn();
  render(() => (
    <Done
      result={CLEANED as CleanResult}
      disk={{ disk: CLEANED.disk, goalBytes: 80e9 }}
      undoing={true}
      undoBusy={false}
      undoError={null}
      onHome={onHome}
      onUndo={onUndo}
      onHistory={onHistory}
    />
  ));
  for (const name of ["Back to home", "Undo", "View history"]) {
    const b = screen.getByRole("button", { name });
    expect(b).toBeDisabled();
    fireEvent.click(b);
  }
  expect([onHome, onUndo, onHistory].map((f) => f.mock.calls.length)).toEqual([0, 0, 0]);
}

function disabledTreatmentIsTheDesignSystems() {
  expect(css("../../styles/organic.css")).toMatch(/\.btn:disabled\s*\{\s*opacity:\s*0\.45;\s*cursor:\s*not-allowed;/);
  expect(css("../../shell/shell.css")).toMatch(/\.nav-item\[aria-disabled="true"\]\s*\{[^}]*opacity:\s*var\(--opacity-disabled\);/);
}

describe("disabled actions (REQ-DCLEAN-016)", () => {
  it("Clean is disabled at 0 B and never fires (A1, A2, A3)", cleanIsDisabledAtZeroAndNeverFires);
  it("done actions are disabled while an Undo runs (A2, A3)", doneActionsAreDisabledWhileUndoing);
  it("disabled controls use the design system's 45% treatment (A2)", disabledTreatmentIsTheDesignSystems);
});

// ── REQ-DCLEAN-020.A4 · focus follows the phase ──────────────────

async function focusMovesToEachPhaseHeading() {
  const finish = await startScan(view(), { clean: () => CLEANED });
  await waitFor(() => expect(screen.getByRole("heading", { name: "Looking around…" })).toHaveFocus());
  finish();
  await screen.findByTestId("results");
  await waitFor(() => expect(screen.getByTestId("results-heading")).toHaveFocus());
  fireEvent.click(screen.getByTestId("clean"));
  const dialog = await screen.findByRole("dialog");
  await flush();
  // The dialog opens on Cancel (REQ-DCLEAN-022.A2), not on a heading.
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  fireEvent.click(within(dialog).getByRole("button", { name: "Clean now" }));
  await screen.findByTestId("done");
  await waitFor(() => expect(screen.getByTestId("done-heading")).toHaveFocus());
  fireEvent.click(screen.getByRole("button", { name: "Back to home" }));
  await screen.findByTestId("idle");
  await waitFor(() => expect(screen.getByRole("heading", { level: 1 })).toHaveFocus());
}

async function firstRenderLeavesFocusAlone() {
  engine({ load_cleanup_view: () => view() });
  render(() => <App />);
  await screen.findByTestId("idle");
  await flush();
  expect(document.activeElement).toBe(document.body);
}

describe("keyboard focus order (REQ-DCLEAN-020)", () => {
  it("each phase change focuses the new phase's heading (A4)", focusMovesToEachPhaseHeading);
  it("the first render does not move focus (A4, REQ-DCLEAN-010.A5)", firstRenderLeavesFocusAlone);
});

// ── REQ-DCLEAN-021 · roles, names and states ─────────────────────

async function checkboxIsNamedByCategoryAndDescribedBySafety() {
  const results = await toResults();
  const a = within(results).getByRole("checkbox", { name: "Category A" });
  expect(a).toHaveAccessibleDescription("Safe");
  expect(within(results).getByRole("checkbox", { name: "Category B" })).toHaveAccessibleDescription("Review");
  expect(a).not.toHaveAccessibleName("Select");
}

async function disclosureExposesStateAndRegion() {
  const results = await toResults();
  const d = within(results).getByRole("button", { name: "Show items in Category A" });
  expect(d).toHaveAttribute("aria-expanded", "false");
  const region = document.getElementById(d.getAttribute("aria-controls")!);
  expect(region).not.toBeNull();
  expect(region).not.toBeVisible();
  fireEvent.click(d);
  expect(d).toHaveAttribute("aria-expanded", "true");
  expect(region).toBeVisible();
}

async function progressBarsExposeValueAndRange() {
  let finishClean!: (v: unknown) => void;
  const finish = await startScan(view(), { clean: () => new Promise((r) => (finishClean = r)) });
  const scanBar = screen.getByRole("progressbar", { name: "Scan progress" });
  expect([scanBar.getAttribute("aria-valuemin"), scanBar.getAttribute("aria-valuemax"), scanBar.getAttribute("aria-valuenow")]).toEqual(["0", "100", "0"]);
  emitScan(0.5, []);
  expect(scanBar).toHaveAttribute("aria-valuenow", "50");
  finish();
  await screen.findByTestId("results");
  fireEvent.click(screen.getByTestId("clean"));
  fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Clean now" }));
  const cleanBar = await screen.findByRole("progressbar", { name: "Clean progress" });
  expect([cleanBar.getAttribute("aria-valuemin"), cleanBar.getAttribute("aria-valuemax")]).toEqual(["0", "100"]);
  finishClean(CLEANED);
}

async function meterAndRingHaveTextEquivalents() {
  engine({ load_cleanup_view: () => view() });
  render(() => <App />);
  await screen.findByTestId("idle");
  const images = screen.getAllByRole("img", { name: "74 GB free of 512 GB" });
  expect(images.map((i) => i.getAttribute("data-testid")).sort()).toEqual(["meter-bar", "usage-ring"]);
  // Decorative icons are hidden; the only labelled graphics are the logo and these two.
  for (const svg of document.querySelectorAll("svg")) {
    const hidden = svg.closest('[aria-hidden="true"]') !== null;
    const labelled = svg.getAttribute("role") === "img" && !!svg.getAttribute("aria-label");
    expect(hidden || labelled).toBe(true);
  }
}

describe("roles, names and states (REQ-DCLEAN-021)", () => {
  it("checkboxes are named by category and described by their safety tag (A1, A5)", checkboxIsNamedByCategoryAndDescribedBySafety);
  it("disclosures expose expanded state and their region (A2)", disclosureExposesStateAndRegion);
  it("scan and clean bars are progress indicators with value and range (A3)", progressBarsExposeValueAndRange);
  it("the ring and sidebar bar carry a text equivalent; icons are hidden (A4)", meterAndRingHaveTextEquivalents);
});

// ── REQ-DCLEAN-023 · announcements ───────────────────────────────

async function announcesScanStartCategoriesAndTotal() {
  const finish = await startScan();
  expect(announcer()).toHaveAttribute("aria-live", "polite");
  expect(announcer()).toHaveTextContent(SCAN_STARTED);
  await flush();
  const focused = document.activeElement;
  emitScan(0.2, []);
  expect(announcer()).toHaveTextContent(SCAN_STARTED); // a tick alone is not announced (A1)
  emitScan(0.5, ["cat_a"]);
  expect(announcer()).toHaveTextContent(/^Category A checked$/);
  expect(document.activeElement).toBe(focused); // announcing never moves focus
  finish();
  await screen.findByTestId("results");
  await waitFor(() => expect(announcer()).toHaveTextContent(/^Found 4.8 GB you can clear$/));
}

async function announcesCleanStartAndFreedTotal() {
  await toResults(view(), { clean: () => CLEANED });
  fireEvent.click(screen.getByTestId("clean"));
  fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Clean now" }));
  expect(announcer()).toHaveTextContent(CLEAN_STARTED);
  await screen.findByTestId("done");
  await waitFor(() => expect(announcer()).toHaveTextContent(/^4.2 GB cleared$/));
  expect(announcer().contains(document.activeElement)).toBe(false);
}

async function errorsAreAnnouncedInAnAlert() {
  engine({
    load_cleanup_view: () => view(),
    scan: () => Promise.reject({ code: "lock-held", title: "Locked", detail: "held", retryable: true }),
  });
  render(() => <App />);
  fireEvent.click(within(await screen.findByTestId("idle")).getByRole("button", { name: "Start scan" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Another disky process is using the snapshot");
  expect(alert.contains(document.activeElement)).toBe(false);
}

describe("status announcements (REQ-DCLEAN-023)", () => {
  it("announces scan start, each finished category and the total, politely (A1, A2)", announcesScanStartCategoriesAndTotal);
  it("announces clean start and the freed total (A2)", announcesCleanStartAndFreedTotal);
  it("errors are announced by an alert region without taking focus", errorsAreAnnouncedInAnAlert);
});

// ── REQ-DCLEAN-030.A3 · item cap ─────────────────────────────────

function capsItemsAtTheLargest200WithMore() {
  const n = RESULTS_ITEM_CAP + 50;
  const items = Array.from({ length: n }, (_, i) => ({ name: `f${i}`, path: `/f${i}`, bytes: i + 1 }));
  const v = parseCleanupView(view({ categories: [] }));
  v.categories = [{ id: "big", name: "Big", description: "d", icon: "box", safety: "safe", bytes: 1, items }];
  render(() => (
    <Results
      view={v}
      state={{ ...INITIAL_STATE, phase: "results", expanded: "big" }}
      onToggleExpand={() => undefined}
      onToggleSelect={() => undefined}
      onSelectSafe={() => undefined}
      onSelectAll={() => undefined}
      onBack={() => undefined}
    />
  ));
  const list = screen.getByTestId("category-items");
  const rows = list.querySelectorAll("tr.result-item");
  expect(rows).toHaveLength(RESULTS_ITEM_CAP);
  expect(rows[0]).toHaveTextContent(`f${n - 1}`); // largest first
  expect(within(list).getByTestId("more-items")).toHaveTextContent(/^\+50 more$/);
}

describe("render budgets (REQ-DCLEAN-030)", () => {
  it("a category lists its 200 largest items, then +N more (A3)", capsItemsAtTheLargest200WithMore);
});
