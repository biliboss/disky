# Changelog

All notable changes to disky will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
