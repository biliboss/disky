/**
 * Idle-phase copy (REQ-DCLEAN-003). Pure, so each wording rule is testable.
 */
import { formatBytes, relativeTime } from "../../lib/format";

export const IDLE_HEADING = "Let’s clear some room.";

const DESCRIPTION_LEAD = "Smart Clean looks through caches, temporary files, logs and app leftovers.";

/**
 * The description stays truthful to the mode (REQ-DCLEAN-003.A5): the Trash
 * promise only when trash-by-default is on.
 */
export function idleDescription(trashByDefault: boolean): string {
  return trashByDefault
    ? `${DESCRIPTION_LEAD} Nothing is removed until you review it, and everything goes to the Trash first.`
    : `${DESCRIPTION_LEAD} Nothing is removed until you review it.`;
}

/** Kicker from the newest snapshot's timestamp (REQ-DCLEAN-003.A2). */
export function lastScanKicker(lastScanAt: string | null, now: number = Date.now()): string {
  return lastScanAt === null ? "No scans yet" : `Last scan ${relativeTime(lastScanAt, now)}`;
}

/** Assistant shortcut line (REQ-DCLEAN-003.A3). */
export function assistantShortcut(suggestions: number): string {
  return suggestions > 0 ? `${suggestions} suggestions waiting` : "Watching your space goal";
}

/** Large files shortcut line, from the engine's count (REQ-DCLEAN-003.A4). */
export function largeFilesShortcut(count: number): string {
  if (count === 0) return "No files over 2 GB";
  return `${count} ${count === 1 ? "file" : "files"} over 2 GB`;
}

export const SPACE_MAP_SHORTCUT = "See what’s taking up room";

/**
 * The below-goal notification's text (REQ-DCLEAN-010), worded as the
 * snapshot's `toastText`.
 *
 * @implements REQ-DCLEAN-010
 */
export function notificationText(freeBytes: number, goalBytes: number, safeBytes: number): string {
  return `You’re down to ${formatBytes(freeBytes)} free, under your ${formatBytes(goalBytes)} goal. I can clear ${formatBytes(safeBytes)} of safe junk once you confirm.`;
}

/** The notification's sender line, "Assistant · <agent>" (REQ-DSCR-009.A5); "Assistant" until the engine names the agent. */
export function notificationSender(agentName: string | null): string {
  return agentName ? `Assistant · ${agentName}` : "Assistant";
}

export const NOTIFICATION_WHEN = "now";
export const NOTIFICATION_LATER = "Later";
export const NOTIFICATION_REVIEW = "Review cleanup";
