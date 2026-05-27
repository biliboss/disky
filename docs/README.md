# disky · docs/

Committed artifacts that surround the binary: release reports, research
HTML, recovered design notes. Anything visual / explanatory that is not
source code lives here.

## Layout

```
docs/
├── README.md                                 this file
├── reports/                                  per-version delivery reports
│   ├── v0.10.0-release.html                  CANONICAL · flow-metrics format
│   ├── v0.10.0-release-editorial.html        editorial draft (skeleton-markdoc)
│   ├── v0.10.0-release-simple.html           inline-CSS draft predecessor
│   └── v0.10.0-roadmap-monte-carlo.html      pre-cut roadmap (same shape as canonical)
└── research/                                 recovered prior-session HTML
    ├── disky-top10-recovered.html            May 21 · top10 agent enhancements
    └── rust-vs-giants-recovered.html         May 27 · Rust build size vs alts
```

## Canonical report format

Per-version release reports use the **flow-metrics** shape:

1. Header eyebrow / h1 / sub / metadata strip
2. Section 01 — Flow tiles (CT p50/p85, LT p50, throughput, WIP, backlog, done)
3. Section 02 — Kanban (3 cols: Done / Dropped / Deferred — post-release)
4. Section 03 — Cycle Time histogram (SVG bars)
5. Section 04 — Lead Time bars (per-tag)
6. Section 05 — Monte Carlo CDF for next version (10k sims, live JS)
7. Section 06 — Throughput per active day (grouped bars + table)
8. Section 07 — Recommended order (table, S→L, value-first)
9. Section 08 — Bonus / spike notes
10. Section 09 — Próximos passos

Single HTML file, zero deps, brut palette from AGENTS.md "Design Feel"
(paper / ink / rust / olive + Fraunces / Manrope / JetBrains Mono).
SVG rendered inline; Monte Carlo computed in JS at page load (live).

## Conventions

- Never overwrite a previous report — add a new file per cut.
- Drafts kept alongside as `-simple.html` / `-editorial.html` for diff.
- Research HTML stays in `research/`, suffixed `-recovered.html` when
  rebuilt from session JSONL.

## Generating a new release report

Easiest: copy the most recent canonical, update the JS data block + table
rows + tiles. Source of metrics:

```bash
git log --format='%at|%s' | grep feat   # for throughput / CT
git tag --sort=-creatordate              # for LT per tag
```
