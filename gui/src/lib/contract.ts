/**
 * GUI data contract (REQ-DCLEAN-027): TypeScript mirror of the engine's
 * `src/contract.rs`, plus strict parsers that every Tauri `invoke` / `listen`
 * payload passes through before it reaches a component.
 *
 * The parsers fail safe (REQ-DCLEAN-029.A4): any malformed field throws a
 * `ContractError` for the whole payload — they never return a partial list.
 * Bytes stay exact integers (REQ-DCLEAN-027.A1); formatting happens only in
 * `format.ts`, at render time.
 */

export type Safety = "safe" | "review";
export type CleanMode = "trash" | "delete"; // lint-boundary-allow: clean mode, not a category
/** The reversible clean mode: items move to the Trash. */
export const TRASH_MODE: CleanMode = "trash"; // lint-boundary-allow: clean mode, not a category
/** The permanent clean mode. */
export const DELETE_MODE: CleanMode = "delete";

export interface Disk {
  volume: string;
  capacity_bytes: number;
  free_bytes: number;
}

/** How often the Assistant's proactive checks run. */
export type CheckFrequency = "hourly" | "daily" | "weekly";
/** The scheduled-scan period. */
export type ScanSchedule = "off" | "weekly" | "monthly";
export const SCAN_SCHEDULES: readonly ScanSchedule[] = ["off", "weekly", "monthly"];

/**
 * Every preference, as the engine's shared store holds it (REQ-DSCR-001,
 * `src/prefs.rs`). The GUI reads these and never supplies defaults of its
 * own (A5); Settings changes them one key at a time through `setPrefs`.
 */
export interface Preferences {
  trash_by_default: boolean;
  skip_recently_used: boolean;
  safe_only: boolean;
  warn_below_goal: boolean;
  goal_bytes: number;
  checks: { enabled: boolean; frequency: CheckFrequency };
  agent: string;
  scheduled_scan: ScanSchedule;
  /** Folders never touched, as entered (`~/…` or absolute). */
  excluded: string[];
}

/** The four Settings toggles, by store key. */
export type ToggleKey = "trash_by_default" | "skip_recently_used" | "safe_only" | "warn_below_goal";

export interface ItemView {
  name: string;
  path: string;
  bytes: number;
}

export interface CategoryView {
  id: string;
  name: string;
  description: string;
  /** Lucide icon name from the engine table (REQ-DCLEAN-005). */
  icon: string;
  safety: Safety;
  bytes: number;
  items: ItemView[];
}

/** One category the engine checks, hit or not (REQ-DCLEAN-004). */
export interface CatalogueEntry {
  id: string;
  name: string;
  icon: string;
  safety: Safety;
}

export interface CleanupView {
  disk: Disk;
  goal_bytes: number;
  last_scan_at: string | null;
  snapshot_taken_at: string | null;
  is_stale: boolean;
  preferences: Preferences;
  /** Files in the snapshot of at least 2 GB physical (REQ-DCLEAN-003.A4). */
  large_files_count: number;
  categories: CategoryView[];
  /** Every category the engine checks, in engine order. */
  catalogue: CatalogueEntry[];
  /** Paths the scan could not read for permission reasons (REQ-DCLEAN-018.A2). */
  skipped_paths: number;
}

export interface ScanProgress {
  fraction: number;
  current_path: string;
  completed_category_ids: string[];
  /** Physical bytes per completed category with hits; absent → nothing found. */
  completed_category_bytes: Record<string, number>;
}

/** One clean progress event, sent after each item (REQ-DCLEAN-008). */
export interface CleanProgress {
  /** The category being processed now, in selection order. */
  category_id: string;
  done_items: number;
  total_items: number;
  /** done_items / total_items, 0..1. */
  fraction: number;
}

export interface Failure {
  path: string;
  reason: string;
}

export interface HistoryItem {
  original_path: string;
  trash_path: string | null;
  bytes: number;
  status: string;
}

export interface HistoryEntry {
  id: string;
  /** "Smart Clean", "Assistant cleanup" or "Cleanup" (REQ-DCLEAN-009.A5). */
  title: string;
  ts: string;
  mode: CleanMode;
  categories: string[];
  bytes: number;
  items: HistoryItem[];
  restored_at: string | null;
}

export interface CleanResult {
  mode: CleanMode;
  freed_bytes: number;
  succeeded: string[];
  failed: Failure[];
  /** Entry Undo restores; null when nothing was cleaned. */
  history_id: string | null;
  history_entry: HistoryEntry | null;
  /** The volume's usage read after the clean (REQ-DCLEAN-002.A4). */
  disk: Disk;
}

