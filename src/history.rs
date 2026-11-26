//! Clean-history log (REQ-DCLEAN-034).
//!
//! Every applied clean appends one [`Entry`] to `history.json` in disky's
//! data directory, next to the snapshots. `disky history` lists the log and
//! `disky restore <id>` moves an entry's trashed items back.
//!
//! The log is shared by every writer (CLI, GUI), so each read-modify-write
//! holds an exclusive advisory lock on a sibling `history.lock` and swaps
//! the file in with temp-file + rename — a crash never leaves it half
//! written, and two concurrent writers never lose an entry.

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::exit::{DiskyError, ExitCode};

/// On-disk format version of `history.json`.
pub const LOG_VERSION: u32 = 1;

/// Title of a clean run from Smart Clean (REQ-DCLEAN-009.A5).
pub const TITLE_SMART_CLEAN: &str = "Smart Clean";
/// Title of a clean whose review the assistant opened (REQ-DCLEAN-009.A5).
pub const TITLE_ASSISTANT: &str = "Assistant cleanup";
/// Title of a `disky cleanup --apply` run, and of entries written before
/// titles existed.
pub const TITLE_CLI: &str = "Cleanup";
/// Title of a clean run from the Large files screen (REQ-DSCR-013.A4).
pub const TITLE_LARGE_FILES: &str = "Large files";
/// Title of a clean run from the Duplicates screen (REQ-DSCR-014.A4).
pub const TITLE_DUPLICATES: &str = "Duplicates";

/// Trashed items stay restorable this long; older entries read "Emptied"
/// (REQ-DSCR-015.A3, A4).
pub const RESTORE_WINDOW_DAYS: i64 = 30;

fn default_title() -> String {
    TITLE_CLI.to_string()
}

/// How a clean disposed of its items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Trash,
    Delete,
}

/// Where one item stands relative to restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// Moved to the Trash; restorable.
    InTrash,
    /// Moved back to its original path.
    Restored,
    /// Something already exists at the original path; left in the Trash.
    Conflict,
    /// No longer at its recorded Trash path.
    Missing,
    /// Permanently deleted (delete mode); never restorable.
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub original_path: String,
    /// `None` in delete mode.
    pub trash_path: Option<String>,
    pub bytes: u64,
    pub status: ItemStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    /// What ran the clean: [`TITLE_SMART_CLEAN`], [`TITLE_ASSISTANT`] or
    /// [`TITLE_CLI`]. Entries from before titles read as [`TITLE_CLI`].
    #[serde(default = "default_title")]
    pub title: String,
    /// RFC 3339 UTC.
    pub ts: String,
    pub mode: Mode,
    pub categories: Vec<String>,
    pub bytes: u64,
    pub items: Vec<Item>,
    /// Time the last item was restored (RFC 3339 UTC). Set only once every
    /// item is back.
    pub restored_at: Option<String>,
}

impl Entry {
    /// A fresh entry stamped now. The id is `YYYY-MM-DD_HH-MM-SS-<suffix>`;
    /// [`append`] guarantees it is unique within the log.
    pub fn new(title: &str, mode: Mode, categories: Vec<String>, items: Vec<Item>) -> Self {
        let now = Utc::now();
        let suffix = now.timestamp_subsec_nanos() % 0x1_0000;
        Self {
            id: format!("{}-{:04x}", now.format("%Y-%m-%d_%H-%M-%S"), suffix),
            title: title.to_string(),
            ts: now.to_rfc3339_opts(SecondsFormat::Millis, true),
            mode,
            categories,
            bytes: items.iter().map(|i| i.bytes).sum(),
            items,
            restored_at: None,
        }
    }
}

/// Outcome of one `restore` call. Paths are original paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreReport {
    pub id: String,
    pub restored: Vec<String>,
    pub conflicts: Vec<String>,
    /// Items that are "no longer in Trash".
    pub missing: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct LogFile {
    version: u32,
    entries: Vec<Entry>,
}

/// `<data dir>/history.json`. The `.json` extension keeps it out of
/// `@latest` and `disky list`, which only look at `*.db`.
pub fn log_path() -> PathBuf {
    crate::snapshots::snapshot_dir().join("history.json")
}

/// Read the log. A missing file is an empty log.
pub fn load(path: &Path) -> Result<Vec<Entry>> {
    let raw = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let log: LogFile = serde_json::from_slice(&raw)
        .with_context(|| format!("history log {} is malformed", path.display()))?;
    Ok(log.entries)
}

/// Entries newest first.
pub fn list(path: &Path) -> Result<Vec<Entry>> {
    let mut entries = load(path)?;
    entries.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| b.id.cmp(&a.id)));
    Ok(entries)
}

