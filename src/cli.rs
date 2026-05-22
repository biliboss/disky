use clap::{Parser, Subcommand, ValueEnum};

use disky::render::Format;

#[derive(Parser)]
#[command(name = "disky", about = "Fast macOS disk analyzer", version)]
pub struct Cli {
    /// Output format. Auto = JSON when stdout is piped, text on a TTY.
    #[arg(long, value_enum, global = true)]
    pub format: Option<FormatArg>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum FormatArg {
    Text,
    Json,
    Ndjson,
}

impl From<FormatArg> for Format {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Text => Format::Text,
            FormatArg::Json => Format::Json,
            FormatArg::Ndjson => Format::Ndjson,
        }
    }
}

/// Snapshot reference accepted by every query subcommand: `@latest`, a
/// snapshot ID (`2026-05-15_11-56`), or a filesystem path.
const SNAPSHOT_HELP: &str = "Snapshot to query: @latest, an ID, or a path";

#[derive(Subcommand)]
pub enum Command {
    /// Scan a directory and store results
    Scan {
        /// Path to scan
        #[arg(default_value = "/")]
        path: String,

        /// Output DuckDB file path (default: auto-named in data dir)
        #[arg(short, long)]
        db: Option<String>,

        /// Also emit the top N largest files in the result (cuts a round-trip
        /// for agents — avoids needing a separate `disky top` call).
        #[arg(long, value_name = "N")]
        emit_top: Option<usize>,

        /// Also emit the top N directories by aggregated size.
        #[arg(long, value_name = "N")]
        emit_dirs: Option<usize>,

        /// Also emit the top N extensions by total size.
        #[arg(long, value_name = "N")]
        emit_ext: Option<usize>,

        /// Also emit overall stats (root, totals, duration). Implied by any
        /// other `--emit-*` flag.
        #[arg(long, default_value_t = false)]
        emit_stats: bool,
    },

    /// Show largest files
    Top {
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
        /// Minimum size in bytes
        #[arg(short, long, default_value_t = 0)]
        min_size: u64,
    },

    /// Show disk usage by extension
    Ext {
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },

    /// Show top directories by size
    Dirs {
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },

    /// Find files matching pattern
    Find {
        /// Glob pattern (e.g. "*.log")
        pattern: String,
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },

    /// Show overall disk stats
    Stats {
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        /// Emit minimal scalar envelope `{kind:"scalar", records:[{bytes,files}]}`.
        /// Cuts tokens for agents that only need totals.
        #[arg(long)]
        summarize: bool,
        /// Print only the bare total-bytes integer to stdout (overrides --format).
        /// Implies --summarize.
        #[arg(long)]
        raw: bool,
    },

    /// Run an arbitrary SQL query against a snapshot
    Query {
        /// SQL — references the `files` table (`path, name, ext, size, mtime, is_dir, depth`)
        sql: String,
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        /// Cap on returned rows
        #[arg(short, long, default_value_t = 1000)]
        limit: usize,
    },

    /// Find well-known disk-hoggy directories (node_modules, target, …).
    /// Defaults to dry-run; pass `--apply` to delete.
    Cleanup {
        /// Comma-separated target categories (default: all known)
        #[arg(short, long, value_delimiter = ',')]
        target: Vec<String>,
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        #[arg(short, long, default_value_t = 100)]
        limit: usize,
        /// Actually delete the listed paths (default: dry-run)
        #[arg(long, default_value_t = false)]
        apply: bool,
        /// With `--apply`, move paths to ~/.Trash instead of permanently
        /// deleting them so they can be restored.
        #[arg(long, default_value_t = false)]
        reversible: bool,
    },

    /// Diff two snapshots — added / removed / grew / shrank files
    Diff {
        /// Snapshot A (the "before"). Accepts @latest, ID, or path.
        a: String,
        /// Snapshot B (the "after"). Accepts @latest, ID, or path.
        b: String,
        #[arg(short, long, default_value_t = 100)]
        limit: usize,
    },

    /// Emit a JSON descriptor of every command, record shape, and error type
    Schema,

    /// Open interactive TUI (default when no subcommand given)
    Tui {
        /// Snapshot to load (default: @latest)
        #[arg(short, long)]
        snapshot: Option<String>,
    },

    /// List available snapshots
    List,

    /// Per-directory growth between two snapshots. Default compares
    /// `@latest` against `@latest~1` so agents see "what grew since the
    /// previous scan". Rate is bytes/day computed from snapshot timestamps.
    Growth {
        /// Earlier snapshot. Accepts @latest, @latest~N, ID, or path.
        #[arg(long, default_value = "@latest~1")]
        since: String,
        /// Later snapshot (default @latest).
        #[arg(long, default_value = "@latest")]
        until: String,
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },

    /// List empty files in a snapshot (size = 0). Useful for finding
    /// placeholders, leftover lockfiles, and interrupted-write detritus.
    Empty {
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        #[arg(short, long, default_value_t = 100)]
        limit: usize,
    },

    /// List files older than the given duration (e.g. `365d`, `6mo`, `2y`).
    /// Excludes files whose mtime is unknown.
    Old {
        /// Cutoff age — `30d`, `2w`, `6mo`, `1y`.
        #[arg(long, value_name = "DURATION")]
        older_than: String,
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
        #[arg(short, long, default_value_t = 100)]
        limit: usize,
    },

    /// Apply restic-style retention policy to snapshots. Default is dry-run;
    /// pass `--apply` to delete. Refuses to run with no `--keep-*` flag.
    Forget {
        #[arg(long, value_name = "N")]
        keep_last: Option<usize>,
        #[arg(long, value_name = "N")]
        keep_daily: Option<usize>,
        #[arg(long, value_name = "N")]
        keep_weekly: Option<usize>,
        #[arg(long, value_name = "N")]
        keep_monthly: Option<usize>,
        #[arg(long, value_name = "N")]
        keep_yearly: Option<usize>,
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
}