/** What an Undo restored (REQ-DCLEAN-009.A4, A7). Paths are original paths. */
export interface RestoreReport {
  id: string;
  restored: string[];
  conflicts: string[];
  /** Items "no longer in Trash" (REQ-DCLEAN-034.A4). */
  missing: string[];
}

export interface RestoreResult {
  report: RestoreReport;
  /** The volume's usage read after the restore. */
  disk: Disk;
}

/** disky's exit-code slug for a snapshot locked by another process. */
export const LOCK_HELD = "lock-held";

/**
 * A failed engine command (REQ-DCLEAN-018), typed by disky's exit-code slug
 * (`code`: "lock-held", "io", …). Dispatch on `code`, never on `detail`.
 */
export class EngineError extends Error {
  constructor(
    readonly code: string,
    readonly detail: string,
    readonly retryable: boolean,
  ) {
    super(detail);
    this.name = "EngineError";
  }
}

/** Turn whatever a rejected `invoke` carried into an {@link EngineError}. */
export function parseEngineError(raw: unknown): EngineError {
  if (raw instanceof EngineError) return raw;
  if (typeof raw === "object" && raw !== null && typeof (raw as Obj).code === "string") {
    const o = raw as Obj;
    const detail = typeof o.detail === "string" ? o.detail : String(o.title ?? o.code);
    return new EngineError(o.code as string, detail, o.retryable === true);
  }
  const detail = raw instanceof Error ? raw.message : String(raw);
  return new EngineError("generic", detail, false);
}

/** A payload that does not match the contract. `path` names the field. */
export class ContractError extends Error {
  constructor(
    readonly path: string,
    detail: string,
  ) {
    super(`${path}: ${detail}`);
    this.name = "ContractError";
  }
}

type Obj = Record<string, unknown>;

function obj(v: unknown, path: string): Obj {
  if (typeof v !== "object" || v === null || Array.isArray(v)) {
    throw new ContractError(path, "expected an object");
  }
  return v as Obj;
}

function arr(v: unknown, path: string): unknown[] {
  if (!Array.isArray(v)) throw new ContractError(path, "expected an array");
  return v;
}

function str(v: unknown, path: string): string {
  if (typeof v !== "string") throw new ContractError(path, "expected a string");
  return v;
}

function strOrNull(v: unknown, path: string): string | null {
  return v === null ? null : str(v, path);
}

function bool(v: unknown, path: string): boolean {
  if (typeof v !== "boolean") throw new ContractError(path, "expected a boolean");
  return v;
}

/** A byte count: a non-negative safe integer (REQ-DCLEAN-027.A1). */
function bytes(v: unknown, path: string): number {
  if (typeof v !== "number" || !Number.isSafeInteger(v) || v < 0) {
    throw new ContractError(path, `expected a non-negative integer byte count, got ${JSON.stringify(v)}`);
  }
  return v;
}

/** A count: a non-negative safe integer. */
function count(v: unknown, path: string): number {
  if (typeof v !== "number" || !Number.isSafeInteger(v) || v < 0) {
    throw new ContractError(path, `expected a non-negative integer count, got ${JSON.stringify(v)}`);
  }
  return v;
}

/** Exactly "safe" or "review" (REQ-DCLEAN-027.A2). */
function safety(v: unknown, path: string): Safety {
  if (v !== "safe" && v !== "review") {
    throw new ContractError(path, `expected "safe" or "review", got ${JSON.stringify(v)}`);
  }
  return v;
}

function mode(v: unknown, path: string): CleanMode {
  if (v !== "trash" && v !== "delete") { // lint-boundary-allow: clean mode, not a category
    throw new ContractError(path, `expected "trash" or "delete", got ${JSON.stringify(v)}`); // lint-boundary-allow: clean mode
  }
  return v;
}

function parseItem(v: unknown, path: string): ItemView {
  const o = obj(v, path);
  return { name: str(o.name, `${path}.name`), path: str(o.path, `${path}.path`), bytes: bytes(o.bytes, `${path}.bytes`) };
}

