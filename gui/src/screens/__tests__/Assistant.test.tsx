import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => undefined }));

import { App } from "../../App";
import { resetDiskState } from "../../lib/diskState";
import { resetSmartCleanStore } from "../smartClean/store";
import { resetAssistantStore } from "../assistant/store";
import type { AssistantView } from "../../lib/contract";
import { snapshotAssistantView } from "../../../tests/fixtures/snapshotAssistant";
import { snapshotReviewCategories } from "../../../tests/fixtures/snapshotReview";

const PREFS = {
  trash_by_default: true,
  skip_recently_used: true,
  safe_only: true,
  warn_below_goal: true,
  goal_bytes: 80e9,
  checks: { enabled: true, frequency: "daily" },
  agent: "claude",
  scheduled_scan: "weekly",
  excluded: [] as string[],
};

/**
 * A stand-in engine: one preferences store that `set_prefs` writes and
 * every view (`load_assistant`, `load_cleanup_view`) reads, so the screen
 * can only show what the store holds.
 */
function fakeEngine(assistant: AssistantView = snapshotAssistantView()) {
  const store = { prefs: structuredClone(PREFS), assistant: structuredClone(assistant) };
  invoke.mockImplementation(async (cmd: string, args: any) => {
    const p = store.prefs as any;
    switch (cmd) {
      case "load_assistant":
        return {
          ...structuredClone(store.assistant),
          goal_bytes: p.goal_bytes,
          checks: structuredClone(p.checks),
          agent: p.agent,
          agent_name: store.assistant.agents.find((a) => a.id === p.agent)?.name ?? "Claude Code",
          suggestions: p.checks.enabled ? structuredClone(store.assistant.suggestions) : [],
        };
      case "load_cleanup_view":
        return {
          disk: store.assistant.disk,
          goal_bytes: p.goal_bytes,
          last_scan_at: new Date().toISOString(),
          snapshot_taken_at: new Date().toISOString(),
          is_stale: false,
          preferences: structuredClone(p),
          large_files_count: 0,
          skipped_paths: 0,
          categories: snapshotReviewCategories(),
          catalogue: snapshotReviewCategories().map(({ id, name, icon, safety }) => ({ id, name, icon, safety })),
        };
      case "set_prefs": {
        const { key, value } = args as { key: string; value: string };
        if (key === "goal_bytes") p.goal_bytes = Number(value);
        else if (key === "checks.enabled") p.checks.enabled = value === "true";
        else if (key === "checks.frequency") p.checks.frequency = value;
        else if (key === "agent") p.agent = value;
        else throw new Error(`unexpected preference ${key}`);
        return structuredClone(p);
      }
      case "clean":
        return {
          mode: "trash",
          freed_bytes: 1e9,
          succeeded: [],
          failed: [],
          history_id: "h1",
          history_entry: null,
          disk: store.assistant.disk,
        };
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
  return store;
}

async function renderAssistant() {
  window.location.hash = "#/assistant";
  render(() => <App />);
  const a = await screen.findByTestId("assistant");
  await within(a).findByTestId("suggestion-goal");
  return a;
}

function setCalls(key: string) {
  return invoke.mock.calls.filter(([cmd, args]) => cmd === "set_prefs" && (args as any).key === key);
}

// ── Feed (REQ-DSCR-009) ─────────────────────────────────────────

async function feedRendersTheEngineCardsWithTheAgentName() {
  fakeEngine();
  const a = await renderAssistant();
  expect(within(a).getByTestId("assistant-status")).toHaveTextContent(/^Watching · Claude Code · checks daily$/);
  expect(within(a).getByRole("heading", { level: 1 })).toHaveTextContent("Assistant");
  const keys = within(a)
    .getAllByRole("article")
    .map((c) => c.dataset.testid);
  expect(keys).toEqual(["suggestion-goal", "suggestion-dev", "suggestion-dl", "suggestion-left"]);
  const goal = within(a).getByTestId("suggestion-goal");
  expect(goal).toHaveClass("assistant-card-goal");
  expect(goal).toHaveTextContent("Claude Code");
  expect(goal).toHaveTextContent(/Free space is down to 74 GB, under your 80 GB goal\./);
  expect(within(goal).getByRole("button", { name: "Review & confirm" })).toBeInTheDocument();
  expect(within(goal).getByRole("button", { name: "Not now" })).toBeInTheDocument();
  expect(within(goal).getByTestId("suggestion-size")).toHaveTextContent("12.1 GB");
  const dev = within(a).getByTestId("suggestion-dev");
  expect(within(dev).getByRole("button", { name: "Review" })).toBeInTheDocument();
  expect(within(dev).getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
}

async function dismissingACardHidesIt() {
  fakeEngine();
  const a = await renderAssistant();
  fireEvent.click(within(within(a).getByTestId("suggestion-dev")).getByRole("button", { name: "Dismiss" }));
  fireEvent.click(within(within(a).getByTestId("suggestion-goal")).getByRole("button", { name: "Not now" }));
  expect(within(a).queryByTestId("suggestion-dev")).toBeNull();
  expect(within(a).queryByTestId("suggestion-goal")).toBeNull();
  expect(within(a).getByTestId("suggestion-dl")).toBeInTheDocument();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "set_prefs" || cmd === "clean")).toHaveLength(0);
}

async function checksOffShowsTheEmptyFeedAndPaused() {
  fakeEngine();
  const a = await renderAssistant();
  const sw = within(a).getByRole("switch", { name: "Proactive checks" });
  expect(sw).toHaveAttribute("aria-checked", "true");
  fireEvent.click(sw);
  await waitFor(() => expect(sw).toHaveAttribute("aria-checked", "false"));
  expect(setCalls("checks.enabled").map(([, a]) => (a as any).value)).toEqual(["false"]);
  expect(within(a).getByTestId("assistant-status")).toHaveTextContent(/^Paused$/);
  expect(within(a).getByTestId("assistant-empty")).toHaveTextContent(
    "Nothing to review. You’ll hear from the assistant when something changes.",
  );
  expect(within(a).queryAllByRole("article")).toHaveLength(0);
}

async function badgeAndShortcutCountTheActionableCards() {
  fakeEngine();
  const a = await renderAssistant();
  const nav = screen.getByRole("navigation", { name: "Main" });
  const entry = within(nav).getByRole("button", { name: /^Assistant/ });
  await waitFor(() => expect(entry.querySelector(".nav-badge")).toHaveTextContent(/^4$/));
  fireEvent.click(within(within(a).getByTestId("suggestion-left")).getByRole("button", { name: "Dismiss" }));
  expect(entry.querySelector(".nav-badge")).toHaveTextContent(/^3$/);

  fireEvent.click(within(nav).getByRole("button", { name: /^Smart Clean/ }));
  const shortcut = await screen.findByTestId("shortcut-assistant");
  expect(shortcut).toHaveTextContent("3 suggestions waiting");
}

async function reviewLandsOnResultsWithTheCardsCategories() {
  fakeEngine();
  const a = await renderAssistant();
  fireEvent.click(within(within(a).getByTestId("suggestion-dev")).getByRole("button", { name: "Review" }));
  const results = await screen.findByTestId("results");
  expect(within(results).getByTestId("results-kicker")).toHaveTextContent(
    /^Prepared by Claude Code · review before cleaning$/,
  );
  const checked = within(results)
    .getAllByRole("checkbox")
    .filter((c) => c.getAttribute("aria-checked") === "true")
    .map((c) => c.getAttribute("aria-label"));
  expect(checked).toEqual(["Developer junk"]);
  const expanded = within(results)
    .getAllByRole("button", { expanded: true })
    .map((b) => b.getAttribute("aria-controls"));
  expect(expanded).toHaveLength(1);
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "clean")).toHaveLength(0);
}

