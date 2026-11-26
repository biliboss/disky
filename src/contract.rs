//! The desktop GUI's data contract (REQ-DCLEAN-027), built here in the
//! engine so the GUI never derives it itself (REQ-DCLEAN-001).
//!
//! * [`CleanupView`] — what the Smart Clean screen renders: disk usage, the
//!   goal, snapshot age and the reclaimable categories with their items.
//! * [`ScanProgress`] — one progress event while a scan runs.
//! * [`CatalogueEntry`] — every category the engine checks, hit or not, so
//!   the scanning screen lists all of them even on a first scan
//!   (REQ-DCLEAN-004).
//! * [`CleanResult`] — what a clean did, with its history entry and the
//!   volume's disk usage read after the clean, so the meter, ring and goal
//!   tag update without a manual refresh (REQ-DCLEAN-002.A4).
//! * [`CleanProgress`] — one event per cleaned item, naming the category in
//!   progress (REQ-DCLEAN-008).
//! * [`RestoreResult`] — what an Undo restored (REQ-DCLEAN-009.A4, A7).
//! * [`HistoryView`] — the History screen's rows, each entry with its detail
//!   and state, and the total cleared (REQ-DSCR-015). `disky history` prints
//!   the same rows.
//! * [`MapView`] — a folder's immediate children for the Space map
//!   (REQ-DSCR-003), the same as `disky map`.
//! * [`LargeView`] — the Large files screen's rows (REQ-DSCR-004), the same
//!   as `disky large`.
//! * [`TrashResult`] — what moving reviewed files to the Trash did
//!   (REQ-DSCR-006), the same as `disky trash`.
//! * [`EngineError`] — a failed command, typed by disky's exit-code slug so
//!   the GUI never matches on message text (REQ-DCLEAN-018).
//!
//! Every byte value is a `u64` of **physical** bytes (REQ-DCLEAN-027.A1).
//! Category ids, names, descriptions and safety come from
//! [`cleanup::CATEGORIES`] (REQ-DCLEAN-027.A4). A category's `bytes` is the
//! sum of its items' `bytes` by construction (REQ-DCLEAN-027.A3).
//!
//! @implements REQ-DCLEAN-002
//! @implements REQ-DCLEAN-003
//! @implements REQ-DCLEAN-004
//! @implements REQ-DCLEAN-008
//! @implements REQ-DCLEAN-009
//! @implements REQ-DCLEAN-018
//! @implements REQ-DCLEAN-027
//! @implements REQ-DCLEAN-029
//! @implements REQ-DSCR-001
//! @implements REQ-DSCR-002
//! @implements REQ-DSCR-003
//! @implements REQ-DSCR-004
//! @implements REQ-DSCR-006
//! @implements REQ-DSCR-015

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use duckdb::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::cleanup::{self, ApplyMode, ApplyOutcome, CleanupHit, Failure, Safety, CATEGORIES};
use crate::history;
use crate::scan::ScanTick;

pub use crate::disk::{disk_usage, Disk};

/// The space goal used when the store has none: the design snapshot's
/// 80 GB (REQ-DSCR-001 defaults).
pub use crate::prefs::DEFAULT_GOAL_BYTES;

/// A snapshot older than this is stale (REQ-DCLEAN-029.A2): 24 hours.
pub const STALE_AFTER_SECS: i64 = 24 * 60 * 60;

/// A file at least this large (physical bytes) counts toward the idle
/// screen's "Large files" shortcut (REQ-DCLEAN-003.A4): 2 GB, decimal.
pub const LARGE_FILE_BYTES: u64 = 2_000_000_000;

/// Most hits a view lists. Generous: the GUI scrolls.
pub const VIEW_LIMIT: usize = 10_000;

/// Reason given for a path the user did not review in the current view.
pub const FAILURE_NOT_REVIEWED: &str = "not reviewed";

/// Event channel the GUI backend emits [`ScanProgress`] on.
pub const SCAN_PROGRESS_EVENT: &str = "scan://progress";

/// Event channel the GUI backend emits [`CleanProgress`] on.
pub const CLEAN_PROGRESS_EVENT: &str = "clean://progress";

/// System Settings → Privacy & Security → Full Disk Access
/// (REQ-DCLEAN-018.A2).
pub const FULL_DISK_ACCESS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

/// The preferences the GUI renders and edits: the shared store's record
/// (REQ-DSCR-001.A5). `Preferences` is kept as the contract's name for it.
pub use crate::prefs::Prefs as Preferences;

/// One reclaimable path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
    /// Basename of `path`.
    pub name: String,
    /// Absolute path.
    pub path: String,
    /// Physical bytes.
    pub bytes: u64,
}

/// One reclaimable category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryView {
    /// Engine slug (`cleanup::Category::name`).
    pub id: String,
    /// Display label (`cleanup::Category::label`).
    pub name: String,
    pub description: String,
    /// Lucide icon name (`cleanup::Category::icon`).
    pub icon: String,
    /// `review` if any item is `review`, as in the CLI summary.
    pub safety: Safety,
    /// Sum of `items[].bytes`.
    pub bytes: u64,
    pub items: Vec<ItemView>,
}

/// One category the engine checks, whether or not it has hits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogueEntry {
    /// Engine slug (`cleanup::Category::name`).
    pub id: String,
    /// Display label (`cleanup::Category::label`).
    pub name: String,
    /// Lucide icon name (`cleanup::Category::icon`).
    pub icon: String,
    pub safety: Safety,
}

/// Every category in [`CATEGORIES`], in table order.
pub fn catalogue() -> Vec<CatalogueEntry> {
    CATEGORIES
        .iter()
        .map(|c| CatalogueEntry {
            id: c.name.to_string(),
            name: c.label.to_string(),
            icon: c.icon.to_string(),
            safety: c.safety,
        })
        .collect()
}

/// Everything the Smart Clean screen renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanupView {
    pub disk: Disk,
    /// The store's space goal ([`Preferences::goal_bytes`]).
    pub goal_bytes: u64,
    /// When the snapshot's scan finished (RFC 3339 UTC), or null.
    pub last_scan_at: Option<String>,
    /// When the snapshot's scan started — the moment it reflects.
    pub snapshot_taken_at: Option<String>,
    /// True when the snapshot is older than [`STALE_AFTER_SECS`], or its age
    /// is unknown (fail safe: warn).
    pub is_stale: bool,
    /// Every preference, read from the store (REQ-DSCR-001.A5).
    pub preferences: Preferences,
    /// Files in the snapshot of at least [`LARGE_FILE_BYTES`] physical bytes.
    pub large_files_count: u64,
    pub categories: Vec<CategoryView>,
    /// Every category the engine checks ([`catalogue`]).
    pub catalogue: Vec<CatalogueEntry>,
    /// Paths the snapshot's scan could not read for permission reasons
    /// (REQ-DCLEAN-018.A2).
    pub skipped_paths: u64,
}

/// One scan progress event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanProgress {
    /// 0..=1.
    pub fraction: f64,
    pub current_path: String,
    pub completed_category_ids: Vec<String>,
    /// Physical bytes of each completed category that has hits. A completed
    /// id missing here found nothing. Sizes travel with completion, so the
    /// GUI never shows one speculatively (REQ-DCLEAN-004.A4).
    pub completed_category_bytes: BTreeMap<String, u64>,
}

/// One clean progress event, sent after each item (REQ-DCLEAN-008).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanProgress {
    /// The category being processed now: the next item's, or the last
    /// item's once all are done. Items run in selection order (A1).
    pub category_id: String,
    pub done_items: u64,
    pub total_items: u64,
    /// `done_items / total_items`, 0..=1.
    pub fraction: f64,
}

/// What a clean did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanResult {
    pub mode: history::Mode,
    /// Physical bytes of the items that were handled — what was actually
    /// reclaimed, not the estimate (REQ-DCLEAN-009.A1, REQ-DCLEAN-018.A3).
    pub freed_bytes: u64,
    /// Paths that were trashed or deleted.
    pub succeeded: Vec<String>,
    /// Paths left untouched, with the reason (REQ-DCLEAN-018.A3).
    pub failed: Vec<Failure>,
    /// Id of the history entry Undo restores; `None` when nothing was
    /// handled. CLI: `disky restore <id>`.
    pub history_id: Option<String>,
    /// `None` when nothing was handled (no entry is written).
    pub history_entry: Option<history::Entry>,
    /// The volume's usage read after the clean finished (REQ-DCLEAN-002.A4).
    pub disk: Disk,
}

