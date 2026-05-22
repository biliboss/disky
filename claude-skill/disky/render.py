#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "python-fasthtml>=0.6",
#   "monsterui>=0.0.15",
# ]
# ///
"""
disky report renderer — single-file FastHTML + Monster UI.

Reads JSON envelopes produced by the `/disky` Claude Code skill and emits
a single self-contained HTML page styled with the disky Design Feel
(paper/ink/rust/olive palette, Fraunces + Manrope, editorial brutalism).

Usage:
    uv run render.py /tmp/disky-skill-<unix>/ > /tmp/disky-report.html

Or as a server (preview while editing):
    uv run render.py /tmp/disky-skill-<unix>/ --serve

Input directory layout (one JSON file per disky command):
    stats.json  top.json  dirs.json  ext.json
    churn24.json  churn7d.json  cleanup.json
    empty.json  old.json
"""
from __future__ import annotations

import json
import sys
from pathlib import Path
from datetime import datetime, timezone

from fasthtml.common import *  # noqa: F403
from monsterui.all import *  # noqa: F403

# ─── disky palette (THE authoritative tokens) ──────────────────────────────
PALETTE = {
    "paper":   "#F5F1E8",
    "ink":     "#14110F",
    "rust":    "#B23A1F",
    "olive":   "#5C6831",
    "dim":     "#6B655E",
    "line":    "#D9D2C2",
    "card":    "#FBF8F0",
    "done":    "#3F6B2A",
    "partial": "#B8841C",
    "deferred":"#8C857B",
    "risk":    "#B23A1F",
}

TAILWIND_CFG = """
tailwind.config = { theme: { extend: {
  colors: """ + json.dumps(PALETTE) + """,
  fontFamily: {
    display: ['Fraunces','serif'],
    sans:    ['Manrope','sans-serif'],
    mono:    ['JetBrains Mono','monospace'],
  }
}}}
"""

CUSTOM_CSS = """
body { background:#F5F1E8; color:#14110F; font-family:'Manrope',sans-serif; }
h1,h2,h3 { font-family:'Fraunces',serif; font-variation-settings:'opsz' 144,'SOFT' 60; }
.num { font-family:'Fraunces',serif; font-variation-settings:'opsz' 144; font-weight:600; }
code,pre { font-family:'JetBrains Mono',monospace; }
.grain { background-image: radial-gradient(rgba(20,17,15,0.05) 1px, transparent 1px); background-size:3px 3px; }
.card { background:#FBF8F0; border:1px solid #D9D2C2; transition:transform .2s ease, box-shadow .2s ease; }
.card:hover { transform:translateY(-2px); box-shadow:0 4px 12px rgba(20,17,15,.08); }
.pill { display:inline-block; font-family:'JetBrains Mono',monospace; font-size:.7rem; padding:2px 8px; border-radius:999px; border:1px solid currentColor; }
.pill-done    { color:#3F6B2A; background:#EAF1E2; border-color:#3F6B2A; }
.pill-partial { color:#8B5E0A; background:#F6EBCF; border-color:#B8841C; }
.pill-risk    { color:#B23A1F; background:#F4D8CF; border-color:#B23A1F; }
.pill-dim     { color:#6B655E; background:#ECE7DA; border-color:#8C857B; }
table { border-collapse:collapse; width:100%; }
thead th { text-align:left; font-weight:600; text-transform:uppercase; letter-spacing:.06em; font-size:.72rem; color:#6B655E; border-bottom:2px solid #14110F; padding:8px 12px; }
tbody td { border-bottom:1px solid #D9D2C2; padding:8px 12px; font-size:.88rem; vertical-align:top; }
tbody tr { transition: background .15s ease; }
tbody tr:hover { background:#F1ECDC; }
.callout { border-left:4px solid currentColor; padding:12px 18px; margin:14px 0; background:#FBF8F0; }
.label { font-family:'JetBrains Mono',monospace; font-size:.7rem; text-transform:uppercase; letter-spacing:.08em; color:#6B655E; }
.codeblk { background:#14110F; color:#F5F1E8; padding:14px 18px; border-radius:6px; overflow-x:auto; font-size:.78rem; line-height:1.55; cursor:pointer; }
.codeblk:hover { background:#2A2520; }
.codeblk[data-copied="true"]::after { content:" ✓ copied"; color:#3F6B2A; }
.fade-in { animation: fadeIn .5s ease forwards; opacity:0; }
@keyframes fadeIn { to { opacity:1; } }
.counted { transition: color .3s ease; }
"""

