# Performance scenarios with explicit target numbers.
#
# Each scenario maps to a benchmark in `benches/` (criterion) or
# `scripts/bench-competitors.sh` (hyperfine). Targets are commitments
# locked in `metrics/baseline.json`; regression > 15% fails the CI gate.

Feature: Scan throughput
  As a Rust-backed disk analyzer
  I should match or beat C-based competitors on raw walk speed

  Scenario: scan 10k tree completes within 1.5× dua
    Given a synthetic tree with 10,000 files at depth <= 4
    When I run `disky scan` and `dua aggregate` via hyperfine
    Then disky's mean runtime is at most 1.5× dua's mean runtime

  Scenario: scan 100k tree completes within 1.3× dua
    Given a synthetic tree with 100,000 files
    When the benchmark runs
    Then disky's mean runtime is at most 1.3× dua's
    And disky's runtime is < 1500 ms on Apple M-class hardware

Feature: Amortized query value
  Given disky persists scans as DuckDB snapshots
  Subsequent queries should obliterate scan-on-every-question

  Scenario: 10 sequential queries on a cached snapshot beat 10 dua re-runs
    Given a snapshot of a 100k-file tree
    When I run 10 of {disky top, disky dirs, disky ext, disky growth, disky stats}
    And separately run dua 10 times on the same tree
    Then disky's total wall time is < 1/10 of dua's
    And per-query latency for disky is < 50 ms

Feature: Growth analysis cost
  Given the agentic flow centers on growth + predict
  Those queries should be cheap enough for real-time use

  Scenario: growth between 2 snapshots in < 100 ms
    Given two snapshots of 100k rows each
    When I run `disky growth --since <prev>`
    Then result returns in < 100 ms

  Scenario: pattern classification over 30 snapshots in < 500 ms
    Given 30 snapshots in the data dir
    When I run `disky pattern --over 30d`
    Then result returns in < 500 ms
    And the per-dir regression uses DuckDB native aggregates, not Rust-side iteration

Feature: Build-time budget
  Inner dev loop must stay sub-second

  Scenario: cargo check incremental < 1 s
    Given a clean target/ + a no-op edit
    When `cargo check --all-targets` runs
    Then it completes in < 1 s

  Scenario: nextest fast tier < 2 s
    When `cargo nextest run --lib --test lib_integration` runs
    Then total time is < 2 s on the baseline machine
