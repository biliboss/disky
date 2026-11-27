//! The preferences store shared by the CLI and the GUI (REQ-DSCR-001).
//!
//! One JSON file, `<data dir>/prefs.json`, next to the snapshots and the
//! clean history. The `.json` extension keeps it out of `@latest` and
//! `disky list`, which only look at `*.db`. `disky prefs` reads and edits it;
//! the GUI's Settings screen calls the same [`set`] through its backend, so a
//! change on either side is what the other reads next (A2).
//!
//! * A missing file is the defaults (A3). A malformed file is an error and is
//!   never replaced: every write path loads first, so it fails before writing.
//! * Every read-modify-write holds an exclusive `flock` on `prefs.lock`
//!   (the history log's [`Lock`]) and swaps the file in with temp file +
//!   rename, so concurrent CLI and GUI writers never leave a truncated file
//!   (A4).
//! * An unknown key or invalid value is a usage error (exit 2) raised before
//!   anything is written (A1).
//!
//! @implements REQ-DSCR-001

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::exit::{DiskyError, ExitCode};
use crate::history::Lock;

/// Default space goal: the design snapshot's 80 GB, decimal.
pub const DEFAULT_GOAL_BYTES: u64 = 80_000_000_000;

/// "Recently used" window for skip-recently-used (REQ-DSCR-002.A2).
pub const RECENT_DAYS: i64 = 30;

/// Agent CLIs the Assistant can name, in display order (REQ-DSCR-007).
pub const AGENT_IDS: &[&str] = &["claude", "codex", "gemini"];

/// Every key `disky prefs --set` accepts, in [`Prefs`] field order.
pub const KEYS: &[&str] = &[
    "trash_by_default",
    "skip_recently_used",
    "safe_only",
    "warn_below_goal",
    "goal_bytes",
    "checks.enabled",
    "checks.frequency",
    "agent",
    "scheduled_scan",
    "excluded",
];

/// How often the Assistant's proactive checks run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Hourly,
    Daily,
    Weekly,
}

/// Background scan period (runs only while the app is open).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Schedule {
    Off,
    Weekly,
    Monthly,
}

/// Proactive checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checks {
    pub enabled: bool,
    pub frequency: Frequency,
}

/// Every preference. Field names are the `disky prefs` keys (nested ones
/// dotted: `checks.enabled`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Prefs {
    /// Clean to the Trash (restorable) rather than deleting.
    pub trash_by_default: bool,
    /// Leave out items modified in the last [`RECENT_DAYS`] days.
    pub skip_recently_used: bool,
    /// Preselect only Safe categories after a scan.
    pub safe_only: bool,
    /// Show the below-goal notification.
    pub warn_below_goal: bool,
    /// Free-space goal, bytes.
    pub goal_bytes: u64,
    pub checks: Checks,
    /// One of [`AGENT_IDS`].
    pub agent: String,
    pub scheduled_scan: Schedule,
    /// Folders never touched, as entered (`~/…` or absolute). A folder that
    /// does not exist stays listed and matches nothing (REQ-DSCR-002.A3).
    pub excluded: Vec<String>,
}

impl Default for Prefs {
    /// The design snapshot's initial state (REQ-DSCR-001).
    fn default() -> Self {
        Prefs {
            trash_by_default: true,
            skip_recently_used: true,
            safe_only: true,
            warn_below_goal: true,
            goal_bytes: DEFAULT_GOAL_BYTES,
            checks: Checks {
                enabled: true,
                frequency: Frequency::Daily,
            },
            agent: "claude".to_string(),
            scheduled_scan: Schedule::Weekly,
            excluded: Vec::new(),
        }
    }
}

impl Prefs {
    /// Excluded folders as absolute paths: `~` and `~/…` expand against
    /// `home`. A `~` entry with no known home is dropped (it cannot match).
    pub fn expanded_excluded(&self, home: Option<&Path>) -> Vec<String> {
        self.excluded
            .iter()
            .filter_map(|e| expand_home(e, home))
            .collect()
    }

    /// Newest mtime an item may have and still be listed, when
    /// skip-recently-used is on: `now - RECENT_DAYS`.
    pub fn recent_cutoff(&self, now_secs: i64) -> Option<i64> {
        self.skip_recently_used
            .then(|| now_secs - RECENT_DAYS * 86_400)
    }
}