COPY_JS = r"""
document.addEventListener('click', e => {
  const blk = e.target.closest('.codeblk');
  if (!blk) return;
  navigator.clipboard.writeText(blk.dataset.cmd || blk.innerText.trim());
  blk.dataset.copied = "true";
  setTimeout(() => blk.dataset.copied = "false", 1500);
});
// Count-up numbers
document.querySelectorAll('[data-count]').forEach(el => {
  const target = parseFloat(el.dataset.count);
  let cur = 0;
  const step = target / 30;
  const tick = () => {
    cur += step;
    if (cur >= target) { el.textContent = el.dataset.format
      ? el.dataset.format.replace('{}', target.toFixed(2))
      : target.toFixed(0); return; }
    el.textContent = el.dataset.format
      ? el.dataset.format.replace('{}', cur.toFixed(2))
      : Math.round(cur).toString();
    requestAnimationFrame(tick);
  };
  tick();
});
"""


# ─── data helpers ──────────────────────────────────────────────────────────

def load_envelope(p: Path, key: str = "records"):
    """Load a disky JSON envelope; return its `records` list (or `record`)."""
    if not p.exists():
        return None
    data = json.loads(p.read_text())
    return data.get(key, data.get("record", data))


def human_bytes(n: int | float) -> str:
    n = float(n)
    for unit in ("B", "KB", "MB", "GB", "TB", "PB"):
        if abs(n) < 1024:
            return f"{n:.2f} {unit}" if unit != "B" else f"{int(n)} B"
        n /= 1024
    return f"{n:.2f} EB"


def trunc(s: str, n: int = 64) -> str:
    return s if len(s) <= n else "…" + s[-(n - 1):]


# ─── sections ──────────────────────────────────────────────────────────────

def section_header(num: str, title: str):
    return Div(
        Span(num, cls="num text-3xl text-rust"),
        H2(title, cls="text-3xl font-display"),
        cls="flex items-baseline gap-4 mb-6 fade-in",
    )


def render_stats_block(stats: dict | None):
    if not stats:
        return Div(P("no stats data", cls="text-dim"))
    total_gb = stats.get("total_bytes", 0) / 1024**3
    files = stats.get("files", 0)
    dirs = stats.get("dirs", 0)
    dur = stats.get("scan_duration_s", 0) or 0
    return Div(
        Div(
            Div(Div("logical size", cls="label"),
                Div(f"{total_gb:.1f} GB", cls="num text-5xl text-ink mt-2",
                    data_count=str(total_gb), data_format="{} GB"),
                Div("(includes APFS sparse — see notes)", cls="text-sm text-dim mt-1"),
                cls="card p-4"),
            Div(Div("entries", cls="label"),
                Div(f"{files + dirs:,}", cls="num text-5xl text-olive mt-2"),
                Div(f"{files:,} files · {dirs:,} dirs", cls="text-sm text-dim mt-1"),
                cls="card p-4"),
            Div(Div("scan time", cls="label"),
                Div(f"{dur} s", cls="num text-5xl text-olive mt-2"),
                Div(f"~{(files + dirs) // max(dur,1):,} entries/sec", cls="text-sm text-dim mt-1"),
                cls="card p-4"),
            cls="grid md:grid-cols-3 gap-6 mb-6",
        ),
    )


def render_top_dirs(rows: list[dict] | None):
    if not rows:
        return None
    return Table(
        Thead(Tr(Th("size"), Th("path"))),
        Tbody(*[Tr(
            Td(human_bytes(r["total_size"])),
            Td(Code(trunc(r["path"], 80))),
        ) for r in rows[:15]]),
        cls="card",
    )


