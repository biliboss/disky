//! disky desktop GUI — Tauri backend.
//!
//! The backend is a thin client of the `disky` library crate (REQ-DCLEAN-001):
//! each command delegates to `disky::*` and never reimplements file walking,
//! size summing or deletion. Each has a CLI equivalent (REQ-DCLEAN-001.A1):
//!
//! | Command             | CLI                                              |
//! |---------------------|--------------------------------------------------|
//! | `load_cleanup_view` | `disky cleanup --snapshot <ref> --format json`   |
//! | `scan`              | `disky scan <root>`                              |
//! | `stop_scan`         | Ctrl-C during `disky scan`                       |
//! | `clean`             | `disky cleanup --apply [--reversible]`           |
//!
//! `scripts/lint-boundary.mjs` enforces the boundary on this file.
//!
//! @implements REQ-DCLEAN-001
//! @implements REQ-DCLEAN-027

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use disky::cleanup::CleanupHit;
use disky::contract::{self, CleanResult, CleanupView, Preferences};
use disky::{history, scan as engine_scan, snapshots};
use tauri::{AppHandle, Emitter, Manager};

/// Least time between two progress events on `scan://progress`.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Per-window engine state.
#[derive(Default)]
struct Engine {
    /// Hits behind the last view the user was shown. `clean` acts only on
    /// paths found here (REQ-DCLEAN-007, REQ-DCLEAN-029.A1).
    reviewed: Mutex<Vec<CleanupHit>>,
    /// Cancel flag of the running scan, if any.
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

type CmdResult<T> = Result<T, String>;

fn msg(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Run blocking engine work off the IPC thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(msg)
}

/// Build a snapshot's view and remember its hits as the reviewed set.
fn load_view(
    app: &AppHandle,
    snapshot_path: &str,
    goal: Option<u64>,
) -> anyhow::Result<CleanupView> {
    let (view, hits) = contract::load(
        snapshot_path,
        goal.unwrap_or(contract::DEFAULT_GOAL_BYTES),
        Preferences::default(),
    )?;
    *app.state::<Engine>().reviewed.lock().unwrap() = hits;
    Ok(view)
}

#[tauri::command]
async fn load_cleanup_view(
    app: AppHandle,
    snapshot: Option<String>,
    goal_bytes: Option<u64>,
) -> CmdResult<CleanupView> {
    blocking(move || {
        let path = snapshots::resolve(snapshot.as_deref().unwrap_or("@latest"))?;
        load_view(&app, &path, goal_bytes)
    })
    .await
}

#[tauri::command]
async fn scan(app: AppHandle, root: Option<String>) -> CmdResult<CleanupView> {
    blocking(move || {
        let root = root.unwrap_or_else(|| "/".to_string());
        let db_path = snapshots::new_snapshot_path()?;
        let cancel = Arc::new(AtomicBool::new(false));
        *app.state::<Engine>().cancel.lock().unwrap() = Some(Arc::clone(&cancel));
        let used = contract::disk_usage(Path::new(&root))?.used_bytes();
        let mut last: Option<Instant> = None;
        let outcome = engine_scan::run_observed(&root, &db_path, cancel, &mut |ev| {
            if let engine_scan::ScanEvent::Tick(t) = ev {
                if last.is_none_or(|l| l.elapsed() >= PROGRESS_EVERY) {
                    let _ = app.emit(
                        contract::SCAN_PROGRESS_EVENT,
                        contract::scan_progress(t, used),
                    );
                    last = Some(Instant::now());
                }
            }
        });
        *app.state::<Engine>().cancel.lock().unwrap() = None;
        outcome?;
        let view = load_view(&app, &db_path, None)?;
        let _ = app.emit(
            contract::SCAN_PROGRESS_EVENT,
            contract::scan_finished(&view),
        );
        Ok(view)
    })
    .await
}

#[tauri::command]
fn stop_scan(app: AppHandle) {
    if let Some(c) = app.state::<Engine>().cancel.lock().unwrap().as_ref() {
        c.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
async fn clean(
    app: AppHandle,
    item_paths: Vec<String>,
    mode: history::Mode,
) -> CmdResult<CleanResult> {
    blocking(move || {
        let reviewed = app.state::<Engine>().reviewed.lock().unwrap().clone();
        contract::clean(&reviewed, &item_paths, mode, &history::log_path())
    })
    .await
}

fn main() {
    tauri::Builder::default()
        .manage(Engine::default())
        .invoke_handler(tauri::generate_handler![
            load_cleanup_view,
            scan,
            stop_scan,
            clean
        ])
        .run(tauri::generate_context!())
        .expect("error while running disky GUI");
}
