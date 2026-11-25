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
//! | `restore`           | `disky restore <history_id>`                     |
//! | `open_full_disk_access` | `open "<contract::FULL_DISK_ACCESS_URL>"`    |
//! | `get_prefs`         | `disky prefs --format json`                      |
//! | `set_prefs`         | `disky prefs --set <key>=<value>`                |
//! | `pick_folder`       | (native picker only; the path is then saved with `set_prefs`) |
//! | `list_history`      | `disky history --format json`                    |
//! | `load_map`          | `disky map [<path>] --format json`               |
//! | `reveal_in_finder`  | `open -R <path>` (a path `load_map` listed)      |
//!
//! Preferences live in the engine's one store (`disky::prefs`): every view
//! reads its goal and toggles from it, and Settings writes it through
//! `set_prefs`, so the CLI and the GUI never hold copies (REQ-DSCR-001.A2).
//!
//! Errors reach the GUI as `contract::EngineError`, typed by disky's
//! exit-code slug (REQ-DCLEAN-018.A1).
//!
//! `scripts/lint-boundary.mjs` enforces the boundary on this file.
//!
//! @implements REQ-DCLEAN-001
//! @implements REQ-DCLEAN-008
//! @implements REQ-DCLEAN-009
//! @implements REQ-DCLEAN-018
//! @implements REQ-DCLEAN-027
//! @implements REQ-DSCR-001
//! @implements REQ-DSCR-016
//! @implements REQ-DSCR-003
//! @implements REQ-DSCR-012
//! @implements REQ-DSCR-015

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use disky::cleanup::CleanupHit;
use disky::contract::{
    self, CleanResult, CleanupView, EngineError, HistoryView, MapView, RestoreResult,
};
use disky::prefs::{self, Prefs};
use disky::{history, scan as engine_scan, snapshots};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

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
    /// Paths the last Space map listed. `reveal_in_finder` reveals only
    /// these (REQ-DSCR-012.A4).
    mapped: Mutex<Vec<String>>,
}

type CmdResult<T> = Result<T, EngineError>;

/// Run blocking engine work off the IPC thread.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| EngineError::from(anyhow::Error::from(e)))?
        .map_err(EngineError::from)
}

/// The shared preferences store (REQ-DSCR-001). A malformed file is an
/// error, never replaced by defaults.
fn read_prefs() -> anyhow::Result<Prefs> {
    prefs::load(&prefs::prefs_path())
}

/// Build a snapshot's view from the store's preferences and remember its hits
/// as the reviewed set.
fn load_view(app: &AppHandle, snapshot_path: &str) -> anyhow::Result<CleanupView> {
    let (view, hits) = contract::load(snapshot_path, &read_prefs()?)?;
    *app.state::<Engine>().reviewed.lock().unwrap() = hits;
    Ok(view)
}

#[tauri::command]
async fn load_cleanup_view(app: AppHandle, snapshot: Option<String>) -> CmdResult<CleanupView> {
    blocking(move || {
        let spec = snapshot.as_deref().unwrap_or("@latest");
        // No snapshot yet: live disk, no categories, "No scans yet"
        // (REQ-DCLEAN-003.A2). Nothing is reviewed, so nothing can be cleaned.
        if spec == "@latest" && snapshots::latest_snapshot().is_none() {
            app.state::<Engine>().reviewed.lock().unwrap().clear();
            return contract::empty_view(&read_prefs()?);
        }
        let path = snapshots::resolve(spec)?;
        load_view(&app, &path)
    })
    .await
}

/// Every preference. CLI: `disky prefs --format json`.
#[tauri::command]
async fn get_prefs() -> CmdResult<Prefs> {
    blocking(read_prefs).await
}

/// Set one preference and return the store as saved. CLI: `disky prefs
/// --set <key>=<value>`. A bad key or value is a `usage` error and nothing
/// is written (REQ-DSCR-001.A1).
#[tauri::command]
async fn set_prefs(key: String, value: String) -> CmdResult<Prefs> {
    blocking(move || prefs::set(&prefs::prefs_path(), &key, &value)).await
}

