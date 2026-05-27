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

/// Synthesize a snapshot DB directly via DuckDB appenders (skips the
/// filesystem-walk path of `disky::scan::run`) so we can stress cleanup
/// against ~1 M rows without writing 1 M actual files. The shape matches
/// what scan would produce: each "project" gets a `node_modules` (or
/// `target`) dir plus N file rows whose paths sit underneath it.
fn synth_big_snapshot(num_projects: usize, files_per_proj: usize) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("big.db");
    let conn = db::open(db_path.to_str().unwrap()).unwrap();
    db::create_schema(&conn).unwrap();

    let mut batch: Vec<db::FileRecord> = Vec::with_capacity(50_000);
    let mut push = |batch: &mut Vec<db::FileRecord>, rec: db::FileRecord, conn: &duckdb::Connection| {
        batch.push(rec);
        if batch.len() >= 50_000 {
            db::append_batch(conn, batch).unwrap();
            batch.clear();
        }
    };

    // half the projects host `node_modules`, half host `target` — both
    // are in the default cleanup target set.
    for i in 0..num_projects {
        let project = format!("/tmp/synth/proj-{:06}", i);
        let cat = if i % 2 == 0 { "node_modules" } else { "target" };
        let target_dir = format!("{}/{}", project, cat);
        push(
            &mut batch,
            db::FileRecord {
                path: target_dir.clone(),
                name: cat.to_string(),
                ext: None,
                size: 0,
                physical_size: None,
                mtime: Some(1_700_000_000),
                is_dir: true,
                depth: 3,
            },
            &conn,
        );
        for f in 0..files_per_proj {
            let path = format!("{}/file-{:04}.bin", target_dir, f);
            push(
                &mut batch,
                db::FileRecord {
                    path,
                    name: format!("file-{:04}.bin", f),
                    ext: Some("bin".into()),
                    size: 1024,
                    physical_size: Some(1024),
                    mtime: Some(1_700_000_000),
                    is_dir: false,
                    depth: 4,
                },
                &conn,
            );
        }
    }
    if !batch.is_empty() {
        db::append_batch(&conn, &batch).unwrap();
    }
    db::write_scan_meta(&conn, "/tmp/synth", 1_700_000_000, Some(1_700_000_100), true, 0, 0)
        .unwrap();
    db::build_indexes(&conn).unwrap();
    drop(conn);
    (dir, db_path)
}

#[test]
fn cleanup_is_fast_on_large_snapshot() {
    // 2_000 projects × 500 files/project + 2_000 target-dir rows
    // ≈ 1.002 M rows total. Roughly the shape of a real macOS home
    // directory with thousands of node_modules / target dirs.
    let (_dir, db_path) = synth_big_snapshot(2_000, 500);
    let conn = db::open(db_path.to_str().unwrap()).unwrap();
    let targets: Vec<String> = cleanup::default_target_names()
        .into_iter()
        .map(String::from)
        .collect();

    let start = std::time::Instant::now();
    let hits = cleanup::scan(&conn, &targets, 10_000).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(hits.len(), 2_000, "expected one hit per project, got {}", hits.len());
    // Each project has 500 × 1024 bytes = 512_000.
    let total_bytes: u64 = hits.iter().map(|h| h.bytes).sum();
    assert_eq!(total_bytes, 2_000 * 500 * 1024);
    let total_files: u64 = hits.iter().map(|h| h.files).sum();
    assert_eq!(total_files, 2_000 * 500);

    // Generous threshold for CI variance. The grouped-query fix lands
    // around ~0.2-0.5 s on an M-series Mac vs ~80 s for the per-target
    // loop on the same fixture.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "cleanup::scan took {:?} on a ~1 M-row snapshot — should be < 5 s",
        elapsed
    );
    eprintln!("cleanup_is_fast_on_large_snapshot: {:?}", elapsed);
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
        snaps.push((db_path.to_string_lossy().into_owned(), base_ts + (i as i64) * 86400));
    }

    let rows = disky::query::growth_over_n(&snaps, 50, Some(1024 * 1024)).unwrap();
    let growing = rows
        .iter()
        .find(|r| r.path.ends_with("/growing"))
        .expect("growing dir missing from results");
    assert!(growing.slope_bytes_per_day > 0, "slope = {}", growing.slope_bytes_per_day);
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
