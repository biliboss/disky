//! The desktop GUI's data contract (REQ-DCLEAN-027), built here in the
//! engine so the GUI never derives it itself (REQ-DCLEAN-001).
//!
//! * [`CleanupView`] — what the Smart Clean screen renders: disk usage, the
//!   goal, snapshot age and the reclaimable categories with their items.
//! * [`ScanProgress`] — one progress event while a scan runs.
//! * [`CleanResult`] — what a clean did, with its history entry.
//!
//! Every byte value is a `u64` of **physical** bytes (REQ-DCLEAN-027.A1).
//! Category ids, names, descriptions and safety come from
//! [`cleanup::CATEGORIES`] (REQ-DCLEAN-027.A4). A category's `bytes` is the
//! sum of its items' `bytes` by construction (REQ-DCLEAN-027.A3).
//!
//! @implements REQ-DCLEAN-027
//! @implements REQ-DCLEAN-029

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use duckdb::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::cleanup::{self, ApplyMode, ApplyOutcome, CleanupHit, Failure, Safety, CATEGORIES};
use crate::history;
use crate::scan::ScanTick;

pub use crate::disk::{disk_usage, Disk};

/// Space goal used until a Settings screen stores one: the design
/// snapshot's value, 80 GB.
pub const DEFAULT_GOAL_BYTES: u64 = 80_000_000_000;

/// A snapshot older than this is stale (REQ-DCLEAN-029.A2): 24 hours.
pub const STALE_AFTER_SECS: i64 = 24 * 60 * 60;

/// Most hits a view lists. Generous: the GUI scrolls.
pub const VIEW_LIMIT: usize = 10_000;

/// Reason given for a path the user did not review in the current view.
pub const FAILURE_NOT_REVIEWED: &str = "not reviewed";

/// Event channel the GUI backend emits [`ScanProgress`] on.
pub const SCAN_PROGRESS_EVENT: &str = "scan://progress";

/// Settings the component reads. Nothing stores them yet, so every value is
/// its spec default (all on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    pub trash_by_default: bool,
    pub safe_only: bool,
    pub notifications: bool,
    pub assistant: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            trash_by_default: true,
            safe_only: true,
            notifications: true,
            assistant: true,
        }
    }
}

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
    /// `review` if any item is `review`, as in the CLI summary.
    pub safety: Safety,
    /// Sum of `items[].bytes`.
    pub bytes: u64,
    pub items: Vec<ItemView>,
}

/// Everything the Smart Clean screen renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanupView {
    pub disk: Disk,
    pub goal_bytes: u64,
    /// When the snapshot's scan finished (RFC 3339 UTC), or null.
    pub last_scan_at: Option<String>,
    /// When the snapshot's scan started — the moment it reflects.
    pub snapshot_taken_at: Option<String>,
    /// True when the snapshot is older than [`STALE_AFTER_SECS`], or its age
    /// is unknown (fail safe: warn).
    pub is_stale: bool,
    pub preferences: Preferences,
    pub categories: Vec<CategoryView>,
}

/// One scan progress event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanProgress {
    /// 0..=1.
    pub fraction: f64,
    pub current_path: String,
    pub completed_category_ids: Vec<String>,
}

/// What a clean did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanResult {
    pub mode: history::Mode,
    /// Physical bytes of the items that were handled.
    pub freed_bytes: u64,
    /// Paths that were trashed or deleted.
    pub succeeded: Vec<String>,
    pub failed: Vec<Failure>,
    /// `None` when nothing was handled (no entry is written).
    pub history_entry: Option<history::Entry>,
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
    goal_bytes: u64,
    preferences: Preferences,
    now_secs: i64,
) -> CleanupView {
    let taken = meta.map(|m| m.started_at);
    CleanupView {
        disk,
        goal_bytes,
        last_scan_at: meta
            .map(|m| m.ended_at.unwrap_or(m.started_at))
            .and_then(rfc3339),
        snapshot_taken_at: taken.and_then(rfc3339),
        is_stale: is_stale(taken, now_secs),
        preferences,
        categories: categories_from_hits(hits),
    }
}

/// Open a snapshot and build its view, with disk usage taken from the
/// volume of the snapshot's scan root (`/` when the root is gone). Returns
/// the hits behind the view too: they are what the user reviews, and the
/// only paths [`clean`] will act on.
pub fn load(
    snapshot_path: &str,
    goal_bytes: u64,
    preferences: Preferences,
) -> Result<(CleanupView, Vec<CleanupHit>)> {
    let conn = crate::db::open(snapshot_path)?;
    let meta = crate::db::read_scan_meta(&conn);
    let root = meta.as_ref().map_or("/", |m| m.root.as_str());
    let disk = disk_usage(Path::new(root)).or_else(|_| disk_usage(Path::new("/")))?;
    let hits = view_hits(&conn)?;
    let now = Utc::now().timestamp();
    let view = cleanup_view(meta.as_ref(), &hits, disk, goal_bytes, preferences, now);
    Ok((view, hits))
}

/// Progress event for a running scan. The walk's total is unknown up front,
/// so `fraction` is physical bytes seen over the volume's used bytes, held
/// below 1 until [`scan_finished`]. Categories are classified after the
/// walk, so none are complete yet.
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
    }
}

/// Final progress event: every category the view lists is complete.
pub fn scan_finished(view: &CleanupView) -> ScanProgress {
    ScanProgress {
        fraction: 1.0,
        current_path: String::new(),
        completed_category_ids: view.categories.iter().map(|c| c.id.clone()).collect(),
    }
}

/// Keep only the hits the user reviewed. `paths` are the exact item paths
/// from the view; a path that is not in `reviewed` is refused, never
/// re-queried.
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
    let (selected, refused) = select_reviewed(reviewed, paths);
    let outcome = cleanup::apply(&selected, apply_mode(mode), Some(log))?;
    Ok(clean_result(mode, &selected, outcome, refused))
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
    let (selected, refused) = select_reviewed(reviewed, paths);
    let outcome = cleanup::apply_with(&selected, apply_mode(mode), Some(log), trash_root)?;
    Ok(clean_result(mode, &selected, outcome, refused))
}

fn clean_result(
    mode: history::Mode,
    selected: &[CleanupHit],
    outcome: ApplyOutcome,
    mut failed: Vec<Failure>,
) -> CleanResult {
    let freed_bytes = selected
        .iter()
        .filter(|h| outcome.handled.contains(&h.path))
        .map(|h| h.physical_bytes)
        .sum();
    failed.extend(outcome.failed);
    CleanResult {
        mode,
        freed_bytes,
        succeeded: outcome.handled,
        failed,
        history_entry: outcome.entry,
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
            DEFAULT_GOAL_BYTES,
            Preferences::default(),
            0,
        );
        let json = serde_json::to_value(&view).unwrap();
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
                1,
                Preferences::default(),
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
        let unknown = cleanup_view(None, &[], disk, 1, Preferences::default(), 0);
        assert!(unknown.is_stale && unknown.last_scan_at.is_none());
    }

    #[test]
    fn default_preferences_are_all_on() {
        let p = Preferences::default();
        assert!(p.trash_by_default && p.safe_only && p.notifications && p.assistant);
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
}
