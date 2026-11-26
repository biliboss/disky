use anyhow::Result;
use flume::bounded;
use indicatif::{ProgressBar, ProgressStyle};
use jwalk::{Parallelism, WalkDir};
use memchr::memrchr;
use serde_json::json;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::db::{self, FileRecord};
use crate::query::SCHEMA_VERSION;

const BATCH_SIZE: usize = 50_000;
const CHANNEL_CAP: usize = 256;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

/// Abstraction over progress reporting: indicatif spinner on a TTY, NDJSON
/// events on stderr when piped (so agents can monitor without parsing prose).
enum Progress {
    Tty(ProgressBar),
    Ndjson { last: Instant },
}

impl Progress {
    fn new() -> Self {
        if std::io::stderr().is_terminal() {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.cyan} {msg} [{elapsed}] [{per_sec}]")
                    .unwrap(),
            );
            pb.enable_steady_tick(Duration::from_millis(80));
            Progress::Tty(pb)
        } else {
            // Emit initial start event so agents see something immediately.
            eprintln!(
                "{}",
                json!({
                    "schema_version": SCHEMA_VERSION,
                    "event": "start",
                })
            );
            Progress::Ndjson {
                last: Instant::now() - PROGRESS_INTERVAL,
            }
        }
    }

    fn tick(&mut self, scanned: u64, bytes: u64) {
        match self {
            Progress::Tty(pb) => {
                pb.set_message(format!("Scanned {:>9} entries", scanned));
            }
            Progress::Ndjson { last } => {
                if last.elapsed() >= PROGRESS_INTERVAL {
                    eprintln!(
                        "{}",
                        json!({
                            "schema_version": SCHEMA_VERSION,
                            "event": "progress",
                            "scanned": scanned,
                            "bytes": bytes,
                        })
                    );
                    *last = Instant::now();
                }
            }
        }
    }

    fn message(&self, msg: &str) {
        if let Progress::Tty(pb) = self {
            pb.set_message(msg.to_string());
        }
    }

    fn finish(self, scanned: u64, bytes: u64, db_path: &str) {
        match self {
            Progress::Tty(pb) => {
                pb.finish_with_message(format!("Done: {} entries → {}", scanned, db_path));
            }
            Progress::Ndjson { .. } => {
                eprintln!(
                    "{}",
                    json!({
                        "schema_version": SCHEMA_VERSION,
                        "event": "done",
                        "scanned": scanned,
                        "bytes": bytes,
                        "db": db_path,
                    })
                );
            }
        }
    }
}

/// Outcome of a scan — `complete=false` when the user cancelled mid-flight.
/// The DB on disk is still queryable (partial); main maps `complete=false` to
/// `ExitCode::PartialScan` (5).
#[derive(Debug, Clone, Copy)]
pub struct ScanOutcome {
    pub complete: bool,
    pub entries: u64,
    pub bytes: u64,
    /// Paths the walk could not read for permission reasons
    /// (REQ-DCLEAN-018.A2). Recorded in `scan_meta.skipped`.
    pub skipped: u64,
}

pub fn run(root: &str, db_path: &str) -> Result<ScanOutcome> {
    run_cancellable(root, db_path, Arc::new(AtomicBool::new(false)))
}

/// Progress of a running scan, reported after each batch is written.
#[derive(Debug, Clone, Default)]
pub struct ScanTick {
    /// Entries (files and dirs) recorded so far.
    pub entries: u64,
    /// Logical bytes seen so far.
    pub bytes: u64,
    /// Physical bytes seen so far (`st_blocks * 512`; logical off Unix).
    pub physical_bytes: u64,
    /// Last path of the batch just written.
    pub current_path: String,
}

/// What a scan reports to its observer.
#[derive(Debug, Clone)]
pub enum ScanEvent<'a> {
    Tick(&'a ScanTick),
    /// The walk is over; indexes are being built.
    Indexing,
}

pub fn run_cancellable(root: &str, db_path: &str, cancel: Arc<AtomicBool>) -> Result<ScanOutcome> {
    let mut progress = Progress::new();
    let outcome = run_observed(root, db_path, cancel, &mut |ev| match ev {
        ScanEvent::Tick(t) => progress.tick(t.entries, t.bytes),
        ScanEvent::Indexing => progress.message("Building indexes..."),
    })?;
    progress.finish(outcome.entries, outcome.bytes, db_path);
    Ok(outcome)
}

