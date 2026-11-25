//! The Assistant's suggestions (REQ-DSCR-008), built from the latest
//! snapshot's cleanup view, the live disk and the preferences — never by the
//! GUI, and never by an agent CLI (DISK-SCREENS-DOC non-goals).
//!
//! In feed order (A1):
//!
//! | key    | kind       | when                                         |
//! |--------|------------|----------------------------------------------|
//! | `goal` | Goal       | free space is below the goal                 |
//! | `ok`   | On track   | free space is at or above the goal           |
//! | `dev`  | Suggestion | the developer (folder-name) categories have hits |
//! | `dl`   | Suggestion | `old_downloads` has hits                     |
//! | `left` | Heads-up   | `app_leftovers` has hits                     |
//!
//! The text follows the design snapshot's sentences, with every figure taken
//! from the snapshot or the disk (A2). A category with no hits produces no
//! suggestion; with no snapshot only `goal` or `ok` is returned, and `goal`
//! then offers nothing (A3). Category ids come from [`CATEGORIES`]; nothing
//! here defines a category.
//!
//! `disky suggest --format json` prints these records whatever
//! `checks.enabled` says (the CLI is diagnostic); the GUI's
//! [`crate::contract::assistant_view`] empties the feed when checks are off
//! (REQ-DSCR-009.A4).
//!
//! The proactive-check and scheduled-scan periods (REQ-DSCR-011) live here
//! too, so the GUI's scheduler only drives a clock.
//!
//! @implements REQ-DSCR-008
//! @implements REQ-DSCR-011

use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::cleanup::{CleanupHit, Safety, CATEGORIES, OLD_DOWNLOADS_DAYS};
use crate::contract::{CategoryView, CleanupView};
use crate::prefs::{Frequency, Schedule};

/// Category id of old downloads (the `dl` suggestion).
pub const DOWNLOADS_CATEGORY: &str = "old_downloads";
/// Category id of app leftovers (the `left` suggestion).
pub const LEFTOVERS_CATEGORY: &str = "app_leftovers";

/// Which suggestion a record is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKey {
    Goal,
    Ok,
    Dev,
    Dl,
    Left,
}

impl SuggestionKey {
    /// The kind tag the card shows (A1).
    pub fn kind(self) -> &'static str {
        match self {
            SuggestionKey::Goal => "Goal",
            SuggestionKey::Ok => "On track",
            SuggestionKey::Dev | SuggestionKey::Dl => "Suggestion",
            SuggestionKey::Left => "Heads-up",
        }
    }
}

/// One Assistant suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub key: SuggestionKey,
    /// "Goal", "On track", "Suggestion" or "Heads-up".
    pub kind: String,
    pub text: String,
    /// Physical bytes the suggestion would clear; null when it offers none.
    pub bytes: Option<u64>,
    /// Engine category ids the review opens with selected, in table order.
    pub categories: Vec<String>,
    /// True when the card has a review action.
    pub actionable: bool,
    /// When its figures were taken (RFC 3339 UTC): now for `goal` / `ok`
    /// (the live disk), the snapshot's scan for the others.
    pub at: Option<String>,
}

/// Size formatting from exact bytes, as the GUI's `formatBytes`
/// (REQ-DCLEAN-028): decimal units; at or above 1 GB one decimal below
/// 100 GB (a trailing ".0" dropped) and none from 100 GB; below 1 GB whole
/// MB, at least "1 MB" for any non-zero value.
pub fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1e9;
    const MB: f64 = 1e6;
    let b = bytes as f64;
    if b >= GB {
        let gb = b / GB;
        if gb >= 100.0 {
            return format!("{} GB", gb.round() as u64);
        }
        let r = (gb * 10.0).round() / 10.0;
        return if r.fract() == 0.0 {
            format!("{} GB", r as u64)
        } else {
            format!("{r:.1} GB")
        };
    }
    if bytes == 0 {
        return "0 MB".to_string();
    }
    format!("{} MB", ((b / MB).round() as u64).max(1))
}

