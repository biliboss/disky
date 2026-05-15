use anyhow::Result;
use chrono::Local;
use std::fs;
use std::path::PathBuf;

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
