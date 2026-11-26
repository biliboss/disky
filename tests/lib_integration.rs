//! In-process integration tests — drive the lib API against a real DuckDB
//! snapshot built from a synthetic temp tree. No `disky` binary required.
//!
//! Shared fixture: one `OnceLock<Fixture>` is built per test file, scanned
//! once, then read by every test. Cost: ~100 ms for the scan vs ~500 ms per
//! shell-out in `tests/agentic.rs`.

use disky::{cleanup, db, query, snapshots};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    db_path: PathBuf,
}

fn fixture() -> &'static Fixture {
    static F: OnceLock<Fixture> = OnceLock::new();
    F.get_or_init(|| {
        let dir = tempfile::tempdir().expect("create tempdir");
        // Scan target is a subdirectory; the DB lives outside it so it
        // doesn't appear as an extra file in the snapshot.
        let root = dir.path().join("tree");
        fs::create_dir_all(root.join("sub/inner")).unwrap();
        fs::write(root.join("big.bin"), vec![0u8; 16 * 1024]).unwrap();
        fs::write(root.join("mid.log"), vec![0u8; 4096]).unwrap();
        fs::write(root.join("small.txt"), vec![0u8; 256]).unwrap();
        fs::write(root.join("sub/inner/nested.png"), vec![0u8; 8192]).unwrap();

        let db_path = dir.path().join("snap.db");
        let outcome = disky::scan::run(root.to_str().unwrap(), db_path.to_str().unwrap())
            .expect("scan succeeds");
        assert!(outcome.complete);
        Fixture { _dir: dir, db_path }
    })
}

fn open() -> duckdb::Connection {
    let f = fixture();
    db::open(f.db_path.to_str().unwrap()).unwrap()
}

#[test]
fn scan_populates_files_table() {
    let conn = open();
    let s = query::stats(&conn).unwrap();
    assert_eq!(s.files, 4, "expected 4 files");
    assert!(s.total_bytes >= 16 * 1024 + 4096 + 256 + 8192);
    assert!(!s.partial);
    assert!(s.scan_root.is_some());
    assert!(s.scanned_at.is_some());
}

#[test]
fn top_files_orders_by_size_desc() {
    let conn = open();
    let rows = query::top_files(&conn, 10, 0).unwrap();
    assert_eq!(rows.len(), 4);
    for win in rows.windows(2) {
        assert!(win[0].size >= win[1].size, "not sorted: {:?}", rows);
    }
    assert_eq!(rows[0].size, 16 * 1024);
}

#[test]
fn top_files_min_size_filter() {
    let conn = open();
    let rows = query::top_files(&conn, 10, 5000).unwrap();
    // Only big.bin (16K) and nested.png (8K)
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.size >= 5000));
}

#[test]
fn by_extension_groups_correctly() {
    let conn = open();
    let rows = query::by_extension(&conn, 10).unwrap();
    let exts: std::collections::HashSet<&str> = rows.iter().map(|r| r.ext.as_str()).collect();
    assert!(exts.contains("bin"));
    assert!(exts.contains("log"));
    assert!(exts.contains("txt"));
    assert!(exts.contains("png"));
}

#[test]
fn raw_query_runs_arbitrary_sql() {
    let conn = open();
    let rows = query::raw_query(&conn, "SELECT COUNT(*) as n FROM files", 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0]["n"].as_u64().unwrap() >= 4);
}

#[test]
fn growth_detects_added_grew_and_removed_dirs() {
    use std::thread::sleep;
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("tree");
    fs::create_dir_all(root.join("stable")).unwrap();
    fs::create_dir_all(root.join("growing")).unwrap();
    fs::create_dir_all(root.join("going-away")).unwrap();
    fs::write(root.join("stable/keep.bin"), vec![0u8; 1024]).unwrap();
    fs::write(root.join("growing/small.bin"), vec![0u8; 1024]).unwrap();
    fs::write(root.join("going-away/doomed.bin"), vec![0u8; 4096]).unwrap();

    let snap_a = dir.path().join("a.db");
    disky::scan::run(root.to_str().unwrap(), snap_a.to_str().unwrap()).unwrap();

    // Mutate: grow `growing`, remove `going-away`, add `new-dir`.
    sleep(Duration::from_millis(50)); // ensure mtime delta
    fs::write(root.join("growing/big.bin"), vec![0u8; 16 * 1024]).unwrap();
    fs::remove_dir_all(root.join("going-away")).unwrap();
    fs::create_dir_all(root.join("new-dir")).unwrap();
    fs::write(root.join("new-dir/hello.txt"), vec![0u8; 2048]).unwrap();

    let snap_b = dir.path().join("b.db");
    disky::scan::run(root.to_str().unwrap(), snap_b.to_str().unwrap()).unwrap();

    let rows =
        disky::query::growth(snap_a.to_str().unwrap(), snap_b.to_str().unwrap(), 100).unwrap();

    let by_path: std::collections::HashMap<String, &disky::query::GrowthRow> =
        rows.iter().map(|r| (r.path.clone(), r)).collect();

    let growing = by_path
        .values()
        .find(|r| r.path.ends_with("/growing"))
        .expect("growing dir missing from results");
    assert_eq!(growing.kind, "grew");
    assert!(growing.delta_bytes > 0);

    let going = by_path
        .values()
        .find(|r| r.path.ends_with("/going-away"))
        .expect("going-away dir missing");
    assert_eq!(going.kind, "removed");
    assert!(going.delta_bytes < 0);

    let new = by_path
        .values()
        .find(|r| r.path.ends_with("/new-dir"))
        .expect("new-dir missing");
    assert_eq!(new.kind, "added");
    assert!(new.delta_bytes > 0);

    // `stable` should NOT appear — unchanged size.
    assert!(by_path.values().all(|r| !r.path.ends_with("/stable")));
}

