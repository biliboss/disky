//! Proactive checks, scheduled scans and the below-goal notification
//! (REQ-DSCR-011) — the decisions only. A pure state machine: each
//! [`Scheduler::tick`] takes "now" and what the app currently knows, and
//! returns what to do. The clock is injected ([`Clock`]), so tests step time
//! without sleeping; `checks.rs` drives it while the app runs.
//!
//! * A1: with proactive checks on, a [`Action::Reload`] fires once per
//!   check period (hourly, daily, weekly).
//! * A2: with a weekly or monthly scheduled scan, a [`Action::StartScan`]
//!   fires at launch and at each check when the latest snapshot is older
//!   than the period — never while a Smart Clean scan or clean (or another
//!   background scan) runs.
//! * A3: with warn-below-goal and checks on, one
//!   [`Action::NotifyBelowGoal`] per below-goal episode: free space dropping
//!   below the goal starts an episode, getting back to the goal ends it.
//! * A4: [`Action`] has no variant that cleans, trashes or deletes, and the
//!   driver only reloads, scans and notifies (see the tests).
//!
//! The periods come from the engine (`disky::assistant`).
//!
//! @implements REQ-DSCR-011

use disky::assistant::{check_period_secs, format_bytes, scan_due};
use disky::prefs::{Frequency, Schedule};

/// "Now", in unix seconds.
pub trait Clock: Send + Sync {
    fn now_secs(&self) -> i64;
}

/// The wall clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    }
}

/// What a tick knows: the store's preferences, the live disk and the app's
/// activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inputs {
    pub checks_enabled: bool,
    pub frequency: Frequency,
    pub schedule: Schedule,
    pub warn_below_goal: bool,
    pub free_bytes: u64,
    pub goal_bytes: u64,
    /// When the latest snapshot's scan started; `None` with no snapshot.
    pub latest_scan_secs: Option<i64>,
    /// A Smart Clean scan or clean is running, or a background scan is.
    pub busy: bool,
}

/// What the app does. Nothing here removes anything (A4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Reload the cleanup view and the suggestions (A1).
    Reload,
    /// Start a background scan (A2).
    StartScan,
    /// Post the macOS below-goal notification (A3).
    NotifyBelowGoal { free_bytes: u64, goal_bytes: u64 },
}

/// The scheduler's memory between ticks.
#[derive(Debug, Default)]
pub struct Scheduler {
    launched: bool,
    last_check: Option<i64>,
    /// The current below-goal episode has been notified.
    notified: bool,
}

impl Scheduler {
    /// What to do at `now`. The launch tick counts as a check for the scan
    /// rule but reloads nothing: the screens load their own data at launch.
    pub fn tick(&mut self, now: i64, i: &Inputs) -> Vec<Action> {
        let mut out = Vec::new();
        let launch = !self.launched;
        self.launched = true;

        let mut check = false;
        if i.checks_enabled {
            match self.last_check {
                None => self.last_check = Some(now),
                Some(t) if now - t >= check_period_secs(i.frequency) => {
                    self.last_check = Some(now);
                    check = true;
                    out.push(Action::Reload);
                }
                Some(_) => {}
            }
        } else {
            // Paused: the next check period starts when checks resume.
            self.last_check = None;
        }

        if (launch || check) && !i.busy && scan_due(i.schedule, i.latest_scan_secs, now) {
            out.push(Action::StartScan);
        }

        if i.free_bytes >= i.goal_bytes {
            self.notified = false;
        } else if i.warn_below_goal && i.checks_enabled && !self.notified {
            self.notified = true;
            out.push(Action::NotifyBelowGoal {
                free_bytes: i.free_bytes,
                goal_bytes: i.goal_bytes,
            });
        }
        out
    }
}

/// The notification's title: "Assistant · <agent>" (REQ-DSCR-009.A5).
pub fn notification_title(agent_name: &str) -> String {
    format!("Assistant · {agent_name}")
}

