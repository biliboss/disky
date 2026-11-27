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
//! | `load_large`        | `disky large [--filter …] --format json`         |
//! | `trash_large`       | `disky trash <path>... --title "Large files"`    |
//! | `load_dupes`        | `disky dupes --format json`                      |
//! | `trash_dupes`       | `disky trash <path>... --title "Duplicates"`     |
//! | `load_assistant`    | `disky suggest` + `disky agents` + `disky prefs` |
//!
//! Preferences live in the engine's one store (`disky::prefs`): every view
//! reads its goal and toggles from it, and Settings writes it through
//! `set_prefs`, so the CLI and the GUI never hold copies (REQ-DSCR-001.A2).
//!
//! While the app runs, `checks.rs` ticks the proactive-check scheduler
//! (`scheduler.rs`, REQ-DSCR-011): it reloads the Assistant view, starts
//! scheduled background scans and posts the below-goal notification. It never
//! cleans or trashes anything.
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
//! @implements REQ-DSCR-004
//! @implements REQ-DSCR-006
//! @implements REQ-DSCR-013
//! @implements REQ-DSCR-005
//! @implements REQ-DSCR-014
//! @implements REQ-DSCR-009
//! @implements REQ-DSCR-010
//! @implements REQ-DSCR-011

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod cache;
mod checks;
mod scheduler;

use disky::cleanup::CleanupHit;
use disky::contract::{
    self, AssistantView, CleanResult, CleanupView, DupesView, EngineError, HistoryView,
    LargeFilter, LargeView, MapView, RestoreResult, TrashResult,
};
use disky::prefs::{self, Prefs};
use disky::{history, scan as engine_scan, snapshots};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

/// Least time between two progress events on `scan://progress`.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Per-window engine state.
#[derive(Default)]
pub(crate) struct Engine {
    /// Hits behind the last view the user was shown. `clean` acts only on
    /// paths found here (REQ-DCLEAN-007, REQ-DCLEAN-029.A1).
    reviewed: Mutex<Vec<CleanupHit>>,
    /// The last cleanup view built, with its hits, keyed by
    /// `contract::view_key`. The Assistant and Smart Clean's review read the
    /// same view: on a large home it takes seconds to build, so "Review"
    /// reuses the Assistant's instead of rebuilding it. Cleared by anything
    /// that changes files (clean, trash, restore); a new scan is a new key.
    view_cache: Mutex<cache::Keyed<(CleanupView, Vec<CleanupHit>)>>,
    /// The last Duplicates result, keyed the same way. Hashing every
    /// candidate reads tens of GB on a real home, so a revisit reuses it
    /// (REQ-DSCR-014.A6). Cleared with the view.
    dupes_cache: Mutex<cache::Keyed<DupesView>>,
    /// Cancel flag of the running scan, if any.
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// Paths the last Space map listed. `reveal_in_finder` reveals only
    /// these (REQ-DSCR-012.A4).
    mapped: Mutex<Vec<String>>,
    /// Paths the last Large files list showed. `trash_large` trashes only
    /// these (REQ-DSCR-006.A2).
    large: Mutex<Vec<String>>,
    /// Copy paths of each group the last Duplicates result showed.
    /// `trash_dupes` trashes only these, and never every copy of a group
    /// (REQ-DSCR-006.A2, A3).
    dupes: Mutex<Vec<Vec<String>>>,
    /// Held by whichever scan runs — Smart Clean's or a scheduled one — so
    /// two never write at once (REQ-DSCR-011.A2).
    pub(crate) scan_lock: Mutex<()>,
    /// Stop flag of the running background scan, if any. A Smart Clean scan
    /// sets it and takes over.
    pub(crate) background_scan: Mutex<Option<Arc<AtomicBool>>>,
    /// A Smart Clean clean is running.
    cleaning: AtomicBool,
}

impl Engine {
    /// A Smart Clean scan or clean, or a background scan, is running: no
    /// background scan may start (REQ-DSCR-011.A2).
    pub(crate) fn busy(&self) -> bool {
        self.cancel.lock().unwrap().is_some()
            || self.cleaning.load(Ordering::SeqCst)
            || self.background_scan.lock().unwrap().is_some()
    }
}

/// Clears [`Engine::cleaning`] when a clean ends, however it ends.
struct Cleaning<'a>(&'a AtomicBool);

impl Drop for Cleaning<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
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
const VIEW_CACHE_TTL: Duration = Duration::from_secs(120);

