/**
 * The design snapshot's Assistant state as the engine's `AssistantView`:
 * 74 of 512 GB free under an 80 GB goal, Claude Code selected, checks on
 * daily, its feed (goal, dev, dl, left) and its AGENTS. The texts are the
 * snapshot's own sentences; tests and the visual check share this. Category
 * ids are those of `snapshotReview.ts`, so a review selects real rows.
 */
import type { AssistantView } from "../../src/lib/contract";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

export function snapshotAssistantView(now: number = Date.now()): AssistantView {
  const at = (ms: number) => new Date(now - ms).toISOString();
  return {
    agent: "claude",
    agent_name: "Claude Code",
    checks: { enabled: true, frequency: "daily" },
    goal_bytes: 80e9,
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    agents: [
      { id: "claude", name: "Claude Code", found: true, path: "/Users/me/.local/bin/claude", display_path: "~/.local/bin/claude" },
      { id: "codex", name: "Codex CLI", found: true, path: "/opt/homebrew/bin/codex", display_path: "/opt/homebrew/bin/codex" },
      { id: "gemini", name: "Gemini CLI", found: false, path: null, display_path: null },
    ],
    suggestions: [
      {
        key: "goal",
        kind: "Goal",
        text: "Free space is down to 74 GB, under your 80 GB goal. I found 12.1 GB in safe categories: caches, temp files, logs and the Trash. Shall I clean them?",
        bytes: 12.1e9,
        categories: ["visual_cat_0", "visual_cat_1", "visual_cat_2", "visual_cat_3", "visual_cat_4"],
        actionable: true,
        at: at(60_000),
      },
      {
        key: "dev",
        kind: "Suggestion",
        text: "Four projects in ~/Projects haven’t been opened since March. Their node_modules and build folders take 7.9 GB and can be reinstalled any time.",
        bytes: 7.9e9,
        categories: ["visual_cat_7"],
        actionable: true,
        at: at(2 * HOUR + 60_000),
      },
      {
        key: "dl",
        kind: "Suggestion",
        text: "Downloads has installers for apps you already have (Xcode, Figma) plus a folder of scans you haven’t opened in 6 months.",
        bytes: 5.3e9,
        categories: ["visual_cat_6"],
        actionable: true,
        at: at(DAY + HOUR),
      },
      {
        key: "left",
        kind: "Heads-up",
        text: "Sketch and Zoom were uninstalled but left 2.2 GB of support files in ~/Library.",
        bytes: 2.7e9,
        categories: ["visual_cat_5"],
        actionable: true,
        at: at(3 * DAY + HOUR),
      },
    ],
  };
}
