# Changelog

All notable changes to disky will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
