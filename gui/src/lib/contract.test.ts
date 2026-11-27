import { describe, expect, it } from "vitest";
import {
  ContractError,
  EngineError,
  LOCK_HELD,
  parseAssistantView,
  parseCleanProgress,
  parseCleanResult,
  parseCleanupView,
  parseDupesView,
  parseEngineError,
  parseHistoryView,
  parseLargeView,
  parseMapView,
  parseRestoreResult,
  parseScanProgress,
  parseTrashResult,
} from "./contract";
import { snapshotAssistantView } from "../../tests/fixtures/snapshotAssistant";

/** A view as the engine's `contract::CleanupView` serialises it. */
function validView(): Record<string, any> {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: "2026-11-25T09:00:00Z",
    snapshot_taken_at: "2026-11-25T08:59:00Z",
    is_stale: false,
    preferences: { trash_by_default: true, skip_recently_used: true, safe_only: true, warn_below_goal: true, goal_bytes: 80e9, checks: { enabled: true, frequency: "daily" }, agent: "claude", scheduled_scan: "weekly", excluded: [] },
    large_files_count: 8,
    skipped_paths: 0,
    categories: [
      {
        id: "app_caches",
        name: "App caches",
        description: "Rebuilt by each app the next time it runs",
        icon: "app-window",
        safety: "safe",
        bytes: 1_500_000_000,
        items: [
          { name: "com.spotify.client", path: "/Users/me/Library/Caches/com.spotify.client", bytes: 900_000_000 },
          { name: "Google", path: "/Users/me/Library/Caches/Google", bytes: 600_000_000 },
        ],
      },
      { id: "logs", name: "Logs & crash reports", description: "d", icon: "file-text", safety: "review", bytes: 0, items: [] },
    ],
    catalogue: [
      { id: "app_caches", name: "App caches", icon: "app-window", safety: "safe" },
      { id: "logs", name: "Logs & crash reports", icon: "file-text", safety: "review" },
      { id: "trash", name: "Trash", icon: "trash", safety: "safe" },
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
  const ok = {
    fraction: 0.5,
    current_path: "/Users/me",
    completed_category_ids: ["temp"],
    completed_category_bytes: { temp: 1_000 },
  };
  expect(parseScanProgress(ok)).toEqual(ok);
  expect(parseScanProgress({ ...ok, fraction: 1 }).fraction).toBe(1);
  expectContractError(() => parseScanProgress({ ...ok, fraction: 1.01 }), "progress.fraction");
  expectContractError(() => parseScanProgress({ ...ok, fraction: -0.1 }), "progress.fraction");
  expectContractError(() => parseScanProgress({ ...ok, fraction: Number.NaN }), "progress.fraction");
}

function catalogueAndCompletedBytesAreStrict() {
  expect(parseCleanupView(validView()).catalogue.map((c) => c.id)).toEqual(["app_caches", "logs", "trash"]);
  const badSafety = validView();
  badSafety.catalogue[2].safety = "maybe";
  expectContractError(() => parseCleanupView(badSafety), "view.catalogue[2].safety");
  const noCatalogue = validView();
  delete noCatalogue.catalogue;
  expectContractError(() => parseCleanupView(noCatalogue), "view.catalogue");

  const ok = { fraction: 1, current_path: "", completed_category_ids: ["temp", "logs"], completed_category_bytes: { temp: 5 } };
  expect(parseScanProgress(ok).completed_category_bytes).toEqual({ temp: 5 });
  expectContractError(
    () => parseScanProgress({ ...ok, completed_category_bytes: { temp: -1 } }),
    "progress.completed_category_bytes.temp",
  );
  const noBytes: Record<string, unknown> = { ...ok };
  delete noBytes.completed_category_bytes;
  expectContractError(() => parseScanProgress(noBytes), "progress.completed_category_bytes");
}

function cleanResultParsesWithAndWithoutEntry() {
  const entry = {
    id: "2026-11-25_09-00-00-00ab",
    title: "Smart Clean",
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
    history_id: entry.id,
    history_entry: entry,
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 80e9 },
  };
  expect(parseCleanResult(raw)).toEqual(raw);
  expect(parseCleanResult({ ...raw, history_id: null, history_entry: null }).history_entry).toBeNull();
  expectContractError(() => parseCleanResult({ ...raw, history_id: 7 }), "result.history_id");
  expectContractError(() => parseCleanResult({ ...raw, history_entry: { ...entry, title: undefined } }), "result.history_entry.title");
  expectContractError(() => parseCleanResult({ ...raw, mode: "shred" }), "result.mode");
  expectContractError(() => parseCleanResult({ ...raw, freed_bytes: 0.5 }), "result.freed_bytes");
}

