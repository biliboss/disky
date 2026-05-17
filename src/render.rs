use anyhow::Result;
use duckdb::Connection;
use humansize::{format_size, BINARY};
use serde::Serialize;
use serde_json::json;

use crate::query::{DirRow, ExtRow, FileRow, Stats, SCHEMA_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
    Ndjson,
}

impl Format {
    pub fn is_machine(self) -> bool {
        matches!(self, Format::Json | Format::Ndjson)
    }
}

/// Auto-pick Json when stdout is piped, else Text. Explicit user choice wins.
pub fn resolve_format(user: Option<Format>) -> Format {
    if let Some(f) = user {
        return f;
    }
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() {
        Format::Text
    } else {
        Format::Json
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("...{}", &s[s.len().saturating_sub(max - 3)..])
    }
}

fn emit_records<T: Serialize>(rows: &[T], kind: &str, format: Format) -> Result<()> {
    match format {
        Format::Ndjson => {
            for r in rows {
                println!("{}", serde_json::to_string(r)?);
            }
        }
        Format::Json => {
            let payload = json!({
                "schema_version": SCHEMA_VERSION,
                "kind": kind,
                "records": rows,
            });
            println!("{}", serde_json::to_string(&payload)?);
        }
        Format::Text => unreachable!("emit_records called for Text"),
    }
    Ok(())
}

pub fn top_files(rows: &[FileRow], format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(rows, "top", format);
    }
    println!("{:<80} {:>12}", "PATH", "SIZE");
    println!("{}", "-".repeat(94));
    for r in rows {
        println!(
            "{:<80} {:>12}",
            truncate(&r.path, 80),
            format_size(r.size, BINARY)
        );
    }
    Ok(())
}

pub fn by_extension(rows: &[ExtRow], format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(rows, "ext", format);
    }
    println!("{:<20} {:>10} {:>14}", "EXT", "FILES", "TOTAL SIZE");
    println!("{}", "-".repeat(46));
    for r in rows {
        println!(
            "{:<20} {:>10} {:>14}",
            r.ext,
            r.files,
            format_size(r.total_size, BINARY)
        );
    }
    Ok(())
}

pub fn top_dirs(rows: &[DirRow], format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(rows, "dirs", format);
    }
    println!("{:<80} {:>12}", "DIRECTORY", "SIZE");
    println!("{}", "-".repeat(94));
    for r in rows {
        println!(
            "{:<80} {:>12}",
            truncate(&r.path, 80),
            format_size(r.total_size, BINARY)
        );
    }
    Ok(())
}

pub fn find_files(rows: &[FileRow], pattern: &str, format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(rows, "find", format);
    }
    println!("{:<80} {:>12}", "PATH", "SIZE");
    println!("{}", "-".repeat(94));
    if rows.is_empty() {
        println!("No files match '{}'", pattern);
        return Ok(());
    }
    for r in rows {
        println!(
            "{:<80} {:>12}",
            truncate(&r.path, 80),
            format_size(r.size, BINARY)
        );
    }
    Ok(())
}

pub fn stats(s: &Stats, format: Format) -> Result<()> {
    if format.is_machine() {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "kind": "stats",
            "record": s,
        });
        println!("{}", serde_json::to_string(&payload)?);
        return Ok(());
    }
    println!("Files:      {:>12}", s.files);
    println!("Dirs:       {:>12}", s.dirs);
    println!("Total size: {:>12}", format_size(s.total_bytes, BINARY));
    println!("Largest:    {:>12}", format_size(s.largest_bytes, BINARY));
    println!("Avg size:   {:>12}", format_size(s.avg_bytes, BINARY));
    Ok(())
}

pub fn raw_query(
    rows: &[serde_json::Map<String, serde_json::Value>],
    format: Format,
) -> Result<()> {
    match format {
        Format::Ndjson => {
            for r in rows {
                println!("{}", serde_json::to_string(r)?);
            }
        }
        Format::Json => {
            let payload = json!({
                "schema_version": SCHEMA_VERSION,
                "kind": "query",
                "records": rows,
            });
            println!("{}", serde_json::to_string(&payload)?);
        }
        Format::Text => {
            if rows.is_empty() {
                println!("(0 rows)");
                return Ok(());
            }
            let columns: Vec<String> = rows[0].keys().cloned().collect();
            let widths: Vec<usize> = columns
                .iter()
                .map(|c| {
                    let max_cell = rows
                        .iter()
                        .map(|r| {
                            r.get(c)
                                .map(|v| match v {
                                    serde_json::Value::String(s) => s.len(),
                                    other => other.to_string().len(),
                                })
                                .unwrap_or(4)
                        })
                        .max()
                        .unwrap_or(c.len());
                    max_cell.max(c.len()).min(80)
                })
                .collect();

            for (c, w) in columns.iter().zip(widths.iter()) {
                print!("{:<width$} ", c, width = w);
            }
            println!();
            println!(
                "{}",
                "-".repeat(widths.iter().sum::<usize>() + columns.len())
            );

            for r in rows {
                for (c, w) in columns.iter().zip(widths.iter()) {
                    let cell = match r.get(c) {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(v) => v.to_string(),
                        None => String::new(),
                    };
                    let truncated = if cell.len() > *w {
                        format!("{}…", &cell[..w.saturating_sub(1)])
                    } else {
                        cell
                    };
                    print!("{:<width$} ", truncated, width = w);
                }
                println!();
            }
        }
    }
    Ok(())
}

/// Used by the TUI's `e` keybind.
pub fn export_html_report(conn: &Connection, db_path: &str) -> Result<()> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let out = format!("/tmp/disky-report-{}.html", ts);

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