#[test]
fn churn_detects_recently_modified_files() {
    use std::time::{Duration, SystemTime};

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("tree");
    fs::create_dir_all(root.join("hot")).unwrap();
    fs::create_dir_all(root.join("cold")).unwrap();
    fs::write(root.join("hot/fresh1.log"), vec![0u8; 4096]).unwrap();
    fs::write(root.join("hot/fresh2.log"), vec![0u8; 8192]).unwrap();
    fs::write(root.join("cold/stable.bin"), vec![0u8; 16384]).unwrap();
    // Force `cold/stable.bin` mtime far in the past (180 days).
    let old = SystemTime::now() - Duration::from_secs(180 * 86400);
    let f = fs::File::options()
        .write(true)
        .open(root.join("cold/stable.bin"))
        .unwrap();
    let times = std::fs::FileTimes::new().set_modified(old);
    f.set_times(times).unwrap();

    let snap = dir.path().join("snap.db");
    disky::scan::run(root.to_str().unwrap(), snap.to_str().unwrap()).unwrap();

    let conn = disky::db::open(snap.to_str().unwrap()).unwrap();
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // Cutoff: 1 day ago. `hot/*` should appear, `cold/*` should not.
    let cutoff = now - 86400;
    let rows = disky::query::churn(&conn, cutoff, 100).unwrap();

    let hot = rows
        .iter()
        .find(|r| r.path.ends_with("/hot"))
        .expect("hot dir missing");
    assert_eq!(hot.recent_files, 2);
    assert!(hot.recent_bytes >= 4096 + 8192);
    assert!(hot.churn_score > 0.0);

    // `cold` should NOT appear (no recent files).
    assert!(rows.iter().all(|r| !r.path.ends_with("/cold")));
}

/// Batched appender for synthetic snapshots (50k rows per flush, like scan).
struct Batch<'a> {
    conn: &'a duckdb::Connection,
    rows: Vec<db::FileRecord>,
}

impl<'a> Batch<'a> {
    fn new(conn: &'a duckdb::Connection) -> Self {
        Batch {
            conn,
            rows: Vec::with_capacity(50_000),
        }
    }

    fn push(&mut self, path: &str, is_dir: bool, size: i64, mtime: i64) {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        self.rows.push(db::FileRecord {
            ext: if is_dir {
                None
            } else {
                name.rsplit_once('.').map(|(_, e)| e.to_string())
            },
            name,
            path: path.to_string(),
            size,
            physical_size: if is_dir { None } else { Some(size) },
            mtime: Some(mtime),
            is_dir,
            depth: path.matches('/').count() as i32,
            atime: None,
        });
        if self.rows.len() >= 50_000 {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if !self.rows.is_empty() {
            db::append_batch(self.conn, &self.rows).unwrap();
            self.rows.clear();
        }
    }
}

const T0: i64 = 1_700_000_000;
const DAY: i64 = 86_400;
const SYNTH_HOME: &str = "/tmp/synth/home";

fn finish_snapshot(conn: duckdb::Connection, root: &str) {
    db::write_scan_meta(&conn, root, T0, Some(T0 + 100), true, 0, 0).unwrap();
    db::build_indexes(&conn).unwrap();
    drop(conn);
}

/// Synthesize a snapshot DB directly via DuckDB appenders (skips the
/// filesystem-walk path of `disky::scan::run`) so we can stress cleanup
/// against ~1 M rows without writing 1 M actual files. The shape matches
/// what scan would produce: each "project" gets a `node_modules` (or
/// `target`) dir plus N file rows whose paths sit underneath it. On top,
/// ~50k rows sit under the location rules' folders (`~/Library/Caches`,
/// `~/Downloads`) so the location path is measured too.
fn synth_big_snapshot(num_projects: usize, files_per_proj: usize) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("big.db");
    let conn = db::open(db_path.to_str().unwrap()).unwrap();
    db::create_schema(&conn).unwrap();
    let mut batch = Batch::new(&conn);

    // half the projects host `node_modules`, half host `target` — both
    // are in the default cleanup target set.
    for i in 0..num_projects {
        let cat = if i % 2 == 0 { "node_modules" } else { "target" };
        let target_dir = format!("/tmp/synth/proj-{:06}/{}", i, cat);
        batch.push(&target_dir, true, 0, T0);
        for f in 0..files_per_proj {
            batch.push(
                &format!("{}/file-{:04}.bin", target_dir, f),
                false,
                1024,
                T0,
            );
        }
    }