fn basename(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    trimmed
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

/// Group hits into categories, in [`CATEGORIES`] order. Categories with no
/// hits are left out. Item bytes are physical, and each category's `bytes`
/// is the sum of its items.
pub fn categories_from_hits(hits: &[CleanupHit]) -> Vec<CategoryView> {
    let mut by_cat: HashMap<&str, Vec<&CleanupHit>> = HashMap::new();
    for h in hits {
        by_cat.entry(h.category.as_str()).or_default().push(h);
    }
    CATEGORIES
        .iter()
        .filter_map(|c| {
            let hits = by_cat.get(c.name)?;
            let items: Vec<ItemView> = hits
                .iter()
                .map(|h| ItemView {
                    name: basename(&h.path),
                    path: h.path.clone(),
                    bytes: h.physical_bytes,
                })
                .collect();
            let safety = if hits.iter().any(|h| h.safety == Safety::Review) {
                Safety::Review
            } else {
                Safety::Safe
            };
            Some(CategoryView {
                id: c.name.to_string(),
                name: c.label.to_string(),
                description: c.description.to_string(),
                icon: c.icon.to_string(),
                safety,
                bytes: items.iter().map(|i| i.bytes).sum(),
                items,
            })
        })
        .collect()
}

/// Every category's hits for a snapshot — the same query as
/// `disky cleanup --limit VIEW_LIMIT` (REQ-DCLEAN-001.A1).
pub fn view_hits(conn: &Connection) -> Result<Vec<CleanupHit>> {
    view_hits_with(conn, &cleanup::ScanContext::from_snapshot(conn))
}

/// [`view_hits`] with explicit runtime inputs (excluded folders, ignore
/// patterns, home). Hits outside the scope never reach the view
/// (REQ-DCLEAN-029.A3).
pub fn view_hits_with(conn: &Connection, ctx: &cleanup::ScanContext) -> Result<Vec<CleanupHit>> {
    let targets: Vec<String> = cleanup::category_names()
        .into_iter()
        .map(String::from)
        .collect();
    cleanup::scan_with(conn, &targets, VIEW_LIMIT, ctx)
}

fn rfc3339(secs: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(secs, 0).map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// True when a snapshot taken at `taken_at_secs` is older than 24 h at
/// `now_secs`. Unknown age is stale.
pub fn is_stale(taken_at_secs: Option<i64>, now_secs: i64) -> bool {
    match taken_at_secs {
        Some(t) => now_secs - t > STALE_AFTER_SECS,
        None => true,
    }
}

/// Assemble the view from a snapshot's metadata and hits.
pub fn cleanup_view(
    meta: Option<&crate::db::ScanMeta>,
    hits: &[CleanupHit],
    disk: Disk,
    preferences: &Preferences,
    large_files_count: u64,
    now_secs: i64,
) -> CleanupView {
    let taken = meta.map(|m| m.started_at);
    CleanupView {
        disk,
        goal_bytes: preferences.goal_bytes,
        last_scan_at: meta
            .map(|m| m.ended_at.unwrap_or(m.started_at))
            .and_then(rfc3339),
        snapshot_taken_at: taken.and_then(rfc3339),
        is_stale: is_stale(taken, now_secs),
        preferences: preferences.clone(),
        large_files_count,
        categories: categories_from_hits(hits),
        catalogue: catalogue(),
        skipped_paths: 0,
    }
}

/// Files in a snapshot of at least [`LARGE_FILE_BYTES`] physical bytes — one
/// query. CLI: `disky query "SELECT count(*) FROM files WHERE NOT is_dir AND
/// physical_size >= 2000000000"`.
pub fn large_files_count(conn: &Connection) -> Result<u64> {
    large_files_count_in(conn, &cleanup::ScanContext::default())
}

/// [`large_files_count`] leaving out files inside `ctx.excluded` and, with
/// skip-recently-used on, files modified since `ctx.recent_cutoff`
/// (REQ-DSCR-002.A1, A2) — the same scope the Large files screen lists.
pub fn large_files_count_in(conn: &Connection, ctx: &cleanup::ScanContext) -> Result<u64> {
    use duckdb::types::Value;
    let mut sql =
        String::from("SELECT count(*) FROM files WHERE NOT is_dir AND physical_size >= ?");
    let mut params: Vec<Value> = vec![Value::BigInt(LARGE_FILE_BYTES as i64)];
    crate::large::scope_clause(ctx, &mut sql, &mut params);
    let refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|v| v as &dyn duckdb::ToSql).collect();
    let n: i64 = conn.query_row(&sql, refs.as_slice(), |r| r.get(0))?;
    Ok(n.max(0) as u64)
}

/// Disk usage of the volume holding `path`: its nearest existing ancestor
/// (a cleaned path itself is gone), else `/`.
pub fn disk_near(path: &Path) -> Result<Disk> {
    path.ancestors()
        .filter(|a| !a.as_os_str().is_empty() && a.exists())
        .find_map(|a| disk_usage(a).ok())
        .map_or_else(
            || disk_usage(Path::new(&crate::platform::default_root())),
            Ok,
        )
}

/// The view when no snapshot exists yet: live disk usage, no categories, and
/// `last_scan_at` null so the idle kicker reads "No scans yet"
/// (REQ-DCLEAN-003.A2). Goal and preferences come from `prefs`.
pub fn empty_view(prefs: &Preferences) -> Result<CleanupView> {
    let disk = disk_usage(Path::new(&crate::platform::default_root()))?;
    Ok(cleanup_view(
        None,
        &[],
        disk,
        prefs,
        0,
        Utc::now().timestamp(),
    ))
}

/// Open a snapshot and build its view, with disk usage taken from the
/// volume of the snapshot's scan root (`/` when the root is gone). Goal and
/// preferences come from the store's `prefs` (REQ-DSCR-001.A5); excluded
/// folders and skip-recently-used scope the hits (REQ-DSCR-002). Returns
/// the hits behind the view too: they are what the user reviews, and the
/// only paths [`clean`] will act on.
pub fn load(snapshot_path: &str, prefs: &Preferences) -> Result<(CleanupView, Vec<CleanupHit>)> {
    let conn = crate::db::open_read(snapshot_path)?;
    let meta = crate::db::read_scan_meta(&conn);
    let root = meta.as_ref().map_or("/", |m| m.root.as_str());
    let disk = disk_near(Path::new(root))?;
    let ctx = cleanup::ScanContext::from_snapshot(&conn).with_prefs(prefs);
    let hits = view_hits_with(&conn, &ctx)?;
    // Exactly what the Large files screen lists (files gone since the scan
    // left out), so the shortcut's count matches the screen's.
    let large = on_disk_only(crate::large::large(
        &conn,
        LargeFilter::All,
        &ctx,
        ctx.now_secs,
    )?)
    .count as u64;
    let now = Utc::now().timestamp();
    let mut view = cleanup_view(meta.as_ref(), &hits, disk, prefs, large, now);
    view.skipped_paths = crate::db::read_scan_skipped(&conn);
    Ok((view, hits))
}

pub use crate::map::{MapChild, MapView};

/// The Space map of `path` (default: the scan root) in the snapshot at
/// `snapshot_path` (REQ-DSCR-003): immediate children by physical bytes,
/// excluded folders left out (REQ-DSCR-002.A1), and the capacity of the
/// scan root's volume for share of disk. CLI: `disky map [<path>]`.
pub fn map_view(snapshot_path: &str, path: Option<&str>, prefs: &Preferences) -> Result<MapView> {
    let conn = crate::db::open_read(snapshot_path)?;
    let ctx = cleanup::ScanContext::from_snapshot(&conn).with_prefs(prefs);
    let root = ctx.scan_root.clone().unwrap_or_else(|| "/".to_string());
    let capacity = disk_near(Path::new(&root))?.capacity_bytes;
    crate::map::map(&conn, path, &ctx.excluded, capacity)
}

/// A path the last scan listed must still be on disk before Finder is asked
/// to reveal it (REQ-DSCR-012.A4). A path removed since the scan is
/// not-found, worded for the screen.
pub fn check_on_disk(path: &str) -> Result<()> {
    if std::fs::symlink_metadata(path).is_ok() {
        return Ok(());
    }
    let name = path.rsplit('/').find(|p| !p.is_empty()).unwrap_or(path);
    Err(crate::exit::DiskyError::new(
        crate::exit::ExitCode::NotFound,
        "no longer on disk",
        format!("{name} was removed after the last scan. Scan again to refresh the map."),
    )
    .into())
}

pub use crate::large::{LargeFile, LargeFilter, LargeView};

/// The Large files screen's list for `filter` in the snapshot at
/// `snapshot_path` (REQ-DSCR-004): files of at least 2 GB physical, largest
/// first. Excluded folders are left out and, with skip-recently-used on, so
/// are files modified in the last 30 days (REQ-DSCR-002.A1, A2) — the same
/// scope as the Smart Clean shortcut's [`large_files_count_in`], so the two
/// counts agree. CLI: `disky large [--filter …]`.
pub fn large_view(
    snapshot_path: &str,
    filter: LargeFilter,
    prefs: &Preferences,
) -> Result<LargeView> {
    let conn = crate::db::open_read(snapshot_path)?;
    let ctx = cleanup::ScanContext::from_snapshot(&conn).with_prefs(prefs);
    Ok(on_disk_only(crate::large::large(
        &conn,
        filter,
        &ctx,
        ctx.now_secs,
    )?))
}

/// Leave out files deleted since the scan: trashing one could only fail.
/// Files this size are few, so one lstat each is cheap. The Smart Clean
/// shortcut counts the same list, so the two never disagree.
pub fn on_disk_only(mut view: LargeView) -> LargeView {
    view.records
        .retain(|f| crate::large::still_on_disk(&f.path));
    view.count = view.records.len();
    view.total_bytes = view.records.iter().map(|f| f.bytes).sum();
    view
}

pub use crate::dupes::{DupeCopy, DupeGroup, DupesView};

/// The Duplicates screen's groups in the snapshot at `snapshot_path`
/// (REQ-DSCR-005): byte-identical regular files of at least `min_size`
/// logical bytes, largest reclaimable first. Excluded folders are left out
/// and, with skip-recently-used on, so are files modified in the last 30
/// days (REQ-DSCR-002.A1, A2). CLI: `disky dupes [--min-size …]`.
pub fn dupes_view(snapshot_path: &str, min_size: u64, prefs: &Preferences) -> Result<DupesView> {
    let conn = crate::db::open_read(snapshot_path)?;
    let ctx = cleanup::ScanContext::from_snapshot(&conn).with_prefs(prefs);
    crate::dupes::dupes(&conn, min_size, &ctx)
}

pub use crate::agents::Agent;
pub use crate::assistant::{Suggestion, SuggestionKey};

/// Everything the Assistant screen renders (REQ-DSCR-009, REQ-DSCR-010):
/// the selected agent, the proactive-check settings, the goal and disk the
/// side panel shows, the three agents (REQ-DSCR-007) and the feed
/// (REQ-DSCR-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantView {
    /// Store id of the selected agent (`prefs.agent`).
    pub agent: String,
    /// Its display name ("Claude Code"), from [`crate::agents::AGENTS`].
    pub agent_name: String,
    pub checks: crate::prefs::Checks,
    pub goal_bytes: u64,
    pub disk: Disk,
    /// Every agent the Assistant knows, in display order, found or not.
    pub agents: Vec<Agent>,
    /// The feed. Empty when proactive checks are off (REQ-DSCR-009.A4).
    pub suggestions: Vec<Suggestion>,
}