/**
 * A slow engine (a large home takes seconds to build the view): the card's
 * Review shows the spinner after 200 ms and can't be pressed twice, then
 * lands on results when the view arrives.
 */
async function slowReviewShowsTheSpinnerOnItsCard() {
  fakeEngine();
  const a = await renderAssistant();
  const base = invoke.getMockImplementation()!;
  let release!: () => void;
  const gate = new Promise<void>((r) => (release = r));
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "load_cleanup_view") await gate;
    return base(cmd, args);
  });
  const card = within(a).getByTestId("suggestion-left");
  const button = within(card).getByRole("button", { name: "Review" });
  fireEvent.click(button);
  expect(button).toHaveAttribute("aria-busy", "true");
  expect(button).toBeDisabled();
  await waitFor(() => expect(card.querySelector(".busy-spinner")).not.toBeNull(), { timeout: 1000 });
  fireEvent.click(button);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "load_cleanup_view")).toHaveLength(1);
  release();
  const results = await screen.findByTestId("results");
  expect(within(results).getByTestId("results-kicker")).toHaveTextContent(/^Prepared by Claude Code/);
}

/** A failed review says why on the card that was clicked, not off-screen. */
async function reviewFailureIsShownOnItsCard() {
  fakeEngine();
  const a = await renderAssistant();
  const base = invoke.getMockImplementation()!;
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "load_cleanup_view")
      throw { code: "lock-held", title: "snapshot locked", detail: "The snapshot is busy. Try again in a moment.", retryable: true };
    return base(cmd, args);
  });
  const card = within(a).getByTestId("suggestion-left");
  fireEvent.click(within(card).getByRole("button", { name: "Review" }));
  expect(await within(card).findByRole("alert")).toHaveTextContent("The snapshot is busy. Try again in a moment.");
  expect(within(card).getByRole("button", { name: "Review" })).not.toBeDisabled();
  expect(screen.queryByTestId("results")).toBeNull();
}

