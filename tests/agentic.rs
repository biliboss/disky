//! Integration tests for the agent-facing CLI surface. Each test scans a tiny
//! synthetic tree and asserts the JSON shape / exit code contract documented
//! in AGENTS.md.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn disky_bin() -> PathBuf {
    // Cargo sets this for integration tests; robust to the build-dir layout.
    let candidate = PathBuf::from(env!("CARGO_BIN_EXE_disky"));
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
    // One level below a shared folder: on Linux `/tmp` is itself a
    // cleanup location (`temp`, REQ-XPLAT-005), so a home directly in it
    // would be reported whole and hide what it holds. Its child
    // `/tmp/disky-it` lies above the scan root and is never in scope.
    let mut d = std::env::temp_dir();
    d.push("disky-it");
    d.push(format!("{}-{}", std::process::id(), id));
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
fn diff_classifies_added_removed_grew() {
    let dir = temp_dir();
    fs::write(dir.join("stay.bin"), vec![0u8; 4096]).unwrap();
    fs::write(dir.join("grew.bin"), vec![0u8; 1024]).unwrap();
    fs::write(dir.join("removed.bin"), vec![0u8; 2048]).unwrap();
    let snap_a = dir.join("a.db");
    let out = Command::new(disky_bin())
        .args(["scan"])
        .arg(&dir)
        .args(["--db"])
        .arg(&snap_a)
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);

    // Now mutate the tree.
    fs::remove_file(dir.join("removed.bin")).unwrap();
    fs::write(dir.join("grew.bin"), vec![0u8; 8192]).unwrap();
    fs::write(dir.join("added.bin"), vec![0u8; 512]).unwrap();
    let snap_b = dir.join("b.db");
    let out = Command::new(disky_bin())
        .args(["scan"])
        .arg(&dir)
        .args(["--db"])
        .arg(&snap_b)
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);

    let v = run_json(&[
        "diff",
        snap_a.to_str().unwrap(),
        snap_b.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(v["kind"], "diff");
    let by_path: std::collections::HashMap<&str, &Value> = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["path"].as_str().unwrap(), r))
        .collect();
    let added = by_path
        .values()
        .find(|r| r["path"].as_str().unwrap().ends_with("/added.bin"))
        .expect("added.bin missing");
    assert_eq!(added["kind"], "added");
    let removed = by_path
        .values()
        .find(|r| r["path"].as_str().unwrap().ends_with("/removed.bin"))
        .expect("removed.bin missing");
    assert_eq!(removed["kind"], "removed");
    let grew = by_path
        .values()
        .find(|r| r["path"].as_str().unwrap().ends_with("/grew.bin"))
        .expect("grew.bin missing");
    assert_eq!(grew["kind"], "grew");
    assert!(grew["delta"].as_i64().unwrap() > 0);
}

#[test]
fn scan_emit_bundle_includes_top_and_stats() {
    let dir = temp_dir();
    fs::write(dir.join("big.bin"), vec![0u8; 16 * 1024]).unwrap();
    fs::write(dir.join("small.txt"), vec![0u8; 64]).unwrap();
    let db = dir.join("snap.db");
    let out = Command::new(disky_bin())
        .args(["scan"])
        .arg(&dir)
        .args(["--db"])
        .arg(&db)
        .args(["--emit-top", "5", "--emit-ext", "5", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let body = String::from_utf8(out.stdout).unwrap();
    let v: Value = serde_json::from_str(body.trim()).expect("scan_bundle JSON");
    assert_eq!(v["kind"], "scan_bundle");
    assert_eq!(v["complete"], true);
    assert!(v["stats"]["files"].as_u64().unwrap() >= 2);
    assert!(!v["top"].as_array().unwrap().is_empty());
    assert!(!v["ext"].as_array().unwrap().is_empty());
}

/// Point every per-user folder disky reads at `home`, on every OS
/// (REQ-XPLAT-002): `HOME`, `USERPROFILE`, `%LOCALAPPDATA%`, `%APPDATA%`.
/// On Windows the Recycle Bin lives on the drive, not in the home, so the
/// Trash root is pinned under `home` too (`DISKY_TRASH_ROOT`).
fn in_home<'a>(cmd: &'a mut Command, home: &std::path::Path) -> &'a mut Command {
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .env("APPDATA", home.join("AppData/Roaming"));
    if cfg!(windows) {
        cmd.env("DISKY_TRASH_ROOT", home.join("RecycleBin"));
    }
    cmd
}

/// Where a reversible clean under the temp `home` puts items on this OS.
fn trash_items(home: &std::path::Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join(".Trash")
    } else if cfg!(windows) {
        home.join("RecycleBin")
    } else {
        home.join(".local/share/Trash/files")
    }
}

/// Count of items in the temp Trash (records such as `$I…` left out).
fn trash_count(home: &std::path::Path) -> usize {
    fs::read_dir(trash_items(home))
        .unwrap()
        .filter(|e| {
            !e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("$I")
        })
        .count()
}

/// A path as the engine prints it: `/` separators on every OS.
fn slash(p: &std::path::Path) -> String {
    disky::platform::to_slash(p)
}

/// The per-OS app cache folder under `home` (REQ-XPLAT-005).
fn app_cache_dir(home: &std::path::Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Caches")
    } else if cfg!(windows) {
        home.join("AppData/Local/Microsoft/Windows/INetCache")
    } else {
        home.join(".cache")
    }
}

