# Roadmap v0.8.0 → v0.12.0

User goal: **agents manage disk space with precision** — analyze content, detect time patterns of usage/size-growth, flag log generators, predict full-disk dates.

This roadmap re-prioritizes the original 30+ enhancement ideas around that goal. Decisions backed by `metrics/competitors-latest.json` (disky 3.62× slower than dua at 1k tree — needs amortized win) + the deferred-milestone priority list.

## North-star agent flow

```
overnight: launchd scans / and ~/Library every 6h, retains 7 daily + 4 weekly + 12 monthly
morning:   agent calls disky_discover → disky_growth --over 24h → disky_predict
result:    "/Library/Caches grew 2.3 GB in the last day (log-shaped, +110 MB/h).
            At this rate your disk fills in 9 days. Recommended cleanup:
            ~/Library/Caches/com.docker.docker (8.2 GB, 95% confidence safe to clear)."
```

Three primitives turn that into reality:

1. **Growth analyzer** — `disky growth --over <duration>` reads N snapshots, emits per-dir Δ + rate.
2. **Pattern classifier** — labels each growing dir as `log-shaped` / `burst` / `stable` / `declining`.
3. **Predictor** — linear extrapolation → "disk fills in X days at this rate".

Plus an automation surface (`disky schedule install` for launchd) and the agentic plumbing (`disky filter --json-input`).

## Versioned plan

| Version | Theme | Headline features | Why it ships in this order |
|---------|-------|-------------------|---------------------------|
| v0.8.0  | **Composability + analysis foundation** | `--json-input` chain · `disky filter` · `disky growth` (point-in-time Δ between 2 snapshots) · `disky explain <path>` · BDD scenario library | Without `growth` agents can't reason about time. Without `--json-input` they can't compose. Without scenarios we can't say "done". |
| v0.9.0  | **Time-series + classification** | `disky growth --over <duration>` (N-snapshot regression) · pattern classifier (`log-shaped` / `burst` / `stable` / `declining`) · `disky churn --over <duration>` (mtime-based activity heatmap) · 10k + 100k competitor benchmarks + post-snapshot query bench | This is the heart of "manage with precision". Bench work justifies new code paths against the 3.62× finding. |
| v0.10.0 | **Prediction + automation** | `disky predict` (when-disk-fills, per-volume + per-dir) · `disky schedule install` (launchd plist for macOS, cron unit for Linux) · `disky watch <path>` (FSEvents incremental rescan) · MCP `disky_predict` + `disky_growth` + `disky_churn` tools | Predict needs the regression from v0.9. Schedule needs forget retention (already shipped) so daily snapshots don't fill the snapshot dir. |
| v0.11.0 | **MCP surface + content intelligence** | MCP `resources/list` (snapshots) + `resources/read` (summary) + `prompts/list` (3 flows: find-disk-hoggers, weekly-report, safe-cleanup-plan) · `disky dups` (blake3) · `.diskyignore` via `ignore` crate · refactor `disky-mcp.rs` → `src/mcp/` | Resources/prompts make growth + predict visible in Claude Desktop. Dups closes the cleanup-suggestion loop. |
| v0.12.0 | **DX, polish, distribution** | `disky completions` · `disky --man` · Homebrew tap · TUI growth view · ASCII bars + colors · auto-report generator (`scripts/render-report.sh`) · CSV/markdown export formats | Once the engine is solid, ship it. |

## Per-version BDD acceptance criteria

Every version's "done" gate = the scenarios in `features/*.feature` pass. See:

- `features/disk-management.feature` — user-facing scenarios for the agent goal
- `features/benchmarks.feature` — performance scenarios with target numbers
- `features/composability.feature` — pipe + chain semantics

Scenarios are documentation FIRST, executable second. Each `.feature` file pairs with a `tests/bdd/<feature>.rs` that drives the lib API to verify Given/When/Then.

## Metrics gates per version

Required before tagging:

- `just metrics` clean, all timings within 15% of baseline (or baseline reseeded with justification)
- `just bench-cmp-10k` run; disky's ratio vs `dua` recorded in CHANGELOG
- New BDD scenarios green
- `cargo clippy --all-targets -- -D warnings` clean

For v0.9.0 onward: also run `just bench-cmp-100k` and the post-snapshot query bench.

## Hard performance commitment

By **v0.9.0** disky must be ≤ 1.5× `dua` on scan-only AND > 100× `dua` on amortized query (scan once + 10 queries). If not met, v0.9.0 ships behind a `--no-snapshot` mode that delegates straight to the walker (saves the DuckDB cost).

By **v0.10.0** the `disky predict` command must return in < 50ms on a 100k-row, 30-snapshot history. Else extrapolation moves to an indexed table (DuckDB native).

## Source of authority

When tradeoffs arise: this roadmap > the original `plan-out-the-implementation-frolicking-origami.md` > the research doc. The goal narrowed; older plans defer.