/// A picked folder in the store's display form: under `home` it reads
/// `~/…` (as the Settings chips show it), elsewhere it stays absolute.
pub fn abbreviate_home(folder: &Path, home: Option<&Path>) -> String {
    let full = normalise_folder(&crate::platform::to_slash(folder));
    let Some(home) = home.map(|h| {
        crate::platform::to_slash(h)
            .trim_end_matches('/')
            .to_string()
    }) else {
        return full;
    };
    if home.is_empty() {
        return full;
    }
    if full == home {
        "~".to_string()
    } else if let Some(rest) = full.strip_prefix(&format!("{home}/")) {
        format!("~/{rest}")
    } else {
        full
    }
}

fn expand_home(entry: &str, home: Option<&Path>) -> Option<String> {
    let rest = if entry == "~" {
        Some("")
    } else {
        entry.strip_prefix("~/")
    };
    match rest {
        None => Some(crate::platform::to_slash_str(entry)),
        Some(rest) => {
            let home = crate::platform::to_slash(home?);
            let home = home.trim_end_matches('/');
            Some(if rest.is_empty() {
                home.to_string()
            } else {
                format!("{home}/{rest}")
            })
        }
    }
}

/// `<data dir>/prefs.json`.
pub fn prefs_path() -> PathBuf {
    crate::snapshots::snapshot_dir().join("prefs.json")
}

/// Read the store. A missing file is [`Prefs::default`]; a malformed one is
/// an error (never silently replaced).
pub fn load(path: &Path) -> Result<Prefs> {
    let raw = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Prefs::default()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let prefs: Prefs = serde_json::from_slice(&raw).map_err(|e| {
        DiskyError::new(
            ExitCode::Generic,
            "malformed preferences",
            format!(
                "preferences file {} is malformed ({e}); fix or remove it",
                path.display()
            ),
        )
    })?;
    validate(&prefs)?;
    Ok(prefs)
}

/// Replace the whole store with `prefs`, atomically and under the lock.
pub fn save(path: &Path, prefs: &Prefs) -> Result<()> {
    validate(prefs)?;
    let _lock = lock(path)?;
    write_atomic(path, prefs)
}

/// Load, apply `f`, validate and save — all under the lock. Nothing is
/// written when the file is malformed or `f` / validation fails.
pub fn update(path: &Path, f: impl FnOnce(&mut Prefs) -> Result<()>) -> Result<Prefs> {
    let _lock = lock(path)?;
    let mut prefs = load(path)?;
    f(&mut prefs)?;
    validate(&prefs)?;
    write_atomic(path, &prefs)?;
    Ok(prefs)
}

/// Set one key (see [`KEYS`]) from its string form and return the result.
pub fn set(path: &Path, key: &str, value: &str) -> Result<Prefs> {
    update(path, |p| apply(p, key, value))
}

/// Apply every `key=value` assignment, or none: all are parsed and applied
/// to one copy before a single save.
pub fn set_all(path: &Path, assignments: &[String]) -> Result<Prefs> {
    let pairs = assignments
        .iter()
        .map(|a| parse_assignment(a))
        .collect::<Result<Vec<_>>>()?;
    update(path, |p| {
        for (k, v) in &pairs {
            apply(p, k, v)?;
        }
        Ok(())
    })
}

/// Split `key=value` (the value may itself contain `=`).
pub fn parse_assignment(s: &str) -> Result<(String, String)> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| usage(format!("expected <key>=<value>, got '{s}'")))?;
    Ok((k.trim().to_string(), v.trim().to_string()))
}

