//! The driver for `scheduler.rs` (REQ-DSCR-011): while the app runs, a
//! thread ticks the [`Scheduler`] once a minute with the store's
//! preferences, the live disk and the app's activity, then performs what it
//! returns. It runs only inside the app process: no launchd agent, nothing
//! after the app quits (DISK-SCREENS-DOC non-goals).
//!
//! * Reload — the Assistant view (feed, goal, agents) is rebuilt by the
//!   engine and sent to the window on [`VIEW_EVENT`] (A1).
//! * StartScan — a background scan of `/` into a new snapshot, on its own
//!   thread so navigation never waits on it (A2). It takes the engine's scan
//!   lock without waiting, so it never overlaps another scan, and a Smart
//!   Clean scan stops it. When it finishes the window hears
//!   [`SCANNED_EVENT`] and the feed is reloaded.
//! * NotifyBelowGoal — a macOS notification titled "Assistant · <agent>"
//!   (A3). Clicking it brings the window forward on Smart Clean (`main.rs`
//!   registers the click handler, which emits [`FOCUS_HOME_EVENT`]).
//!
//! Every action only reads, scans or notifies; none removes anything (A4,
//! asserted in `scheduler.rs`).
//!
//! @implements REQ-DSCR-011

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use disky::contract;
use disky::prefs;
use disky::{scan as engine_scan, snapshots};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::scheduler::{notification_text, notification_title, Action, Clock, Inputs, Scheduler};
use crate::Engine;

/// Time between two ticks.
pub const TICK: Duration = Duration::from_secs(60);

/// A fresh Assistant view, built by the engine (payload: `AssistantView`).
pub const VIEW_EVENT: &str = "assistant://view";
/// A background scan finished; the latest snapshot changed.
pub const SCANNED_EVENT: &str = "assistant://scanned";
/// The below-goal notification was clicked: show Smart Clean.
pub const FOCUS_HOME_EVENT: &str = "assistant://focus-home";

/// Start ticking on a thread of its own.
pub fn spawn(app: AppHandle) {
    std::thread::Builder::new()
        .name("disky-checks".into())
        .spawn(move || run(&app, &crate::scheduler::SystemClock))
        .expect("spawn the checks thread");
}

fn run(app: &AppHandle, clock: &dyn Clock) {
    let mut scheduler = Scheduler::default();
    loop {
        if let Some(inputs) = gather(app) {
            for action in scheduler.tick(clock.now_secs(), &inputs) {
                perform(app, action);
            }
        }
        std::thread::sleep(TICK);
    }
}

/// What the scheduler needs now; `None` when the store can't be read (a
/// malformed store is reported by the screens, never replaced).
fn gather(app: &AppHandle) -> Option<Inputs> {
    let p = prefs::load(&prefs::prefs_path()).ok()?;
    let disk = contract::disk_usage(Path::new("/")).ok()?;
    Some(Inputs {
        checks_enabled: p.checks.enabled,
        frequency: p.checks.frequency,
        schedule: p.scheduled_scan,
        warn_below_goal: p.warn_below_goal,
        free_bytes: disk.free_bytes,
        goal_bytes: p.goal_bytes,
        latest_scan_secs: disky::assistant::latest_scan_secs(),
        busy: app.state::<Engine>().busy(),
    })
}

fn perform(app: &AppHandle, action: Action) {
    match action {
        Action::Reload => emit_view(app),
        Action::StartScan => start_background_scan(app),
        Action::NotifyBelowGoal {
            free_bytes,
            goal_bytes,
        } => notify(app, free_bytes, goal_bytes),
    }
}

/// The engine's Assistant view for the latest snapshot, through the same
/// cache the screens read (one build per snapshot and preferences).
fn assistant_view(app: &AppHandle) -> anyhow::Result<contract::AssistantView> {
    match snapshots::latest_snapshot() {
        Some(path) => {
            let (view, hits) = crate::cached_load(app, &path)?;
            Ok(contract::assistant_view_from(&view, &hits))
        }
        None => contract::assistant_view(None, &prefs::load(&prefs::prefs_path())?),
    }
}

fn emit_view(app: &AppHandle) {
    if let Ok(view) = assistant_view(app) {
        let _ = app.emit(VIEW_EVENT, view);
    }
}

fn start_background_scan(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("disky-background-scan".into())
        .spawn(move || {
            let engine = app.state::<Engine>();
            // Never overlaps another scan, and never waits for one.
            let Ok(_scanning) = engine.scan_lock.try_lock() else {
                return;
            };
            if engine.busy() {
                return;
            }
            let Ok(db_path) = snapshots::new_snapshot_path() else {
                return;
            };
            let stop = Arc::new(AtomicBool::new(false));
            *engine.background_scan.lock().unwrap() = Some(Arc::clone(&stop));
            let outcome = engine_scan::run_observed("/", &db_path, stop, &mut |_| {});
            *engine.background_scan.lock().unwrap() = None;
            if outcome.is_ok() {
                let _ = app.emit(SCANNED_EVENT, ());
                emit_view(&app);
            }
        });
}

fn notify(app: &AppHandle, free_bytes: u64, goal_bytes: u64) {
    let Ok(view) = assistant_view(app) else {
        return;
    };
    // The offer is the engine's goal suggestion, when it has one.
    let safe = view
        .suggestions
        .iter()
        .find(|s| s.key == contract::SuggestionKey::Goal && s.actionable)
        .and_then(|s| s.bytes);
    let _ = app
        .notification()
        .builder()
        .title(notification_title(&view.agent_name))
        .body(notification_text(free_bytes, goal_bytes, safe))
        .show();
}