/// The suggestions for a loaded view and its hits at `now_secs`
/// (REQ-DSCR-008). Hits whose path is gone since the scan (cleaned, moved)
/// are left out, and the categories are rebuilt from the rest, so a
/// suggestion never offers bytes that are no longer there (A2).
pub fn suggestions_for(
    view: &CleanupView,
    hits: &[CleanupHit],
    home: Option<&Path>,
    now_secs: i64,
) -> Vec<Suggestion> {
    let present: Vec<CleanupHit> = hits
        .iter()
        .filter(|h| Path::new(&h.path).symlink_metadata().is_ok())
        .cloned()
        .collect();
    let mut live = view.clone();
    live.categories = categories_from_hits(&present);
    crate::assistant::suggestions(&live, &present, home, now_secs)
}

/// [`AssistantView`] from a loaded view, its hits and the detected agents.
/// With checks off the feed is empty (REQ-DSCR-009.A4); `disky suggest`
/// reports regardless.
pub fn assistant_view_of(
    view: &CleanupView,
    hits: &[CleanupHit],
    agents: Vec<Agent>,
    home: Option<&Path>,
    now_secs: i64,
) -> AssistantView {
    let p = &view.preferences;
    let suggestions = if p.checks.enabled {
        suggestions_for(view, hits, home, now_secs)
    } else {
        Vec::new()
    };
    AssistantView {
        agent: p.agent.clone(),
        agent_name: crate::agents::display_name(&p.agent),
        checks: p.checks,
        goal_bytes: view.goal_bytes,
        disk: view.disk.clone(),
        agents,
        suggestions,
    }
}

/// What a cleanup view depends on: the snapshot (path and modification
/// time) and the saved preferences. Two loads with the same key build the
/// same view, so a client may reuse the first (the GUI's Assistant and
/// Smart Clean review share one load instead of computing it twice).
pub fn view_key(snapshot_path: &str, prefs: &Preferences) -> Result<String> {
    let modified = std::fs::metadata(snapshot_path)?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(format!(
        "{snapshot_path}|{modified}|{}",
        serde_json::to_string(prefs)?
    ))
}

/// The view and hits behind the Assistant: the snapshot at `snapshot_path`
/// (scoped by `prefs`, as Smart Clean's), or the empty view when there is
/// none.
pub fn assistant_inputs(
    snapshot_path: Option<&str>,
    prefs: &Preferences,
) -> Result<(CleanupView, Vec<CleanupHit>)> {
    match snapshot_path {
        Some(p) => load(p, prefs),
        None => Ok((empty_view(prefs)?, Vec::new())),
    }
}

/// The Assistant screen's data for the snapshot at `snapshot_path` (none:
/// goal/ok only), read with the store's `prefs` and this machine's agents.
/// The GUI's `load_assistant` returns exactly this.
pub fn assistant_view(snapshot_path: Option<&str>, prefs: &Preferences) -> Result<AssistantView> {
    let (view, hits) = assistant_inputs(snapshot_path, prefs)?;
    Ok(assistant_view_from(&view, &hits))
}

/// [`assistant_view`] from a view already built (this machine's agents,
/// home and clock), so a client holding the view need not rebuild it.
pub fn assistant_view_from(view: &CleanupView, hits: &[CleanupHit]) -> AssistantView {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    assistant_view_of(
        view,
        hits,
        crate::agents::detect(),
        home.as_deref(),
        Utc::now().timestamp(),
    )
}

/// What moving reviewed files to the Trash did (REQ-DSCR-006.A1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashResult {
    /// Paths moved to the Trash.
    pub handled: Vec<String>,
    /// Paths left untouched, with the reason (missing, move failed, …).
    pub failed: Vec<Failure>,
    /// Physical bytes of the handled paths, measured just before the move.
    pub freed_physical_bytes: u64,
    /// The one history entry written; `None` when nothing was handled.
    pub history_id: Option<String>,
    /// The volume's usage read after the move.
    pub disk: Disk,
}

/// Title of a refusal because a path was not in the reviewed list.
pub const REFUSED_NOT_LISTED: &str = "path not reviewed";
/// Title of a refusal because every copy of a duplicate group was selected.
pub const REFUSED_EVERY_COPY: &str = "every copy selected";

/// `~/.Trash`, the Trash root outside tests.
pub fn user_trash_root() -> Result<PathBuf> {
    cleanup::default_trash(ApplyMode::Trash)
}

/// Physical bytes (`st_blocks * 512`) of `path` and, for a folder,
/// everything beneath it. Symlinks are not followed. Unreadable → 0.
fn physical_bytes_on_disk(path: &Path) -> u64 {
    let Ok(meta) = path.symlink_metadata() else {
        return 0;
    };
    #[cfg(unix)]
    let own = {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    };
    #[cfg(not(unix))]
    let own = meta.len();
    if !meta.is_dir() {
        return own;
    }
    let children: u64 = std::fs::read_dir(path)
        .map(|rd| {
            rd.flatten()
                .map(|e| physical_bytes_on_disk(&e.path()))
                .sum()
        })
        .unwrap_or(0);
    own + children
}

/// Move reviewed files to the Trash at `trash_root` and append ONE history
/// entry titled `title` to `log` (REQ-DSCR-006.A1, REQ-DCLEAN-034), through
/// the same machinery as a clean ([`cleanup::apply_observed`]).
///
/// * `listed` — the paths the engine listed in the view the user is looking
///   at (the last large-files or duplicates result). A path in `paths` that
///   is not in `listed` refuses the whole call, before anything moves
///   (A2). `disky trash` passes its own arguments: paths named on the
///   command line count as reviewed by the user.
/// * `groups` — the duplicate groups the paths came from. Selecting every
///   copy of a group refuses the whole call, so one copy always stays (A3).
///
/// A path that vanished or cannot be moved is reported in `failed`; the rest
/// still go (REQ-DCLEAN-018.A3). Tests pass a temporary `trash_root` and
/// `log` (A4).
pub fn trash_reviewed(
    listed: &[String],
    paths: &[String],
    groups: Option<&[Vec<String>]>,
    title: &str,
    log: &Path,
    trash_root: &Path,
) -> Result<TrashResult> {
    use crate::exit::{DiskyError, ExitCode};
    let mut selected: Vec<&String> = Vec::new();
    for p in paths {
        if !listed.contains(p) {
            return Err(DiskyError::new(
                ExitCode::Generic,
                REFUSED_NOT_LISTED,
                format!("{p} is not in the list you reviewed; nothing was moved"),
            )
            .into());
        }
        if !selected.contains(&p) {
            selected.push(p);
        }
    }
    for g in groups.unwrap_or_default() {
        if !g.is_empty() && g.iter().all(|c| selected.contains(&c)) {
            return Err(DiskyError::new(
                ExitCode::Generic,
                REFUSED_EVERY_COPY,
                format!(
                    "every copy of {} is selected; keep at least one. Nothing was moved",
                    basename(&g[0])
                ),
            )
            .into());
        }
    }
    let hits: Vec<CleanupHit> = selected
        .iter()
        .map(|p| {
            let physical = physical_bytes_on_disk(Path::new(p.as_str()));
            CleanupHit {
                category: String::new(),
                safety: Safety::Review,
                path: (*p).clone(),
                bytes: physical,
                physical_bytes: physical,
                files: 1,
                note: None,
                newest_mtime: None,
            }
        })
        .collect();
    let outcome = cleanup::apply_observed(
        &hits,
        ApplyMode::Trash,
        Some(log),
        trash_root,
        title,
        &mut |_, _| {},
    )?;
    let freed_physical_bytes = hits
        .iter()
        .filter(|h| outcome.handled.contains(&h.path))
        .map(|h| h.physical_bytes)
        .sum();
    let near = hits.first().map_or("/", |h| h.path.as_str());
    Ok(TrashResult {
        disk: disk_near(Path::new(near))?,
        freed_physical_bytes,
        history_id: outcome.entry.map(|e| e.id),
        handled: outcome.handled,
        failed: outcome.failed,
    })
}

/// Move duplicate copies to the Trash with one "Duplicates" history entry
/// (REQ-DSCR-014.A4) — [`trash_reviewed`] with the listed paths and groups
/// taken from `groups`, the duplicates result the user is looking at. A
/// path outside the groups is refused (REQ-DSCR-006.A2), and so is a call
/// that would remove every copy of any group (A3); either way nothing
/// moves. A group whose other copies were already trashed still guards its
/// last copy. CLI: `disky trash <path>... --title Duplicates`.
pub fn trash_dupes(
    groups: &[Vec<String>],
    paths: &[String],
    log: &Path,
    trash_root: &Path,
) -> Result<TrashResult> {
    let listed: Vec<String> = groups.concat();
    trash_reviewed(
        &listed,
        paths,
        Some(groups),
        history::TITLE_DUPLICATES,
        log,
        trash_root,
    )
}

