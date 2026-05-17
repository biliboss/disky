# disky

> Fast macOS disk analyzer — scan, explore, clean up.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org)
[![macOS only](https://img.shields.io/badge/platform-macOS-lightgrey.svg)]()

---

## What it does

- **Scans** your entire disk in ~3 minutes (6.8M files) using parallel traversal
- **Stores** results as DuckDB snapshots — compare before/after cleanup
- **Explores** interactively via ncdu-style TUI
- **Cleans** known disk hoggers (`node_modules`, `target`, `__pycache__`, …)
- **Talks JSON / NDJSON** — `--format json` everywhere, RFC 9457 errors, stable
  exit codes — so it slots into agentic workflows
- **Ships an MCP server** (`disky-mcp`) — Claude Code / Cursor / Zed bind to
  typed tools instead of shelling out and regex-parsing tables

## Install

```bash
cargo install --git https://github.com/biliboss/disky
```

Or download a pre-built binary from [Releases](https://github.com/biliboss/disky/releases).

## Usage

```bash
# Scan and open TUI
disky scan /
disky

# CLI queries — all accept --format json|ndjson and --snapshot @latest|<id>|<path>
disky top                          # largest files
disky dirs                         # largest directories
disky ext                          # usage by extension
disky find "*.log"                 # find by pattern
disky stats                        # totals (partial: true if last scan was cancelled)
disky list                         # snapshot IDs + sizes
disky query "SELECT ext, SUM(size) FROM files GROUP BY ext"
disky cleanup --target node_modules,target  # dry-run; add --apply to delete
disky schema                       # JSON descriptor of commands + records
```

### Agentic / MCP

Run as an MCP server over stdio for Claude Code / Cursor / Zed:

```bash
disky-mcp  # exposes disky_scan, disky_top, disky_dirs, disky_ext, disky_find,
           # disky_stats, disky_query, disky_cleanup, disky_schema,
           # disky_list_snapshots
```

Output is JSON by default when stdout is piped. Exit codes are stable:
`0` ok · `1` generic · `2` usage · `3` io · `4` not-found · `5` partial-scan ·
`6` lock-held. Error payloads on stderr follow RFC 9457 problem details.

## TUI keybindings

| Key | Action |
|-----|--------|
| `↑↓` / `jk` | Navigate |
| `Enter` / `→` | Expand directory |
| `←` / `h` | Collapse / go up |
| `o` | Open in Finder |
| `c` | Copy path to clipboard |
| `e` | Export HTML report |
| `r` | Rescan reminder |
| `q` / `Esc` | Quit |

## How it's fast

- **`jwalk`** parallel traversal — uses all CPU cores
- **`getattrlistbulk`** macOS syscall — batch metadata fetch (vs one syscall per file in `readdir`)
- **`memchr`** — SIMD extension extraction, zero allocations
- **DuckDB Appender** — 50k-entry batch inserts, columnar storage
- **Release profile** — LTO thin, codegen-units=1, panic=abort

Benchmarks on a 500GB SSD with 6.8M files:
- Scan: ~3 min
- Query (top 50 files): <100ms
- TUI startup: <2s

## Snapshots

Snapshots live in `~/Library/Application Support/disky/YYYY-MM-DD_HH-MM.db`.

```bash
disky list                                # show all snapshots
disky tui --snapshot 2026-05-15_11-56     # open by ID
disky tui --snapshot /path/to/snap.db     # open by path
```

## Requirements

- macOS 12+ (arm64 or x86_64)
- Rust 1.75+ (for building from source)

## License

MIT — see [LICENSE](LICENSE)