function parseCategory(v: unknown, path: string): CategoryView {
  const o = obj(v, path);
  const items = arr(o.items, `${path}.items`).map((it, i) => parseItem(it, `${path}.items[${i}]`));
  const total = bytes(o.bytes, `${path}.bytes`);
  const sum = items.reduce((acc, it) => acc + it.bytes, 0); // lint-boundary-allow: verifies the engine total (A3), never displayed
  // REQ-DCLEAN-027.A3
  if (total !== sum) throw new ContractError(`${path}.bytes`, `${total} is not the sum of its items (${sum})`);
  return {
    id: str(o.id, `${path}.id`),
    name: str(o.name, `${path}.name`),
    description: str(o.description, `${path}.description`),
    icon: str(o.icon, `${path}.icon`),
    safety: safety(o.safety, `${path}.safety`),
    bytes: total,
    items,
  };
}

function parseDisk(v: unknown, path: string): Disk {
  const d = obj(v, path);
  return {
    volume: str(d.volume, `${path}.volume`),
    capacity_bytes: bytes(d.capacity_bytes, `${path}.capacity_bytes`),
    free_bytes: bytes(d.free_bytes, `${path}.free_bytes`),
  };
}

function oneOf<T extends string>(v: unknown, allowed: readonly T[], path: string): T {
  if (typeof v !== "string" || !allowed.includes(v as T)) {
    throw new ContractError(path, `expected one of ${allowed.join(", ")}, got ${JSON.stringify(v)}`);
  }
  return v as T;
}

/** The preferences record (`disky prefs`), validated field by field. */
export function parsePreferences(raw: unknown, path = "prefs"): Preferences {
  const p = obj(raw, path);
  const c = obj(p.checks, `${path}.checks`);
  return {
    trash_by_default: bool(p.trash_by_default, `${path}.trash_by_default`),
    skip_recently_used: bool(p.skip_recently_used, `${path}.skip_recently_used`),
    safe_only: bool(p.safe_only, `${path}.safe_only`),
    warn_below_goal: bool(p.warn_below_goal, `${path}.warn_below_goal`),
    goal_bytes: bytes(p.goal_bytes, `${path}.goal_bytes`),
    checks: {
      enabled: bool(c.enabled, `${path}.checks.enabled`),
      frequency: oneOf(c.frequency, ["hourly", "daily", "weekly"] as const, `${path}.checks.frequency`),
    },
    agent: str(p.agent, `${path}.agent`),
    scheduled_scan: oneOf(p.scheduled_scan, SCAN_SCHEDULES, `${path}.scheduled_scan`),
    excluded: arr(p.excluded, `${path}.excluded`).map((e, i) => str(e, `${path}.excluded[${i}]`)),
  };
}

export function parseCleanupView(raw: unknown): CleanupView {
  const o = obj(raw, "view");
  return {
    disk: parseDisk(o.disk, "view.disk"),
    goal_bytes: bytes(o.goal_bytes, "view.goal_bytes"),
    last_scan_at: strOrNull(o.last_scan_at, "view.last_scan_at"),
    snapshot_taken_at: strOrNull(o.snapshot_taken_at, "view.snapshot_taken_at"),
    is_stale: bool(o.is_stale, "view.is_stale"),
    preferences: parsePreferences(o.preferences, "view.preferences"),
    large_files_count: count(o.large_files_count, "view.large_files_count"),
    categories: arr(o.categories, "view.categories").map((c, i) => parseCategory(c, `view.categories[${i}]`)),
    catalogue: arr(o.catalogue, "view.catalogue").map((c, i) => {
      const e = obj(c, `view.catalogue[${i}]`);
      return {
        id: str(e.id, `view.catalogue[${i}].id`),
        name: str(e.name, `view.catalogue[${i}].name`),
        icon: str(e.icon, `view.catalogue[${i}].icon`),
        safety: safety(e.safety, `view.catalogue[${i}].safety`),
      };
    }),
    skipped_paths: count(o.skipped_paths, "view.skipped_paths"),
  };
}

export function parseScanProgress(raw: unknown): ScanProgress {
  const o = obj(raw, "progress");
  const fraction = o.fraction;
  if (typeof fraction !== "number" || !Number.isFinite(fraction) || fraction < 0 || fraction > 1) {
    throw new ContractError("progress.fraction", `expected a number in 0..1, got ${JSON.stringify(fraction)}`);
  }
  return {
    fraction,
    current_path: str(o.current_path, "progress.current_path"),
    completed_category_ids: arr(o.completed_category_ids, "progress.completed_category_ids").map((id, i) =>
      str(id, `progress.completed_category_ids[${i}]`),
    ),
    completed_category_bytes: Object.fromEntries(
      Object.entries(obj(o.completed_category_bytes, "progress.completed_category_bytes")).map(([id, b]) => [
        id,
        bytes(b, `progress.completed_category_bytes.${id}`),
      ]),
    ),
  };
}