/// The native folder picker (REQ-DSCR-016.A3). Returns the chosen folder in
/// the store's display form (`~/…` under home), or null when cancelled. It
/// writes nothing: Settings saves the new list with `set_prefs`.
#[tauri::command]
async fn pick_folder(app: AppHandle) -> CmdResult<Option<String>> {
    blocking(move || {
        let Some(picked) = app.dialog().file().blocking_pick_folder() else {
            return Ok(None);
        };
        let path = picked.into_path()?;
        Ok(Some(prefs::abbreviate_home(&path, dirs_home().as_deref())))
    })
    .await
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
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
        let view = load_view(&app, &db_path)?;
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

/// Clean the reviewed `item_paths` in the order given, emitting one
/// `clean://progress` event per item (REQ-DCLEAN-008). Only paths from the
/// last reviewed view are acted on; any other path is refused by the engine.
#[tauri::command]
async fn clean(
    app: AppHandle,
    item_paths: Vec<String>,
    mode: history::Mode,
    opened_by: Option<String>,
) -> CmdResult<CleanResult> {
    blocking(move || {
        let reviewed = app.state::<Engine>().reviewed.lock().unwrap().clone();
        contract::clean_observed(
            &reviewed,
            &item_paths,
            mode,
            &history::log_path(),
            opened_by.as_deref(),
            |p| {
                let _ = app.emit(contract::CLEAN_PROGRESS_EVENT, p);
            },
        )
    })
    .await
}

/// Undo a Trash clean (REQ-DCLEAN-009.A4): restore its history entry.
#[tauri::command]
async fn restore(history_id: String) -> CmdResult<RestoreResult> {
    blocking(move || contract::restore(&history::log_path(), &history_id)).await
}

/// The History screen's rows (REQ-DSCR-015): the log newest first, each with
/// its detail and state, and the total cleared. CLI: `disky history`.
#[tauri::command]
async fn list_history() -> CmdResult<HistoryView> {
    blocking(|| contract::history_view_now(&history::log_path())).await
}

/// The Space map of `path` (default: the scan root) in the latest snapshot
/// (REQ-DSCR-003, REQ-DSCR-012); null when no snapshot exists yet. CLI:
/// `disky map [<path>] --format json`.
#[tauri::command]
async fn load_map(app: AppHandle, path: Option<String>) -> CmdResult<Option<MapView>> {
    blocking(move || {
        let Some(snapshot) = snapshots::latest_snapshot() else {
            app.state::<Engine>().mapped.lock().unwrap().clear();
            return Ok(None);
        };
        let view = contract::map_view(&snapshot, path.as_deref(), &read_prefs()?)?;
        *app.state::<Engine>().mapped.lock().unwrap() =
            view.records.iter().map(|c| c.path.clone()).collect();
        Ok(Some(view))
    })
    .await
}

/// Reveal a path in Finder (REQ-DSCR-012.A4) — only one the last
/// `load_map` listed; any other path is refused.
#[tauri::command]
async fn reveal_in_finder(app: AppHandle, path: String) -> CmdResult<()> {
    blocking(move || {
        let listed = app.state::<Engine>().mapped.lock().unwrap().contains(&path);
        if !listed {
            return Err(disky::exit::DiskyError::new(
                disky::exit::ExitCode::NotFound,
                "not on the map",
                format!("{path} is not on the Space map shown"),
            )
            .into());
        }
        let status = std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .status()?;
        anyhow::ensure!(status.success(), "open -R exited with {status}");
        Ok(())
    })
    .await
}

/// Open System Settings at Full Disk Access (REQ-DCLEAN-018.A2).
#[tauri::command]
async fn open_full_disk_access() -> CmdResult<()> {
    blocking(|| {
        let status = std::process::Command::new("open")
            .arg(contract::FULL_DISK_ACCESS_URL)
            .status()?;
        anyhow::ensure!(status.success(), "open exited with {status}");
        Ok(())
    })
    .await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Engine::default())
        .invoke_handler(tauri::generate_handler![
            load_cleanup_view,
            get_prefs,
            set_prefs,
            pick_folder,
            scan,
            stop_scan,
            clean,
            restore,
            list_history,
            load_map,
            reveal_in_finder,
            open_full_disk_access
        ])
        .run(tauri::generate_context!())
        .expect("error while running disky GUI");
}
