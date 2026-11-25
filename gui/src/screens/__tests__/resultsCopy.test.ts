import { describe, expect, it } from "vitest";
import { formatBytes } from "../../lib/format";
import { parseCleanupView } from "../../lib/contract";
import {
  EMPTY_RESULTS,
  NO_ITEMS,
  MODE_LABELS,
  cleanLabel,
  confirmTitle,
  disclosureLabel,
  initialMode,
  modeNote,
  resultsHeading,
  resultsKicker,
  safetyNote,
  safetyTag,
  selectedBytes,
  totalBytes,
  moreItemsLabel,
  visibleItems,
} from "../smartClean/resultsCopy";
import { snapshotReviewCategories } from "../../../tests/fixtures/snapshotReview";

const CATS = [
  { id: "a", bytes: 4_200_000_000 },
  { id: "b", bytes: 600_000_000 },
  { id: "c", bytes: 1_800_000_000 },
];

function headingTotalIsTheSumOfListedCategories() {
  expect(totalBytes(CATS)).toBe(6_600_000_000);
  expect(resultsHeading(CATS)).toBe("Found 6.6 GB you can clear");
  expect(resultsHeading(snapshotReviewCategories())).toBe("Found 28 GB you can clear");
}

function kickerHasBothFormsSingularAndPlural() {
  expect(resultsKicker(8, null)).toBe("Scan finished · 8 categories");
  expect(resultsKicker(1, null)).toBe("Scan finished · 1 category");
  expect(resultsKicker(3, "Claude Code")).toBe("Prepared by Claude Code · review before cleaning");
}

function safetyTagClasses() {
  expect(safetyTag("safe")).toEqual({ label: "Safe", class: "tag tag-accent-2" });
  expect(safetyTag("review")).toEqual({ label: "Review", class: "tag tag-accent" });
}

function disclosureIsNamedByItsCategory() {
  expect(disclosureLabel("App caches")).toBe("Show items in App caches");
}

function cleanLabelFollowsTheSelectedTotal() {
  expect(selectedBytes(CATS, ["a", "c"])).toBe(6_000_000_000);
  expect(cleanLabel(selectedBytes(CATS, ["a", "c"]))).toBe("Clean 6 GB");
  expect(cleanLabel(selectedBytes(CATS, ["b"]))).toBe("Clean 600 MB");
  expect(selectedBytes(CATS, [])).toBe(0);
}

function safetyNoteFollowsTrashByDefault() {
  expect(safetyNote(true)).toBe("Items move to the Trash and can be restored from History for 30 days.");
  expect(safetyNote(false)).toBe("Trash is off in Settings, so items will be deleted permanently.");
}

function emptyCopyMatchesTheSpec() {
  expect(EMPTY_RESULTS).toBe("Nothing worth clearing right now.");
  expect(NO_ITEMS).toBe("No items");
}

/** A formatted size back to bytes, with the formatter's worst-case rounding error. */
function unformat(label: string): { bytes: number; error: number } {
  const m = /^([\d.]+) (GB|MB)$/.exec(label);
  if (!m) throw new Error(`unexpected size label ${label}`);
  const n = Number(m[1]);
  if (m[2] === "MB") return { bytes: n * 1e6, error: 0.5e6 };
  return { bytes: n * 1e9, error: n >= 100 ? 0.5e9 : 0.05e9 };
}

function itemSizesAddUpToTheCategorySize() {
  // The fixture passes the strict parser, which already refuses a category
  // whose bytes are not exactly the sum of its items.
  const view = parseCleanupView({
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: null,
    snapshot_taken_at: null,
    is_stale: false,
    preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] },
    large_files_count: 0,
    skipped_paths: 0,
    categories: snapshotReviewCategories(),
    catalogue: [],
  });
  for (const c of view.categories) {
    expect(totalBytes(c.items.map((it, i) => ({ id: String(i), bytes: it.bytes })))).toBe(c.bytes);
    // What the user reads: the formatted item sizes add up to the formatted
    // category size, give or take each label's rounding.
    const shown = c.items.map((it) => unformat(formatBytes(it.bytes)));
    const category = unformat(formatBytes(c.bytes));
    const sum = shown.reduce((acc, s) => acc + s.bytes, 0);
    const slack = shown.reduce((acc, s) => acc + s.error, category.error);
    expect(Math.abs(sum - category.bytes)).toBeLessThanOrEqual(slack + 1);
  }
}

