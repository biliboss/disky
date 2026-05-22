---
name: disky
description: Scan the user's home directory with disky, run the standard query bundle (top, dirs, ext, churn, growth, cleanup, empty, old), and open a single-file HTML report in the browser. Use whenever the user says "/disky", "where did my disk go", "what's eating my SSD", "disk report", "scan my machine", "find cleanup", or wants to see disk-space analysis with safe cleanup recommendations.
---

# /disky — interactive disk-space report

When the user invokes `/disky` (or asks anything matching the description above), this skill runs the full disky agentic flow against their home directory and opens a Monster-UI-styled HTML report in their browser.

## Prerequisites

- `disky` binary on PATH (build from `~/src/disky`: `cargo install --path . --bins`).
- `uv` for the report renderer (`brew install uv` if missing).
- Default browser configured.

## What the skill does

1. **Confirm scope.** Ask the user if they want `$HOME` (default), `/`, or a custom path. Default after 5 s of silence: `$HOME`.
2. **Scan.** Run `disky scan <path> --db /tmp/disky-skill-<unix>.db`. Stream NDJSON progress to the user so they see live feedback.
3. **Query bundle.** Run these in parallel, redirect each to `/tmp/disky-skill-<unix>/`:
   - `disky stats --format json`
   - `disky top --limit 30 --format json`
   - `disky dirs --limit 30 --format json`
   - `disky ext --limit 30 --format json`
   - `disky churn --over 24h --format json`
   - `disky churn --over 7d --format json`
   - `disky cleanup --format json`
   - `disky empty --limit 50 --format json`
   - `disky old --older-than 1y --limit 50 --format json`
4. **Render.** `uv run claude-skill/disky/render.py /tmp/disky-skill-<unix>/ > /tmp/disky-report.html`.
5. **Open.** `open /tmp/disky-report.html` (macOS) or `xdg-open` (Linux).
6. **Summarise.** In chat, post the three biggest insights from the report — biggest single offender, biggest reclaimable category, top churn dir — with the exact `disky cleanup --apply --reversible --target …` commands the user can run.

## Tone & safety

- Surface the OrbStack sparse-file caveat if the top entry is &gt; 1 TB ("OrbStack disk image reports logical size; real physical is much smaller").
- Never run `disky cleanup --apply` automatically. Always present commands; user runs them.
- When recommending a deletion, default `--reversible` (moves to `~/.Trash`, restorable).
- If the user says "do it", run `disky cleanup --apply --reversible` with the exact targets they confirmed — never invent a new target.

## Output contract

The HTML report MUST follow the disky standard report rule (see `AGENTS.md` Design Feel + the standard report rule memory):

- Section 00: standard metrics block (git, machine, timings, binary size).
- Section 01: prior-context recap (what previous /disky runs reported).
- Section 02: delivered (current scan's findings).
- Section 03: roadmap (suggested next moves — interactive buttons).
- Section 04: insights.

Use the disky palette tokens only. Editorial brutalist aesthetic. Fraunces display, Manrope body, JetBrains Mono code.

## Invocation hints

This skill matches these user intents:
- "/disky"
- "scan my machine"
- "where did my disk go"
- "what's eating my SSD"
- "disky report"
- "disk space analysis"
- "find cleanup"
- "show me hot directories"
- "what's churning"

When any of those appears in user input, call this skill before answering generically.