    // Location rows: 100 cache folders + 100 download folders (half older
    // than 90 days), 250 files each = 50k rows.
    let caches = format!("{SYNTH_HOME}/Library/Caches");
    let downloads = format!("{SYNTH_HOME}/Downloads");
    batch.push(&caches, true, 0, T0);
    batch.push(&downloads, true, 0, T0);
    for i in 0..SYNTH_LOCATION_DIRS {
        let cache = format!("{caches}/com.vendor.app{i:03}");
        batch.push(&cache, true, 0, T0);
        let dl = format!("{downloads}/download-{i:03}");
        let dl_mtime = if i % 2 == 0 { T0 - 200 * DAY } else { T0 };
        batch.push(&dl, true, 0, dl_mtime);
        for f in 0..SYNTH_LOCATION_FILES {
            batch.push(&format!("{cache}/blob-{f:04}"), false, 1024, T0);
            batch.push(&format!("{dl}/part-{f:04}.bin"), false, 1024, dl_mtime);
        }
    }
    batch.flush();
    drop(batch);
    finish_snapshot(conn, "/tmp/synth");
    (dir, db_path)
}

const SYNTH_LOCATION_DIRS: usize = 100;
const SYNTH_LOCATION_FILES: usize = 250;

fn synth_ctx(home: &str, root: &str) -> cleanup::ScanContext {
    cleanup::ScanContext {
        home: Some(PathBuf::from(home)),
        scan_root: Some(root.to_string()),
        now_secs: T0 + DAY,
        installed_apps: Some(cleanup::InstalledApps::default()),
        // The synthetic homes use macOS folders on every OS.
        locations: cleanup::MACOS_LOCATIONS,
        ..Default::default()
    }
}

fn all_targets() -> Vec<String> {
    cleanup::default_target_names()
        .into_iter()
        .map(String::from)
        .collect()
}

#[test]
fn cleanup_is_fast_on_large_snapshot() {
    // 2_000 projects × 500 files/project + 2_000 target-dir rows
    // ≈ 1.002 M rows, plus ~50k rows under cache / Downloads locations.
    // Roughly the shape of a real macOS home directory with thousands of
    // node_modules / target dirs.
    let (_dir, db_path) = synth_big_snapshot(2_000, 500);
    let conn = db::open(db_path.to_str().unwrap()).unwrap();
    let ctx = synth_ctx(SYNTH_HOME, "/tmp/synth");

    let start = std::time::Instant::now();
    let hits = cleanup::scan_with(&conn, &all_targets(), 10_000, &ctx).unwrap();
    let elapsed = start.elapsed();

    let dev: Vec<_> = hits
        .iter()
        .filter(|h| h.category == "node_modules" || h.category == "target")
        .collect();
    assert_eq!(
        dev.len(),
        2_000,
        "expected one hit per project, got {}",
        dev.len()
    );
    // Each project has 500 × 1024 bytes = 512_000.
    let dev_bytes: u64 = dev.iter().map(|h| h.bytes).sum();
    assert_eq!(dev_bytes, 2_000 * 500 * 1024);
    let dev_files: u64 = dev.iter().map(|h| h.files).sum();
    assert_eq!(dev_files, 2_000 * 500);

    let per_dir = SYNTH_LOCATION_FILES as u64 * 1024;
    let caches: Vec<_> = hits.iter().filter(|h| h.category == "app_caches").collect();
    assert_eq!(caches.len(), SYNTH_LOCATION_DIRS);
    assert!(caches.iter().all(|h| h.bytes == per_dir));
    let old: Vec<_> = hits
        .iter()
        .filter(|h| h.category == "old_downloads")
        .collect();
    assert_eq!(
        old.len(),
        SYNTH_LOCATION_DIRS / 2,
        "only the 200-day-old half"
    );
    assert!(old.iter().all(|h| h.bytes == per_dir));
    assert_eq!(
        hits.len(),
        2_000 + SYNTH_LOCATION_DIRS + SYNTH_LOCATION_DIRS / 2
    );

    // REQ-DCLEAN-033.A4 — the bound is < 1 s with a release build; the
    // threshold here is generous for debug builds and CI variance. The
    // grouped-query fix lands around ~0.2-0.5 s on an M-series Mac vs
    // ~80 s for the per-target loop on the same fixture.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "cleanup::scan took {:?} on a ~1 M-row snapshot — should be < 5 s",
        elapsed
    );
    eprintln!("cleanup_is_fast_on_large_snapshot: {:?}", elapsed);
}

/// Build a small snapshot from `(path, is_dir, size, mtime)` rows.
fn small_snapshot(rows: &[(&str, bool, i64, i64)], root: &str) -> (TempDir, duckdb::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("small.db");
    let conn = db::open(db_path.to_str().unwrap()).unwrap();
    db::create_schema(&conn).unwrap();
    let mut batch = Batch::new(&conn);
    for (path, is_dir, size, mtime) in rows {
        batch.push(path, *is_dir, *size, *mtime);
    }
    batch.flush();
    drop(batch);
    finish_snapshot(conn, root);
    let conn = db::open(db_path.to_str().unwrap()).unwrap();
    (dir, conn)
}

fn by_path<'a>(hits: &'a [cleanup::CleanupHit], path: &str) -> Option<&'a cleanup::CleanupHit> {
    hits.iter().find(|h| h.path == path)
}

