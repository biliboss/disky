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
    let mut stmt = conn.prepare(
        "SELECT path, size, ext, mtime FROM files
         WHERE is_dir = false AND size >= ?
         ORDER BY size DESC
         LIMIT ?",
    )?;
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
    let mut stmt = conn.prepare(
        "SELECT COALESCE(ext, '(none)') as ext,
                COUNT(*) as count,
                SUM(size) as total_size
         FROM files
         WHERE is_dir = false
         GROUP BY ext
         ORDER BY total_size DESC
         LIMIT ?",
    )?;
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
    let mut stmt = conn.prepare(
        "SELECT parent_path, SUM(size) as total FROM (
             SELECT regexp_replace(path, '/[^/]+$', '') as parent_path, size
             FROM files
             WHERE is_dir = false
         )
         GROUP BY parent_path
         ORDER BY total DESC
         LIMIT ?",
    )?;
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
pub fn old_files(conn: &Connection, cutoff: i64, limit: usize) -> Result<Vec<FileRow>> {
    let mut stmt = conn.prepare(
        "SELECT path, size, ext, mtime FROM files
         WHERE is_dir = false AND mtime IS NOT NULL AND mtime < ?
         ORDER BY mtime ASC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(duckdb::params![cutoff, limit as i64], |row| {
        Ok(FileRow {
            path: row.get::<_, String>(0)?,
            size: row.get::<_, i64>(1)? as u64,
            ext: row.get::<_, Option<String>>(2)?,
            mtime: rfc3339(row.get::<_, Option<i64>>(3)?),
        })
    })?;
    Ok(rows.flatten().collect())
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
}

pub fn stats(conn: &Connection) -> Result<Stats> {
    let mut stmt = conn.prepare(
        "SELECT
            COUNT(*) FILTER (WHERE is_dir = false) as files,
            COUNT(*) FILTER (WHERE is_dir = true)  as dirs,
            COALESCE(SUM(size), 0) as total_bytes,
            COALESCE(MAX(size), 0) as largest,
            COALESCE(AVG(size) FILTER (WHERE is_dir = false AND size > 0), 0) as avg_size
         FROM files",
    )?;
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
