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
- **Exports** HTML cleanup reports with actionable commands

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

# CLI queries
disky top          # largest files
disky dirs         # largest directories
disky ext          # usage by extension
disky find "*.log" # find by pattern
disky stats        # totals
disky list         # available snapshots
```

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
disky list                          # show all snapshots
disky tui --db /path/to/snap.db    # open specific snapshot
```

## Requirements

- macOS 12+ (arm64 or x86_64)
- Rust 1.75+ (for building from source)

## License

MIT — see [LICENSE](LICENSE)