function cleanResultWithoutDiskIsRejected() {
  const raw: Record<string, unknown> = { mode: "trash", freed_bytes: 0, succeeded: [], failed: [], history_id: null, history_entry: null };
  expectContractError(() => parseCleanResult(raw), "result.disk");
  const bad = { ...raw, disk: { volume: "v", capacity_bytes: 1, free_bytes: -1 } };
  expectContractError(() => parseCleanResult(bad), "result.disk.free_bytes");
}

function categoryAndCatalogueIconsAreRequired() {
  const raw = validView();
  delete raw.categories[0].icon;
  expectContractError(() => parseCleanupView(raw), "view.categories[0].icon");
  const cat = validView();
  cat.catalogue[1].icon = 7;
  expectContractError(() => parseCleanupView(cat), "view.catalogue[1].icon");
}

function largeFilesCountMustBeAWholeCount() {
  const missing = validView();
  delete missing.large_files_count;
  expectContractError(() => parseCleanupView(missing), "view.large_files_count");
  const fractional = { ...validView(), large_files_count: 1.5 };
  expectContractError(() => parseCleanupView(fractional), "view.large_files_count");
}

function cleanProgressIsValidated() {
  const ok = { category_id: "engine_cat_a", done_items: 1, total_items: 2, fraction: 0.5 };
  expect(parseCleanProgress(ok)).toEqual(ok);
  expectContractError(() => parseCleanProgress({ ...ok, fraction: 1.5 }), "clean.fraction");
  expectContractError(() => parseCleanProgress({ ...ok, done_items: 3 }), "clean.done_items");
  expectContractError(() => parseCleanProgress({ ...ok, total_items: -1 }), "clean.total_items");
  expectContractError(() => parseCleanProgress({ ...ok, category_id: 1 }), "clean.category_id");
}

function restoreResultParsesTheReportAndDisk() {
  const raw = {
    report: { id: "e1", restored: ["/a"], conflicts: [], missing: ["/b"] },
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 70e9 },
  };
  expect(parseRestoreResult(raw)).toEqual(raw);
  expectContractError(() => parseRestoreResult({ ...raw, report: { ...raw.report, missing: [1] } }), "restore.report.missing[0]");
  expectContractError(() => parseRestoreResult({ report: raw.report }), "restore.disk");
}

function engineErrorsKeepTheExitSlug() {
  const held = parseEngineError({ code: LOCK_HELD, title: "snapshot locked", detail: "held", retryable: true });
  expect(held).toBeInstanceOf(EngineError);
  expect(held.code).toBe(LOCK_HELD);
  expect(held.retryable).toBe(true);
  expect(held.detail).toBe("held");
  const plain = parseEngineError("boom");
  expect(plain.code).toBe("generic");
  expect(plain.detail).toBe("boom");
  expect(parseEngineError(held)).toBe(held);
}

function skippedPathsMustBeAWholeCount() {
  const missing = validView();
  delete missing.skipped_paths;
  expectContractError(() => parseCleanupView(missing), "view.skipped_paths");
  expectContractError(() => parseCleanupView({ ...validView(), skipped_paths: -2 }), "view.skipped_paths");
  expect(parseCleanupView({ ...validView(), skipped_paths: 3 }).skipped_paths).toBe(3);
}

describe("GUI contract for cleaning, undo and errors (REQ-DCLEAN-008, REQ-DCLEAN-009, REQ-DCLEAN-018)", () => {
  it("clean progress events are parsed strictly (REQ-DCLEAN-008)", cleanProgressIsValidated);
  it("a restore result carries the report and the disk (REQ-DCLEAN-009.A4, A7)", restoreResultParsesTheReportAndDisk);
  it("engine errors keep disky's exit-code slug (REQ-DCLEAN-018.A1)", engineErrorsKeepTheExitSlug);
  it("the skipped-path count is a whole number (REQ-DCLEAN-018.A2)", skippedPathsMustBeAWholeCount);
});

