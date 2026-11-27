//! Static JSON-Schema-ish descriptor for disky's machine-facing surface.
//! Emitted via `disky schema` so agents can bind to commands and output
//! shapes without prompt-engineering.

use serde_json::{json, Value};

use crate::cleanup::{LocationFilter, Matcher, CATEGORIES};
use crate::query::SCHEMA_VERSION;

pub fn document() -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "tool": "disky",
        "version": env!("CARGO_PKG_VERSION"),
        "commands": commands(),
        "records": records(),
        "errors": errors(),
        "snapshot_refs": {
            "description": "Accepted by every query command and MCP tool",
            "forms": ["@latest", "<id like 2026-05-15_11-56>", "<filesystem path>"],
        }
    })
}

fn commands() -> Value {
    json!([
        {
            "name": "scan", "args": {
                "path": "string (default '/')",
                "db": "string? (default auto-named in data dir)",
                "emit_top": "int?",
                "emit_dirs": "int?",
                "emit_ext": "int?",
                "emit_stats": "bool"
            },
            "stderr": "NDJSON {start,progress,done} when stderr piped, else spinner",
            "output": "scan_bundle when any emit_* flag is set"
        },
        { "name": "top",    "args": snapshot_with(&["limit:int=50", "min_size:int=0"]), "output": "FileRow[]" },
        { "name": "dirs",   "args": snapshot_with(&["limit:int=30"]),                   "output": "DirRow[]" },
        { "name": "ext",    "args": snapshot_with(&["limit:int=30"]),                   "output": "ExtRow[]" },
        { "name": "find",   "args": snapshot_with(&["pattern:string", "limit:int=50"]), "output": "FileRow[]" },
        { "name": "stats",  "args": snapshot_with(&["summarize:bool", "raw:bool"]),     "output": "Stats | Scalar (with --summarize) | raw u64 (with --raw)" },
        { "name": "query",  "args": snapshot_with(&["sql:string", "limit:int=1000"]),    "output": "Object[]" },
        { "name": "list",   "args": {},                                                  "output": "Snapshot[]" },
        {
            "name": "diff",
            "args": { "a": "@latest|<id>|<path>", "b": "@latest|<id>|<path>", "limit": "int=100" },
            "output": "DiffRow[]"
        },
        {
            "name": "filter",
            "args": {
                "where": "predicate string (e.g. \"size > 1GB AND ext = 'log'\")",
                "limit": "int=1000"
            },
            "input": "JSON envelope on stdin — kinds: top, find, dirs, ext, empty, old, filter, growth",
            "output": "kind='filter' envelope: {schema_version, kind, input_kind, records}"
        },
        {
            "name": "growth",
            "args": { "since": "@latest|@latest~N|<id>|<path> (default @latest~1)", "until": "@latest|<id>|<path> (default @latest)", "over": "duration? (e.g. 7d) — overrides --since", "over_n": "int? — N>=3 enables OLS fit over N most-recent snapshots (kind='growth_n')", "fill_target": "u64? — free-byte budget for projected_fill_date (only with --over-n)", "limit": "int=50" },
            "output": "Two shapes. Default (no --over-n): GrowthRow[] under kind='growth'. Records: {path, kind: grew|shrank|added|removed, size_a:u64, size_b:u64, delta_bytes:i64, rate_bytes_per_day:f64, days_between:f64}. With --over-n N: GrowthNRow[] under kind='growth_n' with envelope fields {schema_version, kind, n_snapshots, fill_target, records}. Records: {path, slope_bytes_per_day:i64, r2:f64, latest_bytes:u64, n_snapshots, sample_paths_ts:[(unix_secs:i64, bytes:u64)], projected_fill_date?:rfc3339}"
        },
        {
            "name": "churn",
            "args": { "over": "duration (default 24h)", "snapshot": "@latest|<id>|<path>", "limit": "int=50" },
            "output": "ChurnRow[] under kind='churn'. Records: {path, recent_files:u64, recent_bytes:u64, total_files:u64, total_bytes:u64, churn_score:f64 (0..1)}"
        },
        { "name": "empty", "args": snapshot_with(&["limit:int=100"]), "output": "FileRow[] under kind='empty'" },
        { "name": "old", "args": snapshot_with(&["older_than:duration", "limit:int=100"]), "output": "FileRow[] under kind='old' (older_than: 30d|2w|6mo|1y syntax)" },
        {
            "name": "forget",
            "args": {
                "keep_last": "int?",
                "keep_daily": "int?",
                "keep_weekly": "int?",
                "keep_monthly": "int?",
                "keep_yearly": "int?",
                "apply": "bool (default false — dry-run)"
            },
            "output": "forget envelope: {kind:'forget', applied:bool, kept:[KeptSnapshot], removed:[SnapshotMeta], skipped_unparseable:[string], total_removed_bytes:u64}"
        },
        {
            "name": "cleanup",
            "args": {
                "target": "string[] (comma-separated)",
                "snapshot": "@latest|<id>|<path>",
                "apply": "bool (default false — dry-run unless set)",
                "reversible": "bool (default false — with apply, trash instead of rm)"
            },
            "output": "cleanup envelope: {kind:'cleanup', applied:bool, removed:[string], history_id:string? (set when --apply recorded a history entry), records:[CleanupHit], summary:[CategorySummary], total_bytes:u64, total_physical_bytes:u64, disk:{volume:string, capacity_bytes:u64, free_bytes:u64}? (set after --apply: the volume's usage once the clean finished)}",
            "targets": cleanup_categories()
        },
        {
            "name": "history",
            "args": { "limit": "int=50" },
            "output": "history envelope: {kind:'history', total_bytes:u64 (physical, freed over the whole log), records:[HistoryRow]} newest first. HistoryRow = HistoryEntry + {detail:string, state:restorable|restored|emptied|deleted}: the rows the desktop GUI's History screen shows. Log file: <data dir>/history.json"
        },
        {
            "name": "large",
            "args": { "filter": "all|over-10gb|unused-1y (default all)", "snapshot": "@latest|<id>|<path>" },
            "output": "large envelope: {kind:'large', filter, count:u64, total_bytes:u64 (physical, over the filtered list), records:[LargeFile]} — regular files >= 2 GB physical (over-10gb: >= 10 GB; unused-1y: last opened > 365 days ago), largest first; excluded folders and skip_recently_used (prefs) apply",
            "errors": "not-found (snapshot)"
        },
        {
            "name": "dupes",
            "args": { "min_size": "bytes (default 1MB = 1000000; KB/MB/GB decimal, KiB/MiB/GiB binary)", "snapshot": "@latest|<id>|<path>" },
            "output": "dupes envelope: {kind:'dupes', min_size:u64, groups:u64, extra_copies:u64, reclaimable_bytes:u64 (physical), records:[DupeGroup]} — regular files with byte-identical contents (size, then a full BLAKE3 hash; never by name), largest reclaimable (bytes x (copies - 1)) first; each file re-checked on disk, hard links count once; excluded folders and skip_recently_used (prefs) apply",
            "errors": "usage (bad --min-size), not-found (snapshot)"
        },
        {
            "name": "trash",
            "args": { "paths": "string[] (required; named paths count as reviewed)", "title": "string (history entry title, default 'Cleanup')" },
            "output": "trash envelope: {kind:'trash', handled:[string], failed:[{path, reason}], freed_physical_bytes:u64, history_id:string?, disk:{volume, capacity_bytes, free_bytes}} — one history entry with the given title; restorable via `disky restore <id>`"
        },
        {
            "name": "map",
            "args": { "path": "string? (absolute folder; default the scan root)", "tiles": "usize? (keep at most N blocks; the rest, and children under 0.5% of the folder, fold into `other`)", "snapshot": "@latest|<id>|<path>" },
            "output": "map envelope: {kind:'map', path, name, root (scan root), total_bytes:u64 (physical), capacity_bytes:u64 (volume), records:[MapChild], other:{count:u64, bytes:u64}?} — immediate children, largest first; excluded folders (prefs) left out; `other` sums what --tiles folded (null otherwise)",
            "errors": "not-found (folder not in the snapshot)"
        },
        {
            "name": "restore",
            "args": { "id": "string (HistoryEntry.id)" },
            "output": "RestoreReport[] (one record) under kind='restore'. Never overwrites; re-run retries items not yet restored",
            "errors": "not-found (unknown id); generic (delete-mode entry, or already restored)"
        },
        {
            "name": "agents",
            "args": {},
            "output": "Agent[] under kind='agents' — claude (Claude Code), codex (Codex CLI), gemini (Gemini CLI), in that order; looked up in PATH, ~/.local/bin, /opt/homebrew/bin, /usr/local/bin; executable bit only, never run"
        },
        {
            "name": "suggest",
            "args": { "snapshot": "@latest|<id>|<path>" },
            "output": "Suggestion[] under kind='suggestions', in feed order: goal (free < goal; offers the safe categories) or ok (no action), then dev (developer categories), dl (old_downloads), left (app_leftovers) when they have hits. No snapshot: goal/ok only, goal without action. Reported whatever checks.enabled says; never cleans",
            "errors": "not-found (snapshot)"
        },
        {
            "name": "prefs",
            "args": { "set": "string[] (repeatable `<key>=<value>`; all applied or none)" },
            "keys": crate::prefs::KEYS,
            "output": "Prefs (one record) under kind='prefs'. Store: <data dir>/prefs.json, shared with the desktop GUI",
            "errors": "usage (unknown key or invalid value; nothing is written); generic (malformed store — never replaced)"
        }
    ])
}

