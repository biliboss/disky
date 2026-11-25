import type { RestoreReport } from "./contract";

/**
 * What a restore could not bring back, and why (REQ-DCLEAN-034.A4): one
 * line per item no longer in the Trash, and one per item whose original
 * path is occupied. Shared by Smart Clean's Undo (REQ-DCLEAN-009.A7) and the
 * History screen's Restore (REQ-DSCR-015.A3).
 */
export function restoreNotices(r: RestoreReport): string[] {
  return [
    ...r.missing.map((p) => `${p} is no longer in Trash, so it couldn't be restored.`),
    ...r.conflicts.map((p) => `${p} wasn't restored: something else is already there. It's still in the Trash.`),
  ];
}