/// Progress event for a running scan. The walk's total is unknown up front,
/// so `fraction` is physical bytes seen over the volume's used bytes, held
/// below 1 until [`scan_finished`]. Categories are classified after the
/// walk, so none are complete yet (REQ-DCLEAN-004 known limitation).
pub fn scan_progress(tick: &ScanTick, used_bytes: u64) -> ScanProgress {
    let fraction = if used_bytes == 0 {
        0.0
    } else {
        (tick.physical_bytes as f64 / used_bytes as f64).clamp(0.0, 0.99)
    };
    ScanProgress {
        fraction,
        current_path: tick.current_path.clone(),
        completed_category_ids: Vec::new(),
        completed_category_bytes: BTreeMap::new(),
    }
}

/// Final progress event: every catalogued category is complete, and those
/// the view lists carry their bytes.
pub fn scan_finished(view: &CleanupView) -> ScanProgress {
    ScanProgress {
        fraction: 1.0,
        current_path: String::new(),
        completed_category_ids: view.catalogue.iter().map(|c| c.id.clone()).collect(),
        completed_category_bytes: view
            .categories
            .iter()
            .map(|c| (c.id.clone(), c.bytes))
            .collect(),
    }
}

/// Keep only the hits the user reviewed. `paths` are the exact item paths
/// from the view; a path that is not in `reviewed` is refused, never
/// re-queried. The selection keeps the order of `paths`, which is the order
/// the clean runs in (REQ-DCLEAN-008.A1).
pub fn select_reviewed(
    reviewed: &[CleanupHit],
    paths: &[String],
) -> (Vec<CleanupHit>, Vec<Failure>) {
    let by_path: HashMap<&str, &CleanupHit> =
        reviewed.iter().map(|h| (h.path.as_str(), h)).collect();
    let mut selected: Vec<CleanupHit> = Vec::new();
    let mut failed = Vec::new();
    for p in paths {
        if selected.iter().any(|h| &h.path == p) {
            continue;
        }
        match by_path.get(p.as_str()) {
            Some(h) => selected.push((*h).clone()),
            None => failed.push(Failure {
                path: p.clone(),
                reason: FAILURE_NOT_REVIEWED.to_string(),
            }),
        }
    }
    (selected, failed)
}

fn apply_mode(mode: history::Mode) -> ApplyMode {
    match mode {
        history::Mode::Trash => ApplyMode::Trash,
        history::Mode::Delete => ApplyMode::Delete,
    }
}

/// History title for a clean (REQ-DCLEAN-009.A5): "Assistant cleanup" when
/// the assistant opened the review, else "Smart Clean".
pub fn clean_title(opened_by: Option<&str>) -> &'static str {
    match opened_by {
        Some(_) => history::TITLE_ASSISTANT,
        None => history::TITLE_SMART_CLEAN,
    }
}

/// Clean exactly the reviewed `paths` through [`cleanup::apply`] and record
/// the history entry in `log`. Trash mode is reversible via `disky restore`
/// (REQ-DCLEAN-034). A path gone since the scan is reported in `failed`
/// with reason `missing`, and the rest proceed (REQ-DCLEAN-029.A1).
pub fn clean(
    reviewed: &[CleanupHit],
    paths: &[String],
    mode: history::Mode,
    log: &Path,
) -> Result<CleanResult> {
    clean_observed(reviewed, paths, mode, log, None, |_| {})
}

/// [`clean`] that reports a [`CleanProgress`] after each item, in the order
/// of `paths` (REQ-DCLEAN-008), and titles the history entry from
/// `opened_by` (REQ-DCLEAN-009.A5). One history entry per call.
pub fn clean_observed(
    reviewed: &[CleanupHit],
    paths: &[String],
    mode: history::Mode,
    log: &Path,
    opened_by: Option<&str>,
    on: impl FnMut(CleanProgress),
) -> Result<CleanResult> {
    let trash = cleanup::default_trash(apply_mode(mode))?;
    clean_in(reviewed, paths, mode, log, &trash, opened_by, on)
}

/// [`clean`] with an explicit Trash root, so tests never touch `~/.Trash`.
#[cfg(test)]
pub(crate) fn clean_with(
    reviewed: &[CleanupHit],
    paths: &[String],
    mode: history::Mode,
    log: &Path,
    trash_root: &Path,
) -> Result<CleanResult> {
    clean_in(reviewed, paths, mode, log, trash_root, None, |_| {})
}

fn clean_in(
    reviewed: &[CleanupHit],
    paths: &[String],
    mode: history::Mode,
    log: &Path,
    trash_root: &Path,
    opened_by: Option<&str>,
    mut on: impl FnMut(CleanProgress),
) -> Result<CleanResult> {
    let (selected, refused) = select_reviewed(reviewed, paths);
    let outcome = cleanup::apply_observed(
        &selected,
        apply_mode(mode),
        Some(log),
        trash_root,
        clean_title(opened_by),
        &mut |_, i| on(clean_progress(&selected, i)),
    )?;
    clean_result(mode, &selected, outcome, refused)
}

/// Progress once item `index` of `selected` is done.
fn clean_progress(selected: &[CleanupHit], index: usize) -> CleanProgress {
    let total = selected.len();
    let done = (index + 1).min(total);
    let current = selected.get(done).or_else(|| selected.get(index));
    CleanProgress {
        category_id: current.map(|h| h.category.clone()).unwrap_or_default(),
        done_items: done as u64,
        total_items: total as u64,
        fraction: if total == 0 {
            1.0
        } else {
            done as f64 / total as f64
        },
    }
}

fn clean_result(
    mode: history::Mode,
    selected: &[CleanupHit],
    outcome: ApplyOutcome,
    mut failed: Vec<Failure>,
) -> Result<CleanResult> {
    let freed_bytes = selected
        .iter()
        .filter(|h| outcome.handled.contains(&h.path))
        .map(|h| h.physical_bytes)
        .sum();
    failed.extend(outcome.failed);
    // Read after every move/delete finished, on the cleaned paths' volume.
    let near = selected.first().map_or("/", |h| h.path.as_str());
    let disk = disk_near(Path::new(near))?;
    Ok(CleanResult {
        mode,
        freed_bytes,
        succeeded: outcome.handled,
        failed,
        history_id: outcome.entry.as_ref().map(|e| e.id.clone()),
        history_entry: outcome.entry,
        disk,
    })
}

/// What an Undo did (REQ-DCLEAN-009.A4): the restore report — restored,
/// conflicts, and items "no longer in Trash" (A7) — and the volume's usage
/// read afterwards, so the meter follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreResult {
    pub report: history::RestoreReport,
    pub disk: Disk,
}

/// Undo a clean: [`history::restore`] on entry `id`, then read disk usage.
/// CLI: `disky restore <id>`.
pub fn restore(log: &Path, id: &str) -> Result<RestoreResult> {
    let report = history::restore(log, id)?;
    let near = report
        .restored
        .first()
        .or(report.missing.first())
        .or(report.conflicts.first())
        .map_or("/", String::as_str);
    let disk = disk_near(Path::new(near))?;
    Ok(RestoreResult { report, disk })
}

/// Where a history entry stands, as the History screen tags it
/// (REQ-DSCR-015.A3, A4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryState {
    /// A Trash clean younger than [`history::RESTORE_WINDOW_DAYS`] with items
    /// not yet back: "Restore". A partial restore (some items no longer in
    /// the Trash) stays here, since `disky restore` retries the rest.
    Restorable,
    /// Every item is back (`restored_at` set): the "Restored" tag.
    Restored,
    /// A Trash clean [`history::RESTORE_WINDOW_DAYS`] or more old, or of
    /// unknown age: the "Emptied" tag.
    Emptied,
    /// A permanent delete: the "Deleted" tag, never restorable (gap-fill).
    Deleted,
}

/// One History row: the log entry plus what the screen shows for it. The
/// CLI's `disky history` records are these same rows (one derivation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRow {
    #[serde(flatten)]
    pub entry: history::Entry,
    /// Category names, or for "Large files" and "Duplicates" entries the
    /// file names ("<n> files" past three) (REQ-DSCR-015.A2).
    pub detail: String,
    pub state: HistoryState,
}

/// Everything the History screen renders (REQ-DSCR-015).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryView {
    /// Physical bytes freed over every entry: "<total> cleared".
    pub total_bytes: u64,
    /// Newest first.
    pub rows: Vec<HistoryRow>,
}

/// Most file names a row's detail lists before it reads "<n> files".
pub const HISTORY_DETAIL_MAX_FILES: usize = 3;

/// The detail line of an entry (REQ-DSCR-015.A2). Category ids become their
/// display names from [`CATEGORIES`]; an unknown id is shown as given.
pub fn history_detail(e: &history::Entry) -> String {
    if e.title == history::TITLE_LARGE_FILES || e.title == history::TITLE_DUPLICATES {
        let n = e.items.len();
        if n > HISTORY_DETAIL_MAX_FILES {
            return format!("{n} files");
        }
        return e
            .items
            .iter()
            .map(|i| basename(&i.original_path))
            .collect::<Vec<_>>()
            .join(", ");
    }
    e.categories
        .iter()
        .map(|id| cleanup::category(id).map_or(id.as_str(), |c| c.label))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Where an entry stands at `now` (REQ-DSCR-015.A3, A4).
pub fn history_state(e: &history::Entry, now: DateTime<Utc>) -> HistoryState {
    if e.mode == history::Mode::Delete {
        return HistoryState::Deleted;
    }
    if e.restored_at.is_some() {
        return HistoryState::Restored;
    }
    match DateTime::parse_from_rfc3339(&e.ts) {
        Ok(ts) if (now - ts.with_timezone(&Utc)).num_days() < history::RESTORE_WINDOW_DAYS => {
            HistoryState::Restorable
        }
        _ => HistoryState::Emptied,
    }
}

/// Build the History view from `entries` (any order) at `now`.
pub fn history_view_of(mut entries: Vec<history::Entry>, now: DateTime<Utc>) -> HistoryView {
    entries.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| b.id.cmp(&a.id)));
    HistoryView {
        total_bytes: entries.iter().map(|e| e.bytes).sum(),
        rows: entries
            .into_iter()
            .map(|entry| HistoryRow {
                detail: history_detail(&entry),
                state: history_state(&entry, now),
                entry,
            })
            .collect(),
    }
}

