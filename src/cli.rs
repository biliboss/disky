use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "disky", about = "Fast macOS disk analyzer", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
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