#[test]
fn cleanup_reports_system_categories() {
    let old = T0 - 200 * DAY;
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/Library/Caches", true, 0, T0),
        ("/h/Library/Caches/com.foo", true, 0, T0),
        ("/h/Library/Caches/com.foo/a.bin", false, 100, T0),
        ("/h/Library/Caches/CloudKit", true, 0, T0),
        ("/h/Library/Caches/CloudKit/x", false, 7, T0),
        ("/h/Library/Caches/.DS_Store", false, 7, T0),
        ("/h/.Trash", true, 0, T0),
        ("/h/.Trash/x.pdf", false, 50, T0),
        ("/h/Downloads", true, 0, T0),
        ("/h/Downloads/old.zip", false, 200, old),
        ("/h/Downloads/new.zip", false, 300, T0),
        ("/h/Downloads/oldproj", true, 0, old),
        ("/h/Downloads/oldproj/node_modules", true, 0, old),
        ("/h/Downloads/oldproj/node_modules/x.js", false, 10, old),
        ("/h/Downloads/mixed", true, 0, old),
        ("/h/Downloads/mixed/stale.txt", false, 1, old),
        ("/h/Downloads/mixed/fresh.txt", false, 1, T0),
        ("/h/Library/Logs/foo.log", false, 40, T0),
    ];
    // Application Support (keyed by name) and Containers (keyed by bundle
    // id): one dir row + one file row each, as a real scan records them.
    let support = "/h/Library/Application Support";
    let containers = "/h/Library/Containers";
    let mut owned: Vec<(String, bool, i64, i64)> = Vec::new();
    for child in [
        format!("{support}/Leftover"),
        format!("{support}/Slack"),
        format!("{support}/com.apple.x"),
        format!("{containers}/com.tinyspeck.slackmacgap"),
        format!("{containers}/com.gone.Widget"),
    ] {
        owned.push((format!("{child}/data"), false, 30, T0));
        owned.push((child, true, 0, T0));
    }
    let rows: Vec<(&str, bool, i64, i64)> = rows
        .iter()
        .copied()
        .chain(owned.iter().map(|(p, d, s, m)| (p.as_str(), *d, *s, *m)))
        .collect();
    let rows = rows.as_slice();
    let (_d, conn) = small_snapshot(rows, "/h");
    let mut ctx = synth_ctx("/h", "/h");
    ctx.installed_apps = Some(cleanup::InstalledApps::from_apps([(
        Some("com.tinyspeck.slackmacgap"),
        vec!["Slack"],
    )]));
    let hits = cleanup::scan_with(&conn, &all_targets(), 1000, &ctx).unwrap();

    let cache = by_path(&hits, "/h/Library/Caches/com.foo").expect("app cache hit");
    assert_eq!(cache.category, "app_caches");
    assert_eq!(cache.safety, cleanup::Safety::Safe);
    assert_eq!(cache.bytes, 100);
    assert!(by_path(&hits, "/h/Library/Caches/CloudKit").is_none());
    assert!(by_path(&hits, "/h/Library/Caches/.DS_Store").is_none());

    let trash = by_path(&hits, "/h/.Trash/x.pdf").expect("trash hit (a file)");
    assert_eq!(trash.category, "trash");
    assert_eq!(trash.safety, cleanup::Safety::Safe);
    assert_eq!((trash.bytes, trash.files), (50, 1));
    assert!(trash.note.as_deref().unwrap().contains("permanently"));

    let dl = by_path(&hits, "/h/Downloads/old.zip").expect("old download");
    assert_eq!(dl.category, "old_downloads");
    assert_eq!(dl.safety, cleanup::Safety::Review);
    assert!(by_path(&hits, "/h/Downloads/new.zip").is_none());
    // A folder with anything recent in it is not old.
    assert!(by_path(&hits, "/h/Downloads/mixed").is_none());
    // The old project is reported once; its node_modules is not double-counted.
    assert_eq!(by_path(&hits, "/h/Downloads/oldproj").unwrap().bytes, 10);
    assert!(by_path(&hits, "/h/Downloads/oldproj/node_modules").is_none());

    let log = by_path(&hits, "/h/Library/Logs/foo.log").expect("log hit");
    assert_eq!(log.category, "logs");
    assert_eq!(log.safety, cleanup::Safety::Review);

    let leftovers: Vec<&str> = hits
        .iter()
        .filter(|h| h.category == "app_leftovers")
        .map(|h| h.path.as_str())
        .collect();
    assert_eq!(leftovers.len(), 2, "got {leftovers:?}");
    assert!(leftovers.contains(&"/h/Library/Application Support/Leftover"));
    assert!(leftovers.contains(&"/h/Library/Containers/com.gone.Widget"));

    // REQ-DCLEAN-033.A1 — every JSON record carries category + safety.
    let json = serde_json::to_value(&hits).unwrap();
    for rec in json.as_array().unwrap() {
        assert!(rec["category"].is_string(), "{rec}");
        let safety = rec["safety"].as_str().unwrap();
        assert!(safety == "safe" || safety == "review", "{rec}");
    }
}

#[test]
fn cleanup_leftovers_need_an_app_list() {
    // No installed apps known → nothing is provably a leftover.
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/Library/Application Support/Whatever", true, 0, T0),
        ("/h/Library/Application Support/Whatever/db", false, 30, T0),
    ];
    let (_d, conn) = small_snapshot(rows, "/h");
    let ctx = synth_ctx("/h", "/h");
    let hits = cleanup::scan_with(&conn, &["app_leftovers".to_string()], 100, &ctx).unwrap();
    assert!(hits.is_empty(), "got {hits:?}");
}

