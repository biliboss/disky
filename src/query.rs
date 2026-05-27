use anyhow::Result;
use chrono::{DateTime, Utc};
use duckdb::Connection;
use serde::Serialize;

/// JSON schema version emitted with every NDJSON / JSON payload. Bump on any
/// breaking change to record shape.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct FileRow {
    pub path: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtRow {
    pub ext: String,
    pub files: u64,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirRow {
    pub path: String,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub files: u64,
    pub dirs: u64,
    pub total_bytes: u64,
    pub largest_bytes: u64,
    pub avg_bytes: u64,
    /// True when the snapshot's last scan was cancelled before completing.
    /// Agents should treat the data as best-effort.
    pub partial: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_duration_s: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<String>,
}

fn rfc3339(mtime: Option<i64>) -> Option<String> {
    let secs = mtime?;
    let dt = DateTime::<Utc>::from_timestamp(secs, 0)?;
    Some(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

pub fn top_files(conn: &Connection, limit: usize, min_size: u64) -> Result<Vec<FileRow>> {
    top_files_inner(conn, limit, min_size, false)
}

/// Variant that orders + returns by physical_size when `physical = true`.
/// Falls back to logical size when physical_size column missing (older
/// snapshots).
pub fn top_files_physical(conn: &Connection, limit: usize, min_size: u64) -> Result<Vec<FileRow>> {
    top_files_inner(conn, limit, min_size, true)
}

fn top_files_inner(
    conn: &Connection,
    limit: usize,
    min_size: u64,
    physical: bool,
) -> Result<Vec<FileRow>> {
    let size_expr = if physical {
        "COALESCE(physical_size, size)"
    } else {
        "size"
    };
    let sql = format!(
        "SELECT path, {size_expr} AS size, ext, mtime FROM files
         WHERE is_dir = false AND {size_expr} >= ?
         ORDER BY {size_expr} DESC
         LIMIT ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(duckdb::params![min_size as i64, limit as i64], |row| {
        Ok(FileRow {
            path: row.get::<_, String>(0)?,
            size: row.get::<_, i64>(1)? as u64,
            ext: row.get::<_, Option<String>>(2)?,
            mtime: rfc3339(row.get::<_, Option<i64>>(3)?),
        })
    })?;
    Ok(rows.flatten().collect())
}

pub fn by_extension(conn: &Connection, limit: usize) -> Result<Vec<ExtRow>> {
    by_extension_inner(conn, limit, false)
}

pub fn by_extension_physical(conn: &Connection, limit: usize) -> Result<Vec<ExtRow>> {
    by_extension_inner(conn, limit, true)
}

fn by_extension_inner(conn: &Connection, limit: usize, physical: bool) -> Result<Vec<ExtRow>> {
    let size_expr = if physical {
        "COALESCE(physical_size, size)"
    } else {
        "size"
    };
    let sql = format!(
        "SELECT COALESCE(ext, '(none)') as ext,
                COUNT(*) as count,
                SUM({size_expr}) as total_size
         FROM files
         WHERE is_dir = false
         GROUP BY ext
         ORDER BY total_size DESC
         LIMIT ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(duckdb::params![limit as i64], |row| {
        Ok(ExtRow {
            ext: row.get::<_, String>(0)?,
            files: row.get::<_, i64>(1)? as u64,
            total_size: row.get::<_, i64>(2)? as u64,
        })
    })?;
    Ok(rows.flatten().collect())
}

pub fn top_dirs(conn: &Connection, limit: usize) -> Result<Vec<DirRow>> {
    top_dirs_inner(conn, limit, false)
}

pub fn top_dirs_physical(conn: &Connection, limit: usize) -> Result<Vec<DirRow>> {
    top_dirs_inner(conn, limit, true)
}

fn top_dirs_inner(conn: &Connection, limit: usize, physical: bool) -> Result<Vec<DirRow>> {
    let size_expr = if physical {
        "COALESCE(physical_size, size)"
    } else {
        "size"
    };
    let sql = format!(
        "SELECT parent_path, SUM(size) as total FROM (
             SELECT regexp_replace(path, '/[^/]+$', '') as parent_path,
                    {size_expr} as size
             FROM files
             WHERE is_dir = false
         )
         GROUP BY parent_path
         ORDER BY total DESC
         LIMIT ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(duckdb::params![limit as i64], |row| {
        Ok(DirRow {
            path: row.get::<_, String>(0)?,
            total_size: row.get::<_, i64>(1)? as u64,
        })
    })?;
    Ok(rows.flatten().collect())
}

/// Files with `size = 0`. Empty files are usually placeholders, lockfiles,
/// or leftovers from interrupted writes — cheap to identify, easy to clean.
pub fn empty_files(conn: &Connection, limit: usize) -> Result<Vec<FileRow>> {
    let mut stmt = conn.prepare(
        "SELECT path, size, ext, mtime FROM files
         WHERE is_dir = false AND size = 0
         ORDER BY path
         LIMIT ?",
    )?;
    let rows = stmt.query_map(duckdb::params![limit as i64], |row| {
        Ok(FileRow {
            path: row.get::<_, String>(0)?,
            size: row.get::<_, i64>(1)? as u64,
            ext: row.get::<_, Option<String>>(2)?,
            mtime: rfc3339(row.get::<_, Option<i64>>(3)?),
        })
    })?;
    Ok(rows.flatten().collect())
}

/// Files older than `cutoff_unix_seconds`. Comparison is against the file's
/// `mtime`; files with NULL mtime are excluded.
///
/// Sanity floor: rows with `mtime <= EPOCH_NOISE_FLOOR` are also excluded.
/// Cargo packs registry crates with mtime=1; npm tarballs with mtime=0;
/// neither represents "old data the user could clean" — both flood the
/// top of an ORDER BY mtime ASC query with junk.
pub fn old_files(conn: &Connection, cutoff: i64, limit: usize) -> Result<Vec<FileRow>> {
    /// 2000-01-01 UTC. Anything before this on a real machine is a
    /// distributor's deterministic build-output timestamp, not real history.
    const EPOCH_NOISE_FLOOR: i64 = 946_684_800;
    let mut stmt = conn.prepare(
        "SELECT path, size, ext, mtime FROM files
         WHERE is_dir = false
           AND mtime IS NOT NULL
           AND mtime > ?
           AND mtime < ?
         ORDER BY mtime ASC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(
        duckdb::params![EPOCH_NOISE_FLOOR, cutoff, limit as i64],
        |row| {
            Ok(FileRow {
                path: row.get::<_, String>(0)?,
                size: row.get::<_, i64>(1)? as u64,
                ext: row.get::<_, Option<String>>(2)?,
                mtime: rfc3339(row.get::<_, Option<i64>>(3)?),
            })
        },
    )?;
    Ok(rows.flatten().collect())
}

/// Per-directory mtime-based churn within a single snapshot. Counts files
/// whose `mtime > cutoff_unix`, sums their bytes, and emits a churn score
/// = recent_bytes / total_bytes per parent dir. High score = active log
/// generator or hot working directory.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChurnRow {
    pub path: String,
    pub recent_files: u64,
    pub recent_bytes: u64,
    pub total_files: u64,
    pub total_bytes: u64,
    /// recent_bytes / total_bytes (0.0 .. 1.0). NaN-safe (0 when denom = 0).
    pub churn_score: f64,
}

pub fn churn(conn: &Connection, cutoff_unix: i64, limit: usize) -> Result<Vec<ChurnRow>> {
    let mut stmt = conn.prepare(
        "WITH dir_stats AS (
             SELECT regexp_replace(path, '/[^/]+$', '') AS d,
                    COUNT(*)                            AS total_files,
                    SUM(size)                           AS total_bytes,
                    COUNT(*) FILTER (WHERE mtime > ?)   AS recent_files,
                    COALESCE(SUM(size) FILTER (WHERE mtime > ?), 0) AS recent_bytes
             FROM files
             WHERE is_dir = false
             GROUP BY d
         )
         SELECT d, recent_files, recent_bytes, total_files, total_bytes,
                CASE WHEN total_bytes > 0
                     THEN CAST(recent_bytes AS DOUBLE) / CAST(total_bytes AS DOUBLE)
                     ELSE 0.0 END AS churn_score
         FROM dir_stats
         WHERE recent_files > 0
         ORDER BY recent_bytes DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(
        duckdb::params![cutoff_unix, cutoff_unix, limit as i64],
        |row| {
            Ok(ChurnRow {
                path: row.get::<_, String>(0)?,
                recent_files: row.get::<_, i64>(1)? as u64,
                recent_bytes: row.get::<_, i64>(2)? as u64,
                total_files: row.get::<_, i64>(3)? as u64,
                total_bytes: row.get::<_, i64>(4)? as u64,
                churn_score: row.get::<_, f64>(5)?,
            })
        },
    )?;
    Ok(rows.flatten().collect())
}

/// Per-directory growth between two snapshots. Aggregates file sizes by
/// direct parent path. Returns up to `limit` rows ordered by absolute Δ.
///
/// `days_between` is derived from `scan_meta.started_at` of each snapshot;
/// when missing, falls back to `1.0` so `rate_bytes_per_day == delta_bytes`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrowthRow {
    pub path: String,
    pub kind: &'static str, // "grew" | "shrank" | "added" | "removed"
    pub size_a: u64,
    pub size_b: u64,
    pub delta_bytes: i64,
    pub rate_bytes_per_day: f64,
    pub days_between: f64,
}

pub fn growth(snap_a: &str, snap_b: &str, limit: usize) -> Result<Vec<GrowthRow>> {
    use duckdb::Connection;
    let conn = Connection::open_in_memory()?;
    let attach_sql = format!(
        "ATTACH '{a}' AS db_a (READ_ONLY);\
         ATTACH '{b}' AS db_b (READ_ONLY);",
        a = snap_a.replace('\'', "''"),
        b = snap_b.replace('\'', "''"),
    );
    conn.execute_batch(&attach_sql)?;

    // Pull scan_meta timestamps for days_between calc. Missing → fall back to 1d.
    let secs_a: Option<i64> = conn
        .query_row("SELECT started_at FROM db_a.scan_meta", [], |r| r.get(0))
        .ok();
    let secs_b: Option<i64> = conn
        .query_row("SELECT started_at FROM db_b.scan_meta", [], |r| r.get(0))
        .ok();
    let days_between = match (secs_a, secs_b) {
        (Some(a), Some(b)) if b > a => (b - a) as f64 / 86400.0,
        _ => 1.0,
    };

    let mut stmt = conn.prepare(
        "WITH dir_a AS (
             SELECT regexp_replace(path, '/[^/]+$', '') AS d, SUM(size) AS s
             FROM db_a.files
             WHERE is_dir = false
             GROUP BY d
         ),
         dir_b AS (
             SELECT regexp_replace(path, '/[^/]+$', '') AS d, SUM(size) AS s
             FROM db_b.files
             WHERE is_dir = false
             GROUP BY d
         )
         SELECT COALESCE(dir_a.d, dir_b.d)         AS path,
                COALESCE(dir_a.s, 0)               AS size_a,
                COALESCE(dir_b.s, 0)               AS size_b,
                COALESCE(dir_b.s, 0) - COALESCE(dir_a.s, 0) AS delta
         FROM dir_a
         FULL OUTER JOIN dir_b ON dir_a.d = dir_b.d
         WHERE COALESCE(dir_b.s, 0) <> COALESCE(dir_a.s, 0)
         ORDER BY ABS(COALESCE(dir_b.s, 0) - COALESCE(dir_a.s, 0)) DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(duckdb::params![limit as i64], |row| {
        let path: String = row.get(0)?;
        let size_a: i64 = row.get(1)?;
        let size_b: i64 = row.get(2)?;
        let delta: i64 = row.get(3)?;
        let kind = if size_a == 0 {
            "added"
        } else if size_b == 0 {
            "removed"
        } else if delta > 0 {
            "grew"
        } else {
            "shrank"
        };
        let rate = if days_between > 0.0 {
            delta as f64 / days_between
        } else {
            delta as f64
        };
        Ok(GrowthRow {
            path,
            kind,
            size_a: size_a.max(0) as u64,
            size_b: size_b.max(0) as u64,
            delta_bytes: delta,
            rate_bytes_per_day: rate,
            days_between,
        })
    })?;
    Ok(rows.flatten().collect())
}

/// N-snapshot growth per directory — fits ordinary-least-squares against
/// each path's (snapshot_ts, dir_size) series. Returns slope (bytes/day),
/// R² (goodness-of-fit, 0..1), and optionally a projected fill-by date.
///
/// `snapshots` is a slice of `(db_path, started_at_unix_secs)`, ordered
/// oldest → newest. Caller is responsible for picking the N most recent
/// snapshots and parsing their timestamps from `scan_meta` or filename.
///
/// `fill_target` is the volume's currently-free byte budget. When set
/// and slope is positive, `projected_fill_date` is RFC 3339 UTC for the
/// moment the directory would consume that many bytes beyond its latest
/// observed size (i.e. naive linear extrapolation: `t_fill = t_last +
/// fill_target / slope_bytes_per_sec`).
#[derive(Debug, Clone, Serialize)]
pub struct GrowthNRow {
    pub path: String,
    pub slope_bytes_per_day: i64,
    pub r2: f64,
    pub latest_bytes: u64,
    pub n_snapshots: usize,
    /// `(unix_secs, bytes)` samples used in the fit, ordered by time. Lets
    /// agents render sparklines without re-querying.
    pub sample_paths_ts: Vec<(i64, u64)>,
    /// RFC 3339 UTC; `None` when slope is non-positive or `fill_target` is None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected_fill_date: Option<String>,
}

pub fn growth_over_n(
    snapshots: &[(String, i64)],
    limit: usize,
    fill_target: Option<u64>,
) -> Result<Vec<GrowthNRow>> {
    use duckdb::Connection;
    if snapshots.len() < 3 {
        return Err(anyhow::anyhow!(
            "growth_over_n: need at least 3 snapshots, got {}",
            snapshots.len()
        ));
    }
    let conn = Connection::open_in_memory()?;
    // ATTACH all snapshots as db0, db1, ..., dbN-1. UNION ALL their
    // per-directory aggregates tagged with the snapshot index.
    let mut attach_sql = String::new();
    for (i, (p, _)) in snapshots.iter().enumerate() {
        attach_sql.push_str(&format!(
            "ATTACH '{}' AS db{} (READ_ONLY);",
            p.replace('\'', "''"),
            i
        ));
    }
    conn.execute_batch(&attach_sql)?;

    let mut union_parts: Vec<String> = Vec::with_capacity(snapshots.len());
    for (i, _) in snapshots.iter().enumerate() {
        union_parts.push(format!(
            "SELECT regexp_replace(path, '/[^/]+$', '') AS d, \
                    SUM(size) AS s, {i} AS snap_idx \
             FROM db{i}.files WHERE is_dir = false GROUP BY d"
        ));
    }
    let union_sql = union_parts.join(" UNION ALL ");

    // Only keep dirs that exist in the latest snapshot (newest = last entry).
    let latest_idx = snapshots.len() - 1;
    let sql = format!(
        "WITH agg AS ({union_sql}),
              latest AS (SELECT d, s AS latest_s FROM agg WHERE snap_idx = {latest_idx})
         SELECT agg.d AS path, agg.snap_idx, agg.s, latest.latest_s
         FROM agg JOIN latest USING (d)
         ORDER BY agg.d, agg.snap_idx"
    );

    let mut stmt = conn.prepare(&sql)?;
    // Collect (path -> Vec<(snap_idx, bytes, latest_bytes)>)
    use std::collections::HashMap;
    let mut series: HashMap<String, Vec<(usize, i64)>> = HashMap::new();
    let mut latest_by_path: HashMap<String, i64> = HashMap::new();

    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let path: String = row.get(0)?;
        let idx: i64 = row.get(1)?;
        let bytes: i64 = row.get(2)?;
        let latest_s: i64 = row.get(3)?;
        series
            .entry(path.clone())
            .or_default()
            .push((idx as usize, bytes));
        latest_by_path.insert(path, latest_s);
    }

    // Build (ts, bytes) points per path using snapshot timestamps.
    let ts: Vec<i64> = snapshots.iter().map(|(_, t)| *t).collect();
    let mut out: Vec<GrowthNRow> = Vec::with_capacity(series.len());
    for (path, mut idx_samples) in series.into_iter() {
        idx_samples.sort_by_key(|(i, _)| *i);
        // Need at least 3 distinct points to compute a meaningful R².
        if idx_samples.len() < 3 {
            continue;
        }
        let points: Vec<(f64, f64)> = idx_samples
            .iter()
            .map(|(i, b)| (ts[*i] as f64, (*b).max(0) as f64))
            .collect();
        let (slope_per_sec, r2) = linfit_slope_r2(&points);
        let slope_per_day = slope_per_sec * 86400.0;
        let latest_bytes = latest_by_path.get(&path).copied().unwrap_or(0).max(0) as u64;
        let latest_ts = *ts.last().unwrap();
        let projected_fill_date = match (slope_per_sec > 0.0, fill_target) {
            (true, Some(target)) => {
                let secs = (target as f64) / slope_per_sec;
                let fill_ts = latest_ts as f64 + secs;
                DateTime::<Utc>::from_timestamp(fill_ts as i64, 0)
                    .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            }
            _ => None,
        };
        let sample_paths_ts: Vec<(i64, u64)> = idx_samples
            .iter()
            .map(|(i, b)| (ts[*i], (*b).max(0) as u64))
            .collect();
        out.push(GrowthNRow {
            path,
            slope_bytes_per_day: slope_per_day as i64,
            r2,
            latest_bytes,
            n_snapshots: idx_samples.len(),
            sample_paths_ts,
            projected_fill_date,
        });
    }
    // Sort by slope desc (biggest growers first).
    out.sort_by_key(|b| std::cmp::Reverse(b.slope_bytes_per_day));
    out.truncate(limit);
    Ok(out)
}

/// Ordinary least squares — returns (slope, r²). Mirrors `predict::linfit`
/// but lives here so query.rs has no cross-module dependency.
fn linfit_slope_r2(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    if n < 2.0 {
        return (0.0, 0.0);
    }
    let mean_x: f64 = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y: f64 = points.iter().map(|p| p.1).sum::<f64>() / n;
    let mut num = 0.0;
    let mut den_x = 0.0;
    let mut den_y = 0.0;
    for (x, y) in points {
        let dx = x - mean_x;
        let dy = y - mean_y;
        num += dx * dy;
        den_x += dx * dx;
        den_y += dy * dy;
    }
    let slope = if den_x == 0.0 { 0.0 } else { num / den_x };
    let r2 = if den_x == 0.0 || den_y == 0.0 {
        if points.len() == 2 {
            1.0
        } else {
            0.0
        }
    } else {
        (num * num) / (den_x * den_y)
    };
    (slope, r2)
}

pub fn find_files(conn: &Connection, pattern: &str, limit: usize) -> Result<Vec<FileRow>> {
    let sql_pattern = pattern.replace('*', "%").replace('?', "_");
    let mut stmt = conn.prepare(
        "SELECT path, size, ext, mtime FROM files
         WHERE name LIKE ? AND is_dir = false
         ORDER BY size DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(duckdb::params![sql_pattern, limit as i64], |row| {
        Ok(FileRow {
            path: row.get::<_, String>(0)?,
            size: row.get::<_, i64>(1)? as u64,
            ext: row.get::<_, Option<String>>(2)?,
            mtime: rfc3339(row.get::<_, Option<i64>>(3)?),
        })
    })?;
    Ok(rows.flatten().collect())
}

/// Run an arbitrary SQL statement and return rows as JSON objects keyed by
/// column name. Heterogeneous columns are coerced to the closest serde_json
/// type — large integers (HugeInt) fall back to strings to preserve precision.
#[derive(Debug, Clone, Serialize)]
pub struct DiffRow {
    pub path: String,
    /// `"added"` (only in b), `"removed"` (only in a), `"grew"`, `"shrank"`.
    pub kind: &'static str,
    pub size_a: u64,
    pub size_b: u64,
    pub delta: i64,
}

/// Diff two snapshots (file-level). Returns rows where the size differs or
/// the file only exists in one side. Ordered by absolute delta, largest first.
pub fn diff(snap_a: &str, snap_b: &str, limit: usize) -> Result<Vec<DiffRow>> {
    use duckdb::Connection;
    // Open an in-memory DB and ATTACH both snapshots read-only so we can FULL
    // OUTER JOIN across them in one statement.
    let conn = Connection::open_in_memory()?;
    let sql = format!(
        "ATTACH '{a}' AS db_a (READ_ONLY);\
         ATTACH '{b}' AS db_b (READ_ONLY);",
        a = snap_a.replace('\'', "''"),
        b = snap_b.replace('\'', "''"),
    );
    conn.execute_batch(&sql)?;

    let mut stmt = conn.prepare(
        "SELECT COALESCE(a.path, b.path)              AS path,
                COALESCE(a.size, 0)                   AS size_a,
                COALESCE(b.size, 0)                   AS size_b,
                COALESCE(b.size, 0) - COALESCE(a.size, 0) AS delta
         FROM db_a.files a
         FULL OUTER JOIN db_b.files b ON a.path = b.path
         WHERE COALESCE(a.is_dir, false) = false
           AND COALESCE(b.is_dir, false) = false
           AND COALESCE(a.size, -1) IS DISTINCT FROM COALESCE(b.size, -1)
         ORDER BY ABS(COALESCE(b.size, 0) - COALESCE(a.size, 0)) DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(duckdb::params![limit as i64], |row| {
        let path: String = row.get(0)?;
        let size_a: i64 = row.get(1)?;
        let size_b: i64 = row.get(2)?;
        let delta: i64 = row.get(3)?;
        let kind = if size_a == 0 && size_b > 0 {
            "added"
        } else if size_a > 0 && size_b == 0 {
            "removed"
        } else if delta > 0 {
            "grew"
        } else {
            "shrank"
        };
        Ok(DiffRow {
            path,
            kind,
            size_a: size_a as u64,
            size_b: size_b as u64,
            delta,
        })
    })?;
    Ok(rows.flatten().collect())
}

pub fn raw_query(
    conn: &Connection,
    sql: &str,
    limit: usize,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>> {
    use duckdb::types::{TimeUnit, Value as DV};
    use serde_json::Value as JV;

    fn convert(v: DV) -> JV {
        match v {
            DV::Null => JV::Null,
            DV::Boolean(b) => JV::Bool(b),
            DV::TinyInt(i) => JV::from(i),
            DV::SmallInt(i) => JV::from(i),
            DV::Int(i) => JV::from(i),
            DV::BigInt(i) => JV::from(i),
            DV::HugeInt(i) => JV::String(i.to_string()),
            DV::UTinyInt(i) => JV::from(i),
            DV::USmallInt(i) => JV::from(i),
            DV::UInt(i) => JV::from(i),
            DV::UBigInt(i) => JV::from(i),
            DV::Float(f) => serde_json::Number::from_f64(f as f64).map_or(JV::Null, JV::Number),
            DV::Double(f) => serde_json::Number::from_f64(f).map_or(JV::Null, JV::Number),
            DV::Decimal(d) => JV::String(d.to_string()),
            DV::Timestamp(_, i) => JV::from(i),
            DV::Text(s) => JV::String(s),
            DV::Blob(b) => JV::String(format!("<blob:{} bytes>", b.len())),
            DV::Date32(i) => JV::from(i),
            DV::Time64(TimeUnit::Microsecond, i) => JV::from(i),
            DV::Time64(_, i) => JV::from(i),
            DV::Interval {
                months,
                days,
                nanos,
            } => serde_json::json!({
                "months": months, "days": days, "nanos": nanos
            }),
            DV::List(v) | DV::Array(v) => JV::Array(v.into_iter().map(convert).collect()),
            DV::Enum(s) => JV::String(s),
            DV::Struct(m) => {
                let mut o = serde_json::Map::new();
                for (k, v) in m.iter() {
                    o.insert(k.clone(), convert(v.clone()));
                }
                JV::Object(o)
            }
            DV::Union(inner) => convert(*inner),
            DV::Map(m) => {
                let arr: Vec<JV> = m
                    .iter()
                    .map(|(k, v)| {
                        serde_json::json!({ "key": convert(k.clone()), "value": convert(v.clone()) })
                    })
                    .collect();
                JV::Array(arr)
            }
        }
    }

    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([])?;
    let column_names: Vec<String> = rows
        .as_ref()
        .map(|s| s.column_names())
        .unwrap_or_default()
        .into_iter()
        .collect();
    let n_cols = column_names.len();

    let mut out = Vec::new();
    let mut taken = 0usize;
    while let Some(row) = rows.next()? {
        if taken >= limit {
            break;
        }
        let mut obj = serde_json::Map::with_capacity(n_cols);
        for (i, name) in column_names.iter().enumerate() {
            let v: DV = row.get(i)?;
            obj.insert(name.clone(), convert(v));
        }
        out.push(obj);
        taken += 1;
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use duckdb::Connection;

    /// Build an in-memory DB with a tiny seeded files table.
    fn seeded() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::create_schema(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO files (path, name, ext, size, mtime, is_dir, depth) VALUES
             ('/a/big.bin',   'big.bin',   'bin', 16384, 1700000000, false, 2),
             ('/a/mid.log',   'mid.log',   'log',  4096, 1700000000, false, 2),
             ('/a/small.txt', 'small.txt', 'txt',   256, 1700000000, false, 2),
             ('/b/other.bin', 'other.bin', 'bin',  8192, 1700000000, false, 2),
             ('/a',           'a',         NULL,      0, 1700000000, true,  1),
             ('/b',           'b',         NULL,      0, 1700000000, true,  1);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn schema_version_is_one() {
        assert_eq!(SCHEMA_VERSION, 1);
    }

    #[test]
    fn top_files_returns_size_descending_with_limit() {
        let conn = seeded();
        let rows = top_files(&conn, 2, 0).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].size, 16384);
        assert_eq!(rows[1].size, 8192);
    }

    #[test]
    fn top_files_respects_min_size_filter() {
        let conn = seeded();
        let rows = top_files(&conn, 10, 5000).unwrap();
        // Only files with size >= 5000 — big.bin (16384) and other.bin (8192).
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.size >= 5000));
    }

    #[test]
    fn top_files_excludes_directories() {
        let conn = seeded();
        let rows = top_files(&conn, 100, 0).unwrap();
        assert_eq!(rows.len(), 4);
        assert!(!rows.iter().any(|r| r.path == "/a" || r.path == "/b"));
    }

    #[test]
    fn by_extension_aggregates_by_ext() {
        let conn = seeded();
        let rows = by_extension(&conn, 10).unwrap();
        let bin = rows.iter().find(|r| r.ext == "bin").unwrap();
        assert_eq!(bin.files, 2);
        assert_eq!(bin.total_size, 16384 + 8192);
    }

    #[test]
    fn stats_aggregates_correctly() {
        let conn = seeded();
        let s = stats(&conn).unwrap();
        assert_eq!(s.files, 4);
        assert_eq!(s.dirs, 2);
        assert_eq!(s.total_bytes, 16384 + 4096 + 256 + 8192);
        assert_eq!(s.largest_bytes, 16384);
        assert!(!s.partial);
    }

    #[test]
    fn rfc3339_handles_none() {
        assert!(rfc3339(None).is_none());
        let s = rfc3339(Some(1_700_000_000)).unwrap();
        assert!(s.starts_with("2023-"));
        assert!(s.ends_with("Z"));
    }

    #[test]
    fn linfit_slope_r2_perfect_line() {
        // y = 2x + 3 over 5 points → slope = 2, r² = 1.
        let pts: Vec<(f64, f64)> = (0..5).map(|i| (i as f64, 2.0 * i as f64 + 3.0)).collect();
        let (slope, r2) = linfit_slope_r2(&pts);
        assert!((slope - 2.0).abs() < 1e-9, "slope = {slope}");
        assert!((r2 - 1.0).abs() < 1e-9, "r2 = {r2}");
    }

    #[test]
    fn linfit_slope_r2_flat_is_zero() {
        let pts = vec![(0.0, 10.0), (1.0, 10.0), (2.0, 10.0), (3.0, 10.0)];
        let (slope, _r2) = linfit_slope_r2(&pts);
        assert_eq!(slope, 0.0);
    }

    #[test]
    fn linfit_slope_r2_noisy_line_high_r2() {
        // Mostly linear with 1-byte jitter — r² should be very close to 1.
        let pts: Vec<(f64, f64)> = (0..6)
            .map(|i| {
                (
                    i as f64,
                    10.0 * i as f64 + if i % 2 == 0 { 0.5 } else { -0.5 },
                )
            })
            .collect();
        let (slope, r2) = linfit_slope_r2(&pts);
        assert!((slope - 10.0).abs() < 1.0);
        assert!(r2 > 0.99, "r2 = {r2}");
    }

    #[test]
    fn growth_over_n_rejects_fewer_than_three() {
        // 2 snapshots — must error out, not silently degrade.
        let snaps: Vec<(String, i64)> = vec![
            ("/tmp/does-not-matter-a.db".into(), 1000),
            ("/tmp/does-not-matter-b.db".into(), 2000),
        ];
        let err = growth_over_n(&snaps, 10, None).unwrap_err();
        assert!(format!("{err}").contains("at least 3"));
    }
}

pub fn stats(conn: &Connection) -> Result<Stats> {
    stats_inner(conn, false)
}

pub fn stats_physical(conn: &Connection) -> Result<Stats> {
    stats_inner(conn, true)
}

fn stats_inner(conn: &Connection, physical: bool) -> Result<Stats> {
    let size_expr = if physical {
        "COALESCE(physical_size, size)"
    } else {
        "size"
    };
    let sql = format!(
        "SELECT
            COUNT(*) FILTER (WHERE is_dir = false) as files,
            COUNT(*) FILTER (WHERE is_dir = true)  as dirs,
            COALESCE(SUM({size_expr}), 0) as total_bytes,
            COALESCE(MAX({size_expr}), 0) as largest,
            COALESCE(AVG({size_expr}) FILTER (WHERE is_dir = false AND {size_expr} > 0), 0) as avg_size
         FROM files"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut row = stmt.query_row([], |row| {
        Ok(Stats {
            files: row.get::<_, i64>(0)? as u64,
            dirs: row.get::<_, i64>(1)? as u64,
            total_bytes: row.get::<_, i64>(2)? as u64,
            largest_bytes: row.get::<_, i64>(3)? as u64,
            avg_bytes: row.get::<_, f64>(4)? as u64,
            partial: false,
            scan_root: None,
            scan_duration_s: None,
            scanned_at: None,
        })
    })?;
    if let Some(meta) = crate::db::read_scan_meta(conn) {
        row.partial = !meta.completed;
        row.scan_root = Some(meta.root.clone());
        row.scan_duration_s = meta.duration_secs();
        row.scanned_at = rfc3339(Some(meta.started_at));
    }
    Ok(row)
}