describe("GUI data contract (REQ-DCLEAN-027, REQ-DCLEAN-029)", () => {
  it("a valid engine view passes unchanged (REQ-DCLEAN-027)", validViewPasses);
  it("safety other than safe/review is rejected (REQ-DCLEAN-027.A2)", rejectsUnknownSafety);
  it("fractional bytes are rejected (REQ-DCLEAN-027.A1)", rejectsFractionalBytes);
  it("string bytes are rejected (REQ-DCLEAN-027.A1)", rejectsStringBytes);
  it("negative bytes are rejected (REQ-DCLEAN-029.A4)", rejectsNegativeBytes);
  it("category bytes must equal the item sum (REQ-DCLEAN-027.A3)", rejectsCategoryBytesThatAreNotTheItemSum);
  it("missing fields fail the whole payload (REQ-DCLEAN-029.A4)", rejectsMissingFieldsWithoutAPartialList);
  it("scan progress fraction stays in 0..1 (REQ-DCLEAN-027)", progressFractionMustBeInZeroToOne);
  it("catalogue and completed category bytes are parsed strictly (REQ-DCLEAN-004.A4)", catalogueAndCompletedBytesAreStrict);
  it("clean results parse with or without a history entry (REQ-DCLEAN-027)", cleanResultParsesWithAndWithoutEntry);
  it("a clean result without its post-clean disk is rejected (REQ-DCLEAN-002.A4)", cleanResultWithoutDiskIsRejected);
  it("category and catalogue icons are required strings (REQ-DCLEAN-005)", categoryAndCatalogueIconsAreRequired);
  it("the large-files count is a whole number from the engine (REQ-DCLEAN-003.A4)", largeFilesCountMustBeAWholeCount);
});

const historyRow = {
  id: "e1",
  title: "Smart Clean",
  ts: "2026-11-17T10:00:00.000Z",
  mode: "trash", // lint-boundary-allow: clean mode, not a category
  categories: ["x"],
  bytes: 6_800_000_000,
  items: [],
  restored_at: null,
  detail: "App caches, Temporary files, Trash",
  state: "restorable",
};

function parsesHistoryRowsWithDerivedFields() {
  const v = parseHistoryView({ total_bytes: 6_800_000_000, rows: [historyRow] });
  expect(v.total_bytes).toBe(6_800_000_000);
  expect(v.rows[0]).toMatchObject({ id: "e1", detail: historyRow.detail, state: "restorable" });
}

function rejectsAnUnknownHistoryState() {
  expect(() => parseHistoryView({ total_bytes: 0, rows: [{ ...historyRow, state: "maybe" }] })).toThrow(ContractError);
  expect(() => parseHistoryView({ total_bytes: -1, rows: [] })).toThrow(ContractError);
}

function parsesAMapViewAndRejectsBadBytes() {
  const map = {
    path: "/",
    name: "/",
    root: "/",
    total_bytes: 3,
    capacity_bytes: 512e9,
    records: [{ name: "Users", path: "/Users", bytes: 3, has_children: true }],
  };
  expect(parseMapView(map).records[0]).toEqual(map.records[0]);
  expect(() => parseMapView({ ...map, records: [{ ...map.records[0], bytes: 1.5 }] })).toThrow(ContractError);
  expect(() => parseMapView({ ...map, records: [{ ...map.records[0], has_children: "yes" }] })).toThrow(ContractError);
  // `other` is optional: absent or null reads as none, present is checked.
  expect(parseMapView(map).other).toBeNull();
  expect(parseMapView({ ...map, other: { count: 12, bytes: 4e9 } }).other).toEqual({ count: 12, bytes: 4e9 });
  expect(() => parseMapView({ ...map, other: { count: 12, bytes: -1 } })).toThrow(ContractError);
}

describe("history and map views (REQ-DSCR-015, REQ-DSCR-003)", () => {
  it("parses history rows with derived fields (REQ-DSCR-015.A2)", parsesHistoryRowsWithDerivedFields);
  it("rejects an unknown history state (REQ-DSCR-015.A4)", rejectsAnUnknownHistoryState);
  it("parses a map view and rejects bad bytes (REQ-DSCR-003.A1)", parsesAMapViewAndRejectsBadBytes);
});