/// The notification's text, worded as the snapshot's toast. The offer is
/// made only when the engine has safe bytes to clear.
pub fn notification_text(free_bytes: u64, goal_bytes: u64, safe_bytes: Option<u64>) -> String {
    let lead = format!(
        "You’re down to {} free, under your {} goal.",
        format_bytes(free_bytes),
        format_bytes(goal_bytes)
    );
    match safe_bytes {
        Some(b) if b > 0 => format!(
            "{lead} I can clear {} of safe junk once you confirm.",
            format_bytes(b)
        ),
        _ => lead,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    const GB: u64 = 1_000_000_000;
    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;

    /// A clock the test moves by hand: no test sleeps.
    struct FakeClock(AtomicI64);

    impl FakeClock {
        fn advance(&self, secs: i64) {
            self.0.fetch_add(secs, Ordering::SeqCst);
        }
    }

    impl Clock for FakeClock {
        fn now_secs(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    const T0: i64 = 1_795_564_800;

    fn inputs() -> Inputs {
        Inputs {
            checks_enabled: true,
            frequency: Frequency::Hourly,
            schedule: Schedule::Off,
            warn_below_goal: true,
            free_bytes: 100 * GB,
            goal_bytes: 80 * GB,
            latest_scan_secs: Some(T0),
            busy: false,
        }
    }

    fn run(s: &mut Scheduler, clock: &FakeClock, i: &Inputs) -> Vec<Action> {
        s.tick(clock.now_secs(), i)
    }

    #[test]
    fn checks_reload_once_per_period_and_not_when_paused() {
        let clock = FakeClock(AtomicI64::new(T0));
        let mut s = Scheduler::default();
        let mut i = inputs();
        assert_eq!(run(&mut s, &clock, &i), []); // launch: screens load themselves
        clock.advance(HOUR - 1);
        assert_eq!(run(&mut s, &clock, &i), []);
        clock.advance(1);
        assert_eq!(run(&mut s, &clock, &i), [Action::Reload]);
        clock.advance(10 * 60);
        assert_eq!(run(&mut s, &clock, &i), []);
        // Daily: the next reload waits a day from the last check.
        i.frequency = Frequency::Daily;
        clock.advance(HOUR);
        assert_eq!(run(&mut s, &clock, &i), []);
        clock.advance(DAY);
        assert_eq!(run(&mut s, &clock, &i), [Action::Reload]);
        // Checks off: nothing reloads, however long it runs.
        i.checks_enabled = false;
        clock.advance(30 * DAY);
        assert_eq!(run(&mut s, &clock, &i), []);
        // Back on: a fresh period starts.
        i.checks_enabled = true;
        assert_eq!(run(&mut s, &clock, &i), []);
        clock.advance(DAY);
        assert_eq!(run(&mut s, &clock, &i), [Action::Reload]);
    }

    #[test]
    fn scheduled_scan_starts_at_launch_and_at_checks_when_the_snapshot_is_old() {
        let clock = FakeClock(AtomicI64::new(T0));
        let mut s = Scheduler::default();
        let mut i = Inputs {
            schedule: Schedule::Weekly,
            latest_scan_secs: Some(T0 - 8 * DAY),
            ..inputs()
        };
        assert_eq!(run(&mut s, &clock, &i), [Action::StartScan]);
        // Not between checks, even though the snapshot is still old.
        clock.advance(60);
        assert_eq!(run(&mut s, &clock, &i), []);
        clock.advance(HOUR);
        assert_eq!(run(&mut s, &clock, &i), [Action::Reload, Action::StartScan]);
        // A fresh snapshot: nothing to scan.
        i.latest_scan_secs = Some(clock.now_secs());
        clock.advance(HOUR);
        assert_eq!(run(&mut s, &clock, &i), [Action::Reload]);
        // Monthly with a 10-day-old snapshot: not due.
        let mut s = Scheduler::default();
        i.schedule = Schedule::Monthly;
        i.latest_scan_secs = Some(clock.now_secs() - 10 * DAY);
        assert_eq!(run(&mut s, &clock, &i), []);
        // No snapshot at all: due.
        let mut s = Scheduler::default();
        i.latest_scan_secs = None;
        assert_eq!(run(&mut s, &clock, &i), [Action::StartScan]);
        // Off: never.
        let mut s = Scheduler::default();
        i.schedule = Schedule::Off;
        assert_eq!(run(&mut s, &clock, &i), []);
    }

    #[test]
    fn no_background_scan_while_smart_clean_scans_or_cleans() {
        let clock = FakeClock(AtomicI64::new(T0));
        let mut s = Scheduler::default();
        let mut i = Inputs {
            schedule: Schedule::Weekly,
            latest_scan_secs: None,
            busy: true,
            ..inputs()
        };
        assert_eq!(run(&mut s, &clock, &i), []);
        clock.advance(HOUR);
        assert_eq!(run(&mut s, &clock, &i), [Action::Reload]);
        // Once idle, the next check starts it.
        i.busy = false;
        clock.advance(HOUR);
        assert_eq!(run(&mut s, &clock, &i), [Action::Reload, Action::StartScan]);
    }

    #[test]
    fn below_goal_notifies_once_per_episode() {
        let clock = FakeClock(AtomicI64::new(T0));
        let mut s = Scheduler::default();
        let mut i = inputs();
        assert_eq!(run(&mut s, &clock, &i), []);
        i.free_bytes = 74 * GB;
        clock.advance(60);
        let notify = Action::NotifyBelowGoal {
            free_bytes: 74 * GB,
            goal_bytes: 80 * GB,
        };
        assert_eq!(run(&mut s, &clock, &i), [notify]);
        // Still below: same episode, no second notification.
        for _ in 0..5 {
            clock.advance(60);
            assert_eq!(run(&mut s, &clock, &i), []);
        }
        // Back at the goal ends the episode; dropping again starts a new one.
        i.free_bytes = 80 * GB;
        clock.advance(60);
        assert_eq!(run(&mut s, &clock, &i), []);
        i.free_bytes = 74 * GB;
        clock.advance(60);
        assert_eq!(run(&mut s, &clock, &i), [notify]);
        // Warn-below-goal off, or checks off: no notification.
        let mut s = Scheduler::default();
        i.warn_below_goal = false;
        assert_eq!(run(&mut s, &clock, &i), []);
        let mut s = Scheduler::default();
        i.warn_below_goal = true;
        i.checks_enabled = false;
        assert_eq!(run(&mut s, &clock, &i), []);
    }

    #[test]
    fn notification_is_worded_from_engine_figures() {
        assert_eq!(notification_title("Claude Code"), "Assistant · Claude Code");
        assert_eq!(
            notification_text(74 * GB, 80 * GB, Some(9_700_000_000)),
            "You’re down to 74 GB free, under your 80 GB goal. I can clear 9.7 GB of safe junk once you confirm."
        );
        assert_eq!(
            notification_text(74 * GB, 80 * GB, None),
            "You’re down to 74 GB free, under your 80 GB goal."
        );
    }

    /// A4: the scheduler can only reload, scan or notify — every action is
    /// listed here, and adding one fails to compile until it is reviewed.
    #[test]
    fn scheduler_actions_never_clean_trash_or_delete() {
        for a in [
            Action::Reload,
            Action::StartScan,
            Action::NotifyBelowGoal {
                free_bytes: 0,
                goal_bytes: 0,
            },
        ] {
            match a {
                Action::Reload | Action::StartScan | Action::NotifyBelowGoal { .. } => {}
            }
        }
    }

    /// A4: neither the scheduler nor its driver reaches any engine call that
    /// cleans, trashes, deletes or restores.
    #[test]
    fn scheduler_has_no_path_to_clean_or_trash() {
        const FORBIDDEN: &[&str] = &[
            "clean", // contract::clean, clean_observed, cleanup::apply; trash_* below
            "trash", // lint-boundary-allow: a forbidden-name list, not a category
            "apply_observed",
            "apply_with",
            "restore",
            "remove_file",
            "remove_dir",
            "delete",
            "std::fs", // lint-boundary-allow: a forbidden-name list, no fs use
        ];
        for (name, src) in [
            ("scheduler.rs", include_str!("scheduler.rs")),
            ("checks.rs", include_str!("checks.rs")),
        ] {
            // Only the code before the test module, without comments, counts.
            let code = src.split("#[cfg(test)]").next().unwrap();
            for (n, line) in code.lines().enumerate() {
                let line = line.split("//").next().unwrap().to_lowercase();
                for f in FORBIDDEN {
                    assert!(!line.contains(f), "{name}:{}: `{f}` in `{line}`", n + 1);
                }
            }
        }
    }
}