#[test]
fn cleanup_missing_location_is_silent() {
    // REQ-DCLEAN-033.A5 — a snapshot with none of the system locations
    // still returns the dev hits, and no error, for every category.
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/src/app/node_modules", true, 0, T0),
        ("/h/src/app/node_modules/x.js", false, 10, T0),
    ];
    let (_d, conn) = small_snapshot(rows, "/h");
    for home in [Some("/h"), Some("/does/not/exist"), None] {
        let mut ctx = synth_ctx("/h", "/h");
        ctx.home = home.map(PathBuf::from);
        let hits = cleanup::scan_with(&conn, &all_targets(), 100, &ctx).unwrap();
        assert_eq!(hits.len(), 1, "home={home:?}: {hits:?}");
        assert_eq!(hits[0].category, "node_modules");
        assert_eq!(hits[0].safety, cleanup::Safety::Review);
    }
}

#[test]
fn cleanup_respects_root_and_ignore() {
    // REQ-DCLEAN-033.A3 — nothing outside the root, under an ignored
    // segment, or inside an excluded folder is returned.
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/src/a/node_modules", true, 0, T0),
        ("/h/src/a/node_modules/x", false, 1, T0),
        ("/h/src/keepme/node_modules", true, 0, T0),
        ("/h/src/keepme/node_modules/x", false, 1, T0),
        ("/h/src/b/target", true, 0, T0),
        ("/h/src/b/target/x", false, 1, T0),
        ("/h/srcother/target", true, 0, T0),
        ("/h/srcother/target/x", false, 1, T0),
        ("/h/Library/Caches/foo", true, 0, T0),
        ("/h/Library/Caches/foo/x", false, 1, T0),
        ("/h/.Trash/y", false, 1, T0),
    ];
    let (_d, conn) = small_snapshot(rows, "/h/src");
    let mut ctx = synth_ctx("/h", "/h/src");
    ctx.ignore = vec!["keepme".to_string()];
    ctx.excluded = vec!["/h/src/b".to_string()];
    let hits = cleanup::scan_with(&conn, &all_targets(), 100, &ctx).unwrap();
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, vec!["/h/src/a/node_modules"]);

    // `scan` (no explicit context) reads the root from scan_meta.
    let hits = cleanup::scan(&conn, &all_targets(), 100).unwrap();
    assert!(
        hits.iter().all(|h| h.path.starts_with("/h/src/")),
        "{hits:?}"
    );
}

#[test]
fn cleanup_trash_is_refused_in_reversible_mode() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("in-trash.bin");
    fs::write(&f, b"x").unwrap();
    let hit = cleanup::CleanupHit {
        category: cleanup::TRASH_CATEGORY.to_string(),
        safety: cleanup::Safety::Safe,
        path: f.to_string_lossy().into_owned(),
        bytes: 1,
        physical_bytes: 1,
        files: 1,
        note: None,
        newest_mtime: None,
    };
    let out = cleanup::apply(std::slice::from_ref(&hit), cleanup::ApplyMode::Trash, None).unwrap();
    assert!(out.handled.is_empty() && out.entry.is_none());
    assert!(
        f.exists(),
        "trash item must be left alone in reversible mode"
    );

    // Permanent mode deletes it (file hits are supported, not just dirs).
    let out = cleanup::apply(&[hit], cleanup::ApplyMode::Delete, None).unwrap();
    assert_eq!(out.handled.len(), 1);
    assert!(!f.exists());
}

#[test]
fn id_for_round_trips_through_resolve() {
    let f = fixture();
    let path = f.db_path.to_str().unwrap();
    let id = snapshots::id_for(path).unwrap();
    // Resolving an absolute path returns it unchanged.
    let resolved = snapshots::resolve(path).unwrap();
    assert_eq!(resolved, path);
    assert_eq!(id, "snap");
}

#[test]
fn growth_over_n_fits_linear_growth_with_high_r2() {
    // Synthesise 5 snapshots of a single growing dir: 1KB, 2KB, 4KB, 8KB, 16KB.
    // The growth is roughly linear in bytes-over-time when ts spans 4 days, so
    // OLS slope is well above zero and R² should be reasonably high.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("tree");
    fs::create_dir_all(root.join("growing")).unwrap();
    // Stable dir present in every snapshot — should also be reported with
    // ~zero slope.
    fs::create_dir_all(root.join("stable")).unwrap();
    fs::write(root.join("stable/keep.bin"), vec![0u8; 1024]).unwrap();

    let sizes = [1024usize, 2048, 4096, 8192, 16384];
    let mut snaps: Vec<(String, i64)> = Vec::new();
    // 1 snapshot per day, starting from a fixed epoch so the test is
    // deterministic regardless of wall-clock.
    let base_ts: i64 = 1_700_000_000;
    for (i, sz) in sizes.iter().enumerate() {
        // Overwrite the growing file with the new size.
        fs::write(root.join("growing/blob.bin"), vec![0u8; *sz]).unwrap();
        let db_path = dir.path().join(format!("snap_{i}.db"));
        disky::scan::run(root.to_str().unwrap(), db_path.to_str().unwrap()).unwrap();
        snaps.push((
            db_path.to_string_lossy().into_owned(),
            base_ts + (i as i64) * 86400,
        ));
    }

    let rows = disky::query::growth_over_n(&snaps, 50, Some(1024 * 1024)).unwrap();
    let growing = rows
        .iter()
        .find(|r| r.path.ends_with("/growing"))
        .expect("growing dir missing from results");
    assert!(
        growing.slope_bytes_per_day > 0,
        "slope = {}",
        growing.slope_bytes_per_day
    );
    // Sub-agent synthesised exponential growth (1, 2, 4, 8, 16 KB) over linear time —
    // OLS linear fit gives R² ≈ 0.87 on that shape. Threshold relaxed to 0.8;
    // upstream growth is non-linear and the test asserts trend, not perfect fit.
    assert!(growing.r2 > 0.8, "r2 = {}", growing.r2);
    assert_eq!(growing.n_snapshots, 5);
    assert_eq!(growing.sample_paths_ts.len(), 5);
    assert_eq!(growing.latest_bytes, 16384);
    assert!(
        growing.projected_fill_date.is_some(),
        "expected projected_fill_date with positive slope + fill_target"
    );

    // Stable dir should also be present with near-zero slope.
    let stable = rows
        .iter()
        .find(|r| r.path.ends_with("/stable"))
        .expect("stable dir missing");
    assert_eq!(stable.slope_bytes_per_day, 0);
    // No projection for a flat series.
    assert!(stable.projected_fill_date.is_none());
}

