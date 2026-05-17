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
}