/// "a", "a and b", "a, b and c".
fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

/// A count as the snapshot words it at the start of a sentence ("Four").
fn count_word(n: usize) -> String {
    const WORDS: [&str; 10] = [
        "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
    ];
    WORDS
        .get(n.wrapping_sub(1))
        .map_or_else(|| n.to_string(), |w| w.to_string())
}

fn basename(path: &str) -> String {
    let t = path.trim_end_matches('/');
    t.rsplit('/').next().unwrap_or(t).to_string()
}

fn rfc3339(secs: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(secs, 0).map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// The longest folder every path in `dirs` sits in.
fn common_folder(dirs: &[PathBuf]) -> Option<PathBuf> {
    let mut iter = dirs.iter();
    let mut common: Vec<_> = iter.next()?.components().collect();
    for d in iter {
        let n = common
            .iter()
            .zip(d.components())
            .take_while(|(a, b)| *a == b)
            .count();
        common.truncate(n);
    }
    let p: PathBuf = common.iter().collect();
    (p.components().count() > 1).then_some(p)
}

/// "March", or "March 2024" when not this year.
fn month_label(secs: i64, now_secs: i64) -> Option<String> {
    let t = DateTime::<Utc>::from_timestamp(secs, 0)?;
    let now = DateTime::<Utc>::from_timestamp(now_secs, 0)?;
    Some(if t.year() == now.year() {
        t.format("%B").to_string()
    } else {
        t.format("%B %Y").to_string()
    })
}

/// Up to two names, largest first, then "and <n> more".
fn some_names(hits: &[&CleanupHit], more_word: &str) -> String {
    let mut sorted: Vec<&&CleanupHit> = hits.iter().collect();
    sorted.sort_by_key(|h| std::cmp::Reverse(h.physical_bytes));
    let names: Vec<String> = sorted.iter().take(2).map(|h| basename(&h.path)).collect();
    let rest = hits.len().saturating_sub(2);
    if rest == 0 {
        join_and(&names)
    } else {
        format!("{}, {} and {rest} {more_word}", names[0], names[1])
    }
}

fn hits_of<'a>(hits: &'a [CleanupHit], ids: &[&str]) -> Vec<&'a CleanupHit> {
    hits.iter()
        .filter(|h| ids.contains(&h.category.as_str()))
        .collect()
}

/// Ids of `cats` that the view lists with bytes, in table order.
fn listed_ids(view: &CleanupView, keep: impl Fn(&CategoryView) -> bool) -> Vec<String> {
    view.categories
        .iter()
        .filter(|c| c.bytes > 0 && keep(c))
        .map(|c| c.id.clone())
        .collect()
}

fn total(view: &CleanupView, ids: &[String]) -> u64 {
    view.categories
        .iter()
        .filter(|c| ids.contains(&c.id))
        .map(|c| c.bytes)
        .sum()
}

fn goal_or_ok(view: &CleanupView, now: Option<String>) -> Suggestion {
    let free = view.disk.free_bytes;
    let goal = view.goal_bytes;
    if free >= goal {
        let freq = match view.preferences.checks.frequency {
            Frequency::Hourly => "hourly",
            Frequency::Daily => "daily",
            Frequency::Weekly => "weekly",
        };
        return Suggestion {
            key: SuggestionKey::Ok,
            kind: SuggestionKey::Ok.kind().into(),
            text: format!(
                "You’re at {} free, above your {} goal. I’ll keep checking {freq}.",
                format_bytes(free),
                format_bytes(goal)
            ),
            bytes: None,
            categories: Vec::new(),
            actionable: false,
            at: now,
        };
    }
    let lead = format!(
        "Free space is down to {}, under your {} goal.",
        format_bytes(free),
        format_bytes(goal)
    );
    let safe = listed_ids(view, |c| c.safety == Safety::Safe);
    let safe_bytes = total(view, &safe);
    // A3: with no snapshot there is nothing to offer (gap-fill wording).
    let (text, bytes, categories) = if view.last_scan_at.is_none() {
        (
            format!("{lead} Run a scan and I’ll look for safe categories to clean."),
            None,
            Vec::new(),
        )
    } else if safe_bytes == 0 {
        (
            format!("{lead} I found nothing in the safe categories to clean right now."),
            None,
            Vec::new(),
        )
    } else {
        (
            format!(
                "{lead} I found {} in safe categories: caches, temp files, logs and the Trash. Shall I clean them?",
                format_bytes(safe_bytes)
            ),
            Some(safe_bytes),
            safe,
        )
    };
    Suggestion {
        key: SuggestionKey::Goal,
        kind: SuggestionKey::Goal.kind().into(),
        text,
        actionable: bytes.is_some(),
        bytes,
        categories,
        at: now,
    }
}

