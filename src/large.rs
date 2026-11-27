//! Large files (REQ-DSCR-004): regular files of at least 2 GB physical,
//! largest first, each with its last-opened (access) time.
//!
//! `disky large` and the GUI's Large files screen read the same [`large`]
//! call: one query over the snapshot. Excluded folders and
//! skip-recently-used scope it exactly as they scope the Smart Clean
//! "Large files" shortcut count (REQ-DSCR-002), so the screen's "All" count
//! and the shortcut always agree.
//!
//! @implements REQ-DSCR-004

use anyhow::Result;
use duckdb::types::Value;
use duckdb::Connection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::cleanup::ScanContext;

/// A file at least this large (physical bytes) is listed: 2 GB, decimal —
/// the same threshold as the Smart Clean shortcut.
pub const MIN_BYTES: u64 = crate::contract::LARGE_FILE_BYTES;

/// The `over-10gb` threshold: 10 GB, decimal.
pub const OVER_10GB_BYTES: u64 = 10_000_000_000;

/// A file last opened more than this many days ago is `unused_1y`.
pub const UNUSED_DAYS: i64 = 365;

/// Which large files to list (REQ-DSCR-004.A2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum LargeFilter {
    /// Every file of at least 2 GB.
    #[default]
    All,
    /// Files of at least 10 GB.
    #[serde(rename = "over-10gb")]
    #[value(name = "over-10gb")]
    Over10Gb,
    /// Files last opened more than 365 days ago.
    #[serde(rename = "unused-1y")]
    #[value(name = "unused-1y")]
    Unused1y,
}

impl LargeFilter {
    pub fn as_str(self) -> &'static str {
        match self {
            LargeFilter::All => "all",
            LargeFilter::Over10Gb => "over-10gb",
            LargeFilter::Unused1y => "unused-1y",
        }
    }
}

impl FromStr for LargeFilter {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "all" => Ok(LargeFilter::All),
            "over-10gb" => Ok(LargeFilter::Over10Gb),
            "unused-1y" => Ok(LargeFilter::Unused1y),
            other => anyhow::bail!("unknown filter `{other}` (all | over-10gb | unused-1y)"),
        }
    }
}

/// One listed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargeFile {
    /// Basename.
    pub name: String,
    /// Absolute path.
    pub path: String,
    /// The parent folder for display, with the home directory as `~`
    /// (REQ-DSCR-013.A2).
    pub location: String,
    /// Physical bytes.
    pub bytes: u64,
    /// Last access time (RFC 3339 UTC), or null when the snapshot predates
    /// access times (REQ-DSCR-004.A3).
    pub last_opened: Option<String>,
    /// True when `last_opened` is more than [`UNUSED_DAYS`] ago.
    pub unused_1y: bool,
}

/// What `disky large` prints and the Large files screen renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargeView {
    pub filter: LargeFilter,
    /// Files in `records`.
    pub count: usize,
    /// Physical bytes over `records`.
    pub total_bytes: u64,
    /// Largest first (ties by path).
    pub records: Vec<LargeFile>,
}

/// True when the snapshot's `files` table has the `atime` column. Snapshots
/// written before it existed lack it (REQ-DSCR-004.A3).
pub fn has_atime(conn: &Connection) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_name = 'files' AND column_name = 'atime'",
        [],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// True when a file last accessed at `atime` counts as unused at `now`:
/// more than 365 days ago. Unknown → false.
pub fn is_unused(atime: Option<i64>, now: i64) -> bool {
    atime.is_some_and(|a| now - a > UNUSED_DAYS * 86_400)
}

/// Append ` AND …` conditions on `files.path` that leave out everything
/// inside `ctx.excluded` (whole path components; excluding `/` excludes
/// everything) — REQ-DSCR-002.A1. Shared by the Large files list, its
/// Smart Clean count and the duplicates finder, so they scope alike.
pub(crate) fn excluded_clause(ctx: &ScanContext, sql: &mut String, params: &mut Vec<Value>) {
    for e in &ctx.excluded {
        let e = e.trim_end_matches('/');
        if e.is_empty() {
            sql.push_str(" AND false");
            return;
        }
        sql.push_str(" AND NOT (path = ? OR (path >= ? AND path < ?))");
        params.push(Value::Text(e.to_string()));
        params.push(Value::Text(format!("{e}/")));
        params.push(Value::Text(format!("{e}0")));
    }
}

/// With skip-recently-used on, append ` AND mtime < cutoff`: a file
/// modified since the cutoff — or of unknown mtime, not provably unused —
/// is left out (REQ-DSCR-002.A2).
pub(crate) fn recent_clause(ctx: &ScanContext, sql: &mut String, params: &mut Vec<Value>) {
    if let Some(cutoff) = ctx.recent_cutoff {
        sql.push_str(" AND mtime < ?");
        params.push(Value::BigInt(cutoff));
    }
}

