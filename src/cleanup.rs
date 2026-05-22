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

#[derive(Debug, Clone, Serialize)]
pub struct CategorySummary {
    pub category: String,
    pub paths: u64,
    pub bytes: u64,
    pub files: u64,
}

/// Aggregate `CleanupHit`s by category. Sorted by total `bytes` desc so the
/// biggest reclamation opportunity is first.
pub fn summarise(hits: &[CleanupHit]) -> Vec<CategorySummary> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<String, CategorySummary> = BTreeMap::new();
    for h in hits {
        let entry = acc
            .entry(h.category.clone())
            .or_insert_with(|| CategorySummary {
                category: h.category.clone(),
                paths: 0,
                bytes: 0,
                files: 0,
            });
        entry.paths += 1;
        entry.bytes += h.bytes;
        entry.files += h.files;
    }
    let mut out: Vec<CategorySummary> = acc.into_values().collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.bytes));
    out
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    fn hit(category: &str, bytes: u64, files: u64) -> CleanupHit {
        CleanupHit {
            category: category.to_string(),
            path: format!("/tmp/{}-{}", category, bytes),
            bytes,
            files,
        }
    }

    #[test]
    fn default_target_names_covers_all_categories() {
        let names = default_target_names();
        assert_eq!(names.len(), TARGETS.len());
        assert!(names.contains(&"node_modules"));
        assert!(names.contains(&"target"));
    }

    #[test]
    fn summarise_aggregates_and_sorts_by_bytes_desc() {
        let hits = vec![
            hit("node_modules", 1024, 5),
            hit("node_modules", 2048, 10),
            hit("target", 8192, 3),
        ];
        let summary = summarise(&hits);
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].category, "target");
        assert_eq!(summary[0].bytes, 8192);
        assert_eq!(summary[1].category, "node_modules");
        assert_eq!(summary[1].bytes, 3072);
        assert_eq!(summary[1].paths, 2);
        assert_eq!(summary[1].files, 15);
    }

    #[test]
    fn summarise_empty_input_returns_empty() {
        let summary = summarise(&[]);
        assert!(summary.is_empty());
    }

    #[test]
    fn basenames_for_skips_unknown_targets() {
        let targets = vec!["node_modules".to_string(), "totally-fake".to_string()];
        let pairs = basenames_for(&targets);
        assert!(pairs.iter().all(|(n, _)| *n == "node_modules"));
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn basenames_for_expands_multi_basename_categories() {
        let targets = vec!["venv".to_string()];
        let pairs = basenames_for(&targets);
        let names: Vec<_> = pairs.iter().map(|(_, b)| *b).collect();
        assert!(names.contains(&".venv"));
        assert!(names.contains(&"venv"));
    }
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
                Err(_) => {
                    if !std::path::Path::new(&h.path).exists() {
                        continue;
                    }
                    eprintln!("cleanup: skip {} (move-to-trash failed)", h.path);
                    continue;
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
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let stamp = format!("{}-{}", now.as_secs(), now.subsec_nanos());
    let mut dest = trash.join(format!("{}-{}", name, stamp));
    let mut n = 0u32;
    while dest.exists() {
        n += 1;
        dest = trash.join(format!("{}-{}-{}", name, stamp, n));
    }
    std::fs::rename(src, &dest)?;
    Ok(dest)
}
