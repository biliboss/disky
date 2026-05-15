use anyhow::Result;
use duckdb::Connection;
use humansize::{format_size, BINARY};

pub fn top_files(conn: &Connection, limit: usize, min_size: u64) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT path, size FROM files
         WHERE is_dir = false AND size >= ?
         ORDER BY size DESC
         LIMIT ?",
    )?;

    let rows = stmt.query_map(duckdb::params![min_size as i64, limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    println!("{:<80} {:>12}", "PATH", "SIZE");
    println!("{}", "-".repeat(94));
    for row in rows.flatten() {
        println!(
            "{:<80} {:>12}",
            truncate(&row.0, 80),
            format_size(row.1 as u64, BINARY)
        );
    }
    Ok(())
}

pub fn by_extension(conn: &Connection, limit: usize) -> Result<()> {
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
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;

    println!("{:<20} {:>10} {:>14}", "EXT", "FILES", "TOTAL SIZE");
    println!("{}", "-".repeat(46));
    for row in rows.flatten() {
        println!(
            "{:<20} {:>10} {:>14}",
            row.0,
            row.1,
            format_size(row.2 as u64, BINARY)
        );
    }
    Ok(())
}

pub fn top_dirs(conn: &Connection, limit: usize) -> Result<()> {
    // DuckDB doesn't aggregate dir sizes automatically — we compute from children
    let mut stmt2 = conn.prepare(
        "SELECT
            parent_path,
            SUM(size) as total
         FROM (
             SELECT
                 regexp_replace(path, '/[^/]+$', '') as parent_path,
                 size
             FROM files
             WHERE is_dir = false
         )
         GROUP BY parent_path
         ORDER BY total DESC
         LIMIT ?",
    )?;

    let rows = stmt2.query_map(duckdb::params![limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    println!("{:<80} {:>12}", "DIRECTORY", "SIZE");
    println!("{}", "-".repeat(94));
    for row in rows.flatten() {
        println!(
            "{:<80} {:>12}",
            truncate(&row.0, 80),
            format_size(row.1 as u64, BINARY)
        );
    }
    Ok(())
}

pub fn find_files(conn: &Connection, pattern: &str, limit: usize) -> Result<()> {
    let sql_pattern = pattern.replace('*', "%").replace('?', "_");
    let mut stmt = conn.prepare(
        "SELECT path, size FROM files
         WHERE name LIKE ? AND is_dir = false
         ORDER BY size DESC
         LIMIT ?",
    )?;

    let rows = stmt.query_map(duckdb::params![sql_pattern, limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    println!("{:<80} {:>12}", "PATH", "SIZE");
    println!("{}", "-".repeat(94));
    let mut found = 0;
    for row in rows.flatten() {
        println!(
            "{:<80} {:>12}",
            truncate(&row.0, 80),
            format_size(row.1 as u64, BINARY)
        );
        found += 1;
    }
    if found == 0 {
        println!("No files match '{}'", pattern);
    }
    Ok(())
}

pub fn stats(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT
            COUNT(*) FILTER (WHERE is_dir = false) as files,
            COUNT(*) FILTER (WHERE is_dir = true)  as dirs,
            SUM(size) as total_bytes,
            MAX(size) as largest,
            AVG(size) FILTER (WHERE is_dir = false AND size > 0) as avg_size
         FROM files",
    )?;

    let row = stmt.query_row([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, f64>(4)?,
        ))
    })?;

    println!("Files:      {:>12}", row.0);
    println!("Dirs:       {:>12}", row.1);
    println!("Total size: {:>12}", format_size(row.2 as u64, BINARY));
    println!("Largest:    {:>12}", format_size(row.3 as u64, BINARY));
    println!("Avg size:   {:>12}", format_size(row.4 as u64, BINARY));
    Ok(())
}

pub fn export_html_report(conn: &Connection, db_path: &str) -> Result<()> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let out = format!("/tmp/disky-report-{}.html", ts);

    // reuse existing display data
    let mut top_lines = vec![];
    let mut stmt = conn
        .prepare("SELECT path, size FROM files WHERE is_dir=false ORDER BY size DESC LIMIT 20")?;
    for row in stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .flatten()
    {
        top_lines.push(format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            row.0,
            format_size(row.1 as u64, BINARY)
        ));
    }

    let html = format!(
        r#"<!DOCTYPE html><html><head><meta charset="UTF-8">
<title>disky report</title>
<style>body{{font-family:monospace;background:#111;color:#eee;padding:2em}}
table{{border-collapse:collapse;width:100%}}td{{padding:4px 8px;border-bottom:1px solid #333}}
tr:hover{{background:#222}}h1{{color:#0ff}}</style></head>
<body><h1>disky — {}</h1>
<h2>Top 20 largest files</h2>
<table>{}</table>
</body></html>"#,
        db_path,
        top_lines.join("")
    );

    fs::write(&out, html)?;
    std::process::Command::new("open").arg(&out).spawn()?;
    eprintln!("Report: {}", out);
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("...{}", &s[s.len().saturating_sub(max - 3)..])
    }
}
