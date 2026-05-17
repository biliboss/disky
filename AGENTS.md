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

## Agent-native output

Query commands (`top`, `dirs`, `ext`, `find`, `stats`, `list`) honour `--format`:

| Flag | Behaviour |
|------|-----------|
| `--format text` (default on a TTY) | Fixed-width ASCII tables |
| `--format json` (default when stdout is piped) | Single JSON envelope `{schema_version, kind, records}`. Bytes as `u64`, paths absolute, `mtime` as RFC 3339 UTC |
| `--format ndjson` | One JSON record per line — stream-friendly for `jq -c` |

Errors in machine mode are emitted to **stderr** as RFC 9457 problem details:
`{schema_version, type, title, status, detail, retryable}`. The `type` URI
(`https://disky.dev/errors/<slug>`) is the stable dispatch key — agents should
match on `type`, not the localized `detail` string.

### Exit code taxonomy

| Code | Slug | Meaning |
|------|------|---------|
| 0 | `ok` | Success |
| 1 | `generic` | Unclassified error |
| 2 | `usage` | Bad CLI usage (emitted by clap) |
| 3 | `io` | I/O or permission error |
| 4 | `not-found` | Snapshot / path not found |
| 5 | `partial-scan` | Scan reached EOF with skipped entries (reserved) |
| 6 | `lock-held` | Snapshot DB locked by another process |

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

## Cleanup

| What | Command | Size |
|------|---------|------|
| Build artifacts | `cargo clean` | ~1GB |
| Scan snapshots | `rm ~/Library/Application\ Support/disky/*.db` | varies |
| Ad-hoc scan files | `rm auto` (gitignored, won't be committed) | ~300MB |

`.gitignore` covers: `/target`, `*.db`, `auto`, `dist/`, `.claude/`

Test with release binary, not debug: `cargo build --release` → `./target/release/disky`.

## Deploy (devopless)

No containers, no registry, no CI gating for releases — artifacts ship direct.

| Scenario | Command |
|----------|---------|
| Normal release | `git tag vX.Y.Z && git push origin vX.Y.Z` → GH Actions builds + uploads `.tar.gz` |
| Hotfix (can't wait for CI) | `cargo build --release --target aarch64-apple-darwin` → `gh release upload vX.Y.Z target/.../disky` |
| Rollback | `gh release download vX.Y.Z -p '*.tar.gz'` → extract + replace binary |
| Share offline | `cargo build --release` → `scp target/release/disky user@host:~/bin/` |

**Before any release tag:** `cargo clippy -- -D warnings && cargo fmt --check` must pass locally — CI will fail otherwise and the release job depends on the build job.

**Versioning:** bump `version` in `Cargo.toml` + add CHANGELOG entry before tagging.