function parsesALargeViewAndRejectsBadFields() {
  const large = {
    filter: "unused-1y",
    count: 1,
    total_bytes: 3e9,
    records: [{ name: "a.iso", path: "/Users/me/a.iso", location: "~", bytes: 3e9, last_opened: null, unused_1y: false }],
  };
  expect(parseLargeView(large)).toEqual(large);
  expect(() => parseLargeView({ ...large, filter: "huge" })).toThrow(ContractError);
  expect(() => parseLargeView({ ...large, records: [{ ...large.records[0], bytes: -1 }] })).toThrow(ContractError);
  expect(() => parseLargeView({ ...large, records: [{ ...large.records[0], unused_1y: 1 }] })).toThrow(ContractError);
}

function parsesATrashResult() {
  const r = {
    handled: ["/a"],
    failed: [{ path: "/b", reason: "missing" }],
    freed_physical_bytes: 7,
    history_id: "e1",
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
  };
  expect(parseTrashResult(r)).toEqual(r);
  expect(() => parseTrashResult({ ...r, freed_physical_bytes: "7" })).toThrow(ContractError);
}

describe("large files and trash contract (REQ-DSCR-004, REQ-DSCR-006)", () => {
  it("parses a large view and rejects bad fields", parsesALargeViewAndRejectsBadFields);
  it("parses a trash result", parsesATrashResult);
});

function parsesADupesViewAndRejectsBadGroups() {
  const dupes = {
    min_size: 1e6,
    groups: 1,
    extra_copies: 1,
    reclaimable_bytes: 4e6,
    records: [
      {
        name: "a.mov",
        bytes: 4e6,
        copies: [
          { path: "/Users/me/x/a.mov", location: "~/x", mtime: "2024-01-01T00:00:00Z" },
          { path: "/Users/me/y/a.mov", location: "~/y", mtime: null },
        ],
      },
    ],
  };
  expect(parseDupesView(dupes)).toEqual(dupes);
  const g = dupes.records[0];
  expect(() => parseDupesView({ ...dupes, records: [{ ...g, copies: [g.copies[0]] }] })).toThrow(ContractError);
  expect(() => parseDupesView({ ...dupes, records: [{ ...g, bytes: -1 }] })).toThrow(ContractError);
  expect(() => parseDupesView({ ...dupes, extra_copies: "1" })).toThrow(ContractError);
}

describe("duplicates contract (REQ-DSCR-005)", () => {
  it("parses a dupes view and rejects bad groups", parsesADupesViewAndRejectsBadGroups);
});

function parsesAnAssistantViewAndRejectsBadSuggestions() {
  const raw = JSON.parse(JSON.stringify(snapshotAssistantView()));
  const v = parseAssistantView(raw);
  expect(v.agent_name).toBe("Claude Code");
  expect(v.checks).toEqual({ enabled: true, frequency: "daily" });
  expect(v.agents.map((a) => a.id)).toEqual(["claude", "codex", "gemini"]);
  expect(v.suggestions.map((s) => s.key)).toEqual(["goal", "dev", "dl", "left"]);
  expect(v.suggestions[0].bytes).toBe(12.1e9);
  // An "ok" card carries no bytes and no action.
  const ok = { key: "ok", kind: "On track", text: "x", bytes: null, categories: [], actionable: false, at: null };
  expect(parseAssistantView({ ...raw, suggestions: [ok] }).suggestions[0].bytes).toBeNull();
  // Fail safe: an unknown key, an actionable card with nothing to review,
  // a found agent with no path or fractional bytes reject the whole view.
  const bad = (patch: (r: any) => void) => {
    const r = JSON.parse(JSON.stringify(raw));
    patch(r);
    return () => parseAssistantView(r);
  };
  expect(bad((r) => (r.suggestions[0].key = "maybe"))).toThrow(ContractError);
  expect(bad((r) => (r.suggestions[1].categories = []))).toThrow(ContractError);
  expect(bad((r) => (r.suggestions[1].bytes = 1.5))).toThrow(ContractError);
  expect(bad((r) => (r.agents[0].path = null))).toThrow(ContractError);
  expect(bad((r) => (r.checks.frequency = "monthly"))).toThrow(ContractError);
}

describe("assistant contract (REQ-DSCR-007, REQ-DSCR-008)", () => {
  it("parses an assistant view and rejects bad suggestions", parsesAnAssistantViewAndRejectsBadSuggestions);
});