/// The cleanup view and hits of `snapshot_path`, reusing the last one built
/// when nothing it depends on changed.
pub(crate) fn cached_load(
    app: &AppHandle,
    snapshot_path: &str,
) -> anyhow::Result<(CleanupView, Vec<CleanupHit>)> {
    let prefs = read_prefs()?;
    let key = contract::view_key(snapshot_path, &prefs)?;
    let engine = app.state::<Engine>();
    if let Some(hit) = engine
        .view_cache
        .lock()
        .unwrap()
        .get(&key, VIEW_CACHE_TTL, Instant::now())
    {
        return Ok(hit);
    }
    let (view, hits) = contract::load(snapshot_path, &prefs)?;
    engine
        .view_cache
        .lock()
        .unwrap()
        .put(key, Instant::now(), (view.clone(), hits.clone()));
    Ok((view, hits))
}

/// Files changed: the next view is built afresh.
fn forget_view(app: &AppHandle) {
    let engine = app.state::<Engine>();
    engine.view_cache.lock().unwrap().clear();
    engine.dupes_cache.lock().unwrap().clear();
}

/// Duplicates take far longer to find than the view to build, so they keep
/// longer; trashing anything still clears them at once.
const DUPES_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

fn load_view(app: &AppHandle, snapshot_path: &str) -> anyhow::Result<CleanupView> {
    let (view, hits) = cached_load(app, snapshot_path)?;
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
        let engine = app.state::<Engine>();
        // A background scan gives way: stop it, then wait for it to drain.
        let _scanning = loop {
            if let Some(stop) = engine.background_scan.lock().unwrap().as_ref() {
                stop.store(true, Ordering::Relaxed);
            }
            match engine.scan_lock.try_lock() {
                Ok(guard) => break guard,
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        };
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
        let engine = app.state::<Engine>();
        engine.cleaning.store(true, Ordering::SeqCst);
        let _cleaning = Cleaning(&engine.cleaning);
        let reviewed = engine.reviewed.lock().unwrap().clone();
        forget_view(&app);
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
async fn restore(app: AppHandle, history_id: String) -> CmdResult<RestoreResult> {
    blocking(move || {
        forget_view(&app);
        contract::restore(&history::log_path(), &history_id)
    })
    .await
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
        // The screen draws at most MAX_TILES blocks; the engine sums the rest
        // into `other` so a crowded folder never yields unreadable slivers.
        let view = contract::map_view(&snapshot, path.as_deref(), &read_prefs()?)?
            .fold(disky::map::MAX_TILES, disky::map::MIN_TILE_SHARE);
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
        // The map shows the last scan: a folder deleted since is still drawn,
        // but the file manager can't reveal it. Say so instead of "open -R
        // exited 1".
        contract::check_on_disk(&path)?;
        // Finder, Explorer or the freedesktop file manager (REQ-XPLAT-008).
        disky::platform::reveal_path(&path).map_err(|e| {
            anyhow::anyhow!(
                "{} could not reveal {path} ({e})",
                disky::platform::file_manager_name()
            )
        })?;
        Ok(())
    })
    .await
}

/// The Large files list for `filter` in the latest snapshot (REQ-DSCR-004,
/// REQ-DSCR-013); null when no snapshot exists yet. The listed paths become
/// the only ones `trash_large` will move. CLI: `disky large --filter <f>`.
#[tauri::command]
async fn load_large(app: AppHandle, filter: LargeFilter) -> CmdResult<Option<LargeView>> {
    blocking(move || {
        let Some(snapshot) = snapshots::latest_snapshot() else {
            app.state::<Engine>().large.lock().unwrap().clear();
            return Ok(None);
        };
        let view = contract::large_view(&snapshot, filter, &read_prefs()?)?;
        *app.state::<Engine>().large.lock().unwrap() =
            view.records.iter().map(|f| f.path.clone()).collect();
        Ok(Some(view))
    })
    .await
}

/// Move the selected large files to the Trash with one "Large files" history
/// entry (REQ-DSCR-006, REQ-DSCR-013.A4). Always the Trash, whatever
/// trash-by-default says. Handled paths leave the listed set. CLI: `disky
/// trash <path>... --title "Large files"`.
#[tauri::command]
async fn trash_large(app: AppHandle, paths: Vec<String>) -> CmdResult<TrashResult> {
    blocking(move || {
        let listed = app.state::<Engine>().large.lock().unwrap().clone();
        forget_view(&app);
        let result = contract::trash_reviewed(
            &listed,
            &paths,
            None,
            history::TITLE_LARGE_FILES,
            &history::log_path(),
            &contract::user_trash_root()?,
        )?;
        app.state::<Engine>()
            .large
            .lock()
            .unwrap()
            .retain(|p| !result.handled.contains(p));
        Ok(result)
    })
    .await
}