/// Append one entry under the log lock. Returns the entry as stored (its id
/// may have been bumped to stay unique).
pub fn append(path: &Path, mut entry: Entry) -> Result<Entry> {
    with_lock(path, |entries| {
        let base = entry.id.clone();
        let mut n = 1u32;
        while entries.iter().any(|e| e.id == entry.id) {
            entry.id = format!("{base}-{n}");
            n += 1;
        }
        entries.push(entry.clone());
        Ok((entry, true))
    })
}

/// Move every not-yet-restored trashed item of entry `id` back to its
/// original path. Never overwrites: an occupied destination is a conflict
/// and the item stays in the Trash. Re-running retries only items that are
/// not yet restored.
pub fn restore(path: &Path, id: &str) -> Result<RestoreReport> {
    with_lock(path, |entries| {
        let entry = entries.iter_mut().find(|e| e.id == id).ok_or_else(|| {
            DiskyError::new(
                ExitCode::NotFound,
                "history entry not found",
                format!("no history entry '{id}' (see `disky history`)"),
            )
        })?;
        if entry.mode == Mode::Delete {
            return Err(DiskyError::new(
                ExitCode::Generic,
                "not restorable",
                format!("entry {id} was a permanent delete and cannot be restored"),
            )
            .into());
        }
        if let Some(at) = &entry.restored_at {
            return Err(DiskyError::new(
                ExitCode::Generic,
                "already restored",
                format!("entry {id} already restored at {at}"),
            )
            .into());
        }

        let mut report = RestoreReport {
            id: id.to_string(),
            ..Default::default()
        };
        let mut failure = None;
        for item in entry.items.iter_mut() {
            if item.status == ItemStatus::Restored {
                continue;
            }
            let Some(trash) = item.trash_path.as_deref() else {
                continue;
            };
            let dest = Path::new(&item.original_path);
            let src = Path::new(trash);
            if dest.symlink_metadata().is_ok() {
                item.status = ItemStatus::Conflict;
                report.conflicts.push(item.original_path.clone());
            } else if src.symlink_metadata().is_err() {
                item.status = ItemStatus::Missing;
                report.missing.push(item.original_path.clone());
            } else {
                let moved = dest
                    .parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .and_then(|()| fs::rename(src, dest));
                match moved {
                    Ok(()) => {
                        // Out of the Trash: drop its .trashinfo / $I record
                        // (REQ-XPLAT-006.A4).
                        crate::trash::forget_record(src);
                        item.status = ItemStatus::Restored;
                        report.restored.push(item.original_path.clone());
                    }
                    Err(e) => {
                        failure =
                            Some(anyhow::Error::from(e).context(format!(
                                "restoring {} from {}",
                                item.original_path, trash
                            )));
                        break;
                    }
                }
            }
        }
        if entry.items.iter().all(|i| i.status == ItemStatus::Restored) {
            entry.restored_at = Some(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
        }
        // Persist progress before surfacing a mid-way failure.
        Ok((failure.map_or(Ok(report), Err), true))
    })?
}

/// Run `f` over the log with an exclusive lock held for the whole
/// read-modify-write. `f` returns `(value, dirty)`; a dirty log is written
/// back atomically.
fn with_lock<R>(path: &Path, f: impl FnOnce(&mut Vec<Entry>) -> Result<(R, bool)>) -> Result<R> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let _lock = Lock::acquire(&path.with_extension("lock"))?;
    let mut entries = load(path)?;
    let (out, dirty) = f(&mut entries)?;
    if dirty {
        save(path, entries)?;
    }
    Ok(out)
}

