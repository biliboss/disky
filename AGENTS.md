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

## Snapshot diff

`disky diff <a> <b> [--limit N]` compares two snapshots and reports the
files that grew, shrank, were added, or were removed — ordered by absolute
delta. Both arguments accept `@latest`, an ID, or a path. JSON `records`
hold `DiffRow { path, kind: added|removed|grew|shrank, size_a, size_b, delta }`.

Useful for "what changed since the last cleanup": scan before, scan after,
`disky diff old_id new_id`.

## Cleanup

`disky cleanup` greps the snapshot for known disk-hoggy directories
(`node_modules`, `target`, `__pycache__`, `.next`, `dist`, `build`,
`.venv`/`venv`, `.gradle`, `.pytest_cache`). Default is dry-run.

```
disky cleanup --snapshot @latest                              # preview
disky cleanup --target node_modules,target --apply            # rm -rf
disky cleanup --target node_modules,target --apply --reversible  # → ~/.Trash
```

JSON output: `{kind:"cleanup", applied:bool, removed:[paths], records:[CleanupHit], summary:[CategorySummary], total_bytes:N}`.

**Performance (v0.11.0):** Single grouped range-join with a TEMP table
of target dirs. DuckDB plans zone-maps + IEJoin in one pass.
**79 s → < 1 s on a 1.77 M-row snapshot.** Integration test
`cleanup_is_fast_on_large_snapshot` in `tests/lib_integration.rs`.
Prior algos (per-target loop, LEFT JOIN with LIKE) preserved in
`src/cleanup.rs` history comment.

## `.diskyignore` loader (v0.11.0)

Module `src/ignore.rs` (170 LOC, std-only) provides:

- `default_skip_substrings()` — built-in baseline (node_modules, target, …).
- `load_diskyignore_chain(scan_root)` — walks ancestor dirs up to `$HOME`
  (or `/`), parses gitignore-subset format (one substring per line,
  `#` comments, blank lines skipped, no globs in v1).
- `should_skip(basename, patterns)` — substring match.

Module ships standalone in v0.11.0. Wiring into the live `src/scan.rs`
skip-list is a v0.11.1 deliverable (~10-line edit). 5 unit tests cover
empty dir, single file, comments, ancestor chain, malformed lines.

## N-snapshot growth — OLS (v0.11.0)

```
disky growth --over-n N [--fill-target <bytes>]   # default N=5, min 3
```

Fits an ordinary-least-squares line through `(snapshot_ts, size)` for
each directory present in the latest snapshot across the N most-recent
snapshots. Envelope `kind="growth_n"`:

```
{path, slope_bytes_per_day, r2, projected_fill_date, latest_bytes,
 n_snapshots, sample_paths_ts[[ts,bytes]…]}
```

Sorted by `slope_bytes_per_day DESC`. Pure f64, no stats crate. Math in
`src/query.rs::linfit_slope_r2`. The 2-snapshot `disky growth` path is
untouched — `--over-n` is additive.

## Pattern classifier (v0.11.0)

Module `src/pattern.rs` (325 LOC, std-only) classifies a directory's
size-over-time series into:

| Pattern | Meaning |
|---|---|
| `log_shaped` | Big initial growth that levels off (caches warming) |
| `burst` | Sudden late jump (download dump, import) |
| `stable` | Roughly flat across the series |
| `declining` | Trending down (cleanup landed, log rotation) |
| `unknown` | Mixed / unclear / < 3 samples |

Decision tree thresholds in `src/pattern.rs`:
- `SPIKE_RATIO = 4.0` (max-stride / median-stride for log_shaped/burst)
- `STABLE_RATIO = 3.0` (raised from sub-agent default 1.5 post-merge)
- `DECLINING_SLOPE_FRAC = -0.01`, `DECLINING_R2_MIN = 0.5`

10 unit tests cover each branch + edges. CLI flag `disky churn
--classify` wiring is a v0.11.1 deliverable; the module is callable
in-process today.

## Schema introspection

`disky schema` prints a JSON document describing commands, record shapes,
error codes, and snapshot-ref forms. Pair it with `--format json` on any
command to let an agent bind without prompt-engineering.

## Snapshot references

All query subcommands accept `--snapshot <ref>` where `<ref>` is:

| Form | Example | Resolves to |
|------|---------|-------------|
| `@latest` (default) | `@latest` | newest `*.db` in the data dir |
| Snapshot ID | `2026-05-15_11-56` | `<data dir>/<id>.db` |
| Filesystem path | `/tmp/scan.db` | itself, untouched |

`disky list` prints the ID, size, and full path.

## Raw SQL