fn dev(
    view: &CleanupView,
    hits: &[CleanupHit],
    home: Option<&Path>,
    now_secs: i64,
) -> Option<Suggestion> {
    let dev_ids: Vec<&str> = CATEGORIES
        .iter()
        .filter(|c| c.is_basename())
        .map(|c| c.name)
        .collect();
    let ids = listed_ids(view, |c| dev_ids.contains(&c.id.as_str()));
    if ids.is_empty() {
        return None;
    }
    let dev_hits = hits_of(hits, &dev_ids);
    // A project is the folder holding a hit (`~/Projects/atlas` for
    // `~/Projects/atlas/node_modules`).
    let mut projects: Vec<PathBuf> = dev_hits
        .iter()
        .filter_map(|h| Path::new(&h.path).parent().map(Path::to_path_buf))
        .collect();
    projects.sort();
    projects.dedup();
    let n = projects.len().max(1);
    let parents: Vec<PathBuf> = projects
        .iter()
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect();
    let place = common_folder(&parents)
        .map(|f| format!(" in {}", crate::prefs::abbreviate_home(&f, home)))
        .unwrap_or_default();
    let since = if dev_hits.iter().all(|h| h.newest_mtime.is_some()) {
        dev_hits
            .iter()
            .filter_map(|h| h.newest_mtime)
            .max()
            .and_then(|m| month_label(m, now_secs))
    } else {
        None
    };
    let labels: Vec<String> = ids
        .iter()
        .filter_map(|id| CATEGORIES.iter().find(|c| c.name == id))
        .map(|c| c.label.to_string())
        .collect();
    let reinstall = ids.iter().any(|id| id == "node_modules" || id == "venv");
    let again = if reinstall { "reinstalled" } else { "rebuilt" };
    let bytes = total(view, &ids);
    let (subject, verb, their) = if n == 1 {
        ("project".to_string(), "hasn’t", "Its")
    } else {
        ("projects".to_string(), "haven’t", "Their")
    };
    let text = match since {
        Some(month) => format!(
            "{} {subject}{place} {verb} been opened since {month}. {their} {} take {} and can be {again} any time.",
            count_word(n),
            join_and(&labels),
            format_bytes(bytes)
        ),
        None => format!(
            "{} {subject}{place} {} {} that take {} and can be {again} any time.",
            count_word(n),
            if n == 1 { "has" } else { "have" },
            join_and(&labels),
            format_bytes(bytes)
        ),
    };
    Some(Suggestion {
        key: SuggestionKey::Dev,
        kind: SuggestionKey::Dev.kind().into(),
        text,
        bytes: Some(bytes),
        categories: ids,
        actionable: true,
        at: view.last_scan_at.clone(),
    })
}

