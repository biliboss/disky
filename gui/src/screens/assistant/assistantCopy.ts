/**
 * Assistant copy (REQ-DSCR-009, REQ-DSCR-010), verbatim from the snapshot's
 * Assistant markup and `renderVals`. Pure, so each wording rule is
 * testable. The suggestion texts themselves are the engine's
 * (REQ-DSCR-008.A2); this module only words the chrome around them.
 */
import type { Agent, CheckFrequency, SuggestionKey } from "../../lib/contract";
import { formatBytes } from "../../lib/format";

export const ASSISTANT_HEADING = "Assistant";

/**
 * The header's status line: "Watching · <agent> · checks <frequency>", or
 * "Paused" with proactive checks off (REQ-DSCR-009.A1).
 *
 * @implements REQ-DSCR-009
 */
export function statusLine(agentName: string, checks: { enabled: boolean; frequency: CheckFrequency }): string {
  return checks.enabled ? `Watching · ${agentName} · checks ${checks.frequency}` : "Paused";
}

/** An empty feed (REQ-DSCR-009.A4). */
export const EMPTY_FEED = "Nothing to review. You’ll hear from the assistant when something changes.";

/**
 * A card's two buttons (REQ-DSCR-009.A2): the goal card's "Review & confirm"
 * / "Not now", every other card's "Review" / "Dismiss".
 *
 * @implements REQ-DSCR-009
 */
export function actionLabels(key: SuggestionKey): { primary: string; secondary: string } {
  return key === "goal" ? { primary: "Review & confirm", secondary: "Not now" } : { primary: "Review", secondary: "Dismiss" };
}

/** The kind tag's class, as the snapshot's feed (`tagClass`). */
export function kindTagClass(key: SuggestionKey): string {
  switch (key) {
    case "goal":
      return "tag tag-accent";
    case "ok":
      return "tag tag-accent-2";
    case "left":
      return "tag tag-outline";
    default:
      return "tag tag-neutral";
  }
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * A card's "when", in the snapshot's style: "Just now", "2h ago",
 * "Yesterday", "3 days ago". Unknown reads as nothing.
 */
export function whenLabel(at: string | null, now: number = Date.now()): string {
  if (at === null) return "";
  const t = Date.parse(at);
  if (Number.isNaN(t)) return "";
  const d = now - t;
  if (d < HOUR) return d < 5 * MINUTE ? "Just now" : `${Math.floor(d / MINUTE)}m ago`;
  if (d < DAY) return `${Math.floor(d / HOUR)}h ago`;
  if (d < 2 * DAY) return "Yesterday";
  return `${Math.floor(d / DAY)} days ago`;
}

// ── Space goal card (REQ-DSCR-010.A1) ─────────────────────────────

export const GOAL_TITLE = "Space goal";
export const KEEP_AT_LEAST = "Keep at least";
export const KEEP_FREE = "free";
/** Accessible name of the range control. */
export const GOAL_SLIDER_LABEL = "Space goal";

/** The range: 20 to 200 GB in 5 GB steps (decimal GB, as the store's goal). */
export const GOAL_MIN_GB = 20;
export const GOAL_MAX_GB = 200;
export const GOAL_STEP_GB = 5;
const GB = 1e9;

/** The slider position for a goal in bytes, clamped to the range. */
export function goalToSlider(goalBytes: number): number {
  return Math.min(GOAL_MAX_GB, Math.max(GOAL_MIN_GB, Math.round(goalBytes / GB)));
}

/** The bytes the store saves for a slider position. */
export function sliderToGoal(gb: number): number {
  return gb * GB;
}

/**
 * The fill bar's width: free ÷ goal, capped at 100% (REQ-DSCR-010.A1).
 *
 * @implements REQ-DSCR-010
 */
export function goalFill(freeBytes: number, goalBytes: number): string {
  if (!(goalBytes > 0)) return "100%";
  return `${Math.min(100, (freeBytes / goalBytes) * 100)}%`;
}

/**
 * The goal card's note (REQ-DSCR-010.A1).
 *
 * @implements REQ-DSCR-010
 */
export function goalNote(freeBytes: number): string {
  return `${formatBytes(freeBytes)} free now. When it drops below the goal, the assistant prepares a cleanup and asks you first.`;
}

// ── Proactive checks card (REQ-DSCR-010.A3) ───────────────────────

export const CHECKS_TITLE = "Proactive checks";
export const CHECKS_NOTE = "Never cleans on its own. Every cleanup needs your confirmation.";
export const FREQUENCY_LABELS: Readonly<Record<CheckFrequency, string>> = {
  hourly: "Hourly",
  daily: "Daily",
  weekly: "Weekly",
};

// ── Agent card (REQ-DSCR-010.A4) ─────────────────────────────────

export const AGENT_TITLE = "Agent";
export const AGENT_SUBTITLE = "Uses a coding CLI already on this Mac.";
export const PRIVACY_NOTE = "Only file paths, sizes and dates are shared with the agent. File contents never leave this Mac.";

/**
 * An agent row's status: "Found · <path>" (plus " · in use" for the
 * selected agent), or "Not installed" (REQ-DSCR-010.A4).
 *
 * @implements REQ-DSCR-010
 */
export function agentStatus(agent: Agent, selected: boolean): string {
  if (!agent.found) return "Not installed";
  const found = `Found · ${agent.display_path ?? agent.path ?? ""}`;
  return selected ? `${found} · in use` : found;
}
