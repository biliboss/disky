//! Integration tests for the agent-facing CLI surface. Each test scans a tiny
//! synthetic tree and asserts the JSON shape / exit code contract documented
//! in AGENTS.md.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn disky_bin() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    let candidate = p.join("disky");
    assert!(
        candidate.exists(),
        "disky binary not built at {} — `cargo test` should build it",
        candidate.display()
    );
    candidate
}

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut d = std::env::temp_dir();
    d.push(format!("disky-it-{}-{}", std::process::id(), id));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn scan_tiny_tree() -> (PathBuf, PathBuf) {
    let dir = temp_dir();
    fs::create_dir_all(dir.join("sub/inner")).unwrap();
    fs::write(dir.join("a.log"), vec![0u8; 4096]).unwrap();
    fs::write(dir.join("b.txt"), vec![0u8; 1024]).unwrap();
    fs::write(dir.join("sub/c.png"), vec![0u8; 8192]).unwrap();
    let db = dir.join("snap.db");
    let out = Command::new(disky_bin())
        .args(["scan"])
        .arg(&dir)
        .args(["--db"])
        .arg(&db)
        .output()
        .unwrap();
    assert!(out.status.success(), "scan failed: {:?}", out);
    (dir, db)
}

fn run_json(args: &[&str]) -> Value {
    let out = Command::new(disky_bin()).args(args).output().unwrap();
    assert!(out.status.success(), "{:?} failed: {:?}", args, out);
    let body = String::from_utf8(out.stdout).unwrap();
    serde_json::from_str(body.trim()).expect("valid JSON")
}

#[test]
fn top_emits_versioned_envelope() {
    let (_dir, db) = scan_tiny_tree();
    let v = run_json(&[
        "top",
        "--snapshot",
        db.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["kind"], "top");
    let records = v["records"].as_array().unwrap();
    assert!(!records.is_empty());
    let largest = &records[0];
    assert!(largest["size"].as_u64().unwrap() >= 8192);
    assert!(largest["path"].is_string());
}

#[test]
fn stats_reports_partial_flag() {
    let (_dir, db) = scan_tiny_tree();
    let v = run_json(&[
        "stats",
        "--snapshot",
        db.to_str().unwrap(),
        "--format",
        "json",
    ]);
    let r = &v["record"];
    assert_eq!(r["partial"], false);
    assert!(r["scan_root"].is_string(), "scan_root missing: {}", r);
    assert!(r["scanned_at"].is_string(), "scanned_at missing: {}", r);
    assert!(r["scan_duration_s"].is_i64() || r["scan_duration_s"].is_u64());
}

#[test]
fn missing_snapshot_exits_not_found() {
    let out = Command::new(disky_bin())
        .args(["stats", "--snapshot", "no-such-id-xyz", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4), "expected exit 4 not-found");
    let err = String::from_utf8(out.stderr).unwrap();
    let v: Value = serde_json::from_str(err.trim()).expect("RFC 9457 JSON on stderr");
    assert_eq!(v["status"], 4);
    assert_eq!(v["type"], "https://disky.dev/errors/not-found");
}

#[test]
fn schema_emits_descriptor() {
    let v = run_json(&["schema"]);
    assert_eq!(v["tool"], "disky");
    assert!(v["commands"].as_array().unwrap().len() >= 8);
    assert!(v["records"]["FileRow"].is_object());
}

#[test]
fn raw_query_runs() {
    let (_dir, db) = scan_tiny_tree();
    let v = run_json(&[
        "query",
        "SELECT COUNT(*) AS n FROM files",
        "--snapshot",
        db.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(v["kind"], "query");
    let n = v["records"][0]["n"].as_u64().unwrap();
    assert!(n >= 4); // 3 files + at least 1 dir
}

#[test]
fn cleanup_reversible_moves_to_trash() {
    let dir = temp_dir();
    fs::create_dir_all(dir.join("proj/node_modules")).unwrap();
    fs::write(dir.join("proj/node_modules/big.js"), vec![0u8; 1024]).unwrap();
    let db = dir.join("snap.db");
    let out = Command::new(disky_bin())
        .args(["scan"])
        .arg(&dir)
        .args(["--db"])
        .arg(&db)
        .output()
        .unwrap();
    assert!(out.status.success(), "scan: {:?}", out);

    let v = run_json(&[
        "cleanup",
        "--snapshot",
        db.to_str().unwrap(),
        "--apply",
        "--reversible",
        "--target",
        "node_modules",
        "--format",
        "json",
    ]);
    assert_eq!(v["applied"], true);
    let removed = v["removed"].as_array().unwrap();
    assert!(!removed.is_empty(), "removed empty: {}", v);
    let nm = dir.join("proj/node_modules");
    assert!(!nm.exists(), "node_modules should have been moved to trash");

    // Best-effort cleanup of the trash entry we just created so we don't
    // leak across test runs. Match prefix on basename + unix ts suffix.
    if let Some(home) = dirs::home_dir() {
        let trash = home.join(".Trash");
        if let Ok(entries) = fs::read_dir(&trash) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("node_modules-") {
                    let _ = fs::remove_dir_all(e.path());
                }
            }
        }
    }
}

#[test]
fn cleanup_dry_run_finds_nothing_in_clean_tree() {
    let (_dir, db) = scan_tiny_tree();
    let v = run_json(&[
        "cleanup",
        "--snapshot",
        db.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(v["kind"], "cleanup");
    assert_eq!(v["applied"], false);
    assert!(v["records"].as_array().unwrap().is_empty());
}