`disky query "<sql>" --snapshot @latest --format json` (or `--format ndjson`)
runs an arbitrary SQL statement against the snapshot. The `files` table has
columns `path, name, ext, size, mtime, is_dir, depth`. Large integers (DuckDB
`HugeInt`) are emitted as strings to preserve precision; everything else maps
to native JSON types. Default row cap: 1000 (`--limit`).

## MCP server — REMOVED in v0.10.0

`disky-mcp` was deleted in v0.10.0. **CLI is the surface.** Any agent that
can shell out invokes `disky <cmd> --format json` directly. Hosts that
cannot shell (Claude Desktop, Cursor, Zed) are unsupported until earn-back
(see Three surfaces section). The `disky-mcp` bin can be resurrected from
git history (last commit before removal: `git log --all --diff-filter=D --
src/bin/disky-mcp.rs`).

## Cancellable scan

`disky scan` installs a SIGINT handler. On Ctrl-C it drains the in-flight
batch, marks the snapshot partial in `scan_meta`, and exits with status
`5` (`partial-scan`). The DB is still queryable; `disky stats` returns
`partial: true`.

## Bundled scan (cut round-trips)

`disky scan` can attach query results to its output so one CLI / MCP call
does what used to take four:

```
disky scan / --emit-top 50 --emit-dirs 20 --emit-ext 30 --format json
```

Returns a `scan_bundle` envelope with `stats`, `top`, `dirs`, `ext`. MCP
`disky_scan` accepts the same `emit_top` / `emit_dirs` / `emit_ext` ints.

## Scan progress (NDJSON on stderr)

When stderr is piped, `disky scan` emits NDJSON events instead of the spinner:

```
{"schema_version":1,"event":"start"}
{"schema_version":1,"event":"progress","scanned":120000,"bytes":48294821}
{"schema_version":1,"event":"done","scanned":342118,"bytes":81293048203,"db":"…"}
```

Throttled to 500ms between `progress` events.

## Agent-native output

Query commands (`top`, `dirs`, `ext`, `find`, `stats`, `list`) honour `--format`:

| Flag | Behaviour |
|------|-----------|
| `--format text` (default on a TTY) | Fixed-width ASCII tables |
| `--format json` (default when stdout is piped) | Single JSON envelope `{schema_version, kind, records}`. Bytes as `u64`, paths absolute, `mtime` as RFC 3339 UTC |
| `--format ndjson` | One JSON record per line — stream-friendly for `jq -c` |

## Design Feel — visual identity

disky's UX has a goal: **magic-feeling, safe, useful**. The CLI is honest. The HTML reports + the Claude Code skill should *delight*. Three pillars carry it:

1. **Contrast.** Paper (`#F5F1E8`) vs ink (`#14110F`). Rust (`#B23A1F`) for the single most important thing on a page. Olive (`#5C6831`) for wins / deltas. No grey-on-grey. No off-white-on-off-white. Numbers in Fraunces (variable serif, opsz 144) so they feel weighty next to Manrope body.
2. **Space.** Generous gutters (`gap-14` between sections). Section numbers as 3-rem Fraunces at left, blood-rust color — leaves an oversized indent without filler text. Tables breathe — 10–12 px padding, never cramped. Empty space carries the user's attention without instruction.
3. **Minimalism.** No cards inside cards. Every chart earns its place — Mermaid only when structure matters; tables otherwise. Status pills use a 7-color palette tied to meaning (done/partial/deferred/risk + 3 neutrals). No box shadows. No gradients. Editorial brutalism, not SaaS marketing.

Plus a fourth pillar quietly: **motion**. Hover lifts on cards (`hover:bg-paper-100`). Sparklines/charts animate-in on first paint. A spinner only appears when work crosses 200 ms. Otherwise the UI feels *instant* — that's the magic.

**Palette (use these tokens, not freelance hex):**

```
paper    #F5F1E8   page background, soft cream
ink      #14110F   primary text, near-black with warmth
rust     #B23A1F   accent — section markers, critical CTAs
olive    #5C6831   success / wins / positive Δ
dim      #6B655E   secondary text
line     #D9D2C2   table dividers, card borders
card     #FBF8F0   raised surfaces
done     #3F6B2A   pill green
partial  #B8841C   pill yellow
risk     #B23A1F   pill red (= rust)
deferred #8C857B   pill grey
```

**Type stack:**
```
display    'Fraunces' (variable serif, opsz 9..144, weight 300..900)
body       'Manrope'
mono/code  'JetBrains Mono'
```

**Tailwind config snippet (copy into any HTML target):**
```js
colors: { paper:'#F5F1E8', ink:'#14110F', rust:'#B23A1F', olive:'#5C6831',
          dim:'#6B655E', line:'#D9D2C2', card:'#FBF8F0',
          done:'#3F6B2A', partial:'#B8841C', deferred:'#8C857B', risk:'#B23A1F' }
fontFamily: { display:['Fraunces','serif'], sans:['Manrope','sans-serif'],
              mono:['JetBrains Mono','monospace'] }
```