/// Set `key` on `prefs` from its string form. Unknown keys and invalid
/// values are usage errors (exit 2).
pub fn apply(prefs: &mut Prefs, key: &str, value: &str) -> Result<()> {
    match key {
        "trash_by_default" => prefs.trash_by_default = parse_bool(key, value)?,
        "skip_recently_used" => prefs.skip_recently_used = parse_bool(key, value)?,
        "safe_only" => prefs.safe_only = parse_bool(key, value)?,
        "warn_below_goal" => prefs.warn_below_goal = parse_bool(key, value)?,
        "goal_bytes" => prefs.goal_bytes = parse_bytes(value)?,
        "checks.enabled" => prefs.checks.enabled = parse_bool(key, value)?,
        "checks.frequency" => prefs.checks.frequency = parse_enum(key, value)?,
        "agent" => {
            if !AGENT_IDS.contains(&value) {
                return Err(usage(format!(
                    "agent must be one of {}, got '{value}'",
                    AGENT_IDS.join(", ")
                )));
            }
            prefs.agent = value.to_string();
        }
        "scheduled_scan" => prefs.scheduled_scan = parse_enum(key, value)?,
        "excluded" => prefs.excluded = parse_folders(value)?,
        _ => {
            return Err(usage(format!(
                "unknown preference '{key}' (keys: {})",
                KEYS.join(", ")
            )))
        }
    }
    Ok(())
}

fn usage(detail: String) -> anyhow::Error {
    DiskyError::new(ExitCode::Usage, "invalid preference", detail).into()
}

fn parse_bool(key: &str, v: &str) -> Result<bool> {
    match v {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(usage(format!("{key} must be true or false, got '{v}'"))),
    }
}

fn parse_enum<T: serde::de::DeserializeOwned>(key: &str, v: &str) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(v.to_string())).map_err(|_| {
        let allowed = match key {
            "checks.frequency" => "hourly, daily or weekly",
            _ => "off, weekly or monthly",
        };
        usage(format!("{key} must be {allowed}, got '{v}'"))
    })
}

/// A byte count: a plain integer, or a number with a decimal (`KB`, `MB`,
/// `GB`, `TB` — powers of 1000, as the GUI formats sizes) or binary (`KiB`
/// … `TiB`) suffix. `80GB` is 80 000 000 000.
pub fn parse_bytes(v: &str) -> Result<u64> {
    let upper = v.trim().to_ascii_uppercase();
    const UNITS: &[(&str, u64)] = &[
        ("TIB", 1 << 40),
        ("GIB", 1 << 30),
        ("MIB", 1 << 20),
        ("KIB", 1 << 10),
        ("TB", 1_000_000_000_000),
        ("GB", 1_000_000_000),
        ("MB", 1_000_000),
        ("KB", 1_000),
        ("B", 1),
    ];
    let (num, mult) = UNITS
        .iter()
        .find_map(|(s, m)| upper.strip_suffix(s).map(|n| (n.trim(), *m)))
        .unwrap_or((upper.as_str(), 1));
    num.parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(mult))
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            usage(format!(
                "goal_bytes must be a positive byte count (e.g. 80000000000 or 80GB), got '{v}'"
            ))
        })
}

/// Excluded folders from a JSON array (`["~/a","/b"]`, safe for paths with
/// commas — what the GUI sends) or a comma-separated list. Empty means none.
fn parse_folders(v: &str) -> Result<Vec<String>> {
    let raw: Vec<String> = if v.trim_start().starts_with('[') {
        serde_json::from_str(v)
            .map_err(|e| usage(format!("excluded: not a JSON array of strings ({e})")))?
    } else {
        v.split(',').map(String::from).collect()
    };
    let mut out: Vec<String> = Vec::new();
    for f in raw {
        let f = normalise_folder(&f);
        if f.is_empty() {
            continue;
        }
        if !out.contains(&f) {
            out.push(f);
        }
    }
    for f in &out {
        check_folder(f)?;
    }
    Ok(out)
}

/// Trim whitespace and trailing slashes (but keep `/` and `~`).
fn normalise_folder(f: &str) -> String {
    let f = crate::platform::to_slash_str(f.trim());
    let f = f.as_str();
    let t = f.trim_end_matches('/');
    if t.is_empty() && f.starts_with('/') {
        "/".to_string()
    } else {
        t.to_string()
    }
}

fn check_folder(f: &str) -> Result<()> {
    if f == "~" || f.starts_with("~/") || crate::platform::is_absolute_str(f) {
        Ok(())
    } else {
        Err(usage(format!(
            "excluded folder '{f}' must be absolute or start with ~/"
        )))
    }
}

fn validate(p: &Prefs) -> Result<()> {
    if p.goal_bytes == 0 {
        return Err(usage("goal_bytes must be positive".into()));
    }
    if !AGENT_IDS.contains(&p.agent.as_str()) {
        return Err(usage(format!(
            "agent must be one of {}, got '{}'",
            AGENT_IDS.join(", "),
            p.agent
        )));
    }
    p.excluded.iter().try_for_each(|f| check_folder(f))
}

