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
    let row = stmt.query_row([], |row| {
        Ok(Stats {
            files: row.get::<_, i64>(0)? as u64,
            dirs: row.get::<_, i64>(1)? as u64,
            total_bytes: row.get::<_, i64>(2)? as u64,
            largest_bytes: row.get::<_, i64>(3)? as u64,
            avg_bytes: row.get::<_, f64>(4)? as u64,
        })
    })?;
    Ok(row)
}
