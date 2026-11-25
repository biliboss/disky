import type { ScanSchedule, ToggleKey } from "../../lib/contract";

/**
 * Settings copy, verbatim from the snapshot's `tg` table and Settings markup
 * (REQ-DSCR-016.A2). Each toggle names the store key it saves.
 */
export const SETTINGS_TOGGLES: readonly { key: ToggleKey; label: string; desc: string }[] = [
  {
    key: "trash_by_default",
    label: "Move to Trash instead of deleting",
    desc: "Cleaned items can be restored for 30 days.",
  },
  {
    key: "skip_recently_used",
    label: "Skip recently used files",
    desc: "Ignore anything opened in the last 30 days.",
  },
  {
    key: "safe_only",
    label: "Preselect only safe categories",
    desc: "Downloads, leftovers and developer files wait for your review.",
  },
  {
    key: "warn_below_goal",
    label: "Warn when below my space goal",
    desc: "Shows a notification even when the window is closed.",
  },
];

export const SCHEDULE_TITLE = "Scheduled scan";
export const SCHEDULE_DESC = "Scans in the background and notifies you. Cleaning always waits for you.";
export const SCHEDULE_LABELS: Record<ScanSchedule, string> = { off: "Off", weekly: "Weekly", monthly: "Monthly" };

export const EXCLUDED_TITLE = "Never touch these folders";
export const EXCLUDED_DESC = "Excluded from scans and from the assistant.";
export const ADD_FOLDER = "Add folder";

/** Accessible name of a chip's remove button (gap-fill, REQ-DSCR-016.A3). */
export function removeFolderLabel(folder: string): string {
  return `Remove ${folder}`;
}