/** Validated like {@link parseScanProgress}: a malformed event throws. */
export function parseCleanProgress(raw: unknown): CleanProgress {
  const o = obj(raw, "clean");
  const fraction = o.fraction;
  if (typeof fraction !== "number" || !Number.isFinite(fraction) || fraction < 0 || fraction > 1) {
    throw new ContractError("clean.fraction", `expected a number in 0..1, got ${JSON.stringify(fraction)}`);
  }
  const done = count(o.done_items, "clean.done_items");
  const total = count(o.total_items, "clean.total_items");
  if (done > total) throw new ContractError("clean.done_items", `${done} exceeds total_items ${total}`);
  return { category_id: str(o.category_id, "clean.category_id"), done_items: done, total_items: total, fraction };
}

function strArr(v: unknown, path: string): string[] {
  return arr(v, path).map((s, i) => str(s, `${path}[${i}]`));
}

export function parseRestoreResult(raw: unknown): RestoreResult {
  const o = obj(raw, "restore");
  const r = obj(o.report, "restore.report");
  return {
    report: {
      id: str(r.id, "restore.report.id"),
      restored: strArr(r.restored, "restore.report.restored"),
      conflicts: strArr(r.conflicts, "restore.report.conflicts"),
      missing: strArr(r.missing, "restore.report.missing"),
    },
    disk: parseDisk(o.disk, "restore.disk"),
  };
}

function parseHistoryEntry(v: unknown, path: string): HistoryEntry {
  const o = obj(v, path);
  return {
    id: str(o.id, `${path}.id`),
    title: str(o.title, `${path}.title`),
    ts: str(o.ts, `${path}.ts`),
    mode: mode(o.mode, `${path}.mode`),
    categories: arr(o.categories, `${path}.categories`).map((c, i) => str(c, `${path}.categories[${i}]`)),
    bytes: bytes(o.bytes, `${path}.bytes`),
    items: arr(o.items, `${path}.items`).map((it, i) => {
      const io = obj(it, `${path}.items[${i}]`);
      return {
        original_path: str(io.original_path, `${path}.items[${i}].original_path`),
        trash_path: strOrNull(io.trash_path, `${path}.items[${i}].trash_path`),
        bytes: bytes(io.bytes, `${path}.items[${i}].bytes`),
        status: str(io.status, `${path}.items[${i}].status`),
      };
    }),
    restored_at: strOrNull(o.restored_at ?? null, `${path}.restored_at`),
  };
}

export function parseCleanResult(raw: unknown): CleanResult {
  const o = obj(raw, "result");
  return {
    mode: mode(o.mode, "result.mode"),
    freed_bytes: bytes(o.freed_bytes, "result.freed_bytes"),
    succeeded: arr(o.succeeded, "result.succeeded").map((s, i) => str(s, `result.succeeded[${i}]`)),
    failed: arr(o.failed, "result.failed").map((f, i) => {
      const fo = obj(f, `result.failed[${i}]`);
      return { path: str(fo.path, `result.failed[${i}].path`), reason: str(fo.reason, `result.failed[${i}].reason`) };
    }),
    history_id: strOrNull(o.history_id, "result.history_id"),
    history_entry: o.history_entry === null ? null : parseHistoryEntry(o.history_entry, "result.history_entry"),
    disk: parseDisk(o.disk, "result.disk"),
  };
}

/** Where a history entry stands (REQ-DSCR-015.A3, A4), from the engine. */
export type HistoryState = "restorable" | "restored" | "emptied" | "deleted";
export const HISTORY_STATES: readonly HistoryState[] = ["restorable", "restored", "emptied", "deleted"];

/**
 * One History row: the log entry plus what the screen shows, derived by the
 * engine (`contract::history_view`) — the GUI only formats when and size.
 */
export interface HistoryRow extends HistoryEntry {
  /** Category names, or file names for "Large files" / "Duplicates" entries. */
  detail: string;
  state: HistoryState;
}

export interface HistoryView {
  /** Physical bytes freed over every entry. */
  total_bytes: number;
  /** Newest first. */
  rows: HistoryRow[];
}

/** The History screen's view (REQ-DSCR-015). CLI: `disky history --format json`. */
export function parseHistoryView(raw: unknown): HistoryView {
  const o = obj(raw, "history");
  return {
    total_bytes: bytes(o.total_bytes, "history.total_bytes"),
    rows: arr(o.rows, "history.rows").map((r, i) => {
      const path = `history.rows[${i}]`;
      const ro = obj(r, path);
      return {
        ...parseHistoryEntry(ro, path),
        detail: str(ro.detail, `${path}.detail`),
        state: oneOf(ro.state, HISTORY_STATES, `${path}.state`),
      };
    }),
  };
}

