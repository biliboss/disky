use anyhow::Result;
use duckdb::Connection;

pub fn open(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    let cpus = num_cpus::get();
    conn.execute_batch(&format!("PRAGMA threads={cpus};"))?;
    Ok(conn)
}

pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS files;
        CREATE TABLE files (
            path        TEXT NOT NULL,
            name        TEXT NOT NULL,
            ext         TEXT,
            size        BIGINT NOT NULL DEFAULT 0,
            mtime       BIGINT,
            is_dir      BOOLEAN NOT NULL DEFAULT false,
            depth       INTEGER NOT NULL DEFAULT 0
        );
        DROP TABLE IF EXISTS scan_meta;
        CREATE TABLE scan_meta (
            root         TEXT NOT NULL,
            started_at   BIGINT NOT NULL,
            ended_at     BIGINT,
            completed    BOOLEAN NOT NULL DEFAULT false,
            entries      BIGINT NOT NULL DEFAULT 0,
            bytes        BIGINT NOT NULL DEFAULT 0
        );
    ",
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn write_scan_meta(
    conn: &Connection,
    root: &str,
    started_at: i64,
    ended_at: Option<i64>,
    completed: bool,
    entries: u64,
    bytes: u64,
) -> Result<()> {
    conn.execute("DELETE FROM scan_meta", [])?;
    conn.execute(
        "INSERT INTO scan_meta (root, started_at, ended_at, completed, entries, bytes)
         VALUES (?, ?, ?, ?, ?, ?)",
        duckdb::params![
            root,
            started_at,
            ended_at,
            completed,
            entries as i64,
            bytes as i64
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ScanMeta {
    pub root: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub completed: bool,
    pub entries: u64,
    pub bytes: u64,
}

impl ScanMeta {
    pub fn duration_secs(&self) -> Option<i64> {
        self.ended_at.map(|e| (e - self.started_at).max(0))
    }
}

pub fn read_scan_meta(conn: &Connection) -> Option<ScanMeta> {
    let mut stmt = conn
        .prepare(
            "SELECT root, started_at, ended_at, completed, entries, bytes FROM scan_meta LIMIT 1",
        )
        .ok()?;
    stmt.query_row([], |row| {
        Ok(ScanMeta {
            root: row.get::<_, String>(0)?,
            started_at: row.get::<_, i64>(1)?,
            ended_at: row.get::<_, Option<i64>>(2)?,
            completed: row.get::<_, bool>(3)?,
            entries: row.get::<_, i64>(4)? as u64,
            bytes: row.get::<_, i64>(5)? as u64,
        })
    })
    .ok()
}

pub fn build_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE INDEX idx_size ON files(size DESC);
        CREATE INDEX idx_ext  ON files(ext);
        ANALYZE;
    ",
    )?;
    Ok(())
}

pub struct FileRecord {
    pub path: String,
    pub name: String,
    pub ext: Option<String>,
    pub size: i64,
    pub mtime: Option<i64>,
    pub is_dir: bool,
    pub depth: i32,
}

pub fn append_batch(conn: &Connection, records: &[FileRecord]) -> Result<()> {
    let mut app = conn.appender("files")?;
    for r in records {
        app.append_row(duckdb::params![
            r.path, r.name, r.ext, r.size, r.mtime, r.is_dir, r.depth,
        ])?;
    }
    app.flush()?;
    Ok(())
}
