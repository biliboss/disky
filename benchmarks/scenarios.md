# Benchmark scenarios

Concrete benchmark IDs locked in `metrics/baseline.json`. Each ID corresponds to one criterion bench (under `benches/`) or one hyperfine run (under `scripts/`).

## Scan benchmarks

| ID | Fixture | Tool | Target (M-class) | Source |
|----|---------|------|------------------|--------|
| `scan-small` | 100-file tree | criterion `benches/scan.rs` | < 5 ms | `mk-bigtree.sh 100` |
| `scan-medium` | 10k-file tree | criterion `benches/scan.rs` | < 100 ms | `mk-bigtree.sh 10000` |
| `scan-large` | 100k-file tree | criterion `benches/scan.rs` | < 1500 ms | `mk-bigtree.sh 100000` |
| `scan-vs-dua-10k` | 10k-file tree | hyperfine `bench-competitors.sh` | disky / dua ≤ 1.5× | `mk-bigtree.sh 10000` |
| `scan-vs-dua-100k` | 100k-file tree | hyperfine `bench-competitors.sh` | disky / dua ≤ 1.3× | `mk-bigtree.sh 100000` |

## Query benchmarks (the amortized win)

| ID | Fixture | Tool | Target |
|----|---------|------|--------|
| `query-top` | 100k-row snapshot | criterion `benches/query.rs` | < 5 ms |
| `query-dirs` | 100k-row snapshot | criterion `benches/query.rs` | < 10 ms |
| `query-ext` | 100k-row snapshot | criterion `benches/query.rs` | < 8 ms |
| `query-stats` | 100k-row snapshot | criterion `benches/query.rs` | < 3 ms |
| `query-x10-vs-dua-x10` | 100k tree, 10 queries | hyperfine | disky total < 1/10 of dua total |

## Time-series benchmarks (v0.9.0+)

| ID | Fixture | Tool | Target |
|----|---------|------|--------|
| `growth-2-snapshots-100k` | 2× 100k-row snapshots | criterion `benches/growth.rs` | < 100 ms |
| `growth-30-snapshots-100k` | 30× 100k-row snapshots | criterion `benches/growth.rs` | < 500 ms |
| `pattern-classify-30` | 30 snapshots | criterion `benches/pattern.rs` | < 500 ms |
| `predict-30-snapshots` | 30 snapshots | criterion `benches/predict.rs` | < 50 ms |

## Build-time benchmarks (track-only, no CI gate)

| ID | Operation | Target | Tracked in |
|----|-----------|--------|-----------|
| `build-check-incremental` | `cargo check --all-targets` (no-op edit) | < 1 s | `metrics/build-timings.jsonl.timings_s.cargo_check_inc` |
| `build-test-fast-tier` | `cargo nextest run --lib --test lib_integration` | < 2 s | `…nextest_fast_tier` |
| `build-debug-incremental` | `cargo build` (no-op edit) | < 1 s | `…build_debug_inc` |
| `build-release-incremental` | `cargo build --release` (no-op edit) | < 1 s | `…build_release_inc` |
| `build-release-cold` | `cargo clean && cargo build --release` | ≤ 180 s | `…build_release_cold` (tag-time only) |

## Promotion rules

- A scenario stays "yellow" until measured on the baseline machine.
- A scenario gets "green" status when 3 consecutive runs are within 10% of target.
- A scenario gets "regressed" status if any single PR's measurement exceeds the budget by >15%.
- Regressed scenarios block release tags.

## Reproducibility

All scenarios reproducible via `just bench` (criterion) + `just bench-cmp-10k` / `bench-cmp-100k` (hyperfine). System info captured per run in `metrics/build-timings.jsonl[*].machine`.

CI runners are slower than dev boxes — separate baseline `metrics/baseline-ci.json` will track the CI side once benches run there.