def render_top_files(rows: list[dict] | None):
    if not rows:
        return None
    return Table(
        Thead(Tr(Th("size"), Th("ext"), Th("path"))),
        Tbody(*[Tr(
            Td(human_bytes(r["size"])),
            Td(Code(r.get("ext") or "—")),
            Td(Code(trunc(r["path"], 80))),
        ) for r in rows[:15]]),
        cls="card",
    )


def render_churn(rows: list[dict] | None, window: str):
    if not rows:
        return P(f"no churn in {window}", cls="text-dim")
    return Table(
        Thead(Tr(Th("recent"), Th("files"), Th("score"), Th("path"))),
        Tbody(*[Tr(
            Td(human_bytes(r["recent_bytes"])),
            Td(f"{r['recent_files']:,}"),
            Td(Span(f"{r['churn_score']*100:.0f}%",
                    cls="pill " + (
                        "pill-risk" if r["churn_score"] > 0.8
                        else "pill-partial" if r["churn_score"] > 0.4
                        else "pill-dim"))),
            Td(Code(trunc(r["path"], 70))),
        ) for r in rows[:12]]),
        cls="card",
    )


def render_cleanup(data: dict | None):
    if not data:
        return P("no cleanup data", cls="text-dim")
    total_gb = data.get("total_bytes", 0) / 1024**3
    summary = data.get("summary", [])
    records = data.get("records", [])
    sum_table = Table(
        Thead(Tr(Th("category"), Th("dirs"), Th("size"), Th("files"))),
        Tbody(*[Tr(
            Td(Code(s["category"])),
            Td(str(s["paths"])),
            Td(human_bytes(s["bytes"])),
            Td(f"{s['files']:,}"),
        ) for s in summary]),
        cls="card mb-4",
    )
    # Action buttons — click to copy safe cleanup command.
    actions = []
    for cat_entry in summary[:6]:
        cat = cat_entry["category"]
        cmd = (f"disky cleanup --snapshot @latest --target {cat} "
               f"--apply --reversible")
        actions.append(Pre(
            f"# free {human_bytes(cat_entry['bytes'])} ({cat_entry['paths']} dirs of {cat})\n{cmd}",
            cls="codeblk fade-in",
            data_cmd=cmd,
            title="click to copy",
        ))
    top_paths = Table(
        Thead(Tr(Th("size"), Th("category"), Th("path"))),
        Tbody(*[Tr(
            Td(human_bytes(r["bytes"])),
            Td(Code(r["category"])),
            Td(Code(trunc(r["path"], 70))),
        ) for r in records[:10]]),
        cls="card",
    )
    headline = Div(
        Div(Div("reclaimable", cls="label"),
            Div(f"{total_gb:.2f} GB", cls="num text-5xl text-olive mt-2",
                data_count=str(total_gb), data_format="{} GB"),
            Div(f"{len(records)} dirs across {len(summary)} categories",
                cls="text-sm text-dim mt-1"),
            cls="card p-4 mb-4 inline-block"),
    )
    return Div(headline, sum_table, H3("safe-cleanup commands", cls="text-lg font-display mb-2"),
               *actions, H3("top reclaimable paths", cls="text-lg font-display mt-6 mb-2"),
               top_paths)


# ─── page assembly ─────────────────────────────────────────────────────────