/// Every cleanup category with its safety class, straight from the engine
/// table (`cleanup::CATEGORIES`).
fn cleanup_categories() -> Value {
    CATEGORIES
        .iter()
        .map(|c| match c.matcher {
            Matcher::Basename(b) => json!({
                "name": c.name,
                "label": c.label,
                "description": c.description,
                "icon": c.icon,
                "safety": c.safety,
                "kind": "basename",
                "basenames": b,
            }),
            Matcher::Location {
                locations, filter, ..
            } => json!({
                "name": c.name,
                "label": c.label,
                "description": c.description,
                "icon": c.icon,
                "safety": c.safety,
                "kind": "location",
                "locations": locations
                    .iter()
                    .map(|l| json!({"pattern": l.pattern, "safety": l.safety}))
                    .collect::<Vec<_>>(),
                "filter": match filter {
                    LocationFilter::All => json!(null),
                    LocationFilter::OlderThanDays(d) => json!({"older_than_days": d}),
                    LocationFilter::AppLeftovers => json!("no_installed_app"),
                },
                "note": c.note,
            }),
        })
        .collect()
}

fn snapshot_with(extras: &[&str]) -> Value {
    let mut m = serde_json::Map::new();
    m.insert(
        "snapshot".into(),
        Value::String("@latest|<id>|<path>".into()),
    );
    for e in extras {
        if let Some((k, v)) = e.split_once(':') {
            m.insert(k.into(), Value::String(v.into()));
        }
    }
    Value::Object(m)
}

