use anyhow::{anyhow, Result};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};

pub fn snapshot_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("disky")
}

pub fn new_snapshot_path() -> Result<String> {
    let dir = snapshot_dir();
    fs::create_dir_all(&dir)?;
    let ts = Local::now().format("%Y-%m-%d_%H-%M").to_string();
    Ok(dir
        .join(format!("{}.db", ts))
        .to_string_lossy()
        .into_owned())
}

pub fn latest_snapshot() -> Option<String> {
    let dir = snapshot_dir();
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    entries
        .last()
        .map(|e| e.path().to_string_lossy().into_owned())
}

pub fn list_snapshots() -> Vec<(String, u64)> {
    let dir = snapshot_dir();
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
        .map(|e| {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            (e.path().to_string_lossy().into_owned(), size)
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

/// Resolve a snapshot spec (`@latest`, an ID like `2026-05-15_11-56`, or a
/// filesystem path) to an absolute DB path. IDs look up `<data_dir>/<id>.db`.
/// Paths are returned untouched.
pub fn resolve(spec: &str) -> Result<String> {
    if spec == "@latest" {
        return latest_snapshot()
            .ok_or_else(|| anyhow!("no snapshot found; run `disky scan` first (not found)"));
    }
    if spec.contains('/') || Path::new(spec).extension().is_some() {
        return Ok(spec.to_string());
    }
    // Treat as an ID — file stem within the data directory.
    let candidate = snapshot_dir().join(format!("{}.db", spec));
    if candidate.exists() {
        return Ok(candidate.to_string_lossy().into_owned());
    }
    Err(anyhow!(
        "snapshot '{}' not found in {} (not found)",
        spec,
        snapshot_dir().display()
    ))
}

/// File stem used as snapshot ID — `2026-05-15_11-56.db` → `2026-05-15_11-56`.
pub fn id_for(path: &str) -> Option<String> {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
}
