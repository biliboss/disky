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

/// Hardcoded safety: paths matching ANY of these substrings are NEVER
/// returned by cleanup, regardless of how their basename matched.
///
/// Why: `cleanup --target target` once matched
/// `~/.cargo/registry/src/index.crates.io-*/cc-1.2.62/src/target/` —
/// the `cc` Rust crate's source module dir, NOT a cargo build output.
/// Running that with `--apply` broke the host's build. This list is
/// the floor; `.diskyignore` will extend it once we ship that feature.
pub const ALWAYS_SKIP_SUBSTRINGS: &[&str] = &[
    // Rust / Cargo
    "/.cargo/registry/",
    "/.cargo/git/",
    "/.rustup/",
    // Node / npm / pnpm
    "/.npm/",
    "/.pnpm-store/",
    "/.yarn/cache/",
    // Python / pip
    "/site-packages/",
    "/.cache/pip/",
    "/.cache/uv/",
    // OS caches
    "/Library/Caches/",
    "/Library/Group Containers/",
    "/Library/Application Support/",
    // Plugin / extension sources
    "/.vscode/extensions/",
    "/.windsurf/extensions/",
    "/.antigravity/",
];

#[inline]
fn is_protected(path: &str) -> bool {
    ALWAYS_SKIP_SUBSTRINGS.iter().any(|s| path.contains(s))
}

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

    #[test]
    fn is_protected_catches_cargo_registry() {
        // The bug that started the ignore list: cc-1.2.62/src/target inside
        // ~/.cargo/registry was matched as a `target` cleanup candidate.
        assert!(is_protected(
            "/Users/me/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/cc-1.2.62/src/target"
        ));
        assert!(is_protected("/Users/me/.cargo/git/checkouts/foo"));
        assert!(is_protected(
            "/Users/me/.rustup/toolchains/stable-aarch64-apple-darwin/lib"
        ));
    }

    #[test]
    fn is_protected_catches_python_site_packages() {
        // venv installs live under site-packages/<pkg>/. We don't want to
        // accidentally clean a `build` subdir of an installed package.
        assert!(is_protected(
            "/Users/me/venv/lib/python3.13/site-packages/foo/build"
        ));
    }

    #[test]
    fn is_protected_catches_os_caches() {
        assert!(is_protected("/Users/me/Library/Caches/Slack/build"));
        assert!(is_protected(
            "/Users/me/Library/Application Support/disky/build"
        ));
        assert!(is_protected(
            "/Users/me/Library/Group Containers/HUAQ24HBR6.dev.orbstack/data"
        ));
    }

    #[test]
    fn is_protected_passes_real_dev_dirs() {
        // These are the common-case targets we DO want to clean.
        assert!(!is_protected("/Users/me/src/myapp/target"));
        assert!(!is_protected("/Users/me/src/myapp/node_modules"));
        assert!(!is_protected("/Users/me/src/myapp/.venv"));
        assert!(!is_protected("/Users/me/src/myapp/build"));
        assert!(!is_protected("/Users/me/src/myapp/dist"));
    }
}