/// The duplicate groups in the latest snapshot (REQ-DSCR-005,
/// REQ-DSCR-014), hashed on a blocking thread; null when no snapshot exists
/// yet. The groups become the only paths `trash_dupes` will move. CLI:
/// `disky dupes`.
#[tauri::command]
async fn load_dupes(app: AppHandle) -> CmdResult<Option<DupesView>> {
    blocking(move || {
        let Some(snapshot) = snapshots::latest_snapshot() else {
            app.state::<Engine>().dupes.lock().unwrap().clear();
            return Ok(None);
        };
        let prefs = read_prefs()?;
        let key = contract::view_key(&snapshot, &prefs)?;
        let engine = app.state::<Engine>();
        let cached = engine
            .dupes_cache
            .lock()
            .unwrap()
            .get(&key, DUPES_CACHE_TTL, Instant::now());
        let view = match cached {
            Some(v) => v,
            None => {
                let v = contract::dupes_view(&snapshot, disky::dupes::DEFAULT_MIN_BYTES, &prefs)?;
                engine
                    .dupes_cache
                    .lock()
                    .unwrap()
                    .put(key, Instant::now(), v.clone());
                v
            }
        };
        *engine.dupes.lock().unwrap() = view.group_paths();
        Ok(Some(view))
    })
    .await
}

/// Move the non-kept duplicate copies to the Trash with one "Duplicates"
/// history entry (REQ-DSCR-006, REQ-DSCR-014.A4). The groups come from the
/// last `load_dupes`, so a call that would remove every copy of a group is
/// refused and moves nothing. Handled paths leave their groups; a group's
/// last copy stays guarded. CLI: `disky trash <path>... --title Duplicates`.
#[tauri::command]
async fn trash_dupes(app: AppHandle, paths: Vec<String>) -> CmdResult<TrashResult> {
    blocking(move || {
        let groups = app.state::<Engine>().dupes.lock().unwrap().clone();
        forget_view(&app);
        let result = contract::trash_dupes(
            &groups,
            &paths,
            &history::log_path(),
            &contract::user_trash_root()?,
        )?;
        for g in app.state::<Engine>().dupes.lock().unwrap().iter_mut() {
            g.retain(|p| !result.handled.contains(p));
        }
        Ok(result)
    })
    .await
}

/// The Assistant screen's data (REQ-DSCR-009, REQ-DSCR-010): the selected
/// agent, the proactive checks, goal and disk, the three agents
/// (REQ-DSCR-007) and the engine's suggestions for the latest snapshot
/// (REQ-DSCR-008) — an empty feed with checks off. CLI: `disky suggest`,
/// `disky agents` and `disky prefs`.
#[tauri::command]
async fn load_assistant(app: AppHandle) -> CmdResult<AssistantView> {
    blocking(move || match snapshots::latest_snapshot() {
        // Same view Smart Clean's review reads: built once, shared.
        Some(path) => {
            let (view, hits) = cached_load(&app, &path)?;
            Ok(contract::assistant_view_from(&view, &hits))
        }
        None => contract::assistant_view(None, &read_prefs()?),
    })
    .await
}

/// Bring the window forward on Smart Clean: the below-goal notification was
/// clicked (REQ-DSCR-011.A3).
fn focus_smart_clean(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit(checks::FOCUS_HOME_EVENT, ());
}

/// Open System Settings at Full Disk Access (REQ-DCLEAN-018.A2).
#[tauri::command]
async fn open_full_disk_access() -> CmdResult<()> {
    blocking(|| {
        // Only macOS gates reads behind a privacy setting; elsewhere the
        // screen offers no link (REQ-XPLAT-010.A2).
        anyhow::ensure!(
            cfg!(target_os = "macos"),
            "Full Disk Access is a macOS setting"
        );
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
        .plugin(tauri_plugin_notification::init())
        .manage(Engine::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let on_click = handle.clone();
            app.notification()
                .on_action(move |_| focus_smart_clean(&on_click))?;
            checks::spawn(handle);
            Ok(())
        })
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
            load_large,
            trash_large,
            load_dupes,
            trash_dupes,
            load_assistant,
            open_full_disk_access
        ])
        .run(tauri::generate_context!())
        .expect("error while running disky GUI");
}
