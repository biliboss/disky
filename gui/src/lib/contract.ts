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

export interface Disk {
  volume: string;
  capacity_bytes: number;
  free_bytes: number;
}

export interface Preferences {
  trash_by_default: boolean;
  safe_only: boolean;
  notifications: boolean;
  assistant: boolean;
}

export interface ItemView {
  name: string;
  path: string;
  bytes: number;
}

export interface CategoryView {
  id: string;
  name: string;
  description: string;
  safety: Safety;
  bytes: number;
  items: ItemView[];
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
}

export interface ScanProgress {
  fraction: number;
  current_path: string;
  completed_category_ids: string[];
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
  history_entry: HistoryEntry | null;
  /** The volume's usage read after the clean (REQ-DCLEAN-002.A4). */
  disk: Disk;
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

export function parseCleanupView(raw: unknown): CleanupView {
  const o = obj(raw, "view");
  const p = obj(o.preferences, "view.preferences");
  return {
    disk: parseDisk(o.disk, "view.disk"),
    goal_bytes: bytes(o.goal_bytes, "view.goal_bytes"),
    last_scan_at: strOrNull(o.last_scan_at, "view.last_scan_at"),
    snapshot_taken_at: strOrNull(o.snapshot_taken_at, "view.snapshot_taken_at"),
    is_stale: bool(o.is_stale, "view.is_stale"),
    preferences: {
      trash_by_default: bool(p.trash_by_default, "view.preferences.trash_by_default"),
      safe_only: bool(p.safe_only, "view.preferences.safe_only"),
      notifications: bool(p.notifications, "view.preferences.notifications"),
      assistant: bool(p.assistant, "view.preferences.assistant"),
    },
    large_files_count: count(o.large_files_count, "view.large_files_count"),
    categories: arr(o.categories, "view.categories").map((c, i) => parseCategory(c, `view.categories[${i}]`)),
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
  };
}

function parseHistoryEntry(v: unknown, path: string): HistoryEntry {
  const o = obj(v, path);
  return {
    id: str(o.id, `${path}.id`),
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
    history_entry: o.history_entry === null ? null : parseHistoryEntry(o.history_entry, "result.history_entry"),
    disk: parseDisk(o.disk, "result.disk"),
  };
}
