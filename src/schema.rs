//! Static JSON-Schema-ish descriptor for disky's machine-facing surface.
//! Emitted via `disky schema` so agents can bind to commands and output
//! shapes without prompt-engineering.

use serde_json::{json, Value};

use crate::cleanup::TARGETS;
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
            "name": "growth",
            "args": { "since": "@latest|@latest~N|<id>|<path> (default @latest~1)", "until": "@latest|<id>|<path> (default @latest)", "limit": "int=50" },
            "output": "GrowthRow[] under kind='growth'. Records: {path, kind: grew|shrank|added|removed, size_a:u64, size_b:u64, delta_bytes:i64, rate_bytes_per_day:f64, days_between:f64}"
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
            "output": "CleanupHit[]",
            "targets": TARGETS.iter().map(|(n, b)| json!({"name": n, "basenames": b})).collect::<Vec<_>>()
        }
    ])
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
        "CleanupHit":      { "category": "string", "path": "string", "bytes": "u64", "files": "u64" },
        "CategorySummary": { "category": "string", "paths": "u64", "bytes": "u64", "files": "u64" },
        "DiffRow":    { "path": "string", "kind": "added|removed|grew|shrank", "size_a": "u64", "size_b": "u64", "delta": "i64" },
        "GrowthRow":  { "path": "string", "kind": "grew|shrank|added|removed", "size_a": "u64", "size_b": "u64", "delta_bytes": "i64", "rate_bytes_per_day": "f64", "days_between": "f64" },
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
