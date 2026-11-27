use clap::{Parser, Subcommand, ValueEnum};

use disky::render::Format;

#[derive(Parser)]
#[command(name = "disky", about = "Fast macOS disk analyzer", version)]
pub struct Cli {
    /// Output format. Auto = JSON when stdout is piped, text on a TTY.
    #[arg(long, value_enum, global = true)]
    pub format: Option<FormatArg>,

    /// Use physical disk usage (st_blocks * 512) instead of logical size
    /// (st_size). Critical for APFS sparse files — OrbStack disk images
    /// report 8.8 TB logical on a 256 GB SSD; physical shows ~13 GB.
    /// Applies to top, dirs, ext, stats. Falls back to logical when
    /// physical isn't captured (older snapshots, non-Unix scans).
    #[arg(long, global = true)]
    pub physical: bool,

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
        /// Path to scan (default: `/`, or the system drive on Windows)
        #[arg(default_value_t = disky::platform::default_root())]
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
        /// Comma-separated categories (default: all for a dry-run; developer
        /// artefacts only with --apply). Developer: node_modules, target,
        /// pycache, next, dist, build, venv, gradle, pytest. System:
        /// system_caches, app_caches, temp, logs, trash, app_leftovers,
        /// old_downloads. `disky schema` lists each with its safety.
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

    /// Filter records from a prior disky JSON envelope (stdin) by a
    /// simple predicate. Composes with any command that emits records.
    ///
    /// Example: `disky top --format json | disky filter --where "size > 1GB"`
    ///
    /// Supported fields: size (u64), ext (str), name (str), path (str).
    /// Supported ops: =, !=, >, <, >=, <=, LIKE.
    /// Literals: integers (with optional KB/MB/GB/TB suffix) or quoted
    /// strings. Chain with AND. Mutually exclusive with `--snapshot`.
    Filter {
        /// Predicate string. Example: `size > 1GB AND ext = 'log'`.
        #[arg(long = "where")]
        where_: Option<String>,
        /// Cap records returned (after filtering).
        #[arg(short, long, default_value_t = 1000)]
        limit: usize,
    },

    /// Per-directory growth between two snapshots. Default compares
    /// `@latest` against `@latest~1` so agents see "what grew since the
    /// previous scan". Rate is bytes/day computed from snapshot timestamps.
    /// Pass `--over <DURATION>` to auto-pick the oldest snapshot within
    /// the window (e.g. `--over 7d`).
    Growth {
        /// Earlier snapshot. Accepts @latest, @latest~N, ID, or path.
        #[arg(long, default_value = "@latest~1")]
        since: String,
        /// Later snapshot (default @latest).
        #[arg(long, default_value = "@latest")]
        until: String,
        /// Auto-pick oldest snapshot whose age >= this duration. Overrides
        /// --since. Format: `7d`, `2w`, `6mo`, `1y` (see duration.rs).
        #[arg(long, value_name = "DURATION")]
        over: Option<String>,
        /// N-snapshot OLS fit. When set (>= 3), runs ordinary-least-squares
        /// against the N most-recent snapshots instead of a 2-snapshot diff.
        /// Emits `kind="growth_n"` envelope with slope/R²/projected fill date.
        #[arg(long, value_name = "N")]
        over_n: Option<usize>,
        /// Free-byte budget the projection extrapolates against. Only used
        /// with `--over-n`. Default: free bytes on the volume that holds
        /// `$HOME` (best-effort; may be 0 if `statvfs` fails).
        #[arg(long, value_name = "BYTES")]
        fill_target: Option<u64>,
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },

    /// Predict when the disk fills based on linear regression over all
    /// snapshots in the data dir. Pass `--free-bytes <N>` so the fit
    /// can compute a fill-by date.
    Predict {
        /// Bytes currently free on the volume. Get via
        /// `df -k / | tail -1 | awk '{print $4*1024}'`.
        #[arg(long, value_name = "BYTES")]
        free_bytes: Option<u64>,
    },

