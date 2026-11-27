/**
 * Results-phase copy and figures (REQ-DCLEAN-005, REQ-DCLEAN-006,
 * REQ-DCLEAN-019). Pure, so each wording rule is testable. Every figure is
 * an engine byte count; this module only adds up the categories the user can
 * see or has selected, and formats at the end (REQ-DCLEAN-028).
 */
import type { CategoryView, CleanMode, Safety } from "../../lib/contract";
import { RESULTS_ITEM_CAP } from "../../lib/budgets";
import { formatBytes } from "../../lib/format";

type Sized = Pick<CategoryView, "id" | "bytes">;

/**
 * Sum of engine category totals. Each `bytes` is the engine's own figure
 * (REQ-DCLEAN-027.A3); no file is measured here.
 */
export function totalBytes(categories: readonly Sized[]): number {
  return categories.reduce((acc, c) => acc + c.bytes, 0); // lint-boundary-allow: adds engine category totals, measures no file
}

/** Total of the selected categories (REQ-DCLEAN-006.A4). */
export function selectedBytes(categories: readonly Sized[], selected: readonly string[]): number {
  return totalBytes(categories.filter((c) => selected.includes(c.id)));
}

/**
 * "Found <total> you can clear", the total being every listed category
 * (REQ-DCLEAN-005).
 *
 * @implements REQ-DCLEAN-005
 */
export function resultsHeading(categories: readonly Sized[]): string {
  return `Found ${formatBytes(totalBytes(categories))} you can clear`;
}

/**
 * The kicker: "Scan finished · <n> categories", or "Prepared by <agent> ·
 * review before cleaning" when the assistant opened the review.
 *
 * @implements REQ-DCLEAN-005
 */
export function resultsKicker(count: number, openedBy: string | null): string {
  if (openedBy !== null) return `Prepared by ${openedBy} · review before cleaning`;
  return `Scan finished · ${count} ${count === 1 ? "category" : "categories"}`;
}

/**
 * The safety tag (REQ-DCLEAN-005.A2): Safe → `tag-accent-2`, Review →
 * `tag-accent`.
 *
 * @implements REQ-DCLEAN-005
 */
export function safetyTag(safety: Safety): { label: string; class: string } {
  return safety === "safe" ? { label: "Safe", class: "tag tag-accent-2" } : { label: "Review", class: "tag tag-accent" };
}

/** The disclosure control's accessible name (REQ-DCLEAN-021.A2). */
export function disclosureLabel(categoryName: string): string {
  return `Show items in ${categoryName}`;
}

/**
 * Footer primary action (REQ-DCLEAN-006.A4).
 *
 * @implements REQ-DCLEAN-006
 */
export function cleanLabel(selectedTotal: number): string {
  return `Clean ${formatBytes(selectedTotal)}`;
}

/**
 * Footer safety note, truthful to the trash-by-default preference
 * (REQ-DCLEAN-006.A5).
 *
 * @implements REQ-DCLEAN-006
 */
export function safetyNote(trashByDefault: boolean): string {
  return trashByDefault
    ? "Items move to the Trash and can be restored from History for 30 days."
    : "Trash is off in Settings, so items will be deleted permanently.";
}

/**
 * The confirm dialog's title, "Clean <total>?" (REQ-DCLEAN-007). Formatted
 * exactly as the footer's {@link cleanLabel}, so the two totals match (A5).
 *
 * @implements REQ-DCLEAN-007
 */
export function confirmTitle(selectedTotal: number): string {
  return `${cleanLabel(selectedTotal)}?`;
}

/** The dialog's two clean modes, in the snapshot's order. */
export const MODE_LABELS: Readonly<Record<CleanMode, string>> = {
  trash: "Move to Trash",
  delete: "Delete permanently",
};

/** The mode the dialog opens with: the trash-by-default preference (REQ-DCLEAN-007.A2). */
export function initialMode(trashByDefault: boolean): CleanMode {
  return trashByDefault ? "trash" : "delete"; // lint-boundary-allow: clean mode, not a category
}

/**
 * The note under the mode switch (REQ-DCLEAN-007.A3).
 *
 * @implements REQ-DCLEAN-007
 */
export function modeNote(mode: CleanMode): string {
  return mode === "trash" ? "You can restore everything from History for 30 days." : "This can't be undone."; // lint-boundary-allow: clean mode, not a category
}

/** Empty results (REQ-DCLEAN-019.A1). */
export const EMPTY_RESULTS = "Nothing worth clearing right now.";

/** An expanded category with no items (REQ-DCLEAN-019.A3). */
export const NO_ITEMS = "No items";

/**
 * The items an expanded category lists (REQ-DCLEAN-030.A3): all of them up
 * to the cap, else the `cap` largest, largest first, and how many are left
 * out for the "+N more" line.
 *
 * @implements REQ-DCLEAN-030
 */
export function visibleItems<T extends { bytes: number }>(
  items: readonly T[],
  cap: number = RESULTS_ITEM_CAP,
): { shown: readonly T[]; more: number } {
  if (items.length <= cap) return { shown: items, more: 0 };
  const shown = [...items].sort((a, b) => b.bytes - a.bytes).slice(0, cap);
  return { shown, more: items.length - cap };
}

/** The line under a capped item list (REQ-DCLEAN-030.A3). */
export function moreItemsLabel(more: number): string {
  return `+${more} more`;
}
