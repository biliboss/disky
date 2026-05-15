use anyhow::Result;
use duckdb::Connection;

pub fn open(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    let cpus = num_cpus::get();
    conn.execute_batch(&format!("PRAGMA threads={cpus};"))?;
    Ok(conn)
}

pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("
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
    ")?;
    Ok(())
}

pub fn build_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE INDEX idx_size ON files(size DESC);
        CREATE INDEX idx_ext  ON files(ext);
        ANALYZE;
    ")?;
    Ok(())
}

pub struct FileRecord {
    pub path:   String,
    pub name:   String,
    pub ext:    Option<String>,
    pub size:   i64,
    pub mtime:  Option<i64>,
    pub is_dir: bool,
    pub depth:  i32,
}

pub fn append_batch(conn: &Connection, records: &[FileRecord]) -> Result<()> {
    let mut app = conn.appender("files")?;
    for r in records {
        app.append_row(duckdb::params![
            r.path,
            r.name,
            r.ext,
            r.size,
            r.mtime,
            r.is_dir,
            r.depth,
        ])?;
    }
    app.flush()?;
    Ok(())
}