/// [`run_cancellable`] that reports progress to `observe` instead of the
/// terminal. The desktop GUI uses this to stream scan progress.
pub fn run_observed(
    root: &str,
    db_path: &str,
    cancel: Arc<AtomicBool>,
    observe: &mut dyn FnMut(ScanEvent<'_>),
) -> Result<ScanOutcome> {
    // Stored with `/` separators on every OS (REQ-XPLAT-001).
    let root_slash = crate::platform::to_slash_str(root);
    let root = root_slash.as_str();
    let conn = db::open(db_path)?;
    db::create_schema(&conn)?;

    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    db::write_scan_meta(&conn, root, started_at, None, false, 0, 0)?;

    let cpus = num_cpus::get();

    let counter = Arc::new(AtomicU64::new(0));
    let bytes_seen = Arc::new(AtomicU64::new(0));
    let physical_seen = Arc::new(AtomicU64::new(0));
    let (tx, rx) = bounded::<Vec<FileRecord>>(CHANNEL_CAP);

    let root_owned = root.to_string();
    let counter_clone = Arc::clone(&counter);
    let bytes_clone = Arc::clone(&bytes_seen);
    let physical_clone = Arc::clone(&physical_seen);
    let cancel_clone = Arc::clone(&cancel);
    let skipped = Arc::new(AtomicU64::new(0));
    let skipped_clone = Arc::clone(&skipped);

    let walker = thread::Builder::new()
        .name("walker".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || -> Result<()> {
            let mut batch: Vec<FileRecord> = Vec::with_capacity(BATCH_SIZE);

            for entry in WalkDir::new(&root_owned)
                .skip_hidden(false)
                .follow_links(false)
                .parallelism(Parallelism::RayonNewPool(cpus))
                .into_iter()
            {
                if cancel_clone.load(Ordering::Relaxed) {
                    break;
                }
                let denied = |e: &jwalk::Error| {
                    e.io_error()
                        .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
                };
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        if denied(&e) {
                            skipped_clone.fetch_add(1, Ordering::Relaxed);
                        }
                        continue;
                    }
                };
                // jwalk reports an unreadable directory on its own entry.
                if entry.read_children_error.as_ref().is_some_and(denied) {
                    skipped_clone.fetch_add(1, Ordering::Relaxed);
                }
                let path = entry.path();
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(e) => {
                        if denied(&e) {
                            skipped_clone.fetch_add(1, Ordering::Relaxed);
                        }
                        continue;
                    }
                };

                let is_dir = meta.is_dir();
                let path_owned: String = crate::platform::to_slash(&path);
                let path_bytes = path_owned.as_bytes();

                let name_start = memrchr(b'/', path_bytes).map(|i| i + 1).unwrap_or(0);
                let name = path_owned[name_start..].to_string();

                let ext = if is_dir {
                    None
                } else {
                    memrchr(b'.', &path_bytes[name_start..])
                        .filter(|&i| name_start + i + 1 < path_bytes.len())
                        .map(|i| path_owned[name_start + i + 1..].to_lowercase())
                };

                let size = if is_dir { 0 } else { meta.len() as i64 };

                // Physical bytes on disk: `st_blocks * 512` on Unix.
                // Fixes APFS sparse file reporting (OrbStack data.img.raw
                // reported 8.8 TB logical on a 256 GB SSD before this).
                #[cfg(unix)]
                let physical_size = {
                    use std::os::unix::fs::MetadataExt;
                    if is_dir {
                        None
                    } else {
                        Some(meta.blocks() as i64 * 512)
                    }
                };
                // No block count on Windows: the length is the best figure
                // std offers (REQ-XPLAT-007).
                #[cfg(not(unix))]
                let physical_size: Option<i64> = (!is_dir).then_some(size);

                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);

                // Last access (REQ-DSCR-004.A1). `stat` itself never
                // updates it, so the scan leaves it as it found it.
                let atime = meta
                    .accessed()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);

                batch.push(FileRecord {
                    path: path_owned,
                    name,
                    ext,
                    size,
                    physical_size,
                    mtime,
                    is_dir,
                    depth: entry.depth() as i32,
                    atime,
                });

                counter_clone.fetch_add(1, Ordering::Relaxed);
                if size > 0 {
                    bytes_clone.fetch_add(size as u64, Ordering::Relaxed);
                }
                let physical = physical_size.unwrap_or(size);
                if physical > 0 {
                    physical_clone.fetch_add(physical as u64, Ordering::Relaxed);
                }

                if batch.len() >= BATCH_SIZE {
                    let send = std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE));
                    if tx.send(send).is_err() {
                        break;
                    }
                }
            }

            if !batch.is_empty() {
                let _ = tx.send(batch);
            }
            Ok(())
        })?;

    let mut tick = ScanTick::default();
    for batch in rx {
        db::append_batch(&conn, &batch)?;
        tick.entries = counter.load(Ordering::Relaxed);
        tick.bytes = bytes_seen.load(Ordering::Relaxed);
        tick.physical_bytes = physical_seen.load(Ordering::Relaxed);
        if let Some(last) = batch.last() {
            tick.current_path.clone_from(&last.path);
        }
        observe(ScanEvent::Tick(&tick));
    }

    walker.join().expect("walker thread panicked")?;

    let cancelled = cancel.load(Ordering::Relaxed);

    observe(ScanEvent::Indexing);
    db::build_indexes(&conn)?;

    let total = counter.load(Ordering::Relaxed);
    let bytes = bytes_seen.load(Ordering::Relaxed);
    let ended_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .ok();
    db::write_scan_meta(&conn, root, started_at, ended_at, !cancelled, total, bytes)?;
    let skipped = skipped.load(Ordering::Relaxed);
    db::write_scan_skipped(&conn, skipped)?;
    Ok(ScanOutcome {
        complete: !cancelled,
        entries: total,
        bytes,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every regular file gets a physical size, and paths are stored with
    /// `/` separators (REQ-XPLAT-001.A1, REQ-XPLAT-007.A1).
    #[test]
    fn physical_size_is_recorded_for_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        std::fs::create_dir_all(root.join("a b")).unwrap();
        std::fs::write(root.join("a b/x.txt"), vec![7u8; 5000]).unwrap();
        let db_path = tmp.path().join("s.db");
        let db_path = db_path.to_str().unwrap();
        run_observed(
            root.to_str().unwrap(),
            db_path,
            Arc::new(AtomicBool::new(false)),
            &mut |_| {},
        )
        .unwrap();
        let conn = db::open(db_path).unwrap();
        let (path, name, physical): (String, String, Option<i64>) = conn
            .query_row(
                "SELECT path, name, physical_size FROM files WHERE NOT is_dir",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(name, "x.txt");
        assert!(!path.contains('\\') || cfg!(unix), "{path}");
        assert_eq!(path, crate::platform::to_slash(&root.join("a b/x.txt")));
        assert!(physical.is_some_and(|p| p > 0), "{physical:?}");
    }
}