/** One immediate child of a mapped folder (REQ-DSCR-003). */
export interface MapChild {
  name: string;
  path: string;
  /** Physical bytes of everything beneath it. */
  bytes: number;
  has_children: boolean;
}

/** A mapped folder. CLI: `disky map [<path>] --format json`. */
export interface MapView {
  path: string;
  name: string;
  /** The snapshot's scan root: "Up" stops here. */
  root: string;
  /** Physical bytes under the folder (the engine's sum of its children). */
  total_bytes: number;
  /** The volume's capacity, for share of disk. */
  capacity_bytes: number;
  /** Largest first. */
  records: MapChild[];
}

export function parseMapView(raw: unknown): MapView {
  const o = obj(raw, "map");
  return {
    path: str(o.path, "map.path"),
    name: str(o.name, "map.name"),
    root: str(o.root, "map.root"),
    total_bytes: bytes(o.total_bytes, "map.total_bytes"),
    capacity_bytes: bytes(o.capacity_bytes, "map.capacity_bytes"),
    records: arr(o.records, "map.records").map((c, i) => {
      const co = obj(c, `map.records[${i}]`);
      return {
        name: str(co.name, `map.records[${i}].name`),
        path: str(co.path, `map.records[${i}].path`),
        bytes: bytes(co.bytes, `map.records[${i}].bytes`),
        has_children: bool(co.has_children, `map.records[${i}].has_children`),
      };
    }),
  };
}

/** Which large files to list (REQ-DSCR-004.A2): `disky large --filter`. */
export type LargeFilter = "all" | "over-10gb" | "unused-1y";
export const LARGE_FILTERS: readonly LargeFilter[] = ["all", "over-10gb", "unused-1y"];

/** One large file (REQ-DSCR-004.A1). */
export interface LargeFile {
  name: string;
  path: string;
  /** Parent folder for display, home as `~` (engine-derived). */
  location: string;
  /** Physical bytes. */
  bytes: number;
  /** RFC 3339 access time, or null for snapshots taken before access times were recorded. */
  last_opened: string | null;
  /** True when last opened more than 365 days ago. */
  unused_1y: boolean;
}

/** The Large files screen's list: `disky large --format json`. */
export interface LargeView {
  filter: LargeFilter;
  count: number;
  /** Physical bytes over `records`. */
  total_bytes: number;
  /** Largest first. */
  records: LargeFile[];
}

export function parseLargeView(raw: unknown): LargeView {
  const o = obj(raw, "large");
  return {
    filter: oneOf(o.filter, LARGE_FILTERS, "large.filter"),
    count: count(o.count, "large.count"),
    total_bytes: bytes(o.total_bytes, "large.total_bytes"),
    records: arr(o.records, "large.records").map((f, i) => {
      const fo = obj(f, `large.records[${i}]`);
      return {
        name: str(fo.name, `large.records[${i}].name`),
        path: str(fo.path, `large.records[${i}].path`),
        location: str(fo.location, `large.records[${i}].location`),
        bytes: bytes(fo.bytes, `large.records[${i}].bytes`),
        last_opened: strOrNull(fo.last_opened, `large.records[${i}].last_opened`),
        unused_1y: bool(fo.unused_1y, `large.records[${i}].unused_1y`),
      };
    }),
  };
}

/** What moving reviewed files to the Trash did (REQ-DSCR-006.A1): `disky trash`. */
export interface TrashResult {
  handled: string[];
  failed: Failure[];
  freed_physical_bytes: number;
  history_id: string | null;
  disk: Disk;
}

export function parseTrashResult(raw: unknown): TrashResult {
  const o = obj(raw, "trashResult");
  return {
    handled: strArr(o.handled, "trashResult.handled"),
    failed: arr(o.failed, "trashResult.failed").map((f, i) => {
      const fo = obj(f, `trashResult.failed[${i}]`);
      return { path: str(fo.path, `trashResult.failed[${i}].path`), reason: str(fo.reason, `trashResult.failed[${i}].reason`) };
    }),
    freed_physical_bytes: bytes(o.freed_physical_bytes, "trashResult.freed_physical_bytes"),
    history_id: strOrNull(o.history_id, "trashResult.history_id"),
    disk: parseDisk(o.disk, "trashResult.disk"),
  };
}