fn downloads(view: &CleanupView, hits: &[CleanupHit]) -> Option<Suggestion> {
    let ids = listed_ids(view, |c| c.id == DOWNLOADS_CATEGORY);
    if ids.is_empty() {
        return None;
    }
    let dl = hits_of(hits, &[DOWNLOADS_CATEGORY]);
    let bytes = total(view, &ids);
    let months = OLD_DOWNLOADS_DAYS / 30;
    let items = match dl.len() {
        0 | 1 => "an item".to_string(),
        n => format!("{n} items"),
    };
    let names = some_names(&dl, "more");
    let names = if names.is_empty() {
        String::new()
    } else {
        format!(" ({names})")
    };
    Some(Suggestion {
        key: SuggestionKey::Dl,
        kind: SuggestionKey::Dl.kind().into(),
        text: format!(
            "Downloads has {items} you haven’t opened in {months} months{names}, taking {}.",
            format_bytes(bytes)
        ),
        bytes: Some(bytes),
        categories: ids,
        actionable: true,
        at: view.last_scan_at.clone(),
    })
}

fn leftovers(view: &CleanupView, hits: &[CleanupHit]) -> Option<Suggestion> {
    let ids = listed_ids(view, |c| c.id == LEFTOVERS_CATEGORY);
    if ids.is_empty() {
        return None;
    }
    let left = hits_of(hits, &[LEFTOVERS_CATEGORY]);
    let bytes = total(view, &ids);
    let names = some_names(&left, "other apps");
    let (who, verb) = match left.len() {
        0 => ("Apps you removed".to_string(), "were"),
        1 => (names, "was"),
        _ => (names, "were"),
    };
    Some(Suggestion {
        key: SuggestionKey::Left,
        kind: SuggestionKey::Left.kind().into(),
        text: format!(
            "{who} {verb} uninstalled but left {} of support files in ~/Library.",
            format_bytes(bytes)
        ),
        bytes: Some(bytes),
        categories: ids,
        actionable: true,
        at: view.last_scan_at.clone(),
    })
}

/// The suggestions for `view` (and the `hits` behind it) at `now_secs`, in
/// feed order (A1). `home` shows folders as `~/…`. Pure: every figure comes
/// from its arguments (A2).
pub fn suggestions(
    view: &CleanupView,
    hits: &[CleanupHit],
    home: Option<&Path>,
    now_secs: i64,
) -> Vec<Suggestion> {
    let mut out = vec![goal_or_ok(view, rfc3339(now_secs))];
    // A3: with no snapshot, only goal or ok.
    if view.last_scan_at.is_none() {
        return out;
    }
    out.extend(dev(view, hits, home, now_secs));
    out.extend(downloads(view, hits));
    out.extend(leftovers(view, hits));
    out
}

/// Seconds between two proactive checks (REQ-DSCR-011.A1).
pub fn check_period_secs(f: Frequency) -> i64 {
    match f {
        Frequency::Hourly => 3_600,
        Frequency::Daily => 86_400,
        Frequency::Weekly => 7 * 86_400,
    }
}

/// Age after which a scheduled scan is due; `None` for `off`
/// (REQ-DSCR-011.A2). A month is 30 days.
pub fn scan_period_secs(s: Schedule) -> Option<i64> {
    match s {
        Schedule::Off => None,
        Schedule::Weekly => Some(7 * 86_400),
        Schedule::Monthly => Some(30 * 86_400),
    }
}

/// A scheduled scan is due when the schedule is on and the latest snapshot
/// is older than its period, or there is none (REQ-DSCR-011.A2).
pub fn scan_due(s: Schedule, latest_scan_secs: Option<i64>, now_secs: i64) -> bool {
    match (scan_period_secs(s), latest_scan_secs) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(period), Some(t)) => now_secs - t > period,
    }
}