/// The History screen's view of the log at `log`, newest first. CLI:
/// `disky history --format json` (same rows, `total_bytes` on the envelope).
pub fn history_view(log: &Path, now: DateTime<Utc>) -> Result<HistoryView> {
    Ok(history_view_of(history::list(log)?, now))
}

/// [`history_view`] at the current time — what `disky history` and the
/// GUI's `list_history` call.
pub fn history_view_now(log: &Path) -> Result<HistoryView> {
    history_view(log, Utc::now())
}

/// A failed engine call as the GUI receives it: disky's RFC 9457 fields,
/// with `code` the stable exit-code slug (`lock-held`, `io`, …). The GUI
/// dispatches on `code`, never on `detail` (REQ-DCLEAN-018.A1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineError {
    pub code: String,
    pub title: String,
    pub detail: String,
    pub retryable: bool,
}

impl From<anyhow::Error> for EngineError {
    fn from(e: anyhow::Error) -> Self {
        let d = crate::exit::classify(e);
        EngineError {
            code: d.code.slug().to_string(),
            title: d.title.to_string(),
            detail: d.detail,
            retryable: d.retryable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(category: &str, path: &str, physical: u64, safety: Safety) -> CleanupHit {
        CleanupHit {
            category: category.to_string(),
            safety,
            path: path.to_string(),
            bytes: physical * 3, // logical differs: the view must use physical
            physical_bytes: physical,
            files: 1,
            note: None,
            newest_mtime: None,
        }
    }

    fn sample() -> Vec<CleanupHit> {
        vec![
            hit("temp", "/private/tmp/a", 10, Safety::Safe),
            hit("node_modules", "/p/app/node_modules", 700, Safety::Review),
            hit("temp", "/private/var/folders/x/y/T/b/", 5, Safety::Review),
            hit("app_caches", "/h/Library/Caches/com.foo", 40, Safety::Safe),
        ]
    }

    #[test]
    fn category_bytes_equal_item_sum() {
        let cats = categories_from_hits(&sample());
        for c in &cats {
            assert_eq!(c.bytes, c.items.iter().map(|i| i.bytes).sum::<u64>());
        }
        let temp = cats.iter().find(|c| c.id == "temp").unwrap();
        assert_eq!(temp.bytes, 15, "physical bytes, not logical");
        assert_eq!(temp.safety, Safety::Review, "most cautious item wins");
        let caches = cats.iter().find(|c| c.id == "app_caches").unwrap();
        assert_eq!(caches.safety, Safety::Safe);
    }

    #[test]
    fn item_name_is_basename() {
        let cats = categories_from_hits(&sample());
        let names: Vec<(&str, &str)> = cats
            .iter()
            .flat_map(|c| c.items.iter().map(|i| (i.name.as_str(), i.path.as_str())))
            .collect();
        assert!(names.contains(&("node_modules", "/p/app/node_modules")));
        assert!(names.contains(&("b", "/private/var/folders/x/y/T/b/")));
        assert!(names.contains(&("com.foo", "/h/Library/Caches/com.foo")));
    }

    #[test]
    fn category_ids_come_from_engine_table() {
        let cats = categories_from_hits(&sample());
        let order: Vec<&str> = cats.iter().map(|c| c.id.as_str()).collect();
        // CATEGORIES order, empty categories dropped.
        assert_eq!(order, ["node_modules", "app_caches", "temp"]);
        for c in &cats {
            let def = cleanup::category(&c.id).expect("id is an engine category");
            assert_eq!(c.name, def.label);
            assert_eq!(c.description, def.description);
            assert_eq!(c.icon, def.icon);
        }
        // Unknown categories never surface.
        let stray = [hit("made_up", "/x", 1, Safety::Safe)];
        assert!(categories_from_hits(&stray).is_empty());
    }

    #[test]
    fn contract_round_trips_and_rejects_unknown_safety() {
        let view = cleanup_view(
            None,
            &sample(),
            Disk {
                volume: "Macintosh HD".into(),
                capacity_bytes: 512_000_000_000,
                free_bytes: 74_000_000_000,
            },
            &Preferences::default(),
            8,
            0,
        );
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["large_files_count"], 8);
        let back: CleanupView = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(back, view);
        assert_eq!(json["categories"][0]["safety"], "review");

        let mut bad = json;
        bad["categories"][0]["safety"] = "maybe".into();
        assert!(serde_json::from_value::<CleanupView>(bad).is_err());
        assert!(serde_json::from_str::<Safety>("\"maybe\"").is_err());
    }

    #[test]
    fn view_marks_snapshot_stale_after_24h() {
        let meta = crate::db::ScanMeta {
            root: "/".into(),
            started_at: 1_700_000_000,
            ended_at: Some(1_700_000_060),
            completed: true,
            entries: 0,
            bytes: 0,
        };
        let disk = Disk {
            volume: "v".into(),
            capacity_bytes: 1,
            free_bytes: 0,
        };
        let at = |now| {
            cleanup_view(
                Some(&meta),
                &[],
                disk.clone(),
                &Preferences::default(),
                0,
                now,
            )
        };
        let fresh = at(meta.started_at + STALE_AFTER_SECS);
        assert!(!fresh.is_stale);
        assert_eq!(
            fresh.snapshot_taken_at.as_deref(),
            Some("2023-11-14T22:13:20Z")
        );
        assert_eq!(fresh.last_scan_at.as_deref(), Some("2023-11-14T22:14:20Z"));
        assert!(at(meta.started_at + STALE_AFTER_SECS + 1).is_stale);
        let unknown = cleanup_view(None, &[], disk, &Preferences::default(), 0, 0);
        assert!(unknown.is_stale && unknown.last_scan_at.is_none());
    }

    #[test]
    fn default_preferences_are_all_on() {
        let p = Preferences::default();
        assert!(p.trash_by_default && p.safe_only && p.warn_below_goal && p.checks.enabled);
        assert!(p.skip_recently_used);
    }

    #[test]
    fn view_goal_and_preferences_come_from_the_store() {
        let prefs = Preferences {
            goal_bytes: 123_000_000_000,
            trash_by_default: false,
            ..Preferences::default()
        };
        let disk = Disk {
            volume: "v".into(),
            capacity_bytes: 1,
            free_bytes: 0,
        };
        let v = cleanup_view(None, &[], disk, &prefs, 0, 0);
        assert_eq!(v.goal_bytes, 123_000_000_000);
        assert_eq!(v.preferences, prefs);
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["preferences"]["checks"]["frequency"], "daily");
        assert_eq!(json["preferences"]["trash_by_default"], false);
    }

