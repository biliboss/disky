/**
 * Cleaning-phase copy (REQ-DCLEAN-008).
 */
import { DELETE_MODE, type CleanMode } from "../../lib/contract";

export const CLEANING_HEADING = "Tidying up…";

/**
 * The step label (A1, A3): "Moving <category>…" for a Trash clean,
 * "Deleting <category>…" for a permanent one. The snapshot always says
 * "Moving"; following the mode is a deliberate deviation.
 *
 * @implements REQ-DCLEAN-008
 */
export function stepLabel(mode: CleanMode, categoryName: string): string {
  return `${mode === DELETE_MODE ? "Deleting" : "Moving"} ${categoryName}…`;
}
