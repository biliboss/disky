use anyhow::Result;
use duckdb::Connection;
use humansize::{format_size, BINARY};
use serde::Serialize;
use serde_json::json;

use crate::cleanup::CleanupHit;
use crate::contract::{DupesView, HistoryView, LargeView, MapView, TrashResult};
use crate::history::RestoreReport;
use crate::query::{DiffRow, DirRow, ExtRow, FileRow, Stats, SCHEMA_VERSION};

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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn is_machine_classifies_formats() {
        assert!(!Format::Text.is_machine());
        assert!(Format::Json.is_machine());
        assert!(Format::Ndjson.is_machine());
    }

    #[test]
    fn resolve_format_honours_user_choice() {
        // Explicit user choice always wins, even if stdout would auto-pick differently.
        assert_eq!(resolve_format(Some(Format::Text)), Format::Text);
        assert_eq!(resolve_format(Some(Format::Json)), Format::Json);
        assert_eq!(resolve_format(Some(Format::Ndjson)), Format::Ndjson);
    }

    #[test]
    fn truncate_passes_short_strings_through() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_prepends_ellipsis_for_long_strings() {
        let result = truncate("aaaaaaaaaaaaaaaaaaaa", 10);
        assert!(result.starts_with("..."));
        assert_eq!(result.len(), 10);
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

/// Emit a minimal scalar envelope for agents that only need totals.
/// Always JSON; `--format` is ignored (the caller chooses --summarize for compactness).
pub fn stats_scalar(s: &Stats) -> Result<()> {
    let payload = json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "scalar",
        "records": [{ "bytes": s.total_bytes, "files": s.files }],
    });
    println!("{}", serde_json::to_string(&payload)?);
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
    if let Some(root) = &s.scan_root {
        println!("Root:       {}", root);
    }
    if let Some(ts) = &s.scanned_at {
        println!("Scanned:    {}", ts);
    }
    if let Some(d) = s.scan_duration_s {
        println!("Duration:   {}s", d);
    }
    if s.partial {
        println!("Status:        PARTIAL (scan was cancelled)");
    }
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

pub fn diff(rows: &[DiffRow], format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(rows, "diff", format);
    }
    println!(
        "{:<8} {:>12} {:>12} {:>14}  PATH",
        "KIND", "SIZE A", "SIZE B", "Δ"
    );
    println!("{}", "-".repeat(80));
    for r in rows {
        let delta_str = if r.delta >= 0 {
            format!("+{}", format_size(r.delta as u64, BINARY))
        } else {
            format!("-{}", format_size(r.delta.unsigned_abs(), BINARY))
        };
        println!(
            "{:<8} {:>12} {:>12} {:>14}  {}",
            r.kind,
            format_size(r.size_a, BINARY),
            format_size(r.size_b, BINARY),
            delta_str,
            truncate(&r.path, 60),
        );
    }
    Ok(())
}

pub fn cleanup(
    hits: &[CleanupHit],
    applied: Option<&[String]>,
    history_id: Option<&str>,
    disk: Option<&crate::disk::Disk>,
    format: Format,
) -> Result<()> {
    let summary = crate::cleanup::summarise(hits);
    let total_bytes: u64 = summary.iter().map(|s| s.bytes).sum();
    let total_physical_bytes: u64 = summary.iter().map(|s| s.physical_bytes).sum();

    if format.is_machine() {
        let mut payload = json!({
            "schema_version": SCHEMA_VERSION,
            "kind": "cleanup",
            "applied": applied.is_some(),
            "removed": applied.unwrap_or(&[]),
            "history_id": history_id,
            "records": hits,
            "summary": summary,
            "total_bytes": total_bytes,
            "total_physical_bytes": total_physical_bytes,
        });
        if let Some(d) = disk {
            payload["disk"] = serde_json::to_value(d)?;
        }
        if matches!(format, Format::Ndjson) {
            for h in hits {
                println!("{}", serde_json::to_string(h)?);
            }
        } else {
            println!("{}", serde_json::to_string(&payload)?);
        }
        return Ok(());
    }
    println!(
        "{:<14} {:<7} {:>12} {:>10}  PATH",
        "CATEGORY", "SAFETY", "SIZE", "FILES"
    );
    println!("{}", "-".repeat(88));
    for h in hits {
        println!(
            "{:<14} {:<7} {:>12} {:>10}  {}",
            h.category,
            h.safety.as_str(),
            format_size(h.bytes, BINARY),
            h.files,
            h.path,
        );
    }
    if !summary.is_empty() {
        println!();
        println!(
            "{:<14} {:<7} {:>12} {:>10} {:>8}",
            "CATEGORY", "SAFETY", "TOTAL", "FILES", "PATHS"
        );
        println!("{}", "-".repeat(58));
        for s in &summary {
            println!(
                "{:<14} {:<7} {:>12} {:>10} {:>8}",
                s.category,
                s.safety.as_str(),
                format_size(s.bytes, BINARY),
                s.files,
                s.paths,
            );
        }
        println!(
            "{:<14} {:<7} {:>12}",
            "TOTAL",
            "",
            format_size(total_bytes, BINARY)
        );
    }
    if let Some(rm) = applied {
        println!();
        println!("Removed {} path(s).", rm.len());
        if let Some(id) = history_id {
            println!("Recorded as history entry {id} (see `disky history`).");
        }
    } else {
        println!();
        println!("(dry-run — re-run with --apply to delete)");
    }
    Ok(())
}