/// Run disky with `HOME` pointed at `home`, so the Trash, the data dir and
/// the clean-history log all live under the test's temp dir — never the
/// real user's.
fn run_in_home(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    in_home(&mut Command::new(disky_bin()), home)
        .env_remove("XDG_DATA_HOME")
        .env_remove("DISKY_CONFIG_PATH")
        .env_remove("DISKY_FORMAT")
        .env_remove("DISKY_SNAPSHOT")
        .args(args)
        .output()
        .unwrap()
}

fn json_in_home(home: &std::path::Path, args: &[&str]) -> Value {
    let out = run_in_home(home, args);
    assert!(out.status.success(), "{:?} failed: {:?}", args, out);
    serde_json::from_str(String::from_utf8(out.stdout).unwrap().trim()).expect("valid JSON")
}

/// A temp `HOME` whose preferences include recently used items, so cleanup
/// sees the freshly written test trees (REQ-DSCR-002.A2: with
/// skip-recently-used on — the default — they would be left out).
fn home_including_recent() -> PathBuf {
    let home = temp_dir();
    json_in_home(
        &home,
        &[
            "prefs",
            "--set",
            "skip_recently_used=false",
            "--format",
            "json",
        ],
    );
    home
}

/// Scan `<home>/proj` (holding one `node_modules`) and clean it to the
/// (temp) Trash. Returns the cleanup envelope.
fn trash_node_modules(home: &std::path::Path) -> Value {
    json_in_home(
        home,
        &[
            "prefs",
            "--set",
            "skip_recently_used=false",
            "--format",
            "json",
        ],
    );
    fs::create_dir_all(home.join("proj/node_modules")).unwrap();
    fs::write(home.join("proj/node_modules/big.js"), vec![0u8; 1024]).unwrap();
    let db = home.join("snap.db");
    let proj = home.join("proj");
    let out = run_in_home(
        home,
        &["scan", proj.to_str().unwrap(), "--db", db.to_str().unwrap()],
    );
    assert!(out.status.success(), "scan: {:?}", out);
    json_in_home(
        home,
        &[
            "cleanup",
            "--snapshot",
            db.to_str().unwrap(),
            "--apply",
            "--reversible",
            "--target",
            "node_modules",
            "--format",
            "json",
        ],
    )
}

#[test]
fn cleanup_reversible_moves_to_trash() {
    let home = temp_dir();
    let v = trash_node_modules(&home);
    assert_eq!(v["applied"], true);
    let removed = v["removed"].as_array().unwrap();
    assert!(!removed.is_empty(), "removed empty: {}", v);
    assert!(
        !home.join("proj/node_modules").exists(),
        "node_modules should have been moved to trash"
    );
    let in_trash = trash_count(&home);
    assert_eq!(in_trash, 1, "moved into the temp HOME's Trash");
    let _ = fs::remove_dir_all(&home);
}