fn save(path: &Path, entries: Vec<Entry>) -> Result<()> {
    let body = serde_json::to_vec_pretty(&LogFile {
        version: LOG_VERSION,
        entries,
    })?;
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
    crate::platform::replace_file(&tmp, path)
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Exclusive OS file lock on a sibling lock file; released on drop (and by the
/// kernel if the process dies). Shared with the preferences store
/// (`prefs.lock`), which follows the same read-modify-write discipline.
pub(crate) struct Lock(#[allow(dead_code)] fs::File);

impl Lock {
    /// Block until the exclusive lock on `path` (created if missing) is held.
    pub(crate) fn acquire(path: &Path) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .with_context(|| format!("opening {}", path.display()))?;
        // `flock` on Unix, `LockFileEx` on Windows (REQ-XPLAT-004); blocks
        // until granted, and the OS releases it when the file closes.
        // (`File::lock` needs Rust 1.89; the MSRV is lower.)
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // SAFETY: flock on a valid, owned fd; blocks until granted.
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if rc != 0 {
                return Err(anyhow::Error::from(std::io::Error::last_os_error())
                    .context(format!("locking {}", path.display())));
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            #[repr(C)]
            struct Overlapped {
                internal: usize,
                internal_high: usize,
                offset: u32,
                offset_high: u32,
                event: *mut std::ffi::c_void,
            }
            #[link(name = "kernel32")]
            extern "system" {
                fn LockFileEx(
                    file: *mut std::ffi::c_void,
                    flags: u32,
                    reserved: u32,
                    bytes_low: u32,
                    bytes_high: u32,
                    overlapped: *mut Overlapped,
                ) -> i32;
            }
            const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x2;
            let mut ov = Overlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                event: std::ptr::null_mut(),
            };
            // SAFETY: a valid, owned handle and a zeroed OVERLAPPED; blocks
            // until the whole-file lock is granted.
            let ok = unsafe {
                LockFileEx(
                    file.as_raw_handle(),
                    LOCKFILE_EXCLUSIVE_LOCK,
                    0,
                    u32::MAX,
                    u32::MAX,
                    &mut ov,
                )
            };
            if ok == 0 {
                return Err(anyhow::Error::from(std::io::Error::last_os_error())
                    .context(format!("locking {}", path.display())));
            }
        }
        Ok(Self(file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake clean: `n` files under `<tmp>/orig`, moved into `<tmp>/Trash`.
    fn trashed_entry(tmp: &Path, names: &[&str]) -> Entry {
        let trash = tmp.join("Trash");
        fs::create_dir_all(&trash).unwrap();
        let items = names
            .iter()
            .map(|n| {
                let orig = tmp.join("orig").join(n);
                let t = trash.join(format!("{n}-1"));
                fs::write(&t, n.as_bytes()).unwrap();
                Item {
                    original_path: crate::platform::to_slash(&orig),
                    trash_path: Some(crate::platform::to_slash(&t)),
                    bytes: n.len() as u64,
                    status: ItemStatus::InTrash,
                }
            })
            .collect();
        Entry::new(
            TITLE_SMART_CLEAN,
            Mode::Trash,
            vec!["node_modules".into()],
            items,
        )
    }

    fn with_ts(mut e: Entry, ts: &str) -> Entry {
        e.ts = ts.into();
        e
    }

    #[test]
    fn append_then_list_is_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        assert!(list(&log).unwrap().is_empty());
        let a = append(
            &log,
            with_ts(
                trashed_entry(tmp.path(), &["a"]),
                "2026-01-01T00:00:00.000Z",
            ),
        )
        .unwrap();
        let b = append(
            &log,
            with_ts(
                trashed_entry(tmp.path(), &["b"]),
                "2026-02-01T00:00:00.000Z",
            ),
        )
        .unwrap();
        let got = list(&log).unwrap();
        assert_eq!(
            got.iter().map(|e| &e.id).collect::<Vec<_>>(),
            [&b.id, &a.id]
        );
        assert_eq!(got[1].items[0].bytes, 1);
        assert_eq!(got[1].mode, Mode::Trash);
    }

    #[test]
    fn append_keeps_ids_unique() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let e = trashed_entry(tmp.path(), &["a"]);
        let first = append(&log, e.clone()).unwrap();
        let second = append(&log, e).unwrap();
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn restore_moves_items_back_and_marks_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let e = append(&log, trashed_entry(tmp.path(), &["a", "b"])).unwrap();
        let r = restore(&log, &e.id).unwrap();
        assert_eq!(r.restored.len(), 2);
        assert!(r.conflicts.is_empty() && r.missing.is_empty());
        let orig = tmp.path().join("orig/a");
        assert_eq!(fs::read(&orig).unwrap(), b"a");
        let saved = &load(&log).unwrap()[0];
        assert!(saved.restored_at.is_some());
        assert!(saved.items.iter().all(|i| i.status == ItemStatus::Restored));
    }

    /// A Linux or Windows Trash keeps a record beside each item; restore
    /// takes it away with the item (REQ-XPLAT-006.A4).
    #[test]
    fn restore_removes_the_trash_record() {
        use crate::trash::{record_of, Bin};
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        for bin in [
            Bin::FreeDesktop(tmp.path().join("fd")),
            Bin::Recycle(tmp.path().join("rb")),
        ] {
            let orig = tmp.path().join("orig/a.txt");
            fs::create_dir_all(orig.parent().unwrap()).unwrap();
            fs::write(&orig, b"a").unwrap();
            let t = bin.put(&orig, 1).unwrap();
            let rec = record_of(&t).unwrap();
            assert!(rec.exists() && !orig.exists());
            let e = append(
                &log,
                Entry::new(
                    TITLE_SMART_CLEAN,
                    Mode::Trash,
                    vec![],
                    vec![Item {
                        original_path: crate::platform::to_slash(&orig),
                        trash_path: Some(crate::platform::to_slash(&t)),
                        bytes: 1,
                        status: ItemStatus::InTrash,
                    }],
                ),
            )
            .unwrap();
            let r = restore(&log, &e.id).unwrap();
            assert_eq!(r.restored.len(), 1, "{bin:?}");
            assert_eq!(fs::read(&orig).unwrap(), b"a");
            assert!(!rec.exists(), "record left behind: {bin:?}");
        }
    }

    #[test]
    fn restore_never_overwrites_and_retries_after_conflict_clears() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let e = append(&log, trashed_entry(tmp.path(), &["a"])).unwrap();
        let orig = tmp.path().join("orig/a");
        fs::create_dir_all(orig.parent().unwrap()).unwrap();
        fs::write(&orig, b"new").unwrap();

        let r = restore(&log, &e.id).unwrap();
        assert_eq!(r.conflicts, [crate::platform::to_slash(&orig)]);
        assert_eq!(fs::read(&orig).unwrap(), b"new", "must not overwrite");
        let trash = e.items[0].trash_path.as_deref().unwrap();
        assert_eq!(fs::read(trash).unwrap(), b"a", "left in the Trash");
        let saved = &load(&log).unwrap()[0];
        assert_eq!(saved.items[0].status, ItemStatus::Conflict);
        assert!(saved.restored_at.is_none());

        // Clearing the conflict lets a re-run finish the job.
        fs::remove_file(&orig).unwrap();
        let r = restore(&log, &e.id).unwrap();
        assert_eq!(r.restored.len(), 1);
        assert_eq!(fs::read(&orig).unwrap(), b"a");
    }

    #[test]
    fn restore_reports_missing_and_restores_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let e = append(&log, trashed_entry(tmp.path(), &["gone", "here"])).unwrap();
        fs::remove_file(e.items[0].trash_path.as_deref().unwrap()).unwrap();
        let r = restore(&log, &e.id).unwrap();
        assert_eq!(r.missing, [e.items[0].original_path.clone()]);
        assert_eq!(r.restored, [e.items[1].original_path.clone()]);
        let saved = &load(&log).unwrap()[0];
        assert_eq!(saved.items[0].status, ItemStatus::Missing);
        assert_eq!(saved.items[1].status, ItemStatus::Restored);
        assert!(saved.restored_at.is_none());
    }

    #[test]
    fn restore_refuses_delete_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let item = Item {
            original_path: "/x".into(),
            trash_path: None,
            bytes: 1,
            status: ItemStatus::Deleted,
        };
        let e = append(
            &log,
            Entry::new(TITLE_CLI, Mode::Delete, vec!["target".into()], vec![item]),
        )
        .unwrap();
        let err = crate::exit::classify(restore(&log, &e.id).unwrap_err());
        assert!(err
            .detail
            .contains("permanent delete and cannot be restored"));
        assert_eq!(err.code, ExitCode::Generic);
    }

    #[test]
    fn restore_refuses_fully_restored_and_unknown_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let e = append(&log, trashed_entry(tmp.path(), &["a"])).unwrap();
        restore(&log, &e.id).unwrap();
        let err = crate::exit::classify(restore(&log, &e.id).unwrap_err());
        assert!(err.detail.contains("already restored at"), "{}", err.detail);

        let err = crate::exit::classify(restore(&log, "nope").unwrap_err());
        assert_eq!(err.code, ExitCode::NotFound);
    }

    #[test]
    fn log_survives_reload_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let e = append(&log, trashed_entry(tmp.path(), &["a"])).unwrap();
        // A fresh read (as after an app restart) sees the identical entry.
        assert_eq!(load(&log).unwrap(), vec![e]);
        // No temp file from the atomic swap is left behind.
        let leftovers = fs::read_dir(tmp.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn concurrent_appends_lose_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let log = log.clone();
                std::thread::spawn(move || {
                    let item = Item {
                        original_path: format!("/p{i}"),
                        trash_path: None,
                        bytes: 1,
                        status: ItemStatus::Deleted,
                    };
                    append(
                        &log,
                        Entry::new(TITLE_CLI, Mode::Delete, vec![], vec![item]),
                    )
                    .unwrap();
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(load(&log).unwrap().len(), 8);
    }

    #[test]
    fn entries_without_a_title_read_as_cli_cleanups() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join("history.json");
        fs::write(
            &log,
            r#"{"version":1,"entries":[{"id":"x","ts":"2026-01-01T00:00:00.000Z","mode":"delete",
               "categories":[],"bytes":0,"items":[],"restored_at":null}]}"#,
        )
        .unwrap();
        assert_eq!(load(&log).unwrap()[0].title, TITLE_CLI);
        let e = append(&log, trashed_entry(tmp.path(), &["a"])).unwrap();
        assert_eq!(load(&log).unwrap()[1].title, TITLE_SMART_CLEAN);
        assert_eq!(e.title, TITLE_SMART_CLEAN);
    }
}