/// `disky prefs` (REQ-DSCR-001.A1): one `Prefs` record under kind `prefs`,
/// or `key = value` lines on a TTY (dotted keys for nested values).
pub fn prefs(p: &crate::prefs::Prefs, format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(std::slice::from_ref(p), "prefs", format);
    }
    let v = serde_json::to_value(p)?;
    let show = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    };
    for key in crate::prefs::KEYS {
        let mut cur = &v;
        for part in key.split('.') {
            cur = &cur[part];
        }
        println!("{key:<20} = {}", show(cur));
    }
    Ok(())
}

/// Clean-history log (REQ-DCLEAN-034, REQ-DSCR-015), newest first. JSON
/// records are [`HistoryRow`]s — each entry plus the `detail` and `state` the
/// GUI's History screen shows — and the envelope carries `total_bytes` over
/// the whole log, from the same [`crate::contract::history_view`].
pub fn history(view: &HistoryView, limit: usize, format: Format) -> Result<()> {
    let rows = &view.rows[..view.rows.len().min(limit)];
    match format {
        Format::Json => {
            let payload = json!({
                "schema_version": SCHEMA_VERSION,
                "kind": "history",
                "total_bytes": view.total_bytes,
                "records": rows,
            });
            println!("{}", serde_json::to_string(&payload)?);
            return Ok(());
        }
        Format::Ndjson => return emit_records(rows, "history", format),
        Format::Text => {}
    }
    if rows.is_empty() {
        println!("No cleanup history yet.");
        return Ok(());
    }
    println!(
        "{:<28} {:<18} {:<24} {:<6} {:>12} {:>6}  {:<10} DETAIL",
        "ID", "TITLE", "WHEN", "MODE", "FREED", "ITEMS", "STATE"
    );
    println!("{}", "-".repeat(125));
    for r in rows {
        let e = &r.entry;
        println!(
            "{:<28} {:<18} {:<24} {:<6} {:>12} {:>6}  {:<10} {}",
            e.id,
            e.title,
            e.ts,
            serde_json::to_value(e.mode)?.as_str().unwrap_or(""),
            format_size(e.bytes, BINARY),
            e.items.len(),
            serde_json::to_value(r.state)?.as_str().unwrap_or(""),
            r.detail,
        );
    }
    println!(
        "{} cleared in total.",
        format_size(view.total_bytes, BINARY)
    );
    Ok(())
}

/// `disky map` (REQ-DSCR-003): one envelope `{kind:"map", path, name, root,
/// total_bytes, capacity_bytes, records:[MapChild]}`, or a size table.
pub fn map(view: &MapView, format: Format) -> Result<()> {
    match format {
        Format::Json => {
            let mut payload = serde_json::to_value(view)?;
            payload["schema_version"] = json!(SCHEMA_VERSION);
            payload["kind"] = json!("map");
            println!("{}", serde_json::to_string(&payload)?);
            return Ok(());
        }
        Format::Ndjson => return emit_records(&view.records, "map", format),
        Format::Text => {}
    }
    println!(
        "{} · {} (physical)",
        view.path,
        format_size(view.total_bytes, BINARY)
    );
    println!("{:<70} {:>12}", "NAME", "SIZE");
    println!("{}", "-".repeat(83));
    for c in &view.records {
        let name = if c.has_children {
            format!("{}/", c.name)
        } else {
            c.name.clone()
        };
        println!(
            "{:<70} {:>12}",
            truncate(&name, 70),
            format_size(c.bytes, BINARY)
        );
    }
    if let Some(o) = &view.other {
        println!(
            "{:<70} {:>12}",
            format!("Other ({} items)", o.count),
            format_size(o.bytes, BINARY)
        );
    }
    Ok(())
}

pub fn restore(r: &RestoreReport, format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(std::slice::from_ref(r), "restore", format);
    }
    for p in &r.restored {
        println!("restored  {p}");
    }
    for p in &r.conflicts {
        println!("conflict  {p} (something already exists there; left in the Trash)");
    }
    for p in &r.missing {
        println!("missing   {p} (no longer in Trash)");
    }
    println!(
        "Entry {}: {} restored, {} conflict(s), {} no longer in Trash.",
        r.id,
        r.restored.len(),
        r.conflicts.len(),
        r.missing.len()
    );
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
    if let Err(e) = crate::platform::open_path(&out) {
        eprintln!("render: {e}");
    }
    eprintln!("Report: {}", out);
    Ok(())
}