/// [`excluded_clause`] then [`recent_clause`]: the preferences' whole scope.
pub(crate) fn scope_clause(ctx: &ScanContext, sql: &mut String, params: &mut Vec<Value>) {
    excluded_clause(ctx, sql, params);
    recent_clause(ctx, sql, params);
}

/// The large files in the snapshot on `conn` for `filter`, scoped by `ctx`:
/// excluded folders are left out, and with skip-recently-used on so is any
/// file modified at or after `ctx.recent_cutoff` (or of unknown mtime)
/// (REQ-DSCR-002.A1, A2). `now` (unix seconds) dates `unused_1y`.
pub fn large(
    conn: &Connection,
    filter: LargeFilter,
    ctx: &ScanContext,
    now: i64,
) -> Result<LargeView> {
    let min = match filter {
        LargeFilter::Over10Gb => OVER_10GB_BYTES,
        _ => MIN_BYTES,
    };
    let atime = if has_atime(conn)? {
        "atime"
    } else {
        "NULL::BIGINT"
    };
    let mut sql = format!(
        "SELECT name, path, physical_size, {atime} AS atime FROM files
         WHERE NOT is_dir AND physical_size >= ?"
    );
    let mut params: Vec<Value> = vec![Value::BigInt(min as i64)];
    scope_clause(ctx, &mut sql, &mut params);
    sql.push_str(" ORDER BY physical_size DESC, path");

    let refs: Vec<&dyn duckdb::ToSql> = params.iter().map(|v| v as &dyn duckdb::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(refs.as_slice(), |r| {
            let atime: Option<i64> = r.get(3)?;
            let path: String = r.get(1)?;
            let parent = std::path::Path::new(&path)
                .parent()
                .unwrap_or(std::path::Path::new("/"));
            Ok(LargeFile {
                name: r.get(0)?,
                location: crate::prefs::abbreviate_home(parent, ctx.home.as_deref()),
                path,
                bytes: r.get::<_, i64>(2)?.max(0) as u64,
                last_opened: crate::query::rfc3339(atime),
                unused_1y: is_unused(atime, now),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let records: Vec<LargeFile> = match filter {
        LargeFilter::Unused1y => rows.into_iter().filter(|f| f.unused_1y).collect(),
        _ => rows,
    };
    Ok(LargeView {
        filter,
        count: records.len(),
        total_bytes: records.iter().map(|f| f.bytes).sum(),
        records,
    })
}

/// True while `path` exists (a symlink counts as itself).
pub(crate) fn still_on_disk(path: &str) -> bool {
    std::path::Path::new(path).symlink_metadata().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: i64 = 1_000_000_000;
    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    /// `(path, is_dir, physical, mtime, atime)` rows.
    fn snapshot(rows: &[(&str, bool, i64, i64, Option<i64>)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        for (p, d, b, m, a) in rows {
            let name = p.rsplit('/').next().unwrap_or(p);
            conn.execute(
                "INSERT INTO files (path, name, ext, size, physical_size, mtime, is_dir, depth, atime)
                 VALUES (?, ?, NULL, ?, ?, ?, ?, 0, ?)",
                duckdb::params![p, name, b * 3, if *d { None } else { Some(*b) }, m, d, a],
            )
            .unwrap();
        }
        conn
    }

    fn sample() -> Connection {
        let old = NOW - 400 * DAY;
        snapshot(&[
            ("/u/vm.img", false, 40 * GB, old, Some(NOW - 10 * DAY)),
            ("/u/old.iso", false, 12 * GB, old, Some(NOW - 500 * DAY)),
            ("/u/movie.mov", false, 3 * GB, old, Some(NOW - 366 * DAY)),
            ("/u/edge.bin", false, 2 * GB, old, Some(NOW - 365 * DAY)),
            (
                "/u/small.bin",
                false,
                2 * GB - 1,
                old,
                Some(NOW - 900 * DAY),
            ),
            ("/u/dir", true, 0, old, None),
            ("/u/null.dmg", false, 5 * GB, old, None),
        ])
    }

    fn paths(v: &LargeView) -> Vec<&str> {
        v.records.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn large_lists_files_of_2gb_and_up_largest_first() {
        let v = large(&sample(), LargeFilter::All, &ScanContext::default(), NOW).unwrap();
        assert_eq!(
            paths(&v),
            [
                "/u/vm.img",
                "/u/old.iso",
                "/u/null.dmg",
                "/u/movie.mov",
                "/u/edge.bin"
            ],
            "under 2 GB and directories are left out"
        );
        assert_eq!(v.count, 5);
        assert_eq!(v.total_bytes, (40 + 12 + 5 + 3 + 2) as u64 * GB as u64);
        assert_eq!(v.records[0].name, "vm.img");
        assert_eq!(v.records[0].location, "/u");
        assert_eq!(v.records[0].bytes, 40 * GB as u64, "physical, not logical");
        assert_eq!(
            v.records[0].last_opened.as_deref(),
            crate::query::rfc3339(Some(NOW - 10 * DAY)).as_deref()
        );
    }

    #[test]
    fn large_filters_over_10gb_and_unused_1y() {
        let ctx = ScanContext::default();
        let big = large(&sample(), LargeFilter::Over10Gb, &ctx, NOW).unwrap();
        assert_eq!(paths(&big), ["/u/vm.img", "/u/old.iso"]);
        assert_eq!((big.count, big.total_bytes), (2, 52 * GB as u64));
        let unused = large(&sample(), LargeFilter::Unused1y, &ctx, NOW).unwrap();
        assert_eq!(paths(&unused), ["/u/old.iso", "/u/movie.mov"]);
        assert_eq!((unused.count, unused.total_bytes), (2, 15 * GB as u64));
        assert!(unused.records.iter().all(|f| f.unused_1y));
    }

    #[test]
    fn unused_1y_is_strictly_more_than_365_days() {
        assert!(!is_unused(Some(NOW - 365 * DAY), NOW));
        assert!(is_unused(Some(NOW - 366 * DAY), NOW));
        assert!(!is_unused(None, NOW), "unknown access time is not unused");
        let v = large(&sample(), LargeFilter::All, &ScanContext::default(), NOW).unwrap();
        let null = v.records.iter().find(|f| f.name == "null.dmg").unwrap();
        assert_eq!((null.last_opened.as_deref(), null.unused_1y), (None, false));
    }

    #[test]
    fn large_honours_exclusions_and_skip_recently_used() {
        let conn = snapshot(&[
            ("/u/keep/a.img", false, 3 * GB, NOW - 90 * DAY, None),
            ("/u/keepx/b.img", false, 3 * GB, NOW - 90 * DAY, None),
            ("/u/fresh.img", false, 3 * GB, NOW - DAY, None),
            ("/u/nomtime.img", false, 3 * GB, 0, None),
        ]);
        conn.execute(
            "UPDATE files SET mtime = NULL WHERE name = 'nomtime.img'",
            [],
        )
        .unwrap();
        let ctx = ScanContext {
            excluded: vec!["/u/keep/".into()],
            recent_cutoff: Some(NOW - 30 * DAY),
            ..Default::default()
        };
        let v = large(&conn, LargeFilter::All, &ctx, NOW).unwrap();
        assert_eq!(paths(&v), ["/u/keepx/b.img"], "components, not prefixes");
        let home = ScanContext {
            home: Some("/u".into()),
            ..ctx.clone()
        };
        let v = large(&conn, LargeFilter::All, &home, NOW).unwrap();
        assert_eq!(v.records[0].location, "~/keepx", "home shown as ~");
        // The view and the Smart Clean shortcut count agree.
        assert_eq!(
            v.count as u64,
            crate::contract::large_files_count_in(&conn, &ctx).unwrap()
        );
    }

    #[test]
    fn snapshot_without_atime_lists_null_last_opened() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE files (path TEXT, name TEXT, ext TEXT, size BIGINT,
                 physical_size BIGINT, mtime BIGINT, is_dir BOOLEAN, depth INTEGER);
             INSERT INTO files VALUES ('/u/a.img', 'a.img', 'img', 1, 3000000000, 0, false, 1);",
        )
        .unwrap();
        assert!(!has_atime(&conn).unwrap());
        let v = large(&conn, LargeFilter::All, &ScanContext::default(), NOW).unwrap();
        assert_eq!(v.count, 1);
        assert_eq!(v.records[0].last_opened, None);
        assert!(!v.records[0].unused_1y);
        let u = large(&conn, LargeFilter::Unused1y, &ScanContext::default(), NOW).unwrap();
        assert_eq!(u.count, 0);
    }

    #[test]
    fn filter_parses_and_serialises_kebab_case() {
        for f in [
            LargeFilter::All,
            LargeFilter::Over10Gb,
            LargeFilter::Unused1y,
        ] {
            assert_eq!(f.as_str().parse::<LargeFilter>().unwrap(), f);
            assert_eq!(serde_json::to_value(f).unwrap(), f.as_str());
        }
        assert!("huge".parse::<LargeFilter>().is_err());
    }
}