    #[test]
    fn large_files_count_honours_exclusions_and_recent_use() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO files (path, name, ext, size, physical_size, mtime, is_dir, depth) VALUES
             ('/h/keep/a', 'a', '', 3000000000, 3000000000, 100, false, 2),
             ('/h/ex/b', 'b', '', 3000000000, 3000000000, 100, false, 2),
             ('/h/exother/c', 'c', '', 3000000000, 3000000000, 100, false, 2),
             ('/h/keep/new', 'new', '', 3000000000, 3000000000, 900, false, 2);",
        )
        .unwrap();
        let mut ctx = cleanup::ScanContext {
            excluded: vec!["/h/ex/".into(), "/h/does-not-exist".into()],
            ..Default::default()
        };
        assert_eq!(
            large_files_count_in(&conn, &ctx).unwrap(),
            3,
            "prefix on components"
        );
        ctx.recent_cutoff = Some(500);
        assert_eq!(large_files_count_in(&conn, &ctx).unwrap(), 2);
        assert_eq!(large_files_count(&conn).unwrap(), 4);
    }

    #[test]
    fn scan_progress_fraction_is_clamped() {
        let tick = |b| ScanTick {
            physical_bytes: b,
            current_path: "/x".into(),
            ..Default::default()
        };
        assert_eq!(scan_progress(&tick(50), 100).fraction, 0.5);
        assert_eq!(scan_progress(&tick(500), 100).fraction, 0.99);
        assert_eq!(scan_progress(&tick(5), 0).fraction, 0.0);
        assert_eq!(scan_progress(&tick(5), 100).current_path, "/x");
    }

    #[test]
    fn select_reviewed_refuses_unreviewed_paths() {
        let reviewed = sample();
        let paths = vec![
            "/private/tmp/a".to_string(),
            "/etc/passwd".to_string(),
            "/private/tmp/a".to_string(),
        ];
        let (sel, failed) = select_reviewed(&reviewed, &paths);
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0].path, "/private/tmp/a");
        assert_eq!(
            failed,
            [Failure {
                path: "/etc/passwd".into(),
                reason: FAILURE_NOT_REVIEWED.into()
            }]
        );
    }

    #[test]
    fn clean_reports_missing_and_proceeds() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let present = tmp.path().join("proj/node_modules");
        std::fs::create_dir_all(&present).unwrap();
        let gone = tmp.path().join("proj2/node_modules");
        let p = |x: &Path| x.to_string_lossy().into_owned();
        let reviewed = vec![
            hit("node_modules", &p(&gone), 9, Safety::Review),
            hit("node_modules", &p(&present), 42, Safety::Review),
        ];
        let paths = vec![p(&gone), p(&present)];

        let r = clean_with(&reviewed, &paths, history::Mode::Trash, &log, &trash).unwrap();
        assert_eq!(r.mode, history::Mode::Trash);
        assert_eq!(r.succeeded, [p(&present)]);
        assert_eq!(
            r.failed,
            [Failure {
                path: p(&gone),
                reason: cleanup::FAILURE_MISSING.into()
            }]
        );
        assert_eq!(r.freed_bytes, 42);
        assert!(!present.exists());
        let entry = r.history_entry.expect("history entry written");
        assert_eq!(entry.items.len(), 1);
        assert!(
            entry.items[0].trash_path.is_some(),
            "trash mode is reversible"
        );
        assert_eq!(history::load(&log).unwrap(), [entry]);
    }

    fn write_file(path: &Path, len: usize) -> String {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; len]).unwrap();
        crate::platform::to_slash(path)
    }

    #[test]
    fn trash_reviewed_moves_listed_files_and_writes_one_titled_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let a = write_file(&tmp.path().join("u/a.img"), 64 * 1024);
        let b = write_file(&tmp.path().join("u/b.iso"), 8 * 1024);
        let gone = crate::platform::to_slash(&tmp.path().join("u/gone.dmg"));
        let listed = vec![a.clone(), b.clone(), gone.clone()];
        let paths = vec![a.clone(), gone.clone(), b.clone(), a.clone()];

        let r = trash_reviewed(
            &listed,
            &paths,
            None,
            history::TITLE_LARGE_FILES,
            &log,
            &trash,
        )
        .unwrap();
        assert_eq!(r.handled, [a.clone(), b.clone()], "duplicates collapse");
        assert_eq!(
            r.failed,
            [Failure {
                path: gone,
                reason: cleanup::FAILURE_MISSING.into()
            }]
        );
        assert!(!Path::new(&a).exists() && !Path::new(&b).exists());
        assert!(r.freed_physical_bytes >= 72 * 1024, "physical bytes freed");
        assert!(r.disk.capacity_bytes > 0);
        let entries = history::load(&log).unwrap();
        assert_eq!(entries.len(), 1, "one history entry");
        let e = &entries[0];
        assert_eq!(Some(e.id.clone()), r.history_id);
        assert_eq!(e.title, history::TITLE_LARGE_FILES);
        assert_eq!(e.mode, history::Mode::Trash);
        assert!(e.categories.is_empty(), "reviewed files carry no category");
        assert_eq!(e.bytes, r.freed_physical_bytes);
        assert!(e.items.iter().all(|i| i
            .trash_path
            .as_deref()
            .is_some_and(|t| t.starts_with(&crate::platform::to_slash(&trash)))));
        assert_eq!(history_detail(e), "a.img, b.iso");
    }

    #[test]
    fn trash_reviewed_refuses_a_path_not_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let a = write_file(&tmp.path().join("u/a.img"), 10);
        let other = write_file(&tmp.path().join("u/other.img"), 10);
        let err = trash_reviewed(
            std::slice::from_ref(&a),
            &[a.clone(), other.clone()],
            None,
            history::TITLE_LARGE_FILES,
            &log,
            &trash,
        )
        .unwrap_err();
        let e = EngineError::from(err);
        assert_eq!(
            (e.code.as_str(), e.title.as_str()),
            ("generic", REFUSED_NOT_LISTED)
        );
        assert!(
            Path::new(&a).exists() && Path::new(&other).exists(),
            "nothing moved"
        );
        assert!(!log.exists(), "no history entry");
    }

    #[test]
    fn trash_reviewed_refuses_every_copy_of_a_group() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let a = write_file(&tmp.path().join("x/dup.bin"), 10);
        let b = write_file(&tmp.path().join("y/dup.bin"), 10);
        let groups = vec![vec![a.clone(), b.clone()]];
        let listed = vec![a.clone(), b.clone()];
        let err = trash_reviewed(
            &listed,
            &listed,
            Some(&groups),
            history::TITLE_DUPLICATES,
            &log,
            &trash,
        )
        .unwrap_err();
        assert_eq!(EngineError::from(err).title, REFUSED_EVERY_COPY);
        assert!(Path::new(&a).exists() && Path::new(&b).exists());
        // One copy kept: allowed.
        let r = trash_reviewed(
            &listed,
            std::slice::from_ref(&b),
            Some(&groups),
            history::TITLE_DUPLICATES,
            &log,
            &trash,
        )
        .unwrap();
        assert_eq!(r.handled, [b]);
        assert!(Path::new(&a).exists());
    }

    #[test]
    fn trash_dupes_keeps_one_copy_of_every_group() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let a1 = write_file(&tmp.path().join("x/a.bin"), 10);
        let a2 = write_file(&tmp.path().join("y/a.bin"), 10);
        let b1 = write_file(&tmp.path().join("x/b.bin"), 20);
        let b2 = write_file(&tmp.path().join("y/b.bin"), 20);
        let b3 = write_file(&tmp.path().join("z/b.bin"), 20);
        let groups = vec![
            vec![a1.clone(), a2.clone()],
            vec![b1.clone(), b2.clone(), b3.clone()],
        ];

        // Every copy of `b`, even alongside a fine pick from `a`: refused whole.
        let err = trash_dupes(
            &groups,
            &[a2.clone(), b1.clone(), b2.clone(), b3.clone()],
            &log,
            &trash,
        )
        .unwrap_err();
        assert_eq!(EngineError::from(err).title, REFUSED_EVERY_COPY);
        assert!([&a1, &a2, &b1, &b2, &b3]
            .iter()
            .all(|p| Path::new(p.as_str()).exists()));
        assert!(!log.exists(), "nothing moved, no entry");

        // A path outside the groups: refused.
        let stray = write_file(&tmp.path().join("stray.bin"), 10);
        let err = trash_dupes(&groups, std::slice::from_ref(&stray), &log, &trash).unwrap_err();
        assert_eq!(EngineError::from(err).title, REFUSED_NOT_LISTED);

        // One kept per group: moved, one "Duplicates" entry.
        let r = trash_dupes(&groups, &[a2.clone(), b2.clone(), b3.clone()], &log, &trash).unwrap();
        assert_eq!(r.handled, [a2.clone(), b2.clone(), b3.clone()]);
        assert!(Path::new(&a1).exists() && Path::new(&b1).exists());
        let entries = history::load(&log).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, history::TITLE_DUPLICATES);

        // The last copy left in a group stays guarded.
        let left = vec![vec![a1.clone()], vec![b1.clone()]];
        let err = trash_dupes(&left, std::slice::from_ref(&a1), &log, &trash).unwrap_err();
        assert_eq!(EngineError::from(err).title, REFUSED_EVERY_COPY);
        assert!(Path::new(&a1).exists());
    }

    #[test]
    fn dupes_view_reads_the_snapshot_with_prefs() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("s.db");
        let db = db.to_str().unwrap();
        let files: Vec<String> = ["ex/a.bin", "b.bin", "c.bin"]
            .iter()
            .map(|r| write_file(&tmp.path().join("u").join(r), 2048))
            .collect();
        {
            let conn = crate::db::open(db).unwrap();
            crate::db::create_schema(&conn).unwrap();
            for p in &files {
                let meta = std::fs::metadata(p).unwrap();
                let mtime = meta
                    .modified()
                    .unwrap()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                conn.execute(
                    "INSERT INTO files (path, name, size, physical_size, mtime, is_dir, depth)
                     VALUES (?, ?, 2048, 4096, ?, false, 2)",
                    duckdb::params![p, basename(p), mtime],
                )
                .unwrap();
            }
            let root = tmp.path().join("u");
            crate::db::write_scan_meta(&conn, root.to_str().unwrap(), 0, Some(1), true, 0, 0)
                .unwrap();
        }
        let include_recent = Preferences {
            skip_recently_used: false,
            ..Preferences::default()
        };
        let v = dupes_view(db, 1, &include_recent).unwrap();
        assert_eq!(v.groups, 1);
        assert_eq!(v.records[0].copies.len(), 3);
        assert_eq!(v.records[0].bytes, 4096, "physical bytes per copy");
        let excluded = Preferences {
            excluded: vec![tmp.path().join("u/ex").to_string_lossy().into_owned()],
            ..include_recent.clone()
        };
        assert_eq!(
            dupes_view(db, 1, &excluded).unwrap().records[0]
                .copies
                .len(),
            2
        );
        // Default prefs skip recently used files: these were just written.
        assert_eq!(
            dupes_view(db, 1, &Preferences::default()).unwrap().groups,
            0
        );
    }

    #[test]
    fn large_view_reads_the_snapshot_with_prefs() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("s.db");
        let db = db.to_str().unwrap();
        // Real (tiny) files: the view leaves out files gone from disk. The
        // snapshot's sizes are what the scan recorded.
        let u = tmp.path().join("u");
        std::fs::create_dir_all(u.join("ex")).unwrap();
        let at = |rel: &str| crate::platform::to_slash(&u.join(rel));
        for rel in ["ex/a.img", "b.img", "c.img", "gone.img"] {
            std::fs::write(u.join(rel), b"x").unwrap();
        }
        {
            let conn = crate::db::open(db).unwrap();
            crate::db::create_schema(&conn).unwrap();
            let old = Utc::now().timestamp() - 400 * 86_400;
            for (rel, gb) in [("ex/a.img", 9), ("b.img", 3), ("c.img", 1), ("gone.img", 4)] {
                let p = at(rel);
                conn.execute(
                    "INSERT INTO files (path, name, size, physical_size, mtime, is_dir, depth, atime)
                     VALUES (?, ?, 0, ?, ?, false, 2, ?)",
                    duckdb::params![p, basename(&p), gb * 1_000_000_000i64, old, old],
                )
                .unwrap();
            }
            crate::db::write_scan_meta(&conn, &at(""), 0, Some(1), true, 0, 0).unwrap();
        }
        // Deleted after the scan: listed in the snapshot, not offered.
        std::fs::remove_file(u.join("gone.img")).unwrap();
        let prefs = Preferences {
            excluded: vec![at("ex")],
            ..Preferences::default()
        };
        let v = large_view(db, LargeFilter::All, &prefs).unwrap();
        assert_eq!(v.count, 1);
        assert_eq!(v.records[0].path, at("b.img"));
        assert_eq!(v.total_bytes, 3_000_000_000);
        assert!(v.records[0].unused_1y);
        let all = large_view(db, LargeFilter::All, &Preferences::default()).unwrap();
        assert_eq!(all.count, 2);
        // The Smart Clean shortcut counts exactly what the screen lists.
        let (view, _) = load(db, &Preferences::default()).unwrap();
        assert_eq!(view.large_files_count, 2);
    }

    #[test]
    fn clean_result_carries_post_clean_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let dir = tmp.path().join("proj/node_modules");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.to_string_lossy().into_owned();
        let reviewed = vec![hit("node_modules", &p, 7, Safety::Review)];

        let r = clean_with(&reviewed, &[p], history::Mode::Trash, &log, &trash).unwrap();
        assert!(r.disk.capacity_bytes > 0);
        assert!(r.disk.free_bytes <= r.disk.capacity_bytes);
        let json = serde_json::to_value(&r).unwrap();
        assert!(json["disk"]["free_bytes"].is_u64());
        // Nothing selected still reports the volume (falls back to `/`).
        let none = clean_with(&[], &[], history::Mode::Trash, &log, &trash).unwrap();
        assert!(none.disk.capacity_bytes > 0);
    }

    #[test]
    fn disk_near_walks_up_to_an_existing_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let gone = tmp.path().join("a/b/c");
        let d = disk_near(&gone).unwrap();
        assert_eq!(
            d.capacity_bytes,
            disk_usage(tmp.path()).unwrap().capacity_bytes
        );
        assert!(
            disk_near(Path::new("relative/missing"))
                .unwrap()
                .capacity_bytes
                > 0
        );
    }

    #[test]
    fn large_files_count_uses_physical_bytes_at_2gb() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO files (path, name, ext, size, physical_size, mtime, is_dir, depth) VALUES
             ('/a', 'a', '', 9000000000, 2000000000, 0, false, 1),
             ('/b', 'b', '', 9000000000, 1999999999, 0, false, 1),
             ('/c', 'c', '', 3000000000, 3000000000, 0, false, 1),
             ('/d', 'd', '', 5000000000, 5000000000, 0, true, 1),
             ('/e', 'e', '', 5000000000, NULL, 0, false, 1);",
        )
        .unwrap();
        assert_eq!(large_files_count(&conn).unwrap(), 2);
    }

    #[test]
    fn scan_finished_completes_every_category() {
        let view = cleanup_view(
            None,
            &sample(),
            Disk {
                volume: "Macintosh HD".into(),
                capacity_bytes: 512_000_000_000,
                free_bytes: 74_000_000_000,
            },
            &Preferences::default(),
            0,
            0,
        );
        let done = scan_finished(&view);
        assert_eq!(done.fraction, 1.0);
        let ids: Vec<&str> = CATEGORIES.iter().map(|c| c.name).collect();
        assert_eq!(done.completed_category_ids, ids);
        for c in &view.categories {
            assert!(done.completed_category_ids.contains(&c.id));
            assert_eq!(done.completed_category_bytes.get(&c.id), Some(&c.bytes));
        }
        assert_eq!(done.completed_category_bytes.len(), view.categories.len());
    }

    #[test]
    fn catalogue_lists_every_engine_category() {
        let v = empty_view(&Preferences::default()).unwrap();
        assert!(v.categories.is_empty());
        assert_eq!(v.catalogue.len(), CATEGORIES.len());
        for (entry, c) in v.catalogue.iter().zip(CATEGORIES) {
            assert_eq!(entry.id, c.name);
            assert_eq!(entry.name, c.label);
            assert_eq!(entry.icon, c.icon);
            assert_eq!(entry.safety, c.safety);
        }
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["catalogue"][0]["id"], CATEGORIES[0].name);
    }

    #[test]
    fn empty_view_has_no_scan_and_live_disk() {
        let v = empty_view(&Preferences::default()).unwrap();
        assert!(v.last_scan_at.is_none());
        assert!(v.categories.is_empty());
        assert_eq!(v.large_files_count, 0);
        assert!(v.disk.capacity_bytes > 0);
    }

    #[test]
    fn select_reviewed_keeps_the_callers_order() {
        let reviewed = sample();
        let paths: Vec<String> = [
            "/h/Library/Caches/com.foo",
            "/private/tmp/a",
            "/p/app/node_modules",
        ]
        .map(String::from)
        .to_vec();
        let (sel, failed) = select_reviewed(&reviewed, &paths);
        assert!(failed.is_empty());
        let got: Vec<&str> = sel.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(got, paths.iter().map(String::as_str).collect::<Vec<_>>());
    }

    #[test]
    fn clean_reports_progress_per_item_in_selection_order() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let p = |x: &Path| x.to_string_lossy().into_owned();
        let a1 = tmp.path().join("a1/node_modules");
        let a2 = tmp.path().join("a2/node_modules");
        let b1 = tmp.path().join("b1/target");
        for d in [&a1, &a2, &b1] {
            std::fs::create_dir_all(d).unwrap();
        }
        let reviewed = vec![
            hit("target", &p(&b1), 5, Safety::Review),
            hit("node_modules", &p(&a1), 3, Safety::Review),
            hit("node_modules", &p(&a2), 4, Safety::Review),
        ];
        // Selection order: both node_modules first, then target.
        let paths = vec![p(&a1), p(&a2), p(&b1)];
        let mut events = Vec::new();
        let r = clean_in(
            &reviewed,
            &paths,
            history::Mode::Trash,
            &log,
            &trash,
            None,
            |e| events.push(e),
        )
        .unwrap();

        let seen: Vec<(&str, u64, u64)> = events
            .iter()
            .map(|e| (e.category_id.as_str(), e.done_items, e.total_items))
            .collect();
        assert_eq!(
            seen,
            [("node_modules", 1, 3), ("target", 2, 3), ("target", 3, 3)]
        );
        assert!(events.windows(2).all(|w| w[0].fraction <= w[1].fraction));
        assert_eq!(events.last().unwrap().fraction, 1.0);
        assert_eq!(r.succeeded, paths, "cleaned in selection order");
        let entries = history::load(&log).unwrap();
        assert_eq!(entries.len(), 1, "one history entry per clean");
        assert_eq!(r.history_id.as_deref(), Some(entries[0].id.as_str()));
        assert_eq!(entries[0].title, history::TITLE_SMART_CLEAN);
    }

    #[test]
    fn clean_title_follows_who_opened_the_review() {
        assert_eq!(clean_title(None), "Smart Clean");
        assert_eq!(clean_title(Some("Assistant")), "Assistant cleanup");
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let dir = tmp.path().join("proj/node_modules");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.to_string_lossy().into_owned();
        let reviewed = vec![hit("node_modules", &p, 7, Safety::Review)];
        let r = clean_in(
            &reviewed,
            &[p],
            history::Mode::Delete,
            &log,
            &tmp.path().join("Trash"),
            Some("Assistant"),
            |_| {},
        )
        .unwrap();
        let e = r.history_entry.unwrap();
        assert_eq!(e.title, "Assistant cleanup");
        assert_eq!(e.mode, history::Mode::Delete);
        assert_eq!(e.categories, ["node_modules"]);
        assert_eq!(e.bytes, 7);
    }

    #[test]
    fn partial_clean_frees_only_what_was_reclaimed() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let trash = tmp.path().join("Trash");
        let p = |x: &Path| x.to_string_lossy().into_owned();
        let present = tmp.path().join("proj/node_modules");
        std::fs::create_dir_all(&present).unwrap();
        // Already in the Trash: refused in Trash mode, left untouched.
        let in_trash = trash.join("old");
        std::fs::create_dir_all(&in_trash).unwrap();
        let reviewed = vec![
            hit("node_modules", &p(&present), 40, Safety::Review),
            hit("node_modules", &p(&in_trash), 900, Safety::Review),
        ];
        let r = clean_with(
            &reviewed,
            &[p(&present), p(&in_trash)],
            history::Mode::Trash,
            &log,
            &trash,
        )
        .unwrap();
        assert_eq!(r.freed_bytes, 40, "reclaimed, not the 940 estimate");
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].path, p(&in_trash));
        assert!(in_trash.exists());
    }

    #[test]
    fn undo_restores_a_trash_clean_and_reports_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        let log = tmp.path().join("history.json");
        let p = |x: &Path| x.to_string_lossy().into_owned();
        let a = tmp.path().join("a/node_modules");
        let b = tmp.path().join("b/node_modules");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let reviewed = vec![
            hit("node_modules", &p(&a), 1, Safety::Review),
            hit("node_modules", &p(&b), 2, Safety::Review),
        ];
        let r = clean_with(
            &reviewed,
            &[p(&a), p(&b)],
            history::Mode::Trash,
            &log,
            &trash,
        )
        .unwrap();
        let id = r.history_id.unwrap();
        // The user emptied one item from the Trash meanwhile.
        let gone = r.history_entry.unwrap().items[1]
            .trash_path
            .clone()
            .unwrap();
        std::fs::remove_dir_all(&gone).unwrap();

        let u = restore(&log, &id).unwrap();
        assert_eq!(u.report.restored, [p(&a)]);
        assert_eq!(u.report.missing, [p(&b)], "reported, not a silent failure");
        assert!(a.exists());
        assert!(u.disk.capacity_bytes > 0);
    }

    #[test]
    fn engine_errors_carry_the_exit_slug() {
        let held: EngineError = anyhow::Error::from(crate::exit::DiskyError::lock_held("x")).into();
        assert_eq!(held.code, "lock-held");
        assert!(held.retryable);
        let duck: EngineError =
            anyhow::anyhow!("IO Error: Could not set lock on file \"/x.db\"").into();
        assert_eq!(duck.code, "lock-held");
        let other: EngineError = anyhow::anyhow!("boom").into();
        assert_eq!(other.code, "generic");
        let json = serde_json::to_value(&held).unwrap();
        assert_eq!(json["code"], "lock-held");
    }

    fn entry(
        title: &str,
        mode: history::Mode,
        ts: &str,
        cats: &[&str],
        files: &[&str],
    ) -> history::Entry {
        let items = files
            .iter()
            .map(|f| history::Item {
                original_path: format!("/Users/me/Downloads/{f}"),
                trash_path: Some(format!("/Users/me/.Trash/{f}")),
                bytes: 10,
                status: history::ItemStatus::InTrash,
            })
            .collect();
        let mut e = history::Entry::new(
            title,
            mode,
            cats.iter().map(|c| c.to_string()).collect(),
            items,
        );
        e.ts = ts.to_string();
        e.bytes = 100;
        e
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-24T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn days_ago(d: i64) -> String {
        (now() - chrono::Duration::days(d)).to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    #[test]
    fn history_rows_newest_first_with_total() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        assert_eq!(
            history_view(&log, now()).unwrap(),
            HistoryView {
                total_bytes: 0,
                rows: vec![]
            }
        );
        let old = entry(
            "Smart Clean",
            history::Mode::Trash,
            &days_ago(49),
            &["logs"],
            &[],
        );
        let new = entry(
            "Smart Clean",
            history::Mode::Trash,
            &days_ago(7),
            &["trash"],
            &[],
        );
        history::append(&log, old.clone()).unwrap();
        history::append(&log, new.clone()).unwrap();
        let v = history_view(&log, now()).unwrap();
        assert_eq!(v.total_bytes, 200);
        let ids: Vec<&str> = v.rows.iter().map(|r| r.entry.id.as_str()).collect();
        assert_eq!(ids, [new.id.as_str(), old.id.as_str()]);
        // The CLI record is the entry plus the derived fields.
        let j = serde_json::to_value(&v.rows[0]).unwrap();
        assert_eq!(j["title"], "Smart Clean");
        assert_eq!(j["state"], "restorable");
        assert_eq!(j["detail"], cleanup::category("trash").unwrap().label);
        assert!(j["items"].is_array());
    }

    #[test]
    fn detail_uses_category_names() {
        let e = entry(
            "Smart Clean",
            history::Mode::Trash,
            &days_ago(1),
            &["app_caches", "temp", "made_up"],
            &["x"],
        );
        let want = format!(
            "{}, {}, made_up",
            cleanup::category("app_caches").unwrap().label,
            cleanup::category("temp").unwrap().label
        );
        assert_eq!(history_detail(&e), want);
    }

    #[test]
    fn detail_lists_file_names_up_to_three() {
        let three = entry(
            history::TITLE_LARGE_FILES,
            history::Mode::Trash,
            &days_ago(1),
            &[],
            &["a.iso", "b.dmg", "c.zip"],
        );
        assert_eq!(history_detail(&three), "a.iso, b.dmg, c.zip");
        let four = entry(
            history::TITLE_DUPLICATES,
            history::Mode::Trash,
            &days_ago(1),
            &[],
            &["a", "b", "c", "d"],
        );
        assert_eq!(history_detail(&four), "4 files");
    }

    #[test]
    fn state_deleted_for_delete_mode() {
        let e = entry(
            "Cleanup",
            history::Mode::Delete,
            &days_ago(1),
            &["target"],
            &[],
        );
        assert_eq!(history_state(&e, now()), HistoryState::Deleted);
    }

    #[test]
    fn state_emptied_at_thirty_days() {
        let at = |d| {
            history_state(
                &entry("Smart Clean", history::Mode::Trash, &days_ago(d), &[], &[]),
                now(),
            )
        };
        assert_eq!(at(0), HistoryState::Restorable);
        assert_eq!(at(29), HistoryState::Restorable);
        assert_eq!(at(30), HistoryState::Emptied);
        let mut bad = entry("Smart Clean", history::Mode::Trash, "garbage", &[], &[]);
        assert_eq!(history_state(&bad, now()), HistoryState::Emptied);
        bad.ts = days_ago(29) + "x";
        assert_eq!(history_state(&bad, now()), HistoryState::Emptied);
    }

    #[test]
    fn state_restored_when_restored_at() {
        let mut e = entry(
            "Smart Clean",
            history::Mode::Trash,
            &days_ago(2),
            &[],
            &["a"],
        );
        e.items[0].status = history::ItemStatus::Missing; // partial: still restorable
        assert_eq!(history_state(&e, now()), HistoryState::Restorable);
        e.restored_at = Some(days_ago(1));
        assert_eq!(history_state(&e, now()), HistoryState::Restored);
    }

    #[test]
    fn full_disk_access_url_opens_the_privacy_pane() {
        assert!(FULL_DISK_ACCESS_URL.starts_with("x-apple.systempreferences:"));
        assert!(FULL_DISK_ACCESS_URL.ends_with("Privacy_AllFiles"));
    }
}

