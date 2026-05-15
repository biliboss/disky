use anyhow::Result;
use flume::bounded;
use indicatif::{ProgressBar, ProgressStyle};
use jwalk::{Parallelism, WalkDir};
use memchr::memrchr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use crate::db::{self, FileRecord};

const BATCH_SIZE: usize = 50_000;
const CHANNEL_CAP: usize = 256;

pub fn run(root: &str, db_path: &str) -> Result<()> {
    let conn = db::open(db_path)?;
    db::create_schema(&conn)?;

    let cpus = num_cpus::get();

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg} [{elapsed}] [{per_sec}]")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(80));

    let counter = Arc::new(AtomicU64::new(0));
    let (tx, rx) = bounded::<Vec<FileRecord>>(CHANNEL_CAP);

    let root_owned = root.to_string();
    let pb_clone = pb.clone();
    let counter_clone = Arc::clone(&counter);

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
                .flatten()
            {
                let path = entry.path();
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let is_dir = meta.is_dir();
                let path_owned: String = path.to_string_lossy().into_owned();
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

                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64);

                batch.push(FileRecord {
                    path: path_owned,
                    name,
                    ext,
                    size,
                    mtime,
                    is_dir,
                    depth: entry.depth() as i32,
                });

                let n = counter_clone.fetch_add(1, Ordering::Relaxed) + 1;
                if n % 10_000 == 0 {
                    pb_clone.set_message(format!("Scanned {:>9} entries", n));
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

    for batch in rx {
        db::append_batch(&conn, &batch)?;
    }

    walker.join().expect("walker thread panicked")?;

    pb.set_message("Building indexes...");
    db::build_indexes(&conn)?;

    let total = counter.load(Ordering::Relaxed);
    pb.finish_with_message(format!("Done: {} entries → {}", total, db_path));
    Ok(())
}