#[test]
fn contract_totals_match_cleanup_summary() {
    // REQ-DCLEAN-027.A3 / REQ-DCLEAN-001.A1 — the GUI view is the CLI's
    // `cleanup` summary regrouped, in physical bytes.
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/src/a/node_modules", true, 0, T0),
        ("/h/src/a/node_modules/x.js", false, 120, T0),
        ("/h/src/b/node_modules", true, 0, T0),
        ("/h/src/b/node_modules/y.js", false, 30, T0),
        ("/h/src/b/target", true, 0, T0),
        ("/h/src/b/target/out", false, 4000, T0),
        ("/h/Library/Caches/com.foo", true, 0, T0),
        ("/h/Library/Caches/com.foo/c", false, 77, T0),
        ("/h/Library/Logs/x.log", false, 9, T0),
    ];
    let (_d, conn) = small_snapshot(rows, "/h");
    let ctx = synth_ctx("/h", "/h");
    let hits = disky::contract::view_hits_with(&conn, &ctx).unwrap();
    let summary = cleanup::summarise(&hits);
    let cats = disky::contract::categories_from_hits(&hits);

    assert_eq!(cats.len(), summary.len());
    for c in &cats {
        let s = summary.iter().find(|s| s.category == c.id).unwrap();
        assert_eq!(c.bytes, s.physical_bytes, "{}", c.id);
        assert_eq!(c.items.len() as u64, s.paths, "{}", c.id);
        assert_eq!(c.safety, s.safety, "{}", c.id);
        assert_eq!(c.bytes, c.items.iter().map(|i| i.bytes).sum::<u64>());
    }
    let ids: Vec<&str> = cats.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["node_modules", "target", "app_caches", "logs"]);
}

#[test]
fn contract_never_lists_ignored_or_excluded_paths() {
    // REQ-DCLEAN-029.A3 — `.diskyignore` segments and excluded folders
    // never reach the GUI's category list.
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/src/a/node_modules", true, 0, T0),
        ("/h/src/a/node_modules/x", false, 1, T0),
        ("/h/src/keepme/node_modules", true, 0, T0),
        ("/h/src/keepme/node_modules/x", false, 1, T0),
        ("/h/src/private/target", true, 0, T0),
        ("/h/src/private/target/x", false, 1, T0),
        ("/h/Library/Caches/secret", true, 0, T0),
        ("/h/Library/Caches/secret/x", false, 1, T0),
    ];
    let (_d, conn) = small_snapshot(rows, "/h");
    let mut ctx = synth_ctx("/h", "/h");
    ctx.ignore = vec!["keepme".to_string()];
    ctx.excluded = vec![
        "/h/src/private".to_string(),
        "/h/Library/Caches".to_string(),
    ];
    let hits = disky::contract::view_hits_with(&conn, &ctx).unwrap();
    let paths: Vec<String> = disky::contract::categories_from_hits(&hits)
        .into_iter()
        .flat_map(|c| c.items.into_iter().map(|i| i.path))
        .collect();
    assert_eq!(paths, ["/h/src/a/node_modules"]);
}

#[test]
fn contract_load_builds_view_from_snapshot() {
    // REQ-DCLEAN-027 — one call yields the whole view: disk usage of the
    // scan root's volume, snapshot age, default preferences, categories.
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/src/a/node_modules", true, 0, T0),
        ("/h/src/a/node_modules/x.js", false, 120, T0),
    ];
    let (dir, conn) = small_snapshot(rows, "/h");
    drop(conn);
    let path = dir.path().join("small.db");
    let (view, hits) =
        disky::contract::load(path.to_str().unwrap(), &disky::prefs::Prefs::default()).unwrap();
    assert_eq!(view.goal_bytes, 80_000_000_000);
    assert!(view.disk.capacity_bytes > 0 && view.disk.free_bytes <= view.disk.capacity_bytes);
    assert!(view.is_stale, "a 2023 snapshot is older than 24 h");
    assert_eq!(
        view.snapshot_taken_at.as_deref(),
        Some("2023-11-14T22:13:20Z")
    );
    assert!(view.preferences.trash_by_default && view.preferences.safe_only);
    assert_eq!(view.categories.len(), 1);
    assert_eq!(view.categories[0].id, "node_modules");
    assert_eq!(view.categories[0].bytes, 120);
    assert_eq!(hits.len(), 1);
}

