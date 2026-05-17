use clap::{Parser, Subcommand, ValueEnum};

use crate::render::Format;

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

#[derive(Subcommand)]
pub enum Command {
    /// Scan a directory and store results
    Scan {
        /// Path to scan
        #[arg(default_value = "/")]
        path: String,

        /// DuckDB file path
        #[arg(short, long, default_value = "disky.db")]
        db: String,
    },

    /// Show largest files
    Top {
        #[arg(short, long, default_value = "disky.db")]
        db: String,
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
        /// Minimum size in bytes
        #[arg(short, long, default_value_t = 0)]
        min_size: u64,
    },

    /// Show disk usage by extension
    Ext {
        #[arg(short, long, default_value = "disky.db")]
        db: String,
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },

    /// Show top directories by size
    Dirs {
        #[arg(short, long, default_value = "disky.db")]
        db: String,
        #[arg(short, long, default_value_t = 30)]
        limit: usize,
    },

    /// Find files matching pattern
    Find {
        /// Glob pattern (e.g. "*.log")
        pattern: String,
        #[arg(short, long, default_value = "disky.db")]
        db: String,
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },

    /// Show overall disk stats
    Stats {
        #[arg(short, long, default_value = "disky.db")]
        db: String,
    },

    /// Open interactive TUI (default when no subcommand given)
    Tui {
        /// Snapshot DB path (default: latest in ~/.local/share/disky/)
        #[arg(short, long)]
        db: Option<String>,
    },

    /// List available snapshots
    List,
}