Any HTML rendered by the `/disky` skill or release-report tooling MUST use these tokens. Any one-off hex outside this set is a bug.

## Surface scope (decision, v0.10.0 · 2026-05-27)

**CLI is the only surface.** Web server and MCP server were dropped in v0.10.0. Rationale: only real consumer is Claude Code, which shells out — every other surface duplicated the CLI and doubled maintenance. Validated by 4-round grill (2026-05-27, recoverable from session `dc2884dc`).

**One surface, one job:**

| Surface | When it wins |
|---------|--------------|
| CLI (`disky <cmd> --format json`) | Every agent that can shell out. Default and only. |
| `/disky` Claude Code skill | UX layer ON TOP of CLI: AskUserQuestion-driven cleanup wizard + HTML report. Does NOT replace CLI — wraps it. |

**Earn-back criteria** (when MCP / web get re-added):

1. A user on Claude Desktop / Cursor / Zed actually asks for disky and can't shell out → re-implement `disky-mcp` (1 active day from git history).
2. Browser-driven cleanup UX is requested by someone running disky outside an agent → add `disky web` (FastHTML local server, 1-2 active days).

Until either trigger, **CLI is canonical**. JSON envelopes + RFC 9457 errors + `disky schema` are the agent contract.

## Claude Code skill — `/disky` (v2 wizard)

Ships at `claude-skill/disky/`. Symlink to `~/.claude-pessoal/skills/disky/`
(and `~/.claude-mukutu/` if you want it in both profiles). Invoking
`/disky` runs a 4-stage **AskUserQuestion-driven cleanup wizard** — no
web server, no FastHTML, no MCP. Just CLI + the wizard.

Flow:

1. **Triage** — one AskUser classifies intent: free space / what grew / diagnose slowness.
2. **Scan + propose** — runs `disky scan`, `disky stats --physical`, `disky top --physical`, `disky cleanup --dry-run`. Builds a propose-3 table ranked by recovery bytes per category.
3. **Confirm each target** — one AskUser per proposed target with the exact command in the option description. Defaults to `--reversible` (Trash).
4. **Apply** — runs only confirmed targets. Reports JSON envelope back to chat. Accumulates `total_bytes` across the wizard.

**Always use `--physical`** for stats/top/dirs/ext queries. Logical
`size` includes sparse files (OrbStack `data.img.raw` reports 8 TB
logical, ~13 GB physical) and will look like the disk is 87% full when
it isn't. Use `df -h` as ground truth for device free space.

Optional final step: `uv run claude-skill/disky/render.py <db>` generates
an HTML report with the disky brut palette — click-to-copy code blocks
for any commands surfaced (no server, no POST). Reports go in
`docs/reports/` per the convention in `docs/README.md`.

Anti-patterns the skill rejects:

- `cleanup --apply` without explicit AskUser confirmation.
- Proposing targets that didn't surface in `cleanup --dry-run`.
- Inventing GB numbers ("liberará ~5 GB") when the envelope says exactly N bytes.
- Using `disky web` or `disky-mcp` (both removed in v0.10.0).

## Composability — `disky filter --json-input`

Chain disky commands together without re-scanning. Any command emitting a
records envelope can pipe into `disky filter` to apply a predicate.

```
disky top --format json | disky filter --where "size > 1GB"
disky old --older-than 30d --format json | disky filter --where "ext = 'log'"
disky growth --format json | disky filter --where "delta_bytes > 100MB"
```

**Predicate DSL** (intentionally small):

| Element | Examples |
|---------|----------|
| Fields | `size`, `ext`, `name`, `path` |
| Ops | `=`, `!=`, `>`, `<`, `>=`, `<=`, `LIKE` |
| Literals | `1024`, `1KB`, `1MB`, `1GB`, `1TB`, `1KiB`, `'log'`, `"my string"` |
| Chain | `AND` (case-insensitive) |

`LIKE` accepts `%` (any chars) and `_` (single char), SQL-style.

**Accepted input kinds:** `top`, `find`, `dirs`, `ext`, `empty`, `old`, `filter`, `growth`. Mismatch → exit 1 with a clear error message.

**Envelope output** `{kind: "filter", input_kind: "top", records: [...]}` — preserves the originating kind so downstream chains can dispatch.

## Physical vs logical size (APFS sparse files)

`files.size` is `st_size` — the logical/declared size of a file.
`files.physical_size` is `st_blocks * 512` — actual bytes on disk.

On macOS/APFS these can differ by orders of magnitude:

| File | Logical (`size`) | Physical (`physical_size`) |
|------|------------------|----------------------------|
| OrbStack `data.img.raw` | 8.8 TB | 13.1 GB |
| OrbStack `swap.img` | 1 GB | 4 KB |
| Regular files | identical | identical |

Use `physical_size` when answering "how much disk would I free if I delete this?" — that's what `du` measures. Use `size` when answering "how big does this file appear to applications?".

Raw SQL access today:
```sql
SELECT path, size, physical_size FROM files
WHERE is_dir = false ORDER BY physical_size DESC LIMIT 10
```

`disky top --physical` / `dirs --physical` / `stats --physical` / `ext --physical` flags shipped in v0.9.0. **Always pass `--physical` in agent flows** — the default logical sum is dominated by sparse files (OrbStack inflates it 100×) and misleads cleanup decisions.

## Snapshot retention (`disky forget`)

restic-style retention policy. Default dry-run; pass `--apply` to delete.
At least one `--keep-*` flag is required (otherwise exit code 2).

```
disky forget --keep-last 7 --keep-daily 30 --keep-weekly 12 --keep-monthly 12 --keep-yearly 5
disky forget --keep-last 5 --apply
```

Buckets keep the **newest** snapshot per bucket key:

| Flag | Bucket |
|------|--------|
| `--keep-last N` | N newest snapshots |
| `--keep-daily N` | newest per local date, up to N distinct dates |
| `--keep-weekly N` | newest per ISO week |
| `--keep-monthly N` | newest per calendar month |
| `--keep-yearly N` | newest per calendar year |

JSON envelope:
```
{"kind":"forget","applied":false,
 "kept":[{"id":"...","path":"...","bytes":N,"reasons":["last","daily"]}],
 "removed":[{"id":"...","path":"...","bytes":N}],
 "skipped_unparseable":["my-manual-snapshot"],
 "total_removed_bytes":N}
```

User-renamed snapshots (IDs that don't match `YYYY-MM-DD_HH-MM`) land in
`skipped_unparseable` and are **never** removed.

## Config file

`~/.config/disky/config.toml` (or `$DISKY_CONFIG_PATH`) supplies per-flag defaults so agents don't repeat `--format json --snapshot @latest` every call. Layer order: built-in defaults → file → env (`DISKY_FORMAT`, `DISKY_SNAPSHOT`) → CLI flag (CLI always wins).

```toml
[defaults]
format = "json"
snapshot = "@latest"

[scan]
threads = 0          # 0 = num_cpus
strategy = "parallel"  # parallel | sequential | adaptive
respect_gitignore = false
cross_device = false

[output]
color = "auto"       # auto | always | never (NO_COLOR env wins)
```

Malformed config fails fast with exit code 2 (usage) — typos are surfaced rather than silently ignored.

### Scalar stats (cheapest readout)

`disky stats` carries two extra flags for agents that only need totals:

| Flag | Output |
|------|--------|
| `--summarize` | `{schema_version:1, kind:"scalar", records:[{bytes, files}]}` — omits scan root, mtime, partial flag, largest file |
| `--raw` | Bare `bytes` integer on stdout, nothing else. Overrides `--format`. Implies `--summarize` semantics |

Use `--raw` for shell pipelines (`disky stats --raw | numfmt`) and `--summarize` when you still want a JSON envelope but want to skip the heavier fields the default `stats` record carries.

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

**Build footprint (v0.10.1+):** apply the `sccache` + `$CARGO_HOME`
global cache recipe in `CONTRIBUTING.md#build-footprint` before any
multi-project Rust work — kills the per-project 1-2 GB `target/` tax
by sharing compiled artifacts across every repo that touches
`duckdb-sys`, `ratatui`, etc.

## Clippy gotchas

- `sort_by(|a,b| b.x.cmp(&a.x))` → use `sort_by_key(|b| Reverse(b.x))`
- `or_else(|| f())` → use `or_else(f)` when closure is redundant
- Unused struct fields must be prefixed `_field` or removed

## Release

Matrix: `aarch64-apple-darwin` + `x86_64-apple-darwin`. Tag `vX.Y.Z` triggers workflow.
CHANGELOG.md uses Keep a Changelog format — awk extracts entry per tag.

## Dev-side cleanup (build artifacts)

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

## Single surface — CLI only (v0.10.0 · 2026-05-27)

Prior plan ("CLI + web + MCP three surfaces") was rejected after 4-round grill: only real consumer is Claude Code which shells out — every secondary surface duplicated the CLI without buying new users. `disky-mcp` (925 LOC) and the unbuilt `disky web` plan were dropped. CLI is contract.

**Authority for non-CLI hosts:** none. Until a real Claude Desktop / Cursor / Zed user requests disky and can't shell out, MCP / web do not return. Resurrection path documented in CHANGELOG v0.10.0.