/// REQ-DCLEAN-018.A2: a scan that cannot read a directory completes, keeps
/// what it could read, and records how many paths it skipped — the count the
/// GUI's "N paths skipped" line reads from the snapshot.
#[cfg(unix)]
#[test]
fn scan_counts_permission_skipped_paths() {
    use std::os::unix::fs::PermissionsExt;
    // Root reads and deletes regardless of mode bits (containers, CI
    // images): there is no permission failure to observe.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("tree");
    let locked = root.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("secret.bin"), b"s").unwrap();
    fs::write(root.join("open.txt"), b"o").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let db_path = dir.path().join("snap.db");
    let outcome = disky::scan::run(root.to_str().unwrap(), db_path.to_str().unwrap());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    let outcome = outcome.expect("scan completes despite the unreadable dir");
    assert!(outcome.complete);
    assert!(outcome.skipped >= 1, "skipped: {}", outcome.skipped);

    let conn = db::open(db_path.to_str().unwrap()).unwrap();
    assert_eq!(db::read_scan_skipped(&conn), outcome.skipped);
    let s = query::stats(&conn).unwrap();
    assert!(s.files >= 1, "readable paths are still recorded");
    let (view, _) =
        disky::contract::load(db_path.to_str().unwrap(), &disky::prefs::Prefs::default()).unwrap();
    assert_eq!(view.skipped_paths, outcome.skipped);
}

/// REQ-DSCR-002.A1–A3: excluded folders (`~` expanded, matched on whole path
/// components) and skip-recently-used scope cleanup hits; a missing excluded
/// folder matches nothing.
#[test]
fn cleanup_drops_excluded_and_recently_used_hits() {
    let now = T0 + DAY;
    let old = T0 - 200 * DAY;
    let recent = now - 2 * DAY;
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/src/old/node_modules", true, 0, old),
        ("/h/src/old/node_modules/x.js", false, 10, old),
        ("/h/src/fresh/node_modules", true, 0, old),
        ("/h/src/fresh/node_modules/x.js", false, 20, recent),
        ("/h/src/private/target", true, 0, old),
        ("/h/src/private/target/x", false, 30, old),
        ("/h/src/privateer/target", true, 0, old),
        ("/h/src/privateer/target/x", false, 40, old),
    ];
    let (_d, conn) = small_snapshot(rows, "/h");
    let paths = |prefs: &disky::prefs::Prefs| {
        let ctx = synth_ctx("/h", "/h").with_prefs(prefs);
        let mut p: Vec<String> = cleanup::scan_with(&conn, &all_targets(), 100, &ctx)
            .unwrap()
            .into_iter()
            .map(|h| h.path)
            .collect();
        p.sort();
        p
    };
    let mut prefs = disky::prefs::Prefs {
        excluded: vec!["~/src/private".into(), "/h/does/not/exist".into()],
        ..Default::default()
    };
    // Skip on (default): the fresh node_modules is left out (A2); the
    // excluded folder is dropped but not its `privateer` sibling (A1); the
    // missing folder matches nothing (A3).
    assert_eq!(
        paths(&prefs),
        ["/h/src/old/node_modules", "/h/src/privateer/target"]
    );
    prefs.skip_recently_used = false;
    assert_eq!(
        paths(&prefs),
        [
            "/h/src/fresh/node_modules",
            "/h/src/old/node_modules",
            "/h/src/privateer/target"
        ]
    );
    assert_eq!(prefs.excluded.len(), 2, "missing folder kept in the list");
}

/// REQ-DSCR-002 through the GUI path: `contract::load` scopes its view by the
/// store's preferences.
#[test]
fn contract_load_honours_the_preferences_store() {
    let rows: &[(&str, bool, i64, i64)] = &[
        ("/h/src/a/node_modules", true, 0, T0),
        ("/h/src/a/node_modules/x.js", false, 120, T0),
        ("/h/src/b/node_modules", true, 0, T0),
        ("/h/src/b/node_modules/x.js", false, 80, T0),
    ];
    let (dir, conn) = small_snapshot(rows, "/h");
    drop(conn);
    let path = dir.path().join("small.db");
    let prefs = disky::prefs::Prefs {
        excluded: vec!["/h/src/b".into()],
        goal_bytes: 50_000_000_000,
        ..Default::default()
    };
    let (view, hits) = disky::contract::load(path.to_str().unwrap(), &prefs).unwrap();
    assert_eq!(view.goal_bytes, 50_000_000_000);
    assert_eq!(view.preferences, prefs);
    let got: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(got, ["/h/src/a/node_modules"]);
}

