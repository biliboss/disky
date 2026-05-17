//! Find — and optionally remove — well-known disk-hoggy directories
//! (`node_modules`, `target`, `__pycache__`, …) discovered in a snapshot.

use anyhow::Result;
use duckdb::Connection;
use serde::Serialize;

/// Built-in target categories. Each maps to one or more directory basenames.
pub const TARGETS: &[(&str, &[&str])] = &[
    ("node_modules", &["node_modules"]),
    ("target", &["target"]),
    ("pycache", &["__pycache__"]),
    ("next", &[".next"]),
    ("dist", &["dist"]),
    ("build", &["build"]),
    ("venv", &[".venv", "venv"]),
    ("gradle", &[".gradle"]),
    ("pytest", &[".pytest_cache"]),
];

/// Default target set when the caller passes no `--target` flag.
pub fn default_target_names() -> Vec<&'static str> {
    TARGETS.iter().map(|(name, _)| *name).collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupHit {
    pub category: String,
    pub path: String,
    pub bytes: u64,
    pub files: u64,
}

fn basenames_for(targets: &[String]) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for t in targets {
        if let Some((name, basenames)) = TARGETS.iter().find(|(n, _)| n == t) {
            for b in *basenames {
                out.push((*name, *b));
            }
        }
    }
    out
}

pub fn scan(conn: &Connection, targets: &[String], limit: usize) -> Result<Vec<CleanupHit>> {
    let pairs = basenames_for(targets);
    if pairs.is_empty() {
        return Ok(vec![]);
    }

    let basenames: Vec<String> = pairs.iter().map(|(_, b)| (*b).to_string()).collect();
    let placeholders: Vec<String> = (0..basenames.len()).map(|_| "?".to_string()).collect();

    // Find candidate directories. Sum children via prefix match on path.
    // `WHERE name IN (?, ?, …) AND is_dir`. Cap with subquery LIMIT for safety.
    let sql = format!(
        "WITH targets AS (
             SELECT path, name FROM files
             WHERE is_dir AND name IN ({})
         )
         SELECT t.name, t.path,
                COALESCE(SUM(f.size), 0) AS bytes,
                COUNT(f.path) AS files
         FROM targets t
         LEFT JOIN files f
           ON f.path LIKE (t.path || '/%')
         GROUP BY t.name, t.path
         ORDER BY bytes DESC
         LIMIT ?",
        placeholders.join(",")
    );

    let mut params: Vec<duckdb::types::Value> = basenames
        .iter()
        .map(|b| duckdb::types::Value::Text(b.clone()))
        .collect();
    params.push(duckdb::types::Value::BigInt(limit as i64));

    let basename_to_category: std::collections::HashMap<&'static str, &'static str> =
        pairs.iter().map(|(cat, base)| (*base, *cat)).collect();

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn duckdb::ToSql> =
        params.iter().map(|v| v as &dyn duckdb::ToSql).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let name: String = row.get(0)?;
        let path: String = row.get(1)?;
        let bytes: i64 = row.get(2)?;
        let files: i64 = row.get(3)?;
        Ok((name, path, bytes as u64, files as u64))
    })?;

    let mut out = Vec::new();
    for r in rows.flatten() {
        let category = basename_to_category
            .get(r.0.as_str())
            .copied()
            .unwrap_or("unknown")
            .to_string();
        out.push(CleanupHit {
            category,
            path: r.1,
            bytes: r.2,
            files: r.3,
        });
    }
    Ok(out)
}

/// How destructive `apply` should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyMode {
    /// Permanently `remove_dir_all` each path.
    Delete,
    /// Move each path to `~/.Trash/<name>-<unix-secs>` so the user can undo.
    Trash,
}

/// Remove or trash the listed paths. Returns paths that were actually handled.
pub fn apply(hits: &[CleanupHit], mode: ApplyMode) -> Result<Vec<String>> {
    let mut handled = Vec::new();
    for h in hits {
        match mode {
            ApplyMode::Delete => match std::fs::remove_dir_all(&h.path) {
                Ok(()) => handled.push(h.path.clone()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            },
            ApplyMode::Trash => match move_to_trash(&h.path) {
                Ok(_) => handled.push(h.path.clone()),
                Err(e) => {
                    if !std::path::Path::new(&h.path).exists() {
                        // Vanished between scan and apply — skip silently.
                        continue;
                    }
                    return Err(e);
                }
            },
        }
    }
    Ok(handled)
}

fn move_to_trash(path: &str) -> Result<std::path::PathBuf> {
    let trash = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("home dir unavailable"))?
        .join(".Trash");
    std::fs::create_dir_all(&trash)?;
    let src = std::path::Path::new(path);
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "disky-trashed".into());
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest = trash.join(format!("{}-{}", name, ts));
    std::fs::rename(src, &dest)?;
    Ok(dest)
}