#[cfg(test)]
mod assistant_tests {
    use super::*;
    use crate::cleanup::Safety;

    fn present_hit(category: &str, path: &Path, bytes: u64, safety: Safety) -> CleanupHit {
        CleanupHit {
            category: category.into(),
            safety,
            path: path.to_string_lossy().into_owned(),
            bytes,
            physical_bytes: bytes,
            files: 1,
            note: None,
            newest_mtime: None,
        }
    }

    fn agents() -> Vec<Agent> {
        crate::agents::detect_in(&[], None)
    }

    fn view_with(hits: &[CleanupHit], prefs: &Preferences, free: u64) -> CleanupView {
        let mut v = cleanup_view(
            None,
            hits,
            Disk {
                volume: "Macintosh HD".into(),
                capacity_bytes: 512_000_000_000,
                free_bytes: free,
            },
            prefs,
            0,
            0,
        );
        v.last_scan_at = Some("2026-11-24T22:00:00Z".into());
        v
    }

    /// REQ-DSCR-009.A4 / REQ-DSCR-010: the view names the selected agent and
    /// empties the feed when proactive checks are off.
    #[test]
    fn assistant_view_names_the_agent_and_checks_off_empties_the_feed() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("Caches/x");
        std::fs::create_dir_all(&cache).unwrap();
        let hits = vec![present_hit(
            "app_caches",
            &cache,
            5_000_000_000,
            Safety::Safe,
        )];
        let mut prefs = Preferences {
            agent: "codex".into(),
            ..Default::default()
        };
        let v = view_with(&hits, &prefs, 10_000_000_000);
        let a = assistant_view_of(&v, &hits, agents(), None, 0);
        assert_eq!(a.agent, "codex");
        assert_eq!(a.agent_name, "Codex CLI");
        assert_eq!(a.agents.len(), 3);
        assert_eq!(a.goal_bytes, prefs.goal_bytes);
        assert_eq!(a.suggestions.len(), 1);
        assert_eq!(a.suggestions[0].key, SuggestionKey::Goal);
        assert_eq!(a.suggestions[0].categories, ["app_caches"]);