fn records() -> Value {
    json!({
        "FileRow":    { "path": "string", "size": "u64", "ext": "string?", "mtime": "string? (RFC3339 UTC)" },
        "files_table": { "columns": "path, name, ext, size, physical_size, mtime, is_dir, depth", "note": "physical_size = st_blocks*512 on Unix; differs wildly from size for APFS sparse files (e.g. OrbStack data.img: 8.8TB logical vs 13GB physical)" },
        "DirRow":     { "path": "string", "total_size": "u64" },
        "ExtRow":     { "ext": "string", "files": "u64", "total_size": "u64" },
        "Stats":      {
            "files": "u64", "dirs": "u64", "total_bytes": "u64",
            "largest_bytes": "u64", "avg_bytes": "u64",
            "partial": "bool",
            "scan_root": "string?", "scan_duration_s": "i64?", "scanned_at": "string? (RFC3339 UTC)"
        },
        "Scalar":     { "bytes": "u64", "files": "u64" },
        "Snapshot":   { "path": "string", "id": "string?", "bytes": "u64" },
        "CleanupHit":      { "category": "string", "safety": "safe|review", "path": "string", "bytes": "u64 (logical)", "physical_bytes": "u64 (on disk)", "files": "u64", "note": "string? (e.g. trash: permanent, refused with --reversible)" },
        "CategorySummary": { "category": "string", "safety": "safe|review (review if any hit is)", "paths": "u64", "bytes": "u64 (logical)", "physical_bytes": "u64 (on disk)", "files": "u64" },
        "HistoryEntry":    { "id": "string (YYYY-MM-DD_HH-MM-SS-xxxx)", "title": "Smart Clean|Assistant cleanup|Large files|Duplicates|Cleanup", "ts": "string (RFC3339 UTC)", "mode": "trash|delete", "categories": "string[]", "bytes": "u64", "items": "HistoryItem[]", "restored_at": "string? (RFC3339 UTC, set once every item is restored)" },
        "HistoryRow":      { "...": "every HistoryEntry field", "detail": "string (category names, or file names for 'Large files' / 'Duplicates'; '<n> files' past three)", "state": "restorable|restored|emptied|deleted (emptied: trash entry 30+ days old)" },
        "LargeFile":       { "name": "string", "path": "string (absolute)", "location": "string (parent folder, home as ~)", "bytes": "u64 (physical)", "last_opened": "string? (RFC3339 UTC access time; null for snapshots taken before access times were recorded)", "unused_1y": "bool (last opened more than 365 days ago)" },
        "MapChild":        { "name": "string", "path": "string (absolute)", "bytes": "u64 (physical, recursive)", "has_children": "bool" },
        "DupeGroup":       { "name": "string (first copy's basename)", "bytes": "u64 (physical, per copy)", "copies": "DupeCopy[] (2+, oldest mtime first)" },
        "DupeCopy":        { "path": "string (absolute)", "location": "string (parent folder, home as ~)", "mtime": "string? (RFC3339 UTC)" },
        "HistoryItem":     { "original_path": "string", "trash_path": "string? (null in delete mode)", "bytes": "u64", "status": "in_trash|restored|conflict|missing|deleted" },
        "RestoreReport":   { "id": "string", "restored": "string[]", "conflicts": "string[]", "missing": "string[] (no longer in Trash)" },
        "Agent":           { "id": "claude|codex|gemini", "name": "string (display name)", "found": "bool", "path": "string? (absolute executable path)", "display_path": "string? (path with home as ~)" },
        "Suggestion":      { "key": "goal|ok|dev|dl|left", "kind": "Goal|On track|Suggestion|Heads-up", "text": "string (every figure from the snapshot or the disk)", "bytes": "u64? (physical; null when nothing is offered)", "categories": "string[] (cleanup category ids the review selects)", "actionable": "bool", "at": "string? (RFC3339 UTC: now for goal/ok, the scan for the others)" },
        "Prefs":           { "trash_by_default": "bool", "skip_recently_used": "bool (leave out items modified in the last 30 days)", "safe_only": "bool", "warn_below_goal": "bool", "goal_bytes": "u64", "checks": { "enabled": "bool", "frequency": "hourly|daily|weekly" }, "agent": "claude|codex|gemini", "scheduled_scan": "off|weekly|monthly", "excluded": "string[] (absolute or ~/ folders; never touched by cleanup)" },
        "DiffRow":    { "path": "string", "kind": "added|removed|grew|shrank", "size_a": "u64", "size_b": "u64", "delta": "i64" },
        "GrowthRow":  { "path": "string", "kind": "grew|shrank|added|removed", "size_a": "u64", "size_b": "u64", "delta_bytes": "i64", "rate_bytes_per_day": "f64", "days_between": "f64" },
        "ChurnRow":   { "path": "string", "recent_files": "u64", "recent_bytes": "u64", "total_files": "u64", "total_bytes": "u64", "churn_score": "f64 (0..1)" },
        "envelope":   { "schema_version": "u32", "kind": "string", "records": "T[]" },
        "error":      { "schema_version": "u32", "type": "string (URI)", "title": "string", "status": "i32", "detail": "string", "retryable": "bool", "instance": "string (per-error UUID, RFC 9457 instance)" }
    })
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn document_top_level_keys_are_stable() {
        let doc = document();
        assert_eq!(doc["tool"], "disky");
        assert_eq!(doc["schema_version"], SCHEMA_VERSION);
        assert!(doc["commands"].is_array());
        assert!(doc["records"].is_object());
        assert!(doc["errors"].is_array());
        assert!(doc["snapshot_refs"]["forms"].is_array());
    }

    #[test]
    fn every_exit_code_has_schema_entry() {
        let doc = document();
        let codes: std::collections::HashSet<i64> = doc["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["code"].as_i64().unwrap())
            .collect();
        // ExitCode 0..=6 inclusive
        for c in 0..=6 {
            assert!(codes.contains(&c), "missing error code {} in schema", c);
        }
    }

    #[test]
    fn cleanup_categories_carry_label_and_description() {
        let doc = document();
        let cleanup = doc["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "cleanup")
            .expect("cleanup command in schema");
        let targets = cleanup["targets"].as_array().expect("targets");
        assert_eq!(targets.len(), CATEGORIES.len());
        for (t, c) in targets.iter().zip(CATEGORIES) {
            assert_eq!(t["name"], c.name);
            assert_eq!(t["label"], c.label);
            assert_eq!(t["description"], c.description);
            assert_eq!(t["icon"], c.icon);
            assert!(
                !c.label.is_empty() && !c.description.is_empty(),
                "{}",
                c.name
            );
            let safety = t["safety"].as_str().unwrap();
            assert!(safety == "safe" || safety == "review");
        }
    }

    #[test]
    fn dupes_command_is_listed_with_its_record() {
        let doc = document();
        let dupes = doc["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "dupes")
            .expect("dupes command in schema");
        assert!(dupes["args"]["min_size"].is_string());
        assert!(dupes["output"].as_str().unwrap().contains("kind:'dupes'"));
        assert!(doc["records"]["DupeGroup"]["copies"].is_string());
    }

    #[test]
    fn agents_and_suggest_commands_are_listed_with_their_records() {
        let doc = document();
        let cmds = doc["commands"].as_array().unwrap();
        for name in ["agents", "suggest"] {
            assert!(cmds.iter().any(|c| c["name"] == name), "{name} missing");
        }
        let suggest = cmds.iter().find(|c| c["name"] == "suggest").unwrap();
        assert!(suggest["output"]
            .as_str()
            .unwrap()
            .contains("kind='suggestions'"));
        assert!(doc["records"]["Agent"]["found"].is_string());
        assert!(doc["records"]["Suggestion"]["key"].is_string());
    }

    #[test]
    fn prefs_command_lists_every_key() {
        let doc = document();
        let prefs = doc["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "prefs")
            .expect("prefs command in schema");
        let keys: Vec<&str> = prefs["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        assert_eq!(keys, crate::prefs::KEYS);
    }

    #[test]
    fn record_shapes_include_core_types() {
        let doc = document();
        let records = &doc["records"];
        for name in [
            "FileRow",
            "ExtRow",
            "DirRow",
            "Stats",
            "CleanupHit",
            "DiffRow",
            "HistoryEntry",
            "HistoryItem",
            "HistoryRow",
            "MapChild",
            "LargeFile",
            "DupeGroup",
            "DupeCopy",
            "RestoreReport",
            "Prefs",
        ] {
            assert!(records[name].is_object(), "missing record {}", name);
        }
    }
}

fn errors() -> Value {
    json!([
        { "code": 0, "slug": "ok",           "type": "https://disky.dev/errors/ok",           "retryable": false },
        { "code": 1, "slug": "generic",      "type": "https://disky.dev/errors/generic",      "retryable": false },
        { "code": 2, "slug": "usage",        "type": "https://disky.dev/errors/usage",        "retryable": false },
        { "code": 3, "slug": "io",           "type": "https://disky.dev/errors/io",           "retryable": true  },
        { "code": 4, "slug": "not-found",    "type": "https://disky.dev/errors/not-found",    "retryable": false },
        { "code": 5, "slug": "partial-scan", "type": "https://disky.dev/errors/partial-scan", "retryable": false },
        { "code": 6, "slug": "lock-held",    "type": "https://disky.dev/errors/lock-held",    "retryable": true  }
    ])
}