// ── Confirm dialog (REQ-DCLEAN-007) ──────────────────────────────

function confirmTitleTotalEqualsTheFooterTotal() {
  for (const bytes of [0, 600_000_000, 4_200_000_000, 4_800_000_000, 1_234_567_890_123]) {
    expect(confirmTitle(bytes)).toBe(`${cleanLabel(bytes)}?`);
  }
  expect(confirmTitle(4_200_000_000)).toBe("Clean 4.2 GB?");
}

function modeNoteCoversBothModes() {
  expect(modeNote("trash")).toBe("You can restore everything from History for 30 days.");
  expect(modeNote("delete")).toBe("This can't be undone.");
  expect(MODE_LABELS).toEqual({ trash: "Move to Trash", delete: "Delete permanently" });
}

function initialModeFollowsTrashByDefault() {
  expect(initialMode(true)).toBe("trash");
  expect(initialMode(false)).toBe("delete");
}

describe("confirm dialog copy (REQ-DCLEAN-007)", () => {
  it("title total equals the footer total (REQ-DCLEAN-007.A5)", confirmTitleTotalEqualsTheFooterTotal);
  it("the note names each mode's consequence (REQ-DCLEAN-007.A3)", modeNoteCoversBothModes);
  it("the mode starts at the trash-by-default preference (REQ-DCLEAN-007.A2)", initialModeFollowsTrashByDefault);
});

describe("results copy (REQ-DCLEAN-005, REQ-DCLEAN-006, REQ-DCLEAN-019)", () => {
  it("heading total is the sum of the listed categories (REQ-DCLEAN-005)", headingTotalIsTheSumOfListedCategories);
  it("kicker reads Scan finished or Prepared by (REQ-DCLEAN-005)", kickerHasBothFormsSingularAndPlural);
  it("Safe uses tag-accent-2 and Review uses tag-accent (REQ-DCLEAN-005.A2)", safetyTagClasses);
  it("disclosure is named by its category (REQ-DCLEAN-021.A2)", disclosureIsNamedByItsCategory);
  it("item sizes add up to the category size (REQ-DCLEAN-005.A6)", itemSizesAddUpToTheCategorySize);
  it("Clean label follows the selected total (REQ-DCLEAN-006.A4)", cleanLabelFollowsTheSelectedTotal);
  it("footer note follows trash-by-default (REQ-DCLEAN-006.A5)", safetyNoteFollowsTrashByDefault);
  it("empty and no-items copy (REQ-DCLEAN-019.A1, A3)", emptyCopyMatchesTheSpec);
});

function visibleItemsKeepsAllUpToTheCap() {
  const items = [{ bytes: 1 }, { bytes: 3 }, { bytes: 2 }];
  expect(visibleItems(items, 3)).toEqual({ shown: items, more: 0 }); // engine order kept
}

function visibleItemsShowsTheLargestBeyondTheCap() {
  const items = [{ bytes: 1 }, { bytes: 4 }, { bytes: 2 }, { bytes: 3 }];
  expect(visibleItems(items, 2)).toEqual({ shown: [{ bytes: 4 }, { bytes: 3 }], more: 2 });
  expect(moreItemsLabel(2)).toBe("+2 more");
}

describe("item cap (REQ-DCLEAN-030.A3)", () => {
  it("lists every item up to the cap, in engine order", visibleItemsKeepsAllUpToTheCap);
  it("beyond the cap lists the largest, then +N more", visibleItemsShowsTheLargestBeyondTheCap);
});
