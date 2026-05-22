//! In-process integration tests — drive the lib API against a real DuckDB
//! snapshot built from a synthetic temp tree. No `disky` binary required.
//!
//! Shared fixture: one `OnceLock<Fixture>` is built per test file, scanned
//! once, then read by every test. Cost: ~100 ms for the scan vs ~500 ms per
//! shell-out in `tests/agentic.rs`.

use disky::{db, query, snapshots};
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
fn id_for_round_trips_through_resolve() {
    let f = fixture();
    let path = f.db_path.to_str().unwrap();
    let id = snapshots::id_for(path).unwrap();
    // Resolving an absolute path returns it unchanged.
    let resolved = snapshots::resolve(path).unwrap();
    assert_eq!(resolved, path);
    assert_eq!(id, "snap");
}
