import { describe, expect, it } from "vitest";
import { ContractError, parseCleanResult, parseCleanupView, parseScanProgress } from "./contract";

/** A view as the engine's `contract::CleanupView` serialises it. */
function validView(): Record<string, any> {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: "2026-11-25T09:00:00Z",
    snapshot_taken_at: "2026-11-25T08:59:00Z",
    is_stale: false,
    preferences: { trash_by_default: true, safe_only: true, notifications: true, assistant: true },
    large_files_count: 8,
    categories: [
      {
        id: "app_caches",
        name: "App caches",
        description: "Rebuilt by each app the next time it runs",
        safety: "safe",
        bytes: 1_500_000_000,
        items: [
          { name: "com.spotify.client", path: "/Users/me/Library/Caches/com.spotify.client", bytes: 900_000_000 },
          { name: "Google", path: "/Users/me/Library/Caches/Google", bytes: 600_000_000 },
        ],
      },
      { id: "logs", name: "Logs & crash reports", description: "d", safety: "review", bytes: 0, items: [] },
    ],
  };
}

function expectContractError(fn: () => unknown, field: string) {
  try {
    fn();
  } catch (e) {
    expect(e).toBeInstanceOf(ContractError);
    expect((e as ContractError).path).toBe(field);
    return;
  }
  throw new Error(`expected a ContractError at ${field}`);
}

function validViewPasses() {
  const raw = validView();
  expect(parseCleanupView(raw)).toEqual(raw);
}

function rejectsUnknownSafety() {
  const raw = validView();
  raw.categories[0].safety = "maybe";
  expectContractError(() => parseCleanupView(raw), "view.categories[0].safety");
}

function rejectsFractionalBytes() {
  const raw = validView();
  raw.categories[0].items[0].bytes = 1.5;
  expectContractError(() => parseCleanupView(raw), "view.categories[0].items[0].bytes");
}

function rejectsStringBytes() {
  const raw = validView();
  raw.disk.free_bytes = "74000000000";
  expectContractError(() => parseCleanupView(raw), "view.disk.free_bytes");
}

function rejectsNegativeBytes() {
  const raw = validView();
  raw.goal_bytes = -1;
  expectContractError(() => parseCleanupView(raw), "view.goal_bytes");
}

function rejectsCategoryBytesThatAreNotTheItemSum() {
  const raw = validView();
  raw.categories[0].bytes += 1;
  expectContractError(() => parseCleanupView(raw), "view.categories[0].bytes");
}

function rejectsMissingFieldsWithoutAPartialList() {
  const raw = validView();
  delete raw.categories[1].description;
  // The first category is fine, but the whole view is refused.
  expectContractError(() => parseCleanupView(raw), "view.categories[1].description");
  const noStale = validView();
  delete noStale.is_stale;
  expectContractError(() => parseCleanupView(noStale), "view.is_stale");
  expectContractError(() => parseCleanupView(null), "view");
}

function progressFractionMustBeInZeroToOne() {
  const ok = { fraction: 0.5, current_path: "/Users/me", completed_category_ids: ["temp"] };
  expect(parseScanProgress(ok)).toEqual(ok);
  expect(parseScanProgress({ ...ok, fraction: 1 }).fraction).toBe(1);
  expectContractError(() => parseScanProgress({ ...ok, fraction: 1.01 }), "progress.fraction");
  expectContractError(() => parseScanProgress({ ...ok, fraction: -0.1 }), "progress.fraction");
  expectContractError(() => parseScanProgress({ ...ok, fraction: Number.NaN }), "progress.fraction");
}

function cleanResultParsesWithAndWithoutEntry() {
  const entry = {
    id: "2026-11-25_09-00-00-00ab",
    ts: "2026-11-25T09:00:00.000Z",
    mode: "trash",
    categories: ["app_caches"],
    bytes: 900,
    items: [{ original_path: "/a", trash_path: "/t/a", bytes: 900, status: "in_trash" }],
    restored_at: null,
  };
  const raw = {
    mode: "trash",
    freed_bytes: 900,
    succeeded: ["/a"],
    failed: [{ path: "/gone", reason: "missing" }],
    history_entry: entry,
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 80e9 },
  };
  expect(parseCleanResult(raw)).toEqual(raw);
  expect(parseCleanResult({ ...raw, history_entry: null }).history_entry).toBeNull();
  expectContractError(() => parseCleanResult({ ...raw, mode: "shred" }), "result.mode");
  expectContractError(() => parseCleanResult({ ...raw, freed_bytes: 0.5 }), "result.freed_bytes");
}

function cleanResultWithoutDiskIsRejected() {
  const raw: Record<string, unknown> = { mode: "trash", freed_bytes: 0, succeeded: [], failed: [], history_entry: null };
  expectContractError(() => parseCleanResult(raw), "result.disk");
  const bad = { ...raw, disk: { volume: "v", capacity_bytes: 1, free_bytes: -1 } };
  expectContractError(() => parseCleanResult(bad), "result.disk.free_bytes");
}

function largeFilesCountMustBeAWholeCount() {
  const missing = validView();
  delete missing.large_files_count;
  expectContractError(() => parseCleanupView(missing), "view.large_files_count");
  const fractional = { ...validView(), large_files_count: 1.5 };
  expectContractError(() => parseCleanupView(fractional), "view.large_files_count");
}

describe("GUI data contract (REQ-DCLEAN-027, REQ-DCLEAN-029)", () => {
  it("a valid engine view passes unchanged (REQ-DCLEAN-027)", validViewPasses);
  it("safety other than safe/review is rejected (REQ-DCLEAN-027.A2)", rejectsUnknownSafety);
  it("fractional bytes are rejected (REQ-DCLEAN-027.A1)", rejectsFractionalBytes);
  it("string bytes are rejected (REQ-DCLEAN-027.A1)", rejectsStringBytes);
  it("negative bytes are rejected (REQ-DCLEAN-029.A4)", rejectsNegativeBytes);
  it("category bytes must equal the item sum (REQ-DCLEAN-027.A3)", rejectsCategoryBytesThatAreNotTheItemSum);
  it("missing fields fail the whole payload (REQ-DCLEAN-029.A4)", rejectsMissingFieldsWithoutAPartialList);
  it("scan progress fraction stays in 0..1 (REQ-DCLEAN-027)", progressFractionMustBeInZeroToOne);
  it("clean results parse with or without a history entry (REQ-DCLEAN-027)", cleanResultParsesWithAndWithoutEntry);
  it("a clean result without its post-clean disk is rejected (REQ-DCLEAN-002.A4)", cleanResultWithoutDiskIsRejected);
  it("the large-files count is a whole number from the engine (REQ-DCLEAN-003.A4)", largeFilesCountMustBeAWholeCount);
});
