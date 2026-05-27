# Contributing to disky

## Inner loop

Fast tier (default `just test`, <2s):

```
just check       # cargo check --all-targets
just lint        # cargo clippy --all-targets -- -D warnings
just test        # nextest: lib unit + lib_integration only
just fmt         # cargo fmt
```

Release tier (release binary + agentic CLI tests, ~30s):

```
just test-cli    # cargo build --release && nextest --test agentic --test mcp_protocol
```

Full CI parity:

```
just ci          # fmt-check + lint + test + test-cli
```

File-watched dev loop:

```
bacon            # uses bacon.toml — clippy → nextest on every save
```

## Profile hygiene

| Profile | Used by | Notes |
|---------|---------|-------|
| `dev` | `cargo build`, `cargo run` | Our code at `opt-level=0` for fast compile |
| `dev.package."*"` | All deps | At `opt-level=3` so heavy deps (duckdb, ratatui, jwalk) compile hot once |
| `test` (inherits dev) | `cargo test`, `cargo nextest` | **Important**: tests run at `-O0`. Perf-sensitive checks belong in `benches/`, NEVER in `tests/` |
| `dev-fast` | opt-in `cargo build --profile dev-fast` | Our code at `-O1` middle ground |
| `release` | `cargo build --release` | lto=thin, codegen-units=1, panic=abort |
| `bench` (cargo default = release) | `cargo bench` | Criterion harness |

## Performance & metrics rule

**Every report on disky's status carries:**

1. Git metadata — HEAD hash, branch, ahead/behind, author, dirty flag.
2. Toolchain — `rustc` + `cargo` versions.
3. Machine — OS, CPU model, cores, mem.
4. Build timings — `cargo check`, `nextest`, debug+release builds.
5. Binary size — `target/release/disky` (single bin since v0.10.0; `disky-mcp` was removed).
6. Test count by tier (fast / CLI).
7. LOC by directory (src / tests / benches).
8. Competitor benchmark — disky vs `dust` / `dua` / `gdu` / `du` (refreshed monthly or on release tag).
9. Trend vs prior version (Δ vs the last entry whose `commit` differs).

Source of truth: `metrics/build-timings.jsonl` (append-only) + `metrics/baseline.json` (locked) + `metrics/competitors-latest.json` (overwritten).

### How to collect

```
just metrics              # fast — adds one JSONL line, ~10s
just metrics-cold         # adds a line including cold release build, ~3min (tag-time only)
just bench-cmp-10k        # competitor benchmark on 10k synthetic tree, ~30s
just metrics-reseed-baseline   # ONLY when a measured slowdown is intentional
```

### Budgets (advisory)

| Operation | Target | Source |
|-----------|--------|--------|
| `cargo check --all-targets` incremental | < 1s | `timings_s.cargo_check_inc` |
| `just test` (fast tier) | < 2s | `timings_s.nextest_fast_tier` |
| `cargo build` debug incremental | < 1s | `timings_s.build_debug_inc` |
| `cargo build --release` incremental | < 1s | `timings_s.build_release_inc` |
| `cargo build --release` cold | ≤ 180s | `timings_s.build_release_cold` (duckdb bundled is the floor) |

A PR that regresses any of these >15% vs `metrics/baseline.json` needs an explicit justification in the commit body.

## Per-feature checklist

Before opening a PR:

- [ ] Red unit test in `src/<module>.rs#tests` first
- [ ] Implementation makes it green
- [ ] `tests/lib_integration.rs` covers the public surface
- [ ] If CLI-visible: one case in `tests/agentic.rs`
- [ ] If MCP-visible: one case in `tests/mcp_protocol.rs` (once it exists)
- [ ] `src/schema.rs` JSON descriptor updated in the same diff
- [ ] `AGENTS.md` section updated; `CHANGELOG.md` line added
- [ ] `SCHEMA_VERSION` bumped only on breaking shape change (additive → no bump)
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] New dep → justification in commit body, license check, build-time Δ noted
- [ ] Hot-path change → criterion bench added or updated; baseline.json refreshed only if intentional
- [ ] `just metrics` run; new JSONL line committed
- [ ] If shipping a release tag: `just metrics-cold` + `just bench-cmp-10k` also run

## Build footprint — kill the 1-2 GB `target/`

Default Rust build dumps a fat `target/` next to every project (~1.5 GB
here once duckdb + ratatui + jwalk compile). Two changes recover most of
that without changing iteration speed.

### 1. Global cargo cache (registry + git deps shared, no extra work)

`$CARGO_HOME` defaults to `~/.cargo` already — verify it's not overridden
per-project:

```bash
echo "${CARGO_HOME:-$HOME/.cargo}"   # should print ~/.cargo
ls -lah ~/.cargo/{registry,git} 2>/dev/null | head
```

If you have a per-project `.cargo/config.toml` setting `[build] target-dir`
to somewhere local — drop it. The default `target/` is per-project; the
**deps** are already shared globally via `$CARGO_HOME/registry`. No action
needed unless you previously overrode this.

### 2. `sccache` — share compiled artifacts across projects

Single biggest win: every other rust project on this machine that touches
`duckdb-sys`, `ratatui`, etc. re-uses the same `.o` files.

```bash
brew install sccache                   # one-time
mkdir -p ~/.cargo
cat >> ~/.cargo/config.toml <<'EOF'

[build]
rustc-wrapper = "/opt/homebrew/bin/sccache"
EOF
```

Verify the next build hits the wrapper:

```bash
sccache --zero-stats
cargo build --release           # first run still cold
sccache --show-stats            # subsequent runs should show hits >0
```

Optional — bump the cache size (default 10 GB):

```bash
echo 'export SCCACHE_CACHE_SIZE="30G"' >> ~/.zshenv
```

### 3. Recurring hygiene

```bash
cargo clean                            # nuke this project's target/
just metrics                           # confirm build timings still in budget
```

A periodic `find ~/src -name target -type d -exec du -sh {} +` surfaces the
biggest offenders across all Rust projects.

### Why not delete `target/` aggressively in CI / hooks?

Cold release builds run ~3 min (duckdb bundled C++). sccache turns cold
builds into warm ones across projects — that's the whole point. Manual
`cargo clean` only when reclaiming disk explicitly.

## Pre-commit hook

```
bash scripts/install-hooks.sh
```

Runs `cargo fmt --check && cargo clippy --all-targets -- -D warnings` on every commit.

## Tooling

| Tool | Install | Used for |
|------|---------|----------|
| `cargo-nextest` | `cargo install cargo-nextest --locked` | Faster test runner |
| `bacon` | `cargo install bacon` (optional) | File-watched dev loop |
| `sccache` | `brew install sccache` + wire via `~/.cargo/config.toml` (see "Build footprint" §2) | C++ cache for duckdb bundled · shared across all Rust projects |
| `hyperfine` | `brew install hyperfine` | Competitor benchmarks |
| `dust`, `dua-cli`, `gdu` | `brew install dust dua-cli gdu` | Competitor benchmark targets |