pub fn scan(conn: &Connection, targets: &[String], limit: usize) -> Result<Vec<CleanupHit>> {
    let pairs = basenames_for(targets);
    if pairs.is_empty() {
        return Ok(vec![]);
    }

    let basenames: Vec<String> = pairs.iter().map(|(_, b)| (*b).to_string()).collect();
    let placeholders: Vec<String> = (0..basenames.len()).map(|_| "?".to_string()).collect();

    // History (kept for context):
    //   v1: `LEFT JOIN files f ON f.path LIKE (t.path || '/%')` — 79 s on a
    //       1.77 M-row snapshot. DuckDB planned a nested-loop join with
    //       per-row LIKE evaluation.
    //   v2: materialise target dirs in Rust, then issue ONE aggregation
    //       query per target with a path-range predicate. Better than v1
    //       but still O(N_targets × N_files / zone_maps): on real macOS
    //       homes with thousands of `target` / `node_modules` dirs the
    //       per-query overhead (DuckDB statement dispatch, FFI round-trip)
    //       dominates and the whole call lands at 79–250 s.
    //   v3 (this): one grouped query. Stash target dirs in a temporary
    //       table (DuckDB then computes its zone maps) and let the planner
    //       fuse them with `files` via a single range-join + GROUP BY.
    //       Brings 79 s → < 1 s on a 1 M-row synthetic snapshot.
    //
    // We materialise target dirs into a TEMP table (instead of an inline
    // VALUES list) for two reasons: (a) the protected-path filter is
    // applied in Rust, which is simpler than encoding 14 `NOT path LIKE`
    // predicates in SQL; (b) DuckDB can build zone maps on the temp table,
    // which the IEJoin planner exploits.
    let target_sql = format!(
        "SELECT path, name FROM files WHERE is_dir AND name IN ({})",
        placeholders.join(",")
    );
    let target_params: Vec<duckdb::types::Value> = basenames
        .iter()
        .map(|b| duckdb::types::Value::Text(b.clone()))
        .collect();
    let target_refs: Vec<&dyn duckdb::ToSql> = target_params
        .iter()
        .map(|v| v as &dyn duckdb::ToSql)
        .collect();
    let mut stmt = conn.prepare(&target_sql)?;
    let target_rows = stmt.query_map(target_refs.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    // Drop protected paths (cargo registry, node module sources, OS caches,
    // etc.). See ALWAYS_SKIP_SUBSTRINGS for the full list + rationale.
    let target_dirs: Vec<(String, String)> = target_rows
        .flatten()
        .filter(|(p, _)| !is_protected(p))
        .collect();
    drop(stmt);

    if target_dirs.is_empty() {
        return Ok(vec![]);
    }

    // Stage 2: materialise (path, lo, hi, name) into a temp table.
    // We pre-compute the [lo, hi) range bounds so the join predicate is
    // pure scalar (no string concatenation per-row).
    conn.execute_batch(
        "DROP TABLE IF EXISTS _cleanup_targets;
         CREATE TEMP TABLE _cleanup_targets (
             path TEXT NOT NULL,
             lo   TEXT NOT NULL,
             hi   TEXT NOT NULL,
             name TEXT NOT NULL
         );",
    )?;
    {
        let mut app = conn.appender("_cleanup_targets")?;
        for (path, name) in &target_dirs {
            let lo = format!("{}/", path);
            let hi = format!("{}0", path);
            app.append_row(duckdb::params![path, lo, hi, name])?;
        }
        app.flush()?;
    }

    // Stage 3: one grouped range-join. DuckDB plans this as an IEJoin
    // (inequality join), which is roughly linear in N_files + N_targets.
    let mut grouped = conn.prepare(
        "SELECT t.path,
                t.name,
                COALESCE(SUM(f.size), 0) AS bytes,
                COUNT(f.path)             AS files
         FROM _cleanup_targets t
         LEFT JOIN files f
           ON f.is_dir = false
          AND f.path >= t.lo
          AND f.path <  t.hi
         GROUP BY t.path, t.name",
    )?;
    let basename_to_category: std::collections::HashMap<&'static str, &'static str> =
        pairs.iter().map(|(cat, base)| (*base, *cat)).collect();

    let rows = grouped.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;

    let mut out: Vec<CleanupHit> = Vec::with_capacity(target_dirs.len());
    for row in rows.flatten() {
        let (path, name, bytes, files) = row;
        let category = basename_to_category
            .get(name.as_str())
            .copied()
            .unwrap_or("unknown")
            .to_string();
        out.push(CleanupHit {
            category,
            path,
            bytes: bytes as u64,
            files: files as u64,
        });
    }
    drop(grouped);
    let _ = conn.execute_batch("DROP TABLE IF EXISTS _cleanup_targets;");

    out.sort_by_key(|h| std::cmp::Reverse(h.bytes));
    out.truncate(limit);
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
