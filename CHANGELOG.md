# Changelog

All notable changes to disky will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.0] — 2026-05-27

### Removed (BREAKING)
- `disky-mcp` binary deleted. 925 LOC, single-file `src/bin/disky-mcp.rs`. CLI
  is now the only surface. Rationale: only real consumer (Claude Code) shells
  out, and every MCP tool just wrapped the matching CLI subcommand with the
  same JSON envelope — pure duplication for zero new users. Validated by
  4-round grill on 2026-05-27. Hosts that cannot shell out (Claude Desktop /
  Cursor / Zed) are unsupported until earn-back: see AGENTS.md "Single
  surface" section. Resurrection: `git log --all --diff-filter=D -- src/bin/disky-mcp.rs`.
- `[[bin]] disky-mcp` removed from `Cargo.toml`. `cargo install --path .`
  now installs only the `disky` binary.
- Removed punch-list items pre-empted by this decision: MCP `resources/list`,
  MCP `resources/read`, MCP `prompts/list`, MCP progress notifications on
  scan/cleanup, `disky web` FastHTML server, `disky install-mcp` subcommand,
  `tests/mcp_protocol.rs` integration suite. None of them shipped — the plan
  was scoped out before implementation.

### Changed
- `Cargo.toml` version bumped 0.6.0 → 0.10.0 (was previously out of sync with
  git tags; v0.9.0 was the last tagged release).
- `AGENTS.md` "MCP scope" + "Three surfaces" sections rewritten to "Single
  surface — CLI only". Earn-back criteria preserved.

### Notes
- Local profile installs (`disky-mcp` symlinks in `~/.claude-pessoal/` and
  `~/.claude-mukutu/`) should be removed manually:
  ```
  # Edit `~/.claude-pessoal/.claude.json` + `~/.claude-mukutu/.claude.json`,
  # delete the `mcpServers.disky` entry.
  ```

## [0.10.1] — 2026-05-27

### Added
- `CONTRIBUTING.md` "Build footprint" section — receita rust-vs-giants:
  `$CARGO_HOME` global default + `sccache` wired via `~/.cargo/config.toml`.
  Cuts the 1-2 GB `target/` per-project tax by sharing compiled artifacts
  across all Rust repos. Verification commands + cache-size tuning
  included. Followup from v0.10.0 grill (2026-05-27).
- `claude-skill/disky/SKILL.md` v2 — rewrites the `/disky` skill as a
  guided 4-stage wizard (triage → propose → confirm → apply), all driven
  by `AskUserQuestion`. Replaces the prior one-shot HTML report. Defaults
  every destructive op to `--reversible` (Trash). HTML report becomes
  optional decoration at the end.

### Changed
- `metrics/baseline.json` reseeded against commit `6a33586` (post-v0.10.0).
  Captures the single-bin tree (43 MB release binary, 4999 LOC src, 561
  LOC tests, 79 fast-tier tests). Caveat: nextest_fast_tier 60s and
  build_release_inc 13s are above advisory budgets — pre-existing drift
  that the prior baseline (commit `1e441d3`) was masking. Tightening is
  a follow-up phase.
- `scripts/collect-metrics.sh` no longer measures `target/release/disky-mcp`
  (binary removed in v0.10.0). `binary_size_bytes` envelope is now
  `{"disky": N}` only.

### Fixed
- `CONTRIBUTING.md` performance/metrics rule no longer references the
  removed `disky-mcp` binary; rule §5 now mentions only the single
  `disky` bin.

## [Unreleased]


### Added
- `disky cleanup` (CLI + MCP) now adds `summary: [CategorySummary]` and
  `total_bytes` to the JSON envelope, aggregating hits across paths per
  category. Text mode prints a second table beneath the per-path list
  showing TOTAL / FILES / PATHS per category and the grand total. Lets
  an agent answer "how many GB can I reclaim across all node_modules?"
  in one call.
- New `cleanup::summarise` helper + `CategorySummary` record; schema
  descriptor includes it.
- Integration test covers per-category aggregation across two projects
  with overlapping basenames.
- TUI header now shows a red `PARTIAL` badge when the loaded snapshot's
  last scan was cancelled. Surfaces F12 state to humans, matching the
  `partial: true` flag exposed to agents in `disky stats --format json`.
- `disky diff <a> <b>` (+ `disky_diff` MCP tool): compares two snapshots
  and reports added / removed / grew / shrank files ordered by absolute
  delta. Uses DuckDB `ATTACH` for a single-statement FULL OUTER JOIN.
  New `query::diff` + `render::diff` + `DiffRow` record. Lets agents
  answer "what changed since the last scan?" without bespoke SQL.
- `disky scan --emit-top N --emit-dirs N --emit-ext N --emit-stats` bundles
  the query results into a `scan_bundle` envelope so agents skip the usual
  scan→stats→top→dirs round-trip. MCP `disky_scan` accepts the same
  `emit_top`/`emit_dirs`/`emit_ext` integers and now always returns a
  `scan_bundle` (with `complete` from the cancellable scan outcome).
- `disky cleanup --apply --reversible` (and `disky_cleanup` MCP arg
  `reversible: true`) moves paths to `~/.Trash/<name>-<unix-ts>` instead
  of `rm -rf`-ing them, so a misfire can be undone from Finder. Default
  `--apply` without `--reversible` keeps the permanent-delete behaviour.
  Implements the `--reversible` pattern called out in plan §3.
- `cleanup::ApplyMode { Delete, Trash }` enum in the lib.
- Integration test covers the trash path (synthesises a `node_modules`,
  asserts source gone, then sweeps the matching trash entry).
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
