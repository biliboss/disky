import { describe, expect, it } from "vitest";
import {
  EMPTY_FEED,
  PRIVACY_NOTE,
  actionLabels,
  agentStatus,
  goalFill,
  goalNote,
  goalToSlider,
  kindTagClass,
  sliderToGoal,
  statusLine,
  whenLabel,
} from "../assistant/assistantCopy";

function statusLineNamesTheAgentAndFrequencyOrPaused() {
  expect(statusLine("Claude Code", { enabled: true, frequency: "daily" })).toBe("Watching · Claude Code · checks daily");
  expect(statusLine("Codex CLI", { enabled: true, frequency: "hourly" })).toBe("Watching · Codex CLI · checks hourly");
  expect(statusLine("Claude Code", { enabled: false, frequency: "weekly" })).toBe("Paused");
}

function emptyFeedAndPrivacyCopyAreTheSnapshots() {
  expect(EMPTY_FEED).toBe("Nothing to review. You’ll hear from the assistant when something changes.");
  expect(PRIVACY_NOTE).toBe(
    "Only file paths, sizes and dates are shared with the agent. File contents never leave this Mac.",
  );
}

function buttonsAndTagsFollowTheCardKind() {
  expect(actionLabels("goal")).toEqual({ primary: "Review & confirm", secondary: "Not now" });
  for (const k of ["dev", "dl", "left"] as const) expect(actionLabels(k)).toEqual({ primary: "Review", secondary: "Dismiss" });
  expect(kindTagClass("goal")).toBe("tag tag-accent");
  expect(kindTagClass("ok")).toBe("tag tag-accent-2");
  expect(kindTagClass("dev")).toBe("tag tag-neutral");
  expect(kindTagClass("dl")).toBe("tag tag-neutral");
  expect(kindTagClass("left")).toBe("tag tag-outline");
}

function whenReadsLikeTheSnapshot() {
  const now = Date.parse("2026-11-25T12:00:00Z");
  const ago = (ms: number) => new Date(now - ms).toISOString();
  expect(whenLabel(ago(60_000), now)).toBe("Just now");
  expect(whenLabel(ago(2 * 3_600_000), now)).toBe("2h ago");
  expect(whenLabel(ago(30 * 3_600_000), now)).toBe("Yesterday");
  expect(whenLabel(ago(3 * 86_400_000), now)).toBe("3 days ago");
  expect(whenLabel(null, now)).toBe("");
}

function goalCardFiguresComeFromTheEngine() {
  expect(goalFill(74e9, 80e9)).toBe(`${(74 / 80) * 100}%`);
  expect(goalFill(120e9, 80e9)).toBe("100%"); // capped
  expect(goalNote(74e9)).toBe(
    "74 GB free now. When it drops below the goal, the assistant prepares a cleanup and asks you first.",
  );
  expect(goalToSlider(80e9)).toBe(80);
  expect(goalToSlider(5e9)).toBe(20); // clamped to the range
  expect(goalToSlider(900e9)).toBe(200);
  expect(sliderToGoal(95)).toBe(95e9);
}

function agentRowsSayFoundInUseOrNotInstalled() {
  const claude = { id: "claude", name: "Claude Code", found: true, path: "/Users/me/.local/bin/claude", display_path: "~/.local/bin/claude" };
  expect(agentStatus(claude, true)).toBe("Found · ~/.local/bin/claude · in use");
  expect(agentStatus(claude, false)).toBe("Found · ~/.local/bin/claude");
  expect(agentStatus({ id: "gemini", name: "Gemini CLI", found: false, path: null, display_path: null }, false)).toBe(
    "Not installed",
  );
}

describe("Assistant copy (REQ-DSCR-009, REQ-DSCR-010)", () => {
  it("status line names the agent and frequency, or Paused (REQ-DSCR-009.A1)", statusLineNamesTheAgentAndFrequencyOrPaused);
  it("empty feed and privacy copy are the snapshot's (REQ-DSCR-009.A4, REQ-DSCR-010.A4)", emptyFeedAndPrivacyCopyAreTheSnapshots);
  it("buttons and tags follow the card kind (REQ-DSCR-009.A2)", buttonsAndTagsFollowTheCardKind);
  it("when reads like the snapshot (REQ-DSCR-009.A2)", whenReadsLikeTheSnapshot);
  it("goal card figures come from the engine (REQ-DSCR-010.A1)", goalCardFiguresComeFromTheEngine);
  it("agent rows say Found, in use or Not installed (REQ-DSCR-010.A4)", agentRowsSayFoundInUseOrNotInstalled);
});