async function reviewAndConfirmOpensTheDialogAndCleansAsTheAgent() {
  fakeEngine();
  const a = await renderAssistant();
  fireEvent.click(within(within(a).getByTestId("suggestion-goal")).getByRole("button", { name: "Review & confirm" }));
  const dialog = await screen.findByRole("dialog");
  expect(screen.getByTestId("results-kicker")).toHaveTextContent(/^Prepared by Claude Code/);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "clean")).toHaveLength(0);
  fireEvent.click(within(dialog).getByRole("button", { name: "Clean now" }));
  await waitFor(() => expect(invoke.mock.calls.filter(([cmd]) => cmd === "clean")).toHaveLength(1));
  const [, args] = invoke.mock.calls.find(([cmd]) => cmd === "clean")!;
  expect((args as any).openedBy).toBe("Claude Code");
}

// ── Side panel (REQ-DSCR-010) ───────────────────────────────────

async function changingTheGoalSavesItAndBringsTheGoalCardBack() {
  fakeEngine();
  const a = await renderAssistant();
  fireEvent.click(within(within(a).getByTestId("suggestion-goal")).getByRole("button", { name: "Not now" }));
  expect(within(a).queryByTestId("suggestion-goal")).toBeNull();

  const range = within(a).getByRole("slider", { name: "Space goal" });
  expect(range).toHaveAttribute("min", "20");
  expect(range).toHaveAttribute("max", "200");
  expect(range).toHaveAttribute("step", "5");
  fireEvent.input(range, { target: { value: "100" } });
  expect(within(a).getByTestId("assistant-goal")).toHaveTextContent("100 GB");
  fireEvent.change(range, { target: { value: "100" } });
  await waitFor(() => expect(setCalls("goal_bytes").map(([, x]) => (x as any).value)).toEqual(["100000000000"]));
  expect(await within(a).findByTestId("suggestion-goal")).toBeInTheDocument();
  expect(screen.getByText(/^Goal: keep 100 GB free$/)).toBeInTheDocument();
  expect(within(a).getByTestId("assistant-goal-note")).toHaveTextContent(
    "74 GB free now. When it drops below the goal, the assistant prepares a cleanup and asks you first.",
  );
}