/// REQ-DSCR-003.A4: one `disky map` call on a ~1 M-row snapshot is a single
/// grouped query — under 1 s in a release build (generous bound in debug).
#[test]
fn map_is_fast_on_large_snapshot() {
    let (_dir, db_path) = synth_big_snapshot(2_000, 500);
    let conn = db::open(db_path.to_str().unwrap()).unwrap();

    let start = std::time::Instant::now();
    let root = disky::map::map(&conn, None, &[], 1).unwrap();
    let elapsed_root = start.elapsed();
    // 2 000 projects + the synthetic home, largest first.
    assert_eq!(root.records.len(), 2_001);
    assert_eq!(root.records[0].name, "home");
    let per_dir = SYNTH_LOCATION_FILES as u64 * 1024;
    assert_eq!(
        root.records[0].bytes,
        2 * SYNTH_LOCATION_DIRS as u64 * per_dir
    );
    assert!(root.records[1..].iter().all(|c| c.bytes == 500 * 1024));
    assert_eq!(
        root.total_bytes,
        2_000 * 500 * 1024 + 2 * SYNTH_LOCATION_DIRS as u64 * per_dir
    );

    let start = std::time::Instant::now();
    let caches =
        disky::map::map(&conn, Some(&format!("{SYNTH_HOME}/Library/Caches")), &[], 1).unwrap();
    let elapsed_child = start.elapsed();
    assert_eq!(caches.records.len(), SYNTH_LOCATION_DIRS);

    let bound = if cfg!(debug_assertions) {
        std::time::Duration::from_secs(5)
    } else {
        std::time::Duration::from_secs(1)
    };
    for (what, t) in [("root", elapsed_root), ("child", elapsed_child)] {
        assert!(
            t < bound,
            "map of {what} took {t:?} on a ~1 M-row snapshot (bound {bound:?})"
        );
    }
    eprintln!("map_is_fast_on_large_snapshot: root {elapsed_root:?}, child {elapsed_child:?}");
}

/// REQ-DSCR-003 / REQ-DSCR-002.A1 through the library: the GUI's
/// `contract::map_view` honours the preferences store's excluded folders.
#[test]
fn map_view_honours_excluded_folders() {
    let (dir, conn) = small_snapshot(
        &[
            ("/m", true, 0, T0),
            ("/m/keep", true, 0, T0),
            ("/m/keep/a", false, 300, T0),
            ("/m/private", true, 0, T0),
            ("/m/private/b", false, 900, T0),
        ],
        "/m",
    );
    let path = dir.path().join("small.db").to_string_lossy().into_owned();
    drop(conn);
    let prefs = disky::prefs::Prefs {
        excluded: vec!["/m/private".into()],
        ..Default::default()
    };
    let v = disky::contract::map_view(&path, None, &prefs).unwrap();
    let names: Vec<&str> = v.records.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["keep"]);
    assert_eq!(v.total_bytes, 300);
    assert!(v.capacity_bytes > 0);
    let all = disky::contract::map_view(&path, None, &disky::prefs::Prefs::default()).unwrap();
    assert_eq!(all.records[0].name, "private");
}

#[test]
fn scan_records_each_files_access_time() {
    // REQ-DSCR-004.A1 — the scan stores `st_atime`, and only `stat`s, so the
    // recorded value is the one set before the scan.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("tree");
    fs::create_dir_all(&root).unwrap();
    let file = root.join("old.img");
    fs::write(&file, vec![1u8; 4096]).unwrap();
    let accessed = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_accessed(accessed))
        .unwrap();
    let db_path = dir.path().join("snap.db");
    disky::scan::run(root.to_str().unwrap(), db_path.to_str().unwrap()).unwrap();
    let conn = db::open(db_path.to_str().unwrap()).unwrap();
    let atime: Option<i64> = conn
        .query_row("SELECT atime FROM files WHERE name = 'old.img'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(atime, Some(1_500_000_000));
    assert!(disky::large::has_atime(&conn).unwrap());
}

/// Nested dev folders (bug: thousands of `node_modules` inside other
/// `node_modules` made the cleanup view take 8–17 s on a real 10.9 M-row
/// home, so the Assistant's Review looked dead). Roots are pruned to the
/// outermost before summing, every file is counted once under its outermost
/// root, and a nested root inside a recently used one is never reported on
/// its own (deleting it would break a project in use). Also covers the
/// path-component ordering: "/p/a b" must not hide "/p/a/…" nesting.
#[test]
fn nested_dev_folders_sum_once_under_the_outermost() {
    let old = T0 - 400 * DAY;
    let fresh = T0;
    let rows: Vec<(&str, bool, i64, i64)> = vec![
        ("/r/p/a/node_modules", true, 0, old),
        ("/r/p/a/node_modules/x/index.js", false, 100, old),
        ("/r/p/a/node_modules/x/node_modules", true, 0, old),
        ("/r/p/a/node_modules/x/node_modules/y.js", false, 50, old),
        ("/r/p/a b/node_modules", true, 0, old),
        ("/r/p/a b/node_modules/z.js", false, 7, old),
        // Recently used project: its nested old node_modules stays too.
        ("/r/q/node_modules", true, 0, fresh),
        ("/r/q/node_modules/new.js", false, 9, fresh),
        ("/r/q/node_modules/old/node_modules", true, 0, old),
        ("/r/q/node_modules/old/node_modules/o.js", false, 11, old),
    ];
    let (_dir, conn) = small_snapshot(&rows, "/r");
    let mut ctx = synth_ctx("/r", "/r");
    ctx.recent_cutoff = Some(T0 - 30 * DAY);
    let hits = cleanup::scan_with(&conn, &["node_modules".to_string()], 100, &ctx).unwrap();
    let mut got: Vec<(&str, u64, u64)> = hits
        .iter()
        .map(|h| (h.path.as_str(), h.bytes, h.files))
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            ("/r/p/a b/node_modules", 7, 1),
            ("/r/p/a/node_modules", 150, 2)
        ]
    );
}