    /// Per-directory churn — files modified within the last N hours/days.
    /// Identifies log generators and hot working directories.
    Churn {
        /// Time window (e.g. `24h`, `7d`, `30d`).
        #[arg(long, default_value = "24h")]
        over: String,
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
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

    /// A folder's immediate children by physical size, largest first — the
    /// desktop GUI's Space map. Default folder: the snapshot's scan root.
    /// Excluded folders (`disky prefs`) are left out. A folder that is not in
    /// the snapshot exits 4.
    Map {
        /// Absolute folder path (`~/` is expanded). Default: the scan root.
        path: Option<String>,
        /// Keep at most N blocks, as the Space map does: the largest
        /// children, with the rest (and any under 0.5% of the folder)
        /// summed into one "Other" (`other` in JSON).
        #[arg(long, value_name = "N")]
        tiles: Option<usize>,
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
    },

    /// Regular files of at least 2 GB physical, largest first, with their
    /// last-opened time — the desktop GUI's Large files screen. Excluded
    /// folders and skip-recently-used (`disky prefs`) apply.
    Large {
        /// all (>= 2 GB) | over-10gb (>= 10 GB) | unused-1y (last opened
        /// more than 365 days ago).
        #[arg(long, value_enum, default_value = "all")]
        filter: disky::large::LargeFilter,
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
    },

    /// Groups of regular files with byte-identical contents (size first,
    /// then a full content hash; never by name), largest reclaimable first —
    /// the desktop GUI's Duplicates screen. Each file is re-checked on disk;
    /// hard links count once. Excluded folders and skip-recently-used
    /// (`disky prefs`) apply.
    Dupes {
        /// Smallest file compared: bytes, or e.g. 1MB (decimal) / 1MiB.
        #[arg(long, value_name = "BYTES", default_value = "1MB", value_parser = parse_min_size)]
        min_size: u64,
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
    },

    /// Move the named files or folders to the Trash and record one history
    /// entry (restorable with `disky restore <id>`). Paths named here count
    /// as reviewed. A path that is missing or cannot be moved is reported
    /// and the rest still go.
    Trash {
        /// Paths to move (`~/` is expanded; relative paths are resolved
        /// against the current directory).
        #[arg(required = true)]
        paths: Vec<String>,
        /// History entry title, e.g. "Large files".
        #[arg(long, default_value = "Cleanup")]
        title: String,
    },

    /// List the clean-history log (every applied `cleanup`), newest first,
    /// with each entry's detail and state and the total cleared.
    History {
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
    },

    /// Move every trashed item of a history entry back to its original
    /// path. Never overwrites: occupied paths are reported as conflicts and
    /// stay in the Trash. Re-running retries items not yet restored.
    Restore {
        /// Entry id from `disky history`.
        id: String,
    },

    /// Which agent CLIs are installed: Claude Code (claude), Codex CLI
    /// (codex), Gemini CLI (gemini), in that order, each with its path when
    /// found. Looks in PATH, ~/.local/bin, /opt/homebrew/bin and
    /// /usr/local/bin; only checks for an executable file, never runs it.
    Agents,

    /// The Assistant's suggestions from the latest snapshot, the disk and
    /// `disky prefs`: goal (below the goal) or ok, then dev, dl and left
    /// when those categories have hits. Reported whatever checks.enabled
    /// says. Never cleans anything.
    Suggest {
        #[arg(short, long, default_value = "@latest", help = SNAPSHOT_HELP)]
        snapshot: String,
    },

    /// Print every preference, or change some with `--set <key>=<value>`.
    /// The store (`<data dir>/prefs.json`) is shared with the desktop GUI.
    /// Keys: trash_by_default, skip_recently_used, safe_only,
    /// warn_below_goal (true|false), goal_bytes (bytes or e.g. 80GB),
    /// checks.enabled (true|false), checks.frequency (hourly|daily|weekly),
    /// agent (claude|codex|gemini), scheduled_scan (off|weekly|monthly),
    /// excluded (comma list or JSON array of folders; empty clears).
    /// An unknown key or invalid value exits 2 and changes nothing.
    Prefs {
        /// `<key>=<value>`; repeatable. All are applied, or none.
        #[arg(long, value_name = "KEY=VALUE")]
        set: Vec<String>,
    },
}

/// `--min-size` for `disky dupes`: a positive byte count with an optional
/// decimal (`MB` = 10⁶) or binary (`MiB`) suffix, as `disky prefs` parses
/// sizes. Anything else is a usage error (exit 2).
fn parse_min_size(v: &str) -> Result<u64, String> {
    disky::prefs::parse_bytes(v).map_err(|_| {
        format!("--min-size must be a positive byte count (e.g. 1000000 or 1MB), got '{v}'")
    })
}
