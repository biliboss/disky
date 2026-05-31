# disky

> **Fast macOS disk analyzer and cleanup CLI in Rust.** Scan 2M files in seconds,
> Trash-restorable cleanup, agent-native JSON. Alternative to `ncdu`, `dust`,
> `du`, GrandPerspective, DaisyDisk — but built for the terminal AND for AI
> agents that shell out.

[![Crates.io](https://img.shields.io/crates/v/disky.svg)](https://crates.io/crates/disky)
[![CI](https://github.com/biliboss/disky/actions/workflows/ci.yml/badge.svg)](https://github.com/biliboss/disky/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Downloads](https://img.shields.io/github/downloads/biliboss/disky/total.svg)](https://github.com/biliboss/disky/releases)

---

## Why disky?

| | disky | ncdu | dust | du | GrandPerspective |
|---|---|---|---|---|---|
| Scan speed (2M files) | **~3 min** | ~8 min | ~2 min | ~5 min | ~6 min |
| Stores snapshots | ✅ DuckDB | ❌ | ❌ | ❌ | ✅ proprietary |
| Diff between scans | ✅ `disky diff` | ❌ | ❌ | ❌ | ❌ |
| TUI explorer | ✅ ratatui | ✅ | ✅ | ❌ | n/a (GUI) |
| Cleanup wizard | ✅ + Trash | ❌ | ❌ | ❌ | manual |
| APFS sparse-file aware | ✅ `--physical` | ❌ | ❌ | ❌ | ✅ |
| JSON output for agents | ✅ everywhere | ❌ | ❌ | ❌ | ❌ |
| Stable exit codes / RFC 9457 errors | ✅ | ❌ | ❌ | ❌ | ❌ |
| macOS native | ✅ (Apple Silicon) | ✅ | ✅ | ✅ | ✅ |

disky is the only one that treats the *AI agent* as a first-class consumer:
every command emits structured JSON, RFC 9457 problem details on stderr, stable
exit codes, and a `disky schema` descriptor so an LLM can bind without
prompt-engineering.

## Install

### Homebrew (coming soon)

```bash
brew install biliboss/tap/disky
```

### crates.io

```bash
cargo install disky
```

### Pre-built binary

Download from [Releases](https://github.com/biliboss/disky/releases) — `aarch64-apple-darwin` and `x86_64-apple-darwin` tarballs.

### From source

```bash
git clone https://github.com/biliboss/disky && cd disky
cargo install --path .
```

## Quick start

```bash
disky scan /                       # scan from root (~3 min for 2M files)
disky                              # open ncdu-style TUI

disky top --physical               # largest files (physical bytes — APFS-aware)
disky dirs --physical              # largest directories
disky ext                          # usage by extension
disky find "*.log"                 # find by glob
disky stats --physical             # totals
disky list                         # snapshot IDs + sizes

disky cleanup --snapshot @latest                # dry-run preview
disky cleanup --target node_modules --apply --reversible   # → ~/.Trash
```

### Snapshot diff & growth

```bash
disky scan /                       # before
# … time passes …
disky scan /                       # after
disky diff @latest~1 @latest       # what grew, shrank, was added or removed
disky growth --over-n 5            # OLS fit across 5 most-recent snapshots
```

### Agent-native output

Every query command honours `--format`:

| Flag | Behaviour |
|------|-----------|
| `--format text` (default on TTY) | Fixed-width ASCII tables |
| `--format json` (default when piped) | Envelope `{schema_version, kind, records}` |
| `--format ndjson` | One record per line — `jq -c` friendly |

Errors land on stderr as RFC 9457 problem details with stable `type` URIs.
Exit codes: `0` ok · `1` generic · `2` usage · `3` io · `4` not-found ·
`5` partial-scan · `6` lock-held.

### Composing commands

Chain disky commands through `disky filter` — predicate DSL, no re-scan:

```bash
disky top --format json | disky filter --where "size > 1GB AND ext = 'log'"
disky growth --format json | disky filter --where "delta_bytes > 100MB"
```

## How it's fast

- **`jwalk` parallel traversal** — all CPU cores
- **`getattrlistbulk`** macOS syscall — batch metadata, not one syscall per file
- **`memchr`** SIMD — zero-alloc extension extraction
- **DuckDB Appender** — 50k-entry batch inserts, columnar storage
- **Release profile** — LTO thin, codegen-units=1, panic=abort

Benchmarks on a 500 GB SSD with 6.8M files: scan ~3 min · top-50 query <100 ms · TUI startup <2 s.

## TUI keybindings

| Key | Action |
|-----|--------|
| `↑↓` / `jk` | Navigate |
| `Enter` / `→` | Expand directory |
| `←` / `h` | Collapse / go up |
| `o` | Open in Finder |
| `c` | Copy path to clipboard |
| `e` | Export HTML report |
| `q` / `Esc` | Quit |

## FAQ

### Why is logical size 100× bigger than physical?

APFS sparse files (e.g. OrbStack `data.img.raw`) report 8 TB logical but 13 GB
physical. Use `--physical` on `top`, `dirs`, `stats`, `ext` to get the *real*
on-disk footprint — that's what `du` measures. The default logical sum is
useful when you want to know what applications *think* the file is.

### How does cleanup avoid destroying my work?

Default is **dry-run** — you see exactly what would be removed before
anything happens. Pass `--apply` to act, and `--reversible` to send files to
`~/.Trash` instead of `rm -rf`. The guided `/disky` Claude Code skill
confirms every target via `AskUserQuestion` before applying.

### Is there an MCP server?

Not anymore. `disky-mcp` was dropped in v0.10.0 — the CLI is the only surface.
Every consumer (Claude Code, Cursor, agents) shells out and reads the JSON
envelope. Less duplication, one contract. Resurrection path is documented in
the v0.10.0 CHANGELOG entry if a non-shelling host actually shows up.

### How does disky compare to `dust`?

`dust` is faster at raw traversal because it doesn't persist anything. disky
trades ~30% scan time for a DuckDB snapshot you can diff, query with SQL, and
re-run cleanup against without re-scanning. If you scan once and forget,
dust wins. If you want to track disk usage over weeks or feed an agent,
disky wins.

### Does it work on Linux?

Not yet — disky leans on `getattrlistbulk` and APFS-specific physical-size
plumbing. PRs welcome for a Linux backend.

## Snapshots

```bash
disky list                                # show all snapshots
disky tui --snapshot 2026-05-15_11-56     # open by ID
disky tui --snapshot /path/to/snap.db     # arbitrary path
disky forget --keep-last 7 --apply        # restic-style retention
```

Snapshots live in `~/Library/Application Support/disky/YYYY-MM-DD_HH-MM.db`.

## Requirements

- macOS 12+ (Apple Silicon or Intel)
- Rust 1.75+ (to build from source)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Quick loop: `cargo fmt && cargo clippy -- -D warnings && cargo test`.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