def build_page(data_dir: Path) -> FT:
    stats = load_envelope(data_dir / "stats.json", "record") or {}
    top = load_envelope(data_dir / "top.json")
    dirs = load_envelope(data_dir / "dirs.json")
    ext = load_envelope(data_dir / "ext.json")
    churn24 = load_envelope(data_dir / "churn24.json")
    churn7d = load_envelope(data_dir / "churn7d.json")
    cleanup = (json.loads((data_dir / "cleanup.json").read_text())
               if (data_dir / "cleanup.json").exists() else None)

    when = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")

    head = (
        Meta(charset="utf-8"),
        Meta(name="viewport", content="width=device-width,initial-scale=1"),
        Title("disky — disk-space report"),
        Link(rel="preconnect", href="https://fonts.gstatic.com", crossorigin=True),
        Link(rel="stylesheet",
             href=("https://fonts.googleapis.com/css2?"
                   "family=Fraunces:opsz,wght@9..144,300..900&"
                   "family=Manrope:wght@300;400;500;600;700&"
                   "family=JetBrains+Mono:wght@400;500&display=swap")),
        Script(src="https://cdn.tailwindcss.com?plugins=typography"),
        Script(TAILWIND_CFG),
        Style(CUSTOM_CSS),
        Script(COPY_JS, defer=True),
    )

    body_inner = (
        # Header
        Header(
            Div(
                Div(
                    Span("disky · /disky report", cls="label"),
                    Span(when, cls="label"),
                    cls="flex items-center justify-between mb-2",
                ),
                H1("your disk · today", cls="text-5xl font-display leading-[0.95]"),
                P("Scanned ", Code(stats.get("scan_root", "~")),
                  " in ", Code(f"{stats.get('scan_duration_s', 0)} s"),
                  ". Snapshot persisted — every section below is a query against it, "
                  "not another walk of the filesystem.",
                  cls="max-w-2xl text-dim mt-3 leading-relaxed"),
                cls="max-w-6xl mx-auto px-6 py-8",
            ),
            cls="border-b-2 border-ink fade-in",
        ),

        # Main
        Main(
            # 00 — metrics block (mandatory)
            Section(section_header("00", "where you stand"), render_stats_block(stats)),

            # 01 — biggest spaces
            Section(
                section_header("01", "biggest spaces"),
                Div(
                    Div(H3("top directories", cls="text-xl font-display mb-2"),
                        render_top_dirs(dirs), cls="mb-6"),
                    Div(H3("top files", cls="text-xl font-display mb-2"),
                        render_top_files(top)),
                ),
            ),

            # 02 — what's churning (the user's goal)
            Section(
                section_header("02", "what's churning"),
                P("Mtime-based activity heatmap. High score = dir is a log generator or "
                  "active working directory.", cls="text-dim text-sm mb-4"),
                H3("last 24 hours", cls="text-lg font-display mb-2"),
                render_churn(churn24, "24h"),
                H3("last 7 days", cls="text-lg font-display mt-6 mb-2"),
                render_churn(churn7d, "7d"),
            ),

            # 03 — reclaimable
            Section(
                section_header("03", "free disk · safely"),
                render_cleanup(cleanup),
                Div(P("Every command above is safe-by-default — ",
                      Code("--reversible"), " moves to ", Code("~/.Trash"),
                      " instead of permanent delete. Click any command to copy. "
                      "Nothing runs without you pasting it.",
                      cls="text-sm text-dim"),
                    cls="callout mt-4", style="border-left-color:#5C6831;"),
            ),

            # 04 — agent flow
            Section(
                section_header("04", "agent flow"),
                Pre(
                    "# find log generators in the last 24h\n"
                    "disky churn --over 24h --format json | "
                    "disky filter --where \"churn_score > 0.5\"\n\n"
                    "# what grew biggest this week?\n"
                    "disky growth --over 7d --format json | "
                    "disky filter --where \"delta_bytes > 100MB\"\n\n"
                    "# safely free disk\n"
                    "disky cleanup --target target,node_modules --apply --reversible",
                    cls="codeblk",
                ),
            ),

            cls="max-w-6xl mx-auto px-6 py-12 flex flex-col gap-14",
        ),

        # Footer
        Footer(
            P("source: ", Code(stats.get("scan_root", "?")),
              " · scanned ", Code(stats.get("scanned_at", "?"))),
            cls="text-sm text-dim text-center py-8 border-t border-line",
        ),
    )

    return Html(
        Head(*head),
        Body(*body_inner, cls="grain"),
        lang="en",
    )


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2

    data_dir = Path(sys.argv[1])
    if not data_dir.is_dir():
        print(f"error: not a directory: {data_dir}", file=sys.stderr)
        return 2

    page = build_page(data_dir)
    # `to_xml` on an `Html` FT element already emits the doctype.
    print(to_xml(page))
    return 0


if __name__ == "__main__":
    sys.exit(main())