        prefs.checks.enabled = false;
        let v = view_with(&hits, &prefs, 10_000_000_000);
        let a = assistant_view_of(&v, &hits, agents(), None, 0);
        assert!(a.suggestions.is_empty());
        assert!(!a.checks.enabled);
    }

    /// REQ-DSCR-008.A2: bytes a clean already removed are never offered.
    #[test]
    fn suggestions_leave_out_hits_gone_since_the_scan() {
        let tmp = tempfile::tempdir().unwrap();
        let kept = tmp.path().join("a/node_modules");
        std::fs::create_dir_all(&kept).unwrap();
        let gone = tmp.path().join("b/node_modules");
        let hits = vec![
            present_hit("node_modules", &kept, 2_000_000_000, Safety::Review),
            present_hit("node_modules", &gone, 5_000_000_000, Safety::Review),
        ];
        let prefs = Preferences::default();
        let v = view_with(&hits, &prefs, 100_000_000_000);
        let s = suggestions_for(&v, &hits, None, 0);
        let dev = s.iter().find(|s| s.key == SuggestionKey::Dev).unwrap();
        assert_eq!(dev.bytes, Some(2_000_000_000));
    }

    #[test]
    fn a_path_gone_since_the_scan_is_explained() {
        let dir = std::env::temp_dir().join(format!("disky-ondisk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(check_on_disk(dir.to_str().unwrap()).is_ok());
        let gone = dir.join("blobs");
        let err = check_on_disk(&crate::platform::to_slash(&gone)).unwrap_err();
        let e = err.downcast_ref::<crate::exit::DiskyError>().unwrap();
        assert_eq!(e.code, crate::exit::ExitCode::NotFound);
        assert_eq!(
            e.detail,
            "blobs was removed after the last scan. Scan again to refresh the map."
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
