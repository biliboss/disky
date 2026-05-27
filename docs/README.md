# disky · docs/

Committed artifacts that surround the binary: release reports, research
HTML, recovered design notes. Anything visual / explanatory that is not
source code lives here.

## Layout

```
docs/
├── README.md                         this file
├── reports/                          per-version delivery reports
│   ├── v0.10.0-release.html          canonical · skeleton-markdoc template
│   ├── v0.10.0-release-simple.html   draft predecessor — kept for diff
│   └── v0.10.0-roadmap-monte-carlo.html   pre-cut roadmap w/ CT/LT/MC
└── research/                         recovered prior-session HTML
    ├── disky-top10-recovered.html         May 21 · top10 agent enhancements
    └── rust-vs-giants-recovered.html      May 27 · Rust build size vs alts
```

## Conventions

- One canonical release report per tag: `reports/v<X.Y.Z>-release.html`,
  built from `~/.claude-skills/html/templates/skeleton-markdoc.html`.
- Drafts and predecessors stay alongside as `-simple.html` / `-roadmap.html`
  etc — **never overwrite** previous reports; new file per cut.
- Research HTML stays in `research/`, suffixed `-recovered.html` when
  rebuilt from session JSONL.
- All reports follow the AGENTS.md "Design Feel" palette (paper / ink /
  rust / olive / Fraunces + Manrope + JetBrains Mono).

## Generating a new release report

```bash
cp ~/.claude-skills/html/templates/skeleton-markdoc.html \
   docs/reports/v$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)-release.html
# then Edit: <title>, header tag/subtitle/date, and the entire
# <script type="text/markdown" id="src"> markdown block.
```