async function frequencySaves() {
  fakeEngine();
  const a = await renderAssistant();
  const seg = within(a).getByRole("radiogroup", { name: "Proactive checks" });
  expect(within(seg).getByRole("radio", { name: "Daily" })).toBeChecked();
  fireEvent.click(within(seg).getByRole("radio", { name: "Weekly" }));
  await waitFor(() => expect(within(a).getByTestId("assistant-status")).toHaveTextContent("checks weekly"));
  expect(setCalls("checks.frequency").map(([, x]) => (x as any).value)).toEqual(["weekly"]);
}

async function pickingAnAgentSavesItAndAMissingOneIsDisabled() {
  fakeEngine();
  const a = await renderAssistant();
  expect(within(a).getByTestId("agent-claude")).toHaveTextContent("Found · ~/.local/bin/claude · in use");
  expect(within(a).getByTestId("agent-claude")).toHaveAttribute("aria-pressed", "true");
  const gemini = within(a).getByTestId("agent-gemini");
  expect(gemini).toBeDisabled();
  expect(gemini).toHaveTextContent("Not installed");
  fireEvent.click(gemini);
  fireEvent.click(within(a).getByTestId("agent-codex"));
  await waitFor(() => expect(within(a).getByTestId("assistant-status")).toHaveTextContent("Watching · Codex CLI"));
  expect(setCalls("agent").map(([, x]) => (x as any).value)).toEqual(["codex"]);
  expect(within(a).getByTestId("agent-codex")).toHaveAttribute("aria-pressed", "true");
  expect(within(a).getByTestId("suggestion-goal")).toHaveTextContent("Codex CLI");
}

async function belowGoalNotificationNamesTheAgent() {
  fakeEngine();
  await renderAssistant();
  fireEvent.click(within(screen.getByRole("navigation", { name: "Main" })).getByRole("button", { name: /^Smart Clean/ }));
  await screen.findByTestId("idle");
  await waitFor(() => expect(document.querySelector(".notify-who")).toHaveTextContent(/^Assistant · Claude Code$/));
}

beforeEach(() => {
  invoke.mockReset();
  resetDiskState();
  resetSmartCleanStore();
  resetAssistantStore();
  window.location.hash = "";
});
afterEach(cleanup);

describe("Assistant feed (REQ-DSCR-009)", () => {
  it("renders the engine's cards, the goal card on accent-100 and the agent name", feedRendersTheEngineCardsWithTheAgentName);
  it("Not now / Dismiss hide a card and change nothing (A4)", dismissingACardHidesIt);
  it("with checks off the feed is empty and the status reads Paused", checksOffShowsTheEmptyFeedAndPaused);
  it("the sidebar badge and Smart Clean shortcut count the actionable cards (A5)", badgeAndShortcutCountTheActionableCards);
  it("Review lands on results with the kicker and exactly the card's categories (A3)", reviewLandsOnResultsWithTheCardsCategories);
  it("Review & confirm opens the dialog; Clean now cleans as the agent (A3)", reviewAndConfirmOpensTheDialogAndCleansAsTheAgent);
  it("a slow Review shows the spinner on its card and can't be pressed twice (A3)", slowReviewShowsTheSpinnerOnItsCard);
  it("a failed Review says why on its card (A3)", reviewFailureIsShownOnItsCard);
  it("the below-goal notification reads Assistant · <agent>", belowGoalNotificationNamesTheAgent);
});

describe("Assistant side panel (REQ-DSCR-010)", () => {
  it("a new goal is saved, updates the meter and brings the goal card back (A1, A2)", changingTheGoalSavesItAndBringsTheGoalCardBack);
  it("the check frequency is saved (A3)", frequencySaves);
  it("picking an agent saves it; a missing agent is disabled (A4)", pickingAnAgentSavesItAndAMissingOneIsDisabled);
});
