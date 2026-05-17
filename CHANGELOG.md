# Changelog

All notable changes to disky will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `Stats` now carries scan provenance: `scan_root`, `scanned_at` (RFC 3339),
  `scan_duration_s`. `disky stats` text mode prints them under the totals;
  JSON mode adds them as optional fields. Lets agents tell whether a query
  is hitting a fresh snapshot or a week-old one.
- `scan_meta.ended_at` (BIGINT) stored on every scan; `ScanMeta::duration_secs`
  computes the elapsed time.
- Integration test suite (`tests/agentic.rs`): 6 tests covering JSON
  envelope, partial flag + provenance fields, RFC 9457 not-found stderr,
  schema descriptor, raw SQL, cleanup dry-run.
- CI now runs `cargo test --release`.
- README documents agentic surface (JSON/NDJSON, MCP, exit codes).

## [0.6.0] - 2026-05-17

### Added
- Cancellable scan: `Ctrl-C` during `disky scan` now drains the in-flight
  batch, marks the snapshot partial, and exits with status `5`
  (`partial-scan`). The DB on disk is still queryable.
- `scan_meta` table per snapshot — `root, started_at, completed, entries,
  bytes`. `disky stats` surfaces `partial: true` (text + JSON) when the
  last scan was cancelled.
- `scan::ScanOutcome { complete, entries, bytes }` returned from
  `scan::run` and `scan::run_cancellable`.
- `exit::classify` preserves a `DiskyError` if the underlying error
  already is one (lets call sites raise specific codes without going
  through string heuristics).

### Changed
- `ctrlc` dependency added for cross-platform SIGINT handling.

## [0.5.0] - 2026-05-17

### Added
- `disky cleanup` — find well-known disk hoggers (`node_modules`, `target`,
  `__pycache__`, `.next`, `dist`, `build`, `.venv`/`venv`, `.gradle`,
  `.pytest_cache`) in the snapshot, render category/size/files/path. Defaults
  to dry-run; `--apply` actually removes the listed paths. `--target` filters
  to specific categories.
- `disky schema` — emit a JSON descriptor of every command, record shape,
  error code, and snapshot-ref form. Hand-written, no `schemars` dep.
- MCP tools `disky_cleanup` and `disky_schema` mirror the CLI.

## [0.4.0] - 2026-05-17

### Added
- `disky query "<sql>"` — run arbitrary SQL against a snapshot, rendered as
  text table, JSON envelope, or NDJSON. Heterogeneous columns map to native
  JSON types; `HugeInt` is stringified to preserve precision. `--limit`
  caps rows (default 1000).
- Snapshot ID handles: every CLI subcommand and MCP tool now accepts
  `--snapshot @latest | <id> | <path>` via `snapshots::resolve`. IDs are the
  file stem (e.g. `2026-05-15_11-56`) and resolve against the data dir.
- `disky_query` MCP tool mirroring the CLI surface.
- `disky list` (text + JSON) now prints the snapshot ID alongside the path.

### Changed
- CLI subcommands replace `--db <path>` with `--snapshot <ref>`. The legacy
  `disky.db` literal default is gone; queries default to `@latest`.
- `disky scan --db <path>` keeps the old flag for explicit destinations,
  defaults to an auto-named file in the data dir.
- Bad snapshot IDs now exit with status `4 not-found` (verified end-to-end).

## [0.3.0] - 2026-05-17

### Added
- `disky-mcp` binary: minimal stdio MCP server (JSON-RPC 2.0) exposing the
  query layer as typed tools (`disky_scan`, `disky_top`, `disky_dirs`,
  `disky_ext`, `disky_find`, `disky_stats`, `disky_list_snapshots`). No
  external SDK dep — handcrafted protocol covers `initialize`, `tools/list`,
  `tools/call`. Tool errors carry the RFC 9457 payload as `isError` content.
- `[lib]` target — `db`, `exit`, `query`, `render`, `scan`, `snapshots`
  modules now public via the `disky` crate so both binaries reuse the core.
- NDJSON scan progress on stderr when stderr is not a TTY: `start`,
  `progress` (throttled 500ms), `done` events. Spinner keeps working
  interactively.

### Changed
- `Cargo.toml` declares `[lib]` + two `[[bin]]` targets (`disky`, `disky-mcp`).
- `main.rs`, `cli.rs`, `tui/mod.rs` switched from `crate::` to `disky::`
  imports so the binary consumes the library.

## [0.2.0] - 2026-05-17

### Added
- Agent-native output: `--format json|ndjson` on every query command, auto-engaged
  when stdout is not a TTY. JSON envelope includes `schema_version` + `kind`.
- Typed query layer (`src/query.rs`) — pure functions returning
  `Vec<FileRow>`/`Vec<ExtRow>`/`Vec<DirRow>`/`Stats`, decoupled from rendering.
- Renderer split (`src/render.rs`) — text + JSON skins share one struct.
- Stable exit-code taxonomy (0/1/2/3/4/5/6) wired through `src/exit.rs` and
  documented in AGENTS.md.
- RFC 9457 problem-details JSON errors on stderr in machine output modes,
  with stable `type` URIs (`https://disky.dev/errors/<slug>`).
- `mtime` now surfaced in JSON as RFC 3339 UTC, sizes as raw `u64` bytes.

### Changed
- `disky list --format json` emits `{path, bytes}` records (snapshot index).

### Removed
- `src/display.rs` — superseded by `query` + `render` split.

## [0.1.0] - 2026-05-15

### Added
- Full filesystem scan via `jwalk` with parallel traversal (`getattrlistbulk` path on macOS)
- DuckDB embedded storage — persistent snapshots in `~/Library/Application Support/disky/`
- Interactive TUI (`ratatui`) — ncdu-style directory tree, size bars, keybindings
- CLI subcommands: `scan`, `tui`, `top`, `dirs`, `ext`, `find`, `stats`, `list`
- `memchr`-based extension extraction (zero alloc)
- Batch Appender API for DuckDB (50k entries/batch)
- HTML cleanup report export (`e` key in TUI or auto-generated)
- Snapshot management — auto-named `YYYY-MM-DD_HH-MM.db`
- macOS Finder integration (`o` key opens selected path)
- Clipboard copy (`c` key copies path)