/// When the latest snapshot's scan started (unix seconds), or `None` with no
/// snapshot. Read from its id (`YYYY-MM-DD_HH-MM`), else from its
/// `scan_meta` (a renamed snapshot).
pub fn latest_scan_secs() -> Option<i64> {
    let path = crate::snapshots::latest_snapshot()?;
    if let Some(t) = crate::snapshots::id_for(&path)
        .as_deref()
        .and_then(crate::snapshots::parse_id)
    {
        return Some(t.timestamp());
    }
    let conn = crate::db::open_read(&path).ok()?;
    crate::db::read_scan_meta(&conn).map(|m| m.started_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{categories_from_hits, Disk, Preferences};

    const GB: u64 = 1_000_000_000;
    /// 2026-11-25T00:00:00Z.
    const NOW: i64 = 1_795_564_800;

    fn hit(category: &str, path: &str, gb: f64, safety: Safety, mtime: Option<i64>) -> CleanupHit {
        let b = (gb * GB as f64).round() as u64;
        CleanupHit {
            category: category.into(),
            safety,
            path: path.into(),
            bytes: b,
            physical_bytes: b,
            files: 1,
            note: None,
            newest_mtime: mtime,
        }
    }

    fn view(free_gb: u64, hits: &[CleanupHit], scanned: bool) -> CleanupView {
        let prefs = Preferences::default();
        let mut v = crate::contract::cleanup_view(
            None,
            hits,
            Disk {
                volume: "Macintosh HD".into(),
                capacity_bytes: 512 * GB,
                free_bytes: free_gb * GB,
            },
            &prefs,
            0,
            NOW,
        );
        v.categories = categories_from_hits(hits);
        if scanned {
            v.last_scan_at = Some("2026-11-24T22:00:00Z".into());
        }
        v
    }

    /// 2026-03-10.
    const MARCH: i64 = 1_773_100_800;

    /// The design snapshot's categories, in bytes.
    fn snapshot_hits() -> Vec<CleanupHit> {
        vec![
            hit(
                "system_caches",
                "/Library/Caches/x",
                4.2,
                Safety::Safe,
                None,
            ),
            hit(
                "app_caches",
                "/Users/me/Library/Caches/c",
                3.1,
                Safety::Safe,
                None,
            ),
            hit("trash", "/Users/me/.Trash/t", 2.4, Safety::Safe, None),
            hit(
                "temp",
                "/private/var/folders/a/b/T/x",
                1.8,
                Safety::Review,
                None,
            ),
            hit(
                "node_modules",
                "/Users/me/Projects/atlas/node_modules",
                4.1,
                Safety::Review,
                Some(MARCH),
            ),
            hit(
                "node_modules",
                "/Users/me/Projects/beacon/node_modules",
                1.0,
                Safety::Review,
                Some(MARCH - 86_400),
            ),
            hit(
                "target",
                "/Users/me/Projects/comet/target",
                1.2,
                Safety::Review,
                Some(MARCH - 2 * 86_400),
            ),
            hit(
                "build",
                "/Users/me/Projects/delta/build",
                1.6,
                Safety::Review,
                Some(MARCH - 3 * 86_400),
            ),
            hit(
                "old_downloads",
                "/Users/me/Downloads/Xcode_15.dmg",
                3.2,
                Safety::Review,
                None,
            ),
            hit(
                "old_downloads",
                "/Users/me/Downloads/tax-scans-2024",
                1.7,
                Safety::Review,
                None,
            ),
            hit(
                "old_downloads",
                "/Users/me/Downloads/Figma-installer.zip",
                0.4,
                Safety::Review,
                None,
            ),
            hit(
                "app_leftovers",
                "/Users/me/Library/Application Support/Sketch",
                1.4,
                Safety::Review,
                None,
            ),
            hit(
                "app_leftovers",
                "/Users/me/Library/Application Support/zoom.us",
                0.8,
                Safety::Review,
                None,
            ),
        ]
    }

    fn keys(s: &[Suggestion]) -> Vec<SuggestionKey> {
        s.iter().map(|x| x.key).collect()
    }

    const HOME: &str = "/Users/me";

    #[test]
    fn goal_triggers_below_the_goal_and_offers_the_safe_categories() {
        let hits = snapshot_hits();
        let s = suggestions(&view(74, &hits, true), &hits, Some(Path::new(HOME)), NOW);
        assert_eq!(
            keys(&s),
            [
                SuggestionKey::Goal,
                SuggestionKey::Dev,
                SuggestionKey::Dl,
                SuggestionKey::Left
            ]
        );
        let g = &s[0];
        assert_eq!(g.kind, "Goal");
        assert_eq!(
            g.text,
            "Free space is down to 74 GB, under your 80 GB goal. I found 9.7 GB in safe categories: caches, temp files, logs and the Trash. Shall I clean them?"
        );
        assert_eq!(g.bytes, Some(9_700_000_000));
        assert_eq!(g.categories, ["system_caches", "app_caches", "trash"]);
        assert!(g.actionable);
        assert_eq!(g.at.as_deref(), Some("2026-11-25T00:00:00Z"));
    }

    #[test]
    fn ok_triggers_at_or_above_the_goal_with_no_action() {
        let hits = snapshot_hits();
        let s = suggestions(&view(80, &hits, true), &hits, None, NOW);
        let ok = &s[0];
        assert_eq!(ok.key, SuggestionKey::Ok);
        assert_eq!(ok.kind, "On track");
        assert_eq!(
            ok.text,
            "You’re at 80 GB free, above your 80 GB goal. I’ll keep checking daily."
        );
        assert_eq!(ok.bytes, None);
        assert!(ok.categories.is_empty());
        assert!(!ok.actionable);
    }

    #[test]
    fn dev_names_the_projects_their_folder_and_month() {
        let hits = snapshot_hits();
        let s = suggestions(&view(74, &hits, true), &hits, Some(Path::new(HOME)), NOW);
        let d = &s[1];
        assert_eq!(d.kind, "Suggestion");
        assert_eq!(
            d.text,
            "Four projects in ~/Projects haven’t been opened since March. Their node_modules, Cargo target dirs and build folders take 7.9 GB and can be reinstalled any time."
        );
        assert_eq!(d.bytes, Some(7_900_000_000));
        assert_eq!(d.categories, ["node_modules", "target", "build"]);
        assert_eq!(d.at.as_deref(), Some("2026-11-24T22:00:00Z"));
    }

    #[test]
    fn dev_without_known_age_or_shared_folder_still_states_real_figures() {
        // Projects under different top-level folders share no folder worth naming.
        let hits = vec![
            hit("target", "/x/comet/target", 0.7, Safety::Review, None),
            hit("target", "/y/nova/target", 0.5, Safety::Review, Some(MARCH)),
        ];
        let s = suggestions(&view(90, &hits, true), &hits, None, NOW);
        assert_eq!(
            s[1].text,
            "Two projects have Cargo target dirs that take 1.2 GB and can be rebuilt any time."
        );
        let one = vec![hit("target", "/x/comet/target", 1.2, Safety::Review, None)];
        let s = suggestions(&view(90, &one, true), &one, None, NOW);
        assert_eq!(
            s[1].text,
            "One project in /x has Cargo target dirs that take 1.2 GB and can be rebuilt any time."
        );
    }

    #[test]
    fn dl_lists_downloads_untouched_for_three_months() {
        let hits = snapshot_hits();
        let s = suggestions(&view(74, &hits, true), &hits, None, NOW);
        let d = &s[2];
        assert_eq!(d.key, SuggestionKey::Dl);
        assert_eq!(
            d.text,
            "Downloads has 3 items you haven’t opened in 3 months (Xcode_15.dmg, tax-scans-2024 and 1 more), taking 5.3 GB."
        );
        assert_eq!(d.categories, ["old_downloads"]);
        assert_eq!(d.bytes, Some(5_300_000_000));
    }

    #[test]
    fn left_names_the_uninstalled_apps() {
        let hits = snapshot_hits();
        let s = suggestions(&view(74, &hits, true), &hits, None, NOW);
        let l = &s[3];
        assert_eq!(l.key, SuggestionKey::Left);
        assert_eq!(l.kind, "Heads-up");
        assert_eq!(
            l.text,
            "Sketch and zoom.us were uninstalled but left 2.2 GB of support files in ~/Library."
        );
        assert_eq!(l.categories, ["app_leftovers"]);
    }

    #[test]
    fn a_category_with_no_hits_produces_no_suggestion() {
        let hits: Vec<CleanupHit> = snapshot_hits()
            .into_iter()
            .filter(|h| h.category != "old_downloads" && h.category != "app_leftovers")
            .collect();
        let s = suggestions(&view(74, &hits, true), &hits, None, NOW);
        assert_eq!(keys(&s), [SuggestionKey::Goal, SuggestionKey::Dev]);
        let none: Vec<CleanupHit> = Vec::new();
        let s = suggestions(&view(74, &none, true), &none, None, NOW);
        assert_eq!(keys(&s), [SuggestionKey::Goal]);
        // Nothing safe to offer: the goal card has no action.
        assert!(!s[0].actionable);
        assert_eq!(s[0].bytes, None);
    }

    #[test]
    fn no_snapshot_returns_only_goal_or_ok_and_goal_has_no_action() {
        let none: Vec<CleanupHit> = Vec::new();
        let s = suggestions(&view(74, &none, false), &none, None, NOW);
        assert_eq!(keys(&s), [SuggestionKey::Goal]);
        assert!(!s[0].actionable);
        assert!(s[0].categories.is_empty());
        assert_eq!(
            s[0].text,
            "Free space is down to 74 GB, under your 80 GB goal. Run a scan and I’ll look for safe categories to clean."
        );
        let s = suggestions(&view(100, &none, false), &none, None, NOW);
        assert_eq!(keys(&s), [SuggestionKey::Ok]);
        // Even with stale hits passed in, no snapshot means goal/ok only.
        let hits = snapshot_hits();
        let s = suggestions(&view(74, &hits, false), &hits, None, NOW);
        assert_eq!(keys(&s), [SuggestionKey::Goal]);
    }

    #[test]
    fn every_category_id_comes_from_the_engine_table() {
        let hits = snapshot_hits();
        for s in suggestions(&view(74, &hits, true), &hits, None, NOW) {
            for id in &s.categories {
                assert!(
                    CATEGORIES.iter().any(|c| c.name == id),
                    "{id} not in CATEGORIES"
                );
            }
        }
    }

    #[test]
    fn json_shape_uses_snake_case_keys() {
        let hits = snapshot_hits();
        let s = suggestions(&view(74, &hits, true), &hits, None, NOW);
        let v = serde_json::to_value(&s[0]).unwrap();
        assert_eq!(v["key"], "goal");
        for k in ["kind", "text", "bytes", "categories", "actionable", "at"] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        let v = serde_json::to_value(&s[2]).unwrap();
        assert_eq!(v["key"], "dl");
    }

    #[test]
    fn bytes_are_formatted_as_the_gui_does() {
        assert_eq!(format_bytes(0), "0 MB");
        assert_eq!(format_bytes(1), "1 MB");
        assert_eq!(format_bytes(184_000_000), "184 MB");
        assert_eq!(format_bytes(9_700_000_000), "9.7 GB");
        assert_eq!(format_bytes(80_000_000_000), "80 GB");
        assert_eq!(format_bytes(99_960_000_000), "100 GB");
        assert_eq!(format_bytes(438_400_000_000), "438 GB");
    }

    #[test]
    fn check_and_scan_periods_follow_the_preferences() {
        assert_eq!(check_period_secs(Frequency::Hourly), 3_600);
        assert_eq!(check_period_secs(Frequency::Daily), 86_400);
        assert_eq!(check_period_secs(Frequency::Weekly), 604_800);
        assert!(!scan_due(Schedule::Off, None, NOW));
        assert!(scan_due(Schedule::Weekly, None, NOW));
        assert!(!scan_due(Schedule::Weekly, Some(NOW - 7 * 86_400), NOW));
        assert!(scan_due(Schedule::Weekly, Some(NOW - 7 * 86_400 - 1), NOW));
        assert!(!scan_due(Schedule::Monthly, Some(NOW - 29 * 86_400), NOW));
        assert!(scan_due(Schedule::Monthly, Some(NOW - 31 * 86_400), NOW));
    }
}
