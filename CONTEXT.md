# disky — Domain Context

## Purpose

macOS disk analyzer with persistent snapshots. Primary use: track disk usage over time, compare before/after cleanup, identify what's growing.

## Core Concepts

**Snapshot** — result of one full scan. Stored as DuckDB file at `~/.local/share/disky/YYYY-MM-DD_HH-MM.db`. Immutable after creation.

**Scan** — process of traversing the filesystem and writing a Snapshot. Default root: `/`. Blocking with progress spinner. ~3min for full macOS disk.

**Diff** — comparison between two Snapshots. Default: latest vs previous. Shows entries that grew, appeared, or disappeared.

**Cleanup opportunity** — file or directory identified as safe or worth reviewing for deletion, based on size, extension, and path pattern.

## UX Model

**Mode**: Hybrid — TUI for interactive exploration, HTML report for archiving and sharing.

**Entry point**: `disky` launches TUI. Shows latest Snapshot if exists, else triggers Scan first.

**TUI layout**:
- Default view: directory tree (ncdu-style), size accumulated per dir
- Tab: flat file list, sortable by size/date/ext
- `d` key: diff view (latest vs previous Snapshot)

**TUI actions**:
| Key | Action |
|-----|--------|
| ↑↓ | navigate |
| Enter | expand/collapse dir |
| Backspace | go up one level |
| `d` | toggle diff view |
| `o` | open in Finder |
| `c` | copy path to clipboard |
| `e` | export HTML cleanup report |
| `r` | rescan (new Snapshot) |
| `q` | quit |

**Delete**: not in TUI — too risky without Trash support. User deletes externally, rescans to confirm.

## Storage

Snapshots: `~/.local/share/disky/`
Format: `YYYY-MM-DD_HH-MM.db` (DuckDB)
`disky list` — shows available snapshots with file count and total size.

## Stack

- Traversal: `jwalk` + `Parallelism::RayonNewPool` + `memchr` for ext extraction
- Storage: DuckDB embedded (`duckdb` crate, Appender API, batch 50k)
- TUI: `ratatui` + `crossterm`
- CLI: `clap` subcommands

## Out of scope

- Windows / Linux (macOS-first)
- Real-time watching (inotify/FSEvents)
- Network drives
- Trash integration (delete is external)
