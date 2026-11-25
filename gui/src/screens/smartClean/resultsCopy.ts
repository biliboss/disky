/**
 * Results-phase copy and figures (REQ-DCLEAN-005, REQ-DCLEAN-006,
 * REQ-DCLEAN-019). Pure, so each wording rule is testable. Every figure is
 * an engine byte count; this module only adds up the categories the user can
 * see or has selected, and formats at the end (REQ-DCLEAN-028).
 */
import type { CategoryView, Safety } from "../../lib/contract";
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

/** Empty results (REQ-DCLEAN-019.A1). */
export const EMPTY_RESULTS = "Nothing worth clearing right now.";

/** An expanded category with no items (REQ-DCLEAN-019.A3). */
export const NO_ITEMS = "No items";