/// `disky large` (REQ-DSCR-004): one envelope `{kind:"large", filter, count,
/// total_bytes, records:[LargeFile]}`, or a table with `*` marking files
/// unused for over a year.
pub fn large(view: &LargeView, format: Format) -> Result<()> {
    match format {
        Format::Json => {
            let mut payload = serde_json::to_value(view)?;
            payload["schema_version"] = json!(SCHEMA_VERSION);
            payload["kind"] = json!("large");
            println!("{}", serde_json::to_string(&payload)?);
            return Ok(());
        }
        Format::Ndjson => return emit_records(&view.records, "large", format),
        Format::Text => {}
    }
    if view.records.is_empty() {
        println!("No files over 2 GB.");
        return Ok(());
    }
    println!(
        "{} files · {} (physical, filter {})",
        view.count,
        format_size(view.total_bytes, BINARY),
        view.filter.as_str()
    );
    println!("{:>12}  {:<22} PATH", "SIZE", "LAST OPENED");
    println!("{}", "-".repeat(100));
    for f in &view.records {
        let opened = f.last_opened.as_deref().unwrap_or("unknown");
        let mark = if f.unused_1y { "*" } else { " " };
        println!(
            "{:>12}  {:<20}{mark}  {}",
            format_size(f.bytes, BINARY),
            opened,
            truncate(&f.path, 64)
        );
    }
    println!("* last opened more than a year ago");
    Ok(())
}

/// `disky dupes` (REQ-DSCR-005): one envelope `{kind:"dupes", min_size,
/// groups, extra_copies, reclaimable_bytes, records:[DupeGroup]}`, one group
/// per line in ndjson, or a table of groups with their copies.
pub fn dupes(view: &DupesView, format: Format) -> Result<()> {
    match format {
        Format::Json => {
            let mut payload = serde_json::to_value(view)?;
            payload["schema_version"] = json!(SCHEMA_VERSION);
            payload["kind"] = json!("dupes");
            println!("{}", serde_json::to_string(&payload)?);
            return Ok(());
        }
        Format::Ndjson => return emit_records(&view.records, "dupes", format),
        Format::Text => {}
    }
    if view.records.is_empty() {
        println!("No duplicates found.");
        return Ok(());
    }
    println!(
        "{} groups · {} extra copies · {} reclaimable",
        view.groups,
        view.extra_copies,
        format_size(view.reclaimable_bytes, BINARY)
    );
    println!("{:>12}  {:>6}  PATH", "EACH", "COPIES");
    println!("{}", "-".repeat(100));
    for g in &view.records {
        println!(
            "{:>12}  {:>6}  {}",
            format_size(g.bytes, BINARY),
            g.copies.len(),
            truncate(&g.name, 78)
        );
        for c in &g.copies {
            println!("{:>22}{}", "", truncate(&c.path, 78));
        }
    }
    Ok(())
}

/// `disky trash` (REQ-DSCR-006): one envelope `{kind:"trash", handled,
/// failed, freed_physical_bytes, history_id, disk}`, or a summary.
pub fn trash(r: &TrashResult, format: Format) -> Result<()> {
    if format.is_machine() {
        let mut payload = serde_json::to_value(r)?;
        payload["schema_version"] = json!(SCHEMA_VERSION);
        payload["kind"] = json!("trash");
        println!("{}", serde_json::to_string(&payload)?);
        return Ok(());
    }
    for p in &r.handled {
        println!("trashed  {p}");
    }
    for f in &r.failed {
        println!("failed   {} ({})", f.path, f.reason);
    }
    println!(
        "{} freed{}",
        format_size(r.freed_physical_bytes, BINARY),
        r.history_id
            .as_deref()
            .map(|id| format!(" · undo with `disky restore {id}`"))
            .unwrap_or_default()
    );
    Ok(())
}

/// `disky agents` (REQ-DSCR-007): `{kind:"agents", records:[Agent]}`, one
/// agent per line in ndjson, or a table.
pub fn agents(list: &[crate::agents::Agent], format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(list, "agents", format);
    }
    for a in list {
        let status = match &a.display_path {
            Some(p) => format!("Found · {p}"),
            None => "Not installed".to_string(),
        };
        println!("{:<8} {:<12} {status}", a.id, a.name);
    }
    Ok(())
}

/// `disky suggest` (REQ-DSCR-008): `{kind:"suggestions",
/// records:[Suggestion]}` in feed order, one per line in ndjson, or the
/// feed as text.
pub fn suggestions(list: &[crate::assistant::Suggestion], format: Format) -> Result<()> {
    if format.is_machine() {
        return emit_records(list, "suggestions", format);
    }
    for s in list {
        let size = s
            .bytes
            .map(|b| format!(" · {}", crate::assistant::format_bytes(b)))
            .unwrap_or_default();
        println!("[{}] {}{size}", s.kind, s.text);
    }
    Ok(())
}
