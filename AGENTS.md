# disky

Fast macOS disk analyzer — scan, explore, clean up.

## Stack

| Crate | Purpose |
|-------|---------|
| `jwalk 0.8` | Parallel traversal — use `Parallelism::RayonNewPool(cpus)`, NOT `.num_threads()` (wrong API) |
| `duckdb 1.1` (bundled) | Storage — Appender API for batch inserts, 50k/batch |
| `ratatui 0.29` + `crossterm 0.28` | TUI — ncdu-style tree, requires real TTY |
| `flume 0.11` | Bounded channel walker→writer, cap 256 |
| `memchr 2` | `memrchr(b'.', ...)` for ext extraction (2-3x faster than `Path::extension`) |

## Snapshots

Auto-saved to `~/Library/Application Support/disky/YYYY-MM-DD_HH-MM.db` via `dirs::data_local_dir()`.

## CI

`cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo build --release` on `macos-latest`.
Run `cargo fmt` before committing — rustfmt has opinions on inline if-else.

## Clippy gotchas

- `sort_by(|a,b| b.x.cmp(&a.x))` → use `sort_by_key(|b| Reverse(b.x))`
- `or_else(|| f())` → use `or_else(f)` when closure is redundant
- Unused struct fields must be prefixed `_field` or removed

## Release

Matrix: `aarch64-apple-darwin` + `x86_64-apple-darwin`. Tag `vX.Y.Z` triggers workflow.
CHANGELOG.md uses Keep a Changelog format — awk extracts entry per tag.