fn lock(path: &Path) -> Result<Lock> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    Lock::acquire(&path.with_extension("lock"))
}

/// Temp file + rename: a reader sees the old file or the new one, never a
/// truncated one. Callers hold the lock.
fn write_atomic(path: &Path, prefs: &Prefs) -> Result<()> {
    let body = serde_json::to_vec_pretty(prefs)?;
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
    crate::platform::replace_file(&tmp, path)
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::classify;

    fn store() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("prefs.json");
        (tmp, path)
    }

    fn code(e: anyhow::Error) -> ExitCode {
        classify(e).code
    }

    #[test]
    fn missing_file_yields_snapshot_defaults() {
        let (_t, path) = store();
        let p = load(&path).unwrap();
        assert_eq!(p, Prefs::default());
        assert!(p.trash_by_default && p.skip_recently_used && p.safe_only && p.warn_below_goal);
        assert_eq!(p.goal_bytes, 80_000_000_000);
        assert_eq!(
            p.checks,
            Checks {
                enabled: true,
                frequency: Frequency::Daily
            }
        );
        assert_eq!(p.agent, "claude");
        assert_eq!(p.scheduled_scan, Schedule::Weekly);
        assert!(p.excluded.is_empty());
        assert!(!path.exists(), "reading never creates the file");
    }

    #[test]
    fn malformed_file_is_an_error_and_never_replaced() {
        let (_t, path) = store();
        for body in [
            &b"{not json"[..],
            br#"{"bogus_key":1}"#,
            br#"{"agent":"nope"}"#,
        ] {
            fs::write(&path, body).unwrap();
            assert!(load(&path).is_err(), "{}", String::from_utf8_lossy(body));
            assert!(set(&path, "safe_only", "false").is_err());
            assert_eq!(fs::read(&path).unwrap(), body, "file left untouched");
        }
    }

    #[test]
    fn set_then_load_round_trips() {
        let (_t, path) = store();
        let p = set(&path, "safe_only", "false").unwrap();
        assert!(!p.safe_only);
        assert_eq!(load(&path).unwrap(), p);
        let p = set_all(
            &path,
            &[
                "goal_bytes=120GB".into(),
                "checks.frequency=weekly".into(),
                "checks.enabled=false".into(),
                "agent=codex".into(),
                "scheduled_scan=off".into(),
                "excluded=~/Projects/active, /Volumes/x/".into(),
            ],
        )
        .unwrap();
        assert_eq!(p.goal_bytes, 120_000_000_000);
        assert_eq!(p.checks.frequency, Frequency::Weekly);
        assert!(!p.checks.enabled);
        assert_eq!(p.agent, "codex");
        assert_eq!(p.scheduled_scan, Schedule::Off);
        assert_eq!(p.excluded, ["~/Projects/active", "/Volumes/x"]);
        assert!(!p.safe_only, "earlier set kept");
        assert_eq!(load(&path).unwrap(), p);
    }

    #[test]
    fn bad_key_or_value_is_usage_and_changes_nothing() {
        let (_t, path) = store();
        set(&path, "safe_only", "false").unwrap();
        let before = fs::read(&path).unwrap();
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        for (k, v) in [
            ("bogus", "1"),
            ("checks.frequency", "yearly"),
            ("scheduled_scan", "daily"),
            ("safe_only", "yes"),
            ("goal_bytes", "0"),
            ("goal_bytes", "lots"),
            ("agent", "copilot"),
            ("excluded", "relative/path"),
        ] {
            let e = set(&path, k, v).unwrap_err();
            assert_eq!(code(e), ExitCode::Usage, "{k}={v}");
        }
        // One bad assignment among good ones: none applied.
        let e = set_all(&path, &["safe_only=true".into(), "nope=1".into()]).unwrap_err();
        assert_eq!(code(e), ExitCode::Usage);
        assert_eq!(
            code(set_all(&path, &["noequals".into()]).unwrap_err()),
            ExitCode::Usage
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), mtime);
    }

    #[test]
    fn save_is_atomic_and_leaves_no_temp_file() {
        let (t, path) = store();
        save(&path, &Prefs::default()).unwrap();
        set(&path, "agent", "gemini").unwrap();
        let leftovers = fs::read_dir(t.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftovers, 0);
        assert_eq!(load(&path).unwrap().agent, "gemini");
    }

    #[test]
    fn concurrent_writers_never_truncate_or_lose_the_file() {
        let (_t, path) = store();
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let v = if i % 2 == 0 { "true" } else { "false" };
                    set(&path, "safe_only", v).unwrap();
                    load(&path).unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        load(&path).expect("still a valid store");
    }

    #[test]
    fn every_enum_value_parses() {
        let mut p = Prefs::default();
        for (v, want) in [
            ("hourly", Frequency::Hourly),
            ("daily", Frequency::Daily),
            ("weekly", Frequency::Weekly),
        ] {
            apply(&mut p, "checks.frequency", v).unwrap();
            assert_eq!(p.checks.frequency, want);
        }
        for (v, want) in [
            ("off", Schedule::Off),
            ("weekly", Schedule::Weekly),
            ("monthly", Schedule::Monthly),
        ] {
            apply(&mut p, "scheduled_scan", v).unwrap();
            assert_eq!(p.scheduled_scan, want);
        }
        for a in AGENT_IDS {
            apply(&mut p, "agent", a).unwrap();
        }
    }

    #[test]
    fn excluded_accepts_json_arrays_and_dedupes() {
        let mut p = Prefs::default();
        apply(&mut p, "excluded", r#"["/a,b", "~/x/", "/a,b"]"#).unwrap();
        assert_eq!(p.excluded, ["/a,b", "~/x"]);
        apply(&mut p, "excluded", "").unwrap();
        assert!(p.excluded.is_empty());
    }

    #[test]
    fn byte_suffixes_are_decimal_and_binary() {
        assert_eq!(parse_bytes("80GB").unwrap(), 80_000_000_000);
        assert_eq!(parse_bytes("80 gb").unwrap(), 80_000_000_000);
        assert_eq!(parse_bytes("1KiB").unwrap(), 1024);
        assert_eq!(parse_bytes("12345").unwrap(), 12345);
        assert!(parse_bytes("-1").is_err());
        assert!(parse_bytes("99999999999TB").is_err(), "overflow");
    }

    #[test]
    fn excluded_expands_home_and_recent_cutoff_follows_the_toggle() {
        let mut p = Prefs {
            excluded: vec!["~/Projects".into(), "/opt/x".into(), "~".into()],
            ..Prefs::default()
        };
        assert_eq!(
            p.expanded_excluded(Some(Path::new("/Users/me/"))),
            ["/Users/me/Projects", "/opt/x", "/Users/me"]
        );
        assert_eq!(p.expanded_excluded(None), ["/opt/x"]);
        assert_eq!(p.recent_cutoff(100 * 86_400), Some(70 * 86_400));
        p.skip_recently_used = false;
        assert_eq!(p.recent_cutoff(100 * 86_400), None);
    }

    #[test]
    fn picked_folders_abbreviate_home() {
        let home = Some(Path::new("/Users/me/"));
        assert_eq!(
            abbreviate_home(Path::new("/Users/me/Projects/x/"), home),
            "~/Projects/x"
        );
        assert_eq!(abbreviate_home(Path::new("/Users/me"), home), "~");
        assert_eq!(
            abbreviate_home(Path::new("/Users/meow/x"), home),
            "/Users/meow/x"
        );
        assert_eq!(abbreviate_home(Path::new("/Volumes/a"), None), "/Volumes/a");
        let p = Prefs {
            excluded: vec![abbreviate_home(Path::new("/Users/me/a"), home)],
            ..Prefs::default()
        };
        assert_eq!(p.expanded_excluded(home), ["/Users/me/a"], "round-trips");
    }

    #[test]
    fn json_shape_uses_snake_case_keys_and_values() {
        let v = serde_json::to_value(Prefs::default()).unwrap();
        assert_eq!(v["checks"]["frequency"], "daily");
        assert_eq!(v["scheduled_scan"], "weekly");
        assert_eq!(v["goal_bytes"], 80_000_000_000u64);
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        for k in KEYS {
            let top = k.split('.').next().unwrap();
            assert!(keys.contains(&top), "{k}");
        }
    }
}