/// REQ-DCLEAN-002.A4 / REQ-DCLEAN-001.A1: `cleanup --apply` reports the
/// volume's disk usage after the clean, as the GUI's `CleanResult.disk`.
#[test]
fn cleanup_apply_envelope_carries_post_clean_disk() {
    let home = temp_dir();
    let v = trash_node_modules(&home);
    let disk = &v["disk"];
    let capacity = disk["capacity_bytes"].as_u64().expect("capacity_bytes");
    let free = disk["free_bytes"].as_u64().expect("free_bytes");
    assert!(capacity > 0 && free <= capacity, "disk: {}", disk);
    assert!(disk["volume"].as_str().is_some_and(|s| !s.is_empty()));

    // A dry-run moves nothing, so it carries no post-clean disk.
    let db = home.join("snap.db");
    let dry = json_in_home(
        &home,
        &[
            "cleanup",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(dry.get("disk").is_none(), "dry-run: {}", dry);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn history_and_restore_round_trip_through_the_cli() {
    let home = temp_dir();
    let v = trash_node_modules(&home);
    let id = v["history_id"].as_str().expect("history_id").to_string();

    let h = json_in_home(&home, &["history", "--format", "json"]);
    assert_eq!(h["kind"], "history");
    let recs = h["records"].as_array().unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0]["id"], id);
    assert_eq!(recs[0]["mode"], "trash");
    assert_eq!(recs[0]["categories"][0], "node_modules");
    assert!(recs[0]["items"][0]["trash_path"].is_string());
    assert!(recs[0]["restored_at"].is_null());

    let r = json_in_home(&home, &["restore", &id, "--format", "json"]);
    assert_eq!(r["kind"], "restore");
    assert_eq!(r["records"][0]["restored"].as_array().unwrap().len(), 1);
    assert!(home.join("proj/node_modules/big.js").exists());

    // Fully restored → refused with a non-zero exit and an RFC 9457 error.
    let again = run_in_home(&home, &["restore", &id, "--format", "json"]);
    assert_eq!(again.status.code(), Some(1));
    let err: Value = serde_json::from_str(String::from_utf8(again.stderr).unwrap().trim()).unwrap();
    assert!(err["detail"]
        .as_str()
        .unwrap()
        .contains("already restored at"));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn history_is_empty_and_restore_unknown_id_is_not_found() {
    let home = temp_dir();
    let h = json_in_home(&home, &["history", "--format", "json"]);
    assert_eq!(h["kind"], "history");
    assert!(h["records"].as_array().unwrap().is_empty());

    let out = run_in_home(&home, &["restore", "no-such-id", "--format", "json"]);
    assert_eq!(out.status.code(), Some(4));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn cleanup_dry_run_finds_nothing_in_clean_tree() {
    let (_dir, db) = scan_tiny_tree();
    let home = home_including_recent();
    let v = json_in_home(
        &home,
        &[
            "cleanup",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(v["kind"], "cleanup");
    assert_eq!(v["applied"], false);
    assert!(v["records"].as_array().unwrap().is_empty());
    assert_eq!(v["total_bytes"], 0);
    assert!(v["summary"].as_array().unwrap().is_empty());
}

#[test]
fn cleanup_summarises_by_category() {
    let dir = temp_dir();
    fs::create_dir_all(dir.join("p1/node_modules")).unwrap();
    fs::create_dir_all(dir.join("p2/node_modules")).unwrap();
    fs::create_dir_all(dir.join("p1/target")).unwrap();
    fs::write(dir.join("p1/node_modules/a.js"), vec![0u8; 4096]).unwrap();
    fs::write(dir.join("p2/node_modules/b.js"), vec![0u8; 1024]).unwrap();
    fs::write(dir.join("p1/target/blob"), vec![0u8; 8192]).unwrap();
    let db = dir.join("snap.db");
    let out = Command::new(disky_bin())
        .args(["scan"])
        .arg(&dir)
        .args(["--db"])
        .arg(&db)
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);

    let home = home_including_recent();
    let v = json_in_home(
        &home,
        &[
            "cleanup",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    let summary = v["summary"].as_array().unwrap();
    let by_cat: std::collections::HashMap<&str, &Value> = summary
        .iter()
        .map(|s| (s["category"].as_str().unwrap(), s))
        .collect();
    let nm = by_cat["node_modules"];
    assert_eq!(nm["paths"], 2);
    assert!(nm["bytes"].as_u64().unwrap() >= 5120);
    let tg = by_cat["target"];
    assert_eq!(tg["paths"], 1);
    assert!(v["total_bytes"].as_u64().unwrap() >= 13312);
}

#[test]
fn stats_summarize_emits_scalar_envelope() {
    let (_dir, db) = scan_tiny_tree();
    let v = run_json(&["stats", "--snapshot", db.to_str().unwrap(), "--summarize"]);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["kind"], "scalar");
    let records = v["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert!(r["bytes"].as_u64().unwrap() >= 13312);
    assert!(r["files"].as_u64().unwrap() >= 3);
    // Scalar envelope must not leak heavy fields agents do not need.
    assert!(r.get("largest_bytes").is_none());
    assert!(r.get("scan_root").is_none());
}

#[test]
fn stats_raw_emits_bare_integer() {
    let (_dir, db) = scan_tiny_tree();
    let out = Command::new(disky_bin())
        .args(["stats", "--snapshot"])
        .arg(&db)
        .args(["--raw"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let body = String::from_utf8(out.stdout).unwrap();
    let n: u64 = body
        .trim()
        .parse()
        .expect("--raw must print a bare integer");
    assert!(n >= 13312, "expected total bytes >= 13312, got {}", n);
}

#[test]
fn gui_categories_match_cli_cleanup_totals() {
    // REQ-DCLEAN-001.A1 — the GUI's category list (`disky::contract`) and
    // `disky cleanup --format json` agree on the same snapshot.
    let home = home_including_recent();
    let root = home.join("work");
    fs::create_dir_all(root.join("p1/node_modules/deep")).unwrap();
    fs::create_dir_all(root.join("p2/node_modules")).unwrap();
    fs::create_dir_all(root.join("p1/target")).unwrap();
    fs::create_dir_all(root.join("p3/__pycache__")).unwrap();
    fs::write(root.join("p1/node_modules/deep/a.js"), vec![1u8; 5000]).unwrap();
    fs::write(root.join("p2/node_modules/b.js"), vec![1u8; 70_000]).unwrap();
    fs::write(root.join("p1/target/blob"), vec![1u8; 9000]).unwrap();
    fs::write(root.join("p3/__pycache__/m.pyc"), vec![1u8; 300]).unwrap();
    let db = home.join("snap.db");
    let out = run_in_home(
        &home,
        &["scan", root.to_str().unwrap(), "--db", db.to_str().unwrap()],
    );
    assert!(out.status.success(), "scan: {:?}", out);

    let limit = disky::contract::VIEW_LIMIT.to_string();
    let v = json_in_home(
        &home,
        &[
            "cleanup",
            "--snapshot",
            db.to_str().unwrap(),
            "--limit",
            &limit,
            "--format",
            "json",
        ],
    );
    let cli: std::collections::BTreeMap<String, (u64, u64, String)> = v["summary"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["category"].as_str().unwrap().to_string(),
                (
                    s["physical_bytes"].as_u64().unwrap(),
                    s["paths"].as_u64().unwrap(),
                    s["safety"].as_str().unwrap().to_string(),
                ),
            )
        })
        .collect();

    let conn = disky::db::open(db.to_str().unwrap()).unwrap();
    let mut ctx = disky::cleanup::ScanContext::from_snapshot(&conn);
    ctx.home = Some(home.clone()); // the CLI ran with HOME = `home`
    let prefs = disky::prefs::load(&prefs_file(&home)).unwrap();
    let ctx = ctx.with_prefs(&prefs);
    let hits = disky::contract::view_hits_with(&conn, &ctx).unwrap();
    let gui: std::collections::BTreeMap<String, (u64, u64, String)> =
        disky::contract::categories_from_hits(&hits)
            .into_iter()
            .map(|c| {
                (
                    c.id,
                    (c.bytes, c.items.len() as u64, c.safety.as_str().to_string()),
                )
            })
            .collect();

    assert_eq!(gui, cli);
    assert_eq!(
        gui.keys().map(String::as_str).collect::<Vec<_>>(),
        ["node_modules", "pycache", "target"]
    );
    let total: u64 = gui.values().map(|(b, _, _)| b).sum();
    assert_eq!(total, v["total_physical_bytes"].as_u64().unwrap());
    let _ = fs::remove_dir_all(&home);
}

/// `<data dir>/prefs.json` under a temp `HOME` (what `dirs::data_local_dir`
/// resolves to there on macOS and Linux).
fn prefs_file(home: &std::path::Path) -> PathBuf {
    let data = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else if cfg!(windows) {
        home.join("AppData/Local")
    } else {
        home.join(".local/share")
    };
    data.join("disky/prefs.json")
}

/// REQ-DSCR-001.A1: `disky prefs` prints every key with its default.
#[test]
fn prefs_prints_every_key_with_its_default() {
    let home = temp_dir();
    let v = json_in_home(&home, &["prefs", "--format", "json"]);
    assert_eq!(v["kind"], "prefs");
    let p = &v["records"][0];
    for k in [
        "trash_by_default",
        "skip_recently_used",
        "safe_only",
        "warn_below_goal",
    ] {
        assert_eq!(p[k], true, "{k}");
    }
    assert_eq!(p["goal_bytes"], 80_000_000_000u64);
    assert_eq!(p["checks"]["enabled"], true);
    assert_eq!(p["checks"]["frequency"], "daily");
    assert_eq!(p["agent"], "claude");
    assert_eq!(p["scheduled_scan"], "weekly");
    assert_eq!(p["excluded"], serde_json::json!([]));
    assert!(!prefs_file(&home).exists(), "reading never writes");
    let _ = fs::remove_dir_all(&home);
}

/// REQ-DSCR-001.A1, A2: a `--set` persists, and the library (the GUI's
/// backend) reads the same store.
#[test]
fn prefs_set_persists_and_is_shared_with_the_library() {
    let home = temp_dir();
    let v = json_in_home(
        &home,
        &[
            "prefs",
            "--set",
            "safe_only=false",
            "--set",
            "goal_bytes=100GB",
        ],
    );
    assert_eq!(v["records"][0]["safe_only"], false);
    let again = json_in_home(&home, &["prefs", "--format", "json"]);
    assert_eq!(again["records"][0]["safe_only"], false);
    assert_eq!(again["records"][0]["goal_bytes"], 100_000_000_000u64);
    let lib = disky::prefs::load(&prefs_file(&home)).unwrap();
    assert!(!lib.safe_only);
    // …and the reverse: a library write (the GUI's set_prefs) is what the CLI reads.
    disky::prefs::set(&prefs_file(&home), "agent", "gemini").unwrap();
    let cli = json_in_home(&home, &["prefs", "--format", "json"]);
    assert_eq!(cli["records"][0]["agent"], "gemini");
    let _ = fs::remove_dir_all(&home);
}

/// REQ-DSCR-001.A1: an unknown key or invalid value exits 2 (usage) and
/// leaves the store byte-for-byte unchanged.
#[test]
fn prefs_bad_key_or_value_exits_usage_and_changes_nothing() {
    let home = temp_dir();
    json_in_home(&home, &["prefs", "--set", "safe_only=false"]);
    let file = prefs_file(&home);
    let before = fs::read(&file).unwrap();
    for bad in [
        vec!["--set", "bogus=1"],
        vec!["--set", "checks.frequency=yearly"],
        vec!["--set", "safe_only=true", "--set", "agent=nope"],
    ] {
        let mut args = vec!["prefs"];
        args.extend(&bad);
        args.extend(["--format", "json"]);
        let out = run_in_home(&home, &args);
        assert_eq!(out.status.code(), Some(2), "{bad:?}: {out:?}");
        let err: Value =
            serde_json::from_str(String::from_utf8(out.stderr).unwrap().trim()).unwrap();
        assert_eq!(err["type"], "https://disky.dev/errors/usage");
        assert_eq!(
            fs::read(&file).unwrap(),
            before,
            "{bad:?} changed the store"
        );
    }
    let _ = fs::remove_dir_all(&home);
}

/// REQ-DSCR-001.A3: a malformed store is reported, never replaced.
#[test]
fn prefs_malformed_store_is_reported_not_replaced() {
    let home = temp_dir();
    let file = prefs_file(&home);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, b"{oops").unwrap();
    let out = run_in_home(&home, &["prefs", "--format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    let out = run_in_home(
        &home,
        &["prefs", "--set", "safe_only=false", "--format", "json"],
    );
    assert!(!out.status.success());
    assert_eq!(fs::read(&file).unwrap(), b"{oops");
    let _ = fs::remove_dir_all(&home);
}

/// REQ-DSCR-002.A2: skip-recently-used exempts safe categories. A cache
/// touched just now is still reported with the default (skip on), because
/// caches are rebuilt automatically and are always recent.
#[test]
fn skip_recently_used_keeps_fresh_safe_categories() {
    let home = temp_dir();
    let cache = app_cache_dir(&home).join("com.example.fresh");
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("blob"), vec![1u8; 4096]).unwrap();
    let db = home.join("snap.db");
    let out = run_in_home(
        &home,
        &["scan", home.to_str().unwrap(), "--db", db.to_str().unwrap()],
    );
    assert!(out.status.success(), "scan: {:?}", out);
    let v = json_in_home(
        &home,
        &[
            "cleanup",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    let cats: Vec<&str> = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["path"].as_str().unwrap().ends_with("com.example.fresh"))
        .map(|r| r["category"].as_str().unwrap())
        .collect();
    assert_eq!(cats, ["app_caches"], "fresh cache still reported: {v}");
    let _ = fs::remove_dir_all(&home);
}

/// REQ-DSCR-002.A1–A3 through the CLI: `cleanup` drops hits inside an
/// excluded folder (with `~` expanded), keeps a missing excluded folder in
/// the list, and with skip-recently-used on leaves out fresh items.
#[test]
fn cleanup_honours_excluded_folders_and_skip_recently_used() {
    let home = temp_dir();
    let root = home.join("work");
    for p in ["keep/node_modules", "private/node_modules"] {
        fs::create_dir_all(root.join(p)).unwrap();
        fs::write(root.join(p).join("x.js"), vec![1u8; 2048]).unwrap();
    }
    let db = home.join("snap.db");
    let out = run_in_home(
        &home,
        &["scan", root.to_str().unwrap(), "--db", db.to_str().unwrap()],
    );
    assert!(out.status.success(), "scan: {:?}", out);
    let cleanup = |home: &std::path::Path| {
        let v = json_in_home(
            home,
            &[
                "cleanup",
                "--snapshot",
                db.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        let mut paths: Vec<String> = v["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["path"].as_str().unwrap().to_string())
            .collect();
        paths.sort();
        paths
    };

    // Defaults: skip-recently-used on → the fresh tree is left out (A2).
    assert!(cleanup(&home).is_empty());

    json_in_home(
        &home,
        &[
            "prefs",
            "--set",
            "skip_recently_used=false",
            "--set",
            "excluded=~/work/private,~/no/such/folder",
        ],
    );
    let keep = slash(&root.join("keep/node_modules"));
    assert_eq!(cleanup(&home), [keep], "excluded folder dropped (A1)");
    let p = json_in_home(&home, &["prefs", "--format", "json"]);
    assert_eq!(
        p["records"][0]["excluded"],
        serde_json::json!(["~/work/private", "~/no/such/folder"]),
        "a missing folder stays listed (A3)"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn schema_lists_the_prefs_command() {
    let v = run_json(&["schema"]);
    let prefs = v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "prefs")
        .expect("prefs in schema");
    assert!(prefs["keys"].as_array().unwrap().len() >= 10);
    assert!(v["records"]["Prefs"].is_object());
}

/// REQ-DSCR-015 parity: `disky history` records are the GUI's History rows
/// (entry + `detail` + `state`) and the envelope carries the total cleared.
#[test]
fn history_records_carry_detail_state_and_total() {
    let home = temp_dir();
    let v = trash_node_modules(&home);
    let freed = v["total_physical_bytes"].as_u64().unwrap_or(0);
    let h = json_in_home(&home, &["history", "--format", "json"]);
    let rec = &h["records"][0];
    assert_eq!(rec["state"], "restorable", "{h}");
    let label = disky::cleanup::category("node_modules").unwrap().label;
    assert_eq!(rec["detail"], label);
    let total = h["total_bytes"].as_u64().expect("total_bytes");
    assert_eq!(total, rec["bytes"].as_u64().unwrap());
    assert!(total > 0 || freed == 0);

    let id = rec["id"].as_str().unwrap().to_string();
    json_in_home(&home, &["restore", &id, "--format", "json"]);
    let h = json_in_home(&home, &["history", "--format", "json"]);
    assert_eq!(h["records"][0]["state"], "restored");
    // `--limit` trims the rows, never the total.
    let h0 = json_in_home(&home, &["history", "--limit", "0", "--format", "json"]);
    assert!(h0["records"].as_array().unwrap().is_empty());
    assert_eq!(h0["total_bytes"], total);
    let _ = fs::remove_dir_all(&home);
}

/// REQ-DSCR-003.A1–A3 through the CLI: `disky map` lists the scan root's
/// immediate children by physical bytes, drills into a folder, carries the
/// total and capacity, and exits 4 for a folder outside the snapshot.
#[test]
fn map_lists_children_largest_first_and_outside_exits_not_found() {
    let home = temp_dir();
    let root = home.join("tree");
    fs::create_dir_all(root.join("big/inner")).unwrap();
    fs::create_dir_all(root.join("small")).unwrap();
    fs::write(root.join("big/inner/a.bin"), vec![1u8; 64 * 1024]).unwrap();
    fs::write(root.join("small/b.bin"), vec![1u8; 8 * 1024]).unwrap();
    fs::write(root.join("c.txt"), vec![1u8; 100]).unwrap();
    let db = home.join("snap.db");
    let out = run_in_home(
        &home,
        &["scan", root.to_str().unwrap(), "--db", db.to_str().unwrap()],
    );
    assert!(out.status.success(), "scan: {:?}", out);

    let m = json_in_home(
        &home,
        &[
            "map",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(m["kind"], "map");
    assert_eq!(m["path"], slash(&root).as_str());
    let names: Vec<&str> = m["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["big", "small", "c.txt"], "{m}");
    assert_eq!(m["records"][0]["has_children"], true);
    assert_eq!(m["records"][2]["has_children"], false);
    let sum: u64 = m["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["bytes"].as_u64().unwrap())
        .sum();
    assert_eq!(m["total_bytes"].as_u64().unwrap(), sum);
    assert!(m["records"][0]["bytes"].as_u64().unwrap() >= 64 * 1024);
    assert!(m["capacity_bytes"].as_u64().unwrap() > 0);

    let big = root.join("big");
    let inner = json_in_home(
        &home,
        &[
            "map",
            big.to_str().unwrap(),
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(inner["records"][0]["name"], "inner");

    let out = run_in_home(
        &home,
        &[
            "map",
            "/definitely/not/scanned",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(out.status.code(), Some(4));
    let err: Value = serde_json::from_str(String::from_utf8(out.stderr).unwrap().trim()).unwrap();
    assert_eq!(err["type"], "https://disky.dev/errors/not-found");

    // Excluded folders are left out (REQ-DSCR-002.A1).
    json_in_home(
        &home,
        &["prefs", "--set", &format!("excluded={}", big.display())],
    );
    let m = json_in_home(
        &home,
        &[
            "map",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(m["records"][0]["name"], "small", "{m}");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn schema_lists_the_map_command_and_history_rows() {
    let v = run_json(&["schema"]);
    let cmds = v["commands"].as_array().unwrap();
    assert!(cmds.iter().any(|c| c["name"] == "map"));
    assert!(v["records"]["MapChild"].is_object());
    assert!(v["records"]["HistoryRow"].is_object());
}

/// A snapshot DB with large files (no real multi-GB files needed): rows
/// `(path, physical GB, days since last access)` under `/data`, every mtime
/// 400 days old so skip-recently-used keeps them.
fn large_fixture(dir: &std::path::Path) -> PathBuf {
    const GB: i64 = 1_000_000_000;
    let db = dir.join("large.db");
    let conn = disky::db::open(db.to_str().unwrap()).unwrap();
    disky::db::create_schema(&conn).unwrap();
    let now = chrono::Utc::now().timestamp();
    let old = now - 400 * 86_400;
    // Real (tiny) files: `large` leaves out files deleted since the scan.
    let data = dir.join("data");
    fs::create_dir_all(&data).unwrap();
    for (name, gb, days) in [
        ("vm.utm", 38, 240),
        ("master.mov", 18, 800),
        ("ubuntu.iso", 4, 400),
        ("tiny.bin", 1, 900),
    ] {
        fs::write(data.join(name), b"x").unwrap();
        let path = slash(&data.join(name));
        let path = path.as_str();
        conn.execute(
            "INSERT INTO files (path, name, size, physical_size, mtime, is_dir, depth, atime)
             VALUES (?, ?, ?, ?, ?, false, 2, ?)",
            duckdb::params![
                path,
                path.rsplit('/').next().unwrap(),
                gb * GB,
                gb * GB,
                old,
                now - days * 86_400
            ],
        )
        .unwrap();
    }
    disky::db::write_scan_meta(
        &conn,
        data.to_str().unwrap(),
        old,
        Some(old + 1),
        true,
        4,
        0,
    )
    .unwrap();
    db
}

#[test]
fn large_lists_files_over_2gb_with_filters_and_counts() {
    // REQ-DSCR-004.A1, A2 through the CLI.
    let home = temp_dir();
    let db = large_fixture(&home);
    let db = db.to_str().unwrap();
    let v = json_in_home(&home, &["large", "--snapshot", db, "--format", "json"]);
    assert_eq!(v["kind"], "large");
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["filter"], "all");
    let paths: Vec<&str> = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    let data = home.join("data");
    let at = |n: &str| slash(&data.join(n));
    assert_eq!(paths, [at("vm.utm"), at("master.mov"), at("ubuntu.iso")]);
    assert_eq!(v["count"], 3);
    assert_eq!(v["total_bytes"].as_u64().unwrap(), 60_000_000_000);
    let first = &v["records"][0];
    assert_eq!(first["name"], "vm.utm");
    assert!(first["last_opened"].as_str().unwrap().ends_with('Z'));
    assert_eq!(first["unused_1y"], false);

    let u = json_in_home(
        &home,
        &[
            "large",
            "--filter",
            "unused-1y",
            "--snapshot",
            db,
            "--format",
            "json",
        ],
    );
    assert_eq!(u["filter"], "unused-1y");
    assert_eq!(u["count"], 2);
    assert!(u["records"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["unused_1y"] == true));
    let big = json_in_home(
        &home,
        &[
            "large",
            "--filter",
            "over-10gb",
            "--snapshot",
            db,
            "--format",
            "json",
        ],
    );
    assert_eq!(
        (big["count"].as_u64(), big["total_bytes"].as_u64()),
        (Some(2), Some(56_000_000_000))
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn large_bad_filter_exits_usage_and_unknown_snapshot_exits_not_found() {
    let home = temp_dir();
    let db = large_fixture(&home);
    let out = run_in_home(
        &home,
        &[
            "large",
            "--filter",
            "huge",
            "--snapshot",
            db.to_str().unwrap(),
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    let out = run_in_home(
        &home,
        &[
            "large",
            "--snapshot",
            "1999-01-01_00-00",
            "--format",
            "json",
        ],
    );
    assert_eq!(out.status.code(), Some(4));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn trash_moves_named_paths_and_records_one_titled_entry() {
    // REQ-DSCR-006.A1, A4: temp HOME → temp Trash and history log.
    let home = temp_dir();
    let a = home.join("files/a.img");
    let b = home.join("files/b.iso");
    fs::create_dir_all(a.parent().unwrap()).unwrap();
    fs::write(&a, vec![1u8; 64 * 1024]).unwrap();
    fs::write(&b, vec![1u8; 8 * 1024]).unwrap();
    let gone = home.join("files/gone.dmg");
    let v = json_in_home(
        &home,
        &[
            "trash",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            gone.to_str().unwrap(),
            "--title",
            "Large files",
            "--format",
            "json",
        ],
    );
    assert_eq!(v["kind"], "trash");
    assert_eq!(v["handled"].as_array().unwrap().len(), 2, "{v}");
    assert_eq!(v["failed"][0]["path"], slash(&gone).as_str());
    assert_eq!(v["failed"][0]["reason"], "missing");
    assert!(v["freed_physical_bytes"].as_u64().unwrap() >= 72 * 1024);
    assert!(v["disk"]["capacity_bytes"].as_u64().unwrap() > 0);
    assert!(!a.exists() && !b.exists());
    let in_trash = trash_count(&home);
    assert_eq!(in_trash, 2, "moved into the temp HOME's Trash");

    let h = json_in_home(&home, &["history", "--format", "json"]);
    let rows = h["records"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "one history entry");
    assert_eq!(rows[0]["title"], "Large files");
    assert_eq!(rows[0]["id"], v["history_id"]);
    assert_eq!(rows[0]["detail"], "a.img, b.iso");

    // Restorable through the same log.
    let r = json_in_home(
        &home,
        &[
            "restore",
            v["history_id"].as_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(r["records"][0]["restored"].as_array().unwrap().len(), 2);
    assert!(a.exists() && b.exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn schema_lists_the_large_and_trash_commands() {
    let v = run_json(&["schema"]);
    let cmds = v["commands"].as_array().unwrap();
    assert!(cmds.iter().any(|c| c["name"] == "large"));
    assert!(cmds.iter().any(|c| c["name"] == "trash"));
    assert!(v["records"]["LargeFile"].is_object());
}

/// A temp HOME with recent files included, and `<home>/pics` holding two
/// byte-identical photos, a same-named different photo, and a hard link.
/// Returns `(home, snapshot db)`.
fn dupes_fixture() -> (PathBuf, PathBuf) {
    let home = home_including_recent();
    let pics = home.join("pics");
    fs::create_dir_all(pics.join("a")).unwrap();
    fs::create_dir_all(pics.join("b")).unwrap();
    fs::write(pics.join("a/IMG_1.HEIC"), vec![5u8; 2048]).unwrap();
    fs::write(pics.join("b/IMG_1 copy.HEIC"), vec![5u8; 2048]).unwrap();
    fs::write(pics.join("b/IMG_1.HEIC"), vec![6u8; 2048]).unwrap();
    fs::hard_link(pics.join("a/IMG_1.HEIC"), pics.join("a/link.HEIC")).unwrap();
    let db = home.join("snap.db");
    let out = run_in_home(
        &home,
        &["scan", pics.to_str().unwrap(), "--db", db.to_str().unwrap()],
    );
    assert!(out.status.success(), "scan: {:?}", out);
    (home, db)
}

#[test]
fn dupes_cli_json_envelope() {
    let (home, db) = dupes_fixture();
    let v = json_in_home(
        &home,
        &[
            "dupes",
            "--min-size",
            "1",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(v["kind"], "dupes");
    assert_eq!(v["min_size"], 1);
    assert_eq!(v["groups"], 1, "{v}");
    assert_eq!(v["extra_copies"], 1, "the hard link counts once");
    let g = &v["records"][0];
    assert!(g["bytes"].as_u64().unwrap() >= 2048);
    assert_eq!(v["reclaimable_bytes"], g["bytes"]);
    let copies: Vec<&str> = g["copies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert_eq!(copies.len(), 2);
    assert!(
        copies.iter().any(|p| p.ends_with("b/IMG_1 copy.HEIC")),
        "same bytes, different name: {copies:?}"
    );
    assert!(
        !copies.iter().any(|p| p.ends_with("b/IMG_1.HEIC")),
        "same name, different bytes is not a duplicate"
    );
    // Default minimum (1 MB) leaves these 2 KB files out.
    let d = json_in_home(
        &home,
        &[
            "dupes",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert_eq!(
        (d["groups"].as_u64(), d["min_size"].as_u64()),
        (Some(0), Some(1_000_000))
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn dupes_bad_min_size_exits_2() {
    let home = temp_dir();
    for bad in ["0", "lots", "-5"] {
        let out = run_in_home(&home, &["dupes", "--min-size", bad, "--format", "json"]);
        assert_eq!(out.status.code(), Some(2), "{bad}: {out:?}");
    }
    let out = run_in_home(
        &home,
        &[
            "dupes",
            "--snapshot",
            "1999-01-01_00-00",
            "--format",
            "json",
        ],
    );
    assert_eq!(out.status.code(), Some(4));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn schema_lists_the_dupes_command() {
    let v = run_json(&["schema"]);
    let cmds = v["commands"].as_array().unwrap();
    assert!(cmds.iter().any(|c| c["name"] == "dupes"));
    assert!(v["records"]["DupeGroup"].is_object());
}

/// REQ-DSCR-007.A1, A2: `disky agents` lists claude, codex and gemini in that
/// order, finds an executable on PATH, and never runs it.
#[cfg(unix)]
#[test]
fn agents_lists_three_in_order_and_never_runs_them() {
    use std::os::unix::fs::PermissionsExt;
    let home = temp_dir();
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let marker = home.join("codex-ran");
    let exe = bin.join("codex");
    fs::write(&exe, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(disky_bin())
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("PATH", &bin)
        .env_remove("DISKY_CONFIG_PATH")
        .env_remove("DISKY_FORMAT")
        .args(["agents", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "agents: {out:?}");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["kind"], "agents");
    let recs = v["records"].as_array().unwrap();
    let ids: Vec<&str> = recs.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["claude", "codex", "gemini"]);
    let names: Vec<&str> = recs.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Claude Code", "Codex CLI", "Gemini CLI"]);
    assert_eq!(recs[1]["found"], true);
    assert_eq!(recs[1]["path"], slash(&exe).as_str());
    assert_eq!(recs[1]["display_path"], "~/bin/codex");
    assert!(!marker.exists(), "detection ran the CLI");
}

/// REQ-DSCR-008.A1, A3: `disky suggest` reports `ok` (free at or above a tiny
/// goal) and `dev` for a scanned node_modules — with checks off too — and
/// only goal/ok with no snapshot.
#[test]
fn suggest_reports_goal_or_ok_then_category_suggestions() {
    let home = home_including_recent();
    // No snapshot yet: goal or ok only.
    let v = json_in_home(&home, &["suggest", "--format", "json"]);
    assert_eq!(v["kind"], "suggestions");
    let recs = v["records"].as_array().unwrap();
    assert_eq!(recs.len(), 1);
    assert!(recs[0]["key"] == "goal" || recs[0]["key"] == "ok");

    json_in_home(
        &home,
        &[
            "prefs",
            "--set",
            "goal_bytes=1",
            "--set",
            "checks.enabled=false",
            "--format",
            "json",
        ],
    );
    fs::create_dir_all(home.join("proj/app/node_modules")).unwrap();
    fs::write(home.join("proj/app/node_modules/big.js"), vec![1u8; 4096]).unwrap();
    let db = home.join("snap.db");
    let proj = home.join("proj");
    let out = run_in_home(
        &home,
        &["scan", proj.to_str().unwrap(), "--db", db.to_str().unwrap()],
    );
    assert!(out.status.success(), "scan: {out:?}");
    let v = json_in_home(
        &home,
        &[
            "suggest",
            "--snapshot",
            db.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    let recs = v["records"].as_array().unwrap();
    let keys: Vec<&str> = recs.iter().map(|r| r["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["ok", "dev"]);
    assert_eq!(recs[0]["actionable"], false);
    assert!(recs[0]["bytes"].is_null());
    assert_eq!(recs[1]["kind"], "Suggestion");
    assert_eq!(recs[1]["categories"], serde_json::json!(["node_modules"]));
    assert!(recs[1]["bytes"].as_u64().unwrap() > 0);
    assert!(recs[1]["text"]
        .as_str()
        .unwrap()
        .starts_with("One project in ~/proj"));
}

#[test]
fn schema_lists_the_agents_and_suggest_commands() {
    let v = run_json(&["schema"]);
    let cmds = v["commands"].as_array().unwrap();
    assert!(cmds.iter().any(|c| c["name"] == "agents"));
    assert!(cmds.iter().any(|c| c["name"] == "suggest"));
    assert!(v["records"]["Suggestion"].is_object());
    assert!(v["records"]["Agent"].is_object());
}

/// A snapshot another process is reading stays readable (bug: every query
/// opened the snapshot read-write, so DuckDB's exclusive lock made the CLI,
/// and the GUI's own screens, fail with `lock-held` while the GUI's
/// background checks read it). Here a read-only connection stays open while
/// the CLI maps and cleans the same file.
#[test]
fn snapshot_stays_readable_while_another_reader_holds_it() {
    let home = temp_dir();
    let root = home.join("tree");
    fs::create_dir_all(root.join("app/node_modules/x")).unwrap();
    fs::write(root.join("app/node_modules/x/i.js"), vec![1u8; 4096]).unwrap();
    let db = home.join("snap.db");
    let out = run_in_home(
        &home,
        &["scan", root.to_str().unwrap(), "--db", db.to_str().unwrap()],
    );
    assert!(out.status.success(), "scan: {:?}", out);

    let held = disky::db::open_read(db.to_str().unwrap()).unwrap();
    let n: i64 = held
        .query_row("SELECT count(*) FROM files", [], |r| r.get(0))
        .unwrap();
    assert!(n > 0);
    let snap = db.to_str().unwrap();
    let m = json_in_home(&home, &["map", "--snapshot", snap, "--format", "json"]);
    assert_eq!(m["kind"], "map");
    let c = json_in_home(&home, &["cleanup", "--snapshot", snap, "--format", "json"]);
    assert_eq!(c["kind"], "cleanup");
    drop(held);
}
