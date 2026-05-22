//! Minimal MCP server over stdio for disky.
//!
//! Implements just enough of the Model Context Protocol (JSON-RPC 2.0,
//! newline-delimited) to expose disky's query layer as typed tool calls:
//! `initialize`, `tools/list`, `tools/call`. No external MCP SDK dependency.

use disky::exit::{classify, DiskyError, ExitCode};
use disky::policy::{apply_policy, Policy, SnapshotMeta};
use disky::query::{self, SCHEMA_VERSION};
use disky::{cleanup, db, scan, schema, snapshots};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "disky-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut line = String::new();

    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    error_response(Value::Null, -32700, &format!("parse error: {}", e))
                );
                let _ = stdout.flush();
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let params = request.get("params").cloned().unwrap_or(Value::Null);

        // Notifications (no id) get no response.
        let is_notification = request.get("id").is_none();

        let response = match method.as_str() {
            "initialize" => Some(initialize_response(id.clone())),
            "initialized" | "notifications/initialized" => None,
            "ping" => Some(success(id.clone(), json!({}))),
            "tools/list" => Some(success(id.clone(), tools_list())),
            "tools/call" => Some(handle_tool_call(id.clone(), params)),
            "shutdown" => Some(success(id.clone(), json!({}))),
            _ => Some(error_response(
                id.clone(),
                -32601,
                &format!("method not found: {}", method),
            )),
        };

        if is_notification {
            continue;
        }

        if let Some(resp) = response {
            let _ = writeln!(stdout, "{}", resp);
            let _ = stdout.flush();
        }
    }
}

fn initialize_response(id: Value) -> Value {
    success(
        id,
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
        }),
    )
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

/// Convert a `DiskyError` into an MCP tool-call result (`isError: true` content
/// with the RFC 9457 payload as text). Tool errors are not JSON-RPC errors —
/// the call succeeds and the result carries an error flag.
fn tool_error_result(id: Value, err: &DiskyError) -> Value {
    let body = json!({
        "schema_version": SCHEMA_VERSION,
        "type": format!("https://disky.dev/errors/{}", err.code.slug()),
        "title": err.title,
        "status": err.code as i32,
        "detail": err.detail,
        "retryable": err.retryable,
        "instance": err.instance,
    });
    success(
        id,
        json!({
            "isError": true,
            "content": [{ "type": "text", "text": body.to_string() }],
        }),
    )
}

fn tool_ok_result(id: Value, payload: Value) -> Value {
    success(
        id,
        json!({
            "content": [{ "type": "text", "text": payload.to_string() }],
        }),
    )
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "disky_scan",
                "description": "Scan a directory tree and write a new DuckDB snapshot. Returns the snapshot path + final stats.",
                "annotations": {
                    "title": "Scan filesystem",
                    "readOnlyHint": false,
                    "destructiveHint": false,
                    "idempotentHint": false,
                    "openWorldHint": true
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory to scan", "default": "/" },
                        "db": { "type": "string", "description": "Snapshot DB path (default: auto-named in ~/Library/Application Support/disky/)" },
                        "emit_top": { "type": "integer", "description": "Also return top-N files in result (cuts a round-trip)" },
                        "emit_dirs": { "type": "integer", "description": "Also return top-N dirs in result" },
                        "emit_ext": { "type": "integer", "description": "Also return top-N extensions in result" }
                    }
                }
            },
            {
                "name": "disky_top",
                "description": "Largest files in a snapshot. Bytes are u64, paths absolute, mtime RFC 3339 UTC.",
                "annotations": {
                    "title": "Top files by size",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "destructiveHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "snapshot": { "type": "string", "description": "Snapshot path or '@latest'", "default": "@latest" },
                        "limit": { "type": "integer", "default": 50 },
                        "min_size": { "type": "integer", "description": "Min bytes", "default": 0 }
                    }
                }
            },
            {
                "name": "disky_dirs",
                "description": "Top directories by aggregated size.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "snapshot": { "type": "string", "default": "@latest" },
                        "limit": { "type": "integer", "default": 30 }
                    }
                }
            },
            {
                "name": "disky_ext",
                "description": "Disk usage grouped by file extension.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "snapshot": { "type": "string", "default": "@latest" },
                        "limit": { "type": "integer", "default": 30 }
                    }
                }
            },
            {
                "name": "disky_find",
                "description": "Find files matching a glob pattern (e.g. '*.log').",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string" },
                        "snapshot": { "type": "string", "default": "@latest" },
                        "limit": { "type": "integer", "default": 50 }
                    },
                    "required": ["pattern"]
                }
            },
            {
                "name": "disky_stats",
                "description": "Overall stats for a snapshot (file/dir count, total/largest/avg bytes).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "snapshot": { "type": "string", "default": "@latest" }
                    }
                }
            },
            {
                "name": "disky_query",
                "description": "Run an arbitrary SQL query against a snapshot. The `files` table has columns: path, name, ext, size, mtime, is_dir, depth.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "sql": { "type": "string" },
                        "snapshot": { "type": "string", "default": "@latest" },
                        "limit": { "type": "integer", "default": 1000 }
                    },
                    "required": ["sql"]
                }
            },
            {
                "name": "disky_diff",
                "description": "Diff two snapshots — added / removed / grew / shrank files, ordered by absolute delta.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "string", "description": "Snapshot A — @latest, ID, or path" },
                        "b": { "type": "string", "description": "Snapshot B — @latest, ID, or path" },
                        "limit": { "type": "integer", "default": 100 }
                    },
                    "required": ["a", "b"]
                }
            },
            {
                "name": "disky_cleanup",
                "description": "Find well-known disk-hoggy directories (node_modules, target, …). Dry-run unless apply=true. CAUTION: with apply=true this is DESTRUCTIVE — gate behind user confirmation.",
                "annotations": {
                    "title": "Clean disk-hoggy directories",
                    "readOnlyHint": false,
                    "destructiveHint": true,
                    "idempotentHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target": { "type": "array", "items": { "type": "string" }, "description": "Categories (default: all)" },
                        "snapshot": { "type": "string", "default": "@latest" },
                        "limit": { "type": "integer", "default": 100 },
                        "apply": { "type": "boolean", "default": false, "description": "Actually delete the listed paths" },
                        "reversible": { "type": "boolean", "default": false, "description": "With apply=true, move paths to ~/.Trash instead of permanent delete" }
                    }
                }
            },
            {
                "name": "disky_schema",
                "description": "Emit a JSON descriptor of every disky command, record shape, and error type.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "disky_list_snapshots",
                "description": "List available DuckDB snapshots in ~/Library/Application Support/disky/.",
                "annotations": {
                    "title": "List snapshots",
                    "readOnlyHint": true,
                    "idempotentHint": true
                },
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "disky_discover",
                "description": "Self-describe: return the full schema descriptor plus runtime context (cwd, active snapshot, data dir, version). One call replaces what used to take schema + list_snapshots + version.",
                "annotations": {
                    "title": "Discover disky",
                    "readOnlyHint": true,
                    "idempotentHint": true
                },
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "disky_growth",
                "description": "Per-directory growth between two snapshots — kind/delta_bytes/rate_bytes_per_day per parent dir. Default compares @latest vs @latest~1. Pass `over` (e.g. '7d') to auto-pick the oldest snapshot within a window.",
                "annotations": {
                    "title": "Growth between snapshots",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "destructiveHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "since": { "type": "string", "default": "@latest~1" },
                        "until": { "type": "string", "default": "@latest" },
                        "over":  { "type": "string", "description": "Duration like 7d / 2w / 6mo / 1y — overrides `since`" },
                        "limit": { "type": "integer", "default": 50 }
                    }
                }
            },
            {
                "name": "disky_churn",
                "description": "Per-directory mtime-based churn. Files modified within the last N hours/days, sum bytes, churn_score = recent_bytes / total_bytes. Identifies log generators and hot working directories.",
                "annotations": {
                    "title": "Directory churn",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "destructiveHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "over":     { "type": "string", "default": "24h", "description": "Duration like 24h / 7d / 30d" },
                        "snapshot": { "type": "string", "default": "@latest" },
                        "limit":    { "type": "integer", "default": 50 }
                    }
                }
            },
            {
                "name": "disky_predict",
                "description": "Linear extrapolation of disk fill-by date. Reads every snapshot in the data dir, fits an OLS line through (timestamp, total_bytes), projects when usage crosses the free-space ceiling. Pass `free_bytes` to compute the date.",
                "annotations": {
                    "title": "Predict disk-fill date",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "destructiveHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "free_bytes": { "type": "integer", "description": "Bytes currently free on the volume" },
                        "physical":   { "type": "boolean", "default": false }
                    }
                }
            },
            {
                "name": "disky_empty",
                "description": "List files with size = 0. Placeholders, leftover lockfiles, interrupted writes.",
                "annotations": {
                    "title": "Empty files",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "destructiveHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "snapshot": { "type": "string", "default": "@latest" },
                        "limit":    { "type": "integer", "default": 100 }
                    }
                }
            },
            {
                "name": "disky_old",
                "description": "List files older than DURATION (e.g. 30d, 6mo, 1y). Excludes mtime=epoch noise from cargo/npm packs.",
                "annotations": {
                    "title": "Old files",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "destructiveHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "older_than": { "type": "string", "description": "Duration like 30d / 6mo / 1y" },
                        "snapshot":   { "type": "string", "default": "@latest" },
                        "limit":      { "type": "integer", "default": 100 }
                    },
                    "required": ["older_than"]
                }
            },
            {
                "name": "disky_filter",
                "description": "Filter records from a prior disky envelope by predicate. Accepts an envelope JSON object directly (no stdin) plus a `where` clause. Composable with any record-emitting tool.",
                "annotations": {
                    "title": "Filter envelope records",
                    "readOnlyHint": true,
                    "idempotentHint": true,
                    "destructiveHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "envelope": { "type": "object", "description": "A prior disky JSON envelope (must have schema_version, kind, records)" },
                        "where":    { "type": "string", "description": "Predicate like \"size > 1GB AND ext = 'log'\"" },
                        "limit":    { "type": "integer", "default": 1000 }
                    },
                    "required": ["envelope"]
                }
            },
            {
                "name": "disky_forget",
                "description": "Apply restic-style retention policy to snapshots. Default dry-run; pass apply=true to delete. Refuses with usage error if no keep_* flag set. DESTRUCTIVE with apply=true — gate behind user confirmation.",
                "annotations": {
                    "title": "Forget snapshots (retention policy)",
                    "readOnlyHint": false,
                    "destructiveHint": true,
                    "idempotentHint": false
                },
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "keep_last": { "type": "integer" },
                        "keep_daily": { "type": "integer" },
                        "keep_weekly": { "type": "integer" },
                        "keep_monthly": { "type": "integer" },
                        "keep_yearly": { "type": "integer" },
                        "apply": { "type": "boolean", "default": false }
                    }
                }
            }
        ]
    })
}

fn handle_tool_call(id: Value, params: Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result: anyhow::Result<Value> = match name {
        "disky_scan" => tool_scan(&args),
        "disky_top" => tool_top(&args),
        "disky_dirs" => tool_dirs(&args),
        "disky_ext" => tool_ext(&args),
        "disky_find" => tool_find(&args),
        "disky_stats" => tool_stats(&args),
        "disky_query" => tool_query(&args),
        "disky_diff" => tool_diff(&args),
        "disky_cleanup" => tool_cleanup(&args),
        "disky_schema" => Ok(schema::document()),
        "disky_list_snapshots" => tool_list_snapshots(),
        "disky_discover" => tool_discover(),
        "disky_growth" => tool_growth(&args),
        "disky_churn" => tool_churn(&args),
        "disky_predict" => tool_predict(&args),
        "disky_empty" => tool_empty(&args),
        "disky_old" => tool_old(&args),
        "disky_filter" => tool_filter(&args),
        "disky_forget" => tool_forget(&args),
        other => {
            return tool_error_result(
                id,
                &DiskyError::new(
                    ExitCode::Usage,
                    "unknown tool",
                    format!("no such tool: {}", other),
                ),
            );
        }
    };

    match result {
        Ok(payload) => tool_ok_result(id, payload),
        Err(e) => tool_error_result(id, &classify(e)),
    }
}

fn resolve_snapshot(args: &Value) -> anyhow::Result<String> {
    let snap = args
        .get("snapshot")
        .and_then(Value::as_str)
        .unwrap_or("@latest");
    snapshots::resolve(snap)
}

fn tool_scan(args: &Value) -> anyhow::Result<Value> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("/")
        .to_string();
    let db_path = match args.get("db").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => snapshots::new_snapshot_path()?,
    };
    let outcome = scan::run(&path, &db_path)?;
    let conn = db::open(&db_path)?;
    let stats = query::stats(&conn)?;

    let mut bundle = serde_json::Map::new();
    bundle.insert("schema_version".into(), json!(SCHEMA_VERSION));
    bundle.insert("kind".into(), json!("scan_bundle"));
    bundle.insert("snapshot".into(), json!(db_path));
    bundle.insert("complete".into(), json!(outcome.complete));
    bundle.insert("stats".into(), json!(stats));
    if let Some(n) = args.get("emit_top").and_then(Value::as_u64) {
        bundle.insert("top".into(), json!(query::top_files(&conn, n as usize, 0)?));
    }
    if let Some(n) = args.get("emit_dirs").and_then(Value::as_u64) {
        bundle.insert("dirs".into(), json!(query::top_dirs(&conn, n as usize)?));
    }
    if let Some(n) = args.get("emit_ext").and_then(Value::as_u64) {
        bundle.insert("ext".into(), json!(query::by_extension(&conn, n as usize)?));
    }
    Ok(Value::Object(bundle))
}

fn envelope(kind: &str, records: Value) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "kind": kind,
        "records": records,
    })
}

fn tool_top(args: &Value) -> anyhow::Result<Value> {
    let db_path = resolve_snapshot(args)?;
    let conn = db::open(&db_path)?;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
    let min_size = args.get("min_size").and_then(Value::as_u64).unwrap_or(0);
    let rows = query::top_files(&conn, limit, min_size)?;
    Ok(envelope("top", json!(rows)))
}

fn tool_dirs(args: &Value) -> anyhow::Result<Value> {
    let db_path = resolve_snapshot(args)?;
    let conn = db::open(&db_path)?;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(30) as usize;
    let rows = query::top_dirs(&conn, limit)?;
    Ok(envelope("dirs", json!(rows)))
}

fn tool_ext(args: &Value) -> anyhow::Result<Value> {
    let db_path = resolve_snapshot(args)?;
    let conn = db::open(&db_path)?;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(30) as usize;
    let rows = query::by_extension(&conn, limit)?;
    Ok(envelope("ext", json!(rows)))
}

fn tool_find(args: &Value) -> anyhow::Result<Value> {
    let pattern = args
        .get("pattern")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required field 'pattern'"))?
        .to_string();
    let db_path = resolve_snapshot(args)?;
    let conn = db::open(&db_path)?;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
    let rows = query::find_files(&conn, &pattern, limit)?;
    Ok(envelope("find", json!(rows)))
}

fn tool_stats(args: &Value) -> anyhow::Result<Value> {
    let db_path = resolve_snapshot(args)?;
    let conn = db::open(&db_path)?;
    let s = query::stats(&conn)?;
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "stats",
        "record": s,
    }))
}

fn tool_query(args: &Value) -> anyhow::Result<Value> {
    let sql = args
        .get("sql")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required field 'sql'"))?
        .to_string();
    let db_path = resolve_snapshot(args)?;
    let conn = db::open(&db_path)?;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(1000) as usize;
    let rows = query::raw_query(&conn, &sql, limit)?;
    Ok(envelope("query", json!(rows)))
}

fn tool_diff(args: &Value) -> anyhow::Result<Value> {
    let a = args
        .get("a")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required field 'a'"))?;
    let b = args
        .get("b")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required field 'b'"))?;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize;
    let path_a = snapshots::resolve(a)?;
    let path_b = snapshots::resolve(b)?;
    let rows = query::diff(&path_a, &path_b, limit)?;
    Ok(envelope("diff", json!(rows)))
}

fn tool_cleanup(args: &Value) -> anyhow::Result<Value> {
    let db_path = resolve_snapshot(args)?;
    let conn = db::open(&db_path)?;
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(100) as usize;
    let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
    let reversible = args
        .get("reversible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let targets: Vec<String> = args
        .get("target")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| {
            cleanup::default_target_names()
                .into_iter()
                .map(String::from)
                .collect()
        });

    let hits = cleanup::scan(&conn, &targets, limit)?;
    let removed = if apply {
        let mode = if reversible {
            cleanup::ApplyMode::Trash
        } else {
            cleanup::ApplyMode::Delete
        };
        Some(cleanup::apply(&hits, mode)?)
    } else {
        None
    };
    let summary = cleanup::summarise(&hits);
    let total_bytes: u64 = summary.iter().map(|s| s.bytes).sum();
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "cleanup",
        "applied": apply,
        "removed": removed.unwrap_or_default(),
        "records": hits,
        "summary": summary,
        "total_bytes": total_bytes,
    }))
}

fn tool_list_snapshots() -> anyhow::Result<Value> {
    let snaps = snapshots::list_snapshots();
    let records: Vec<_> = snaps
        .iter()
        .map(|(path, size)| {
            json!({
                "path": path,
                "id": snapshots::id_for(path),
                "bytes": size,
            })
        })
        .collect();
    Ok(envelope("snapshots", json!(records)))
}

/// One-shot context for an agent connecting to disky-mcp: schema + cwd +
/// active snapshot + data dir + tool version. Cuts the typical
/// "schema then list_snapshots then version" sequence to a single call.
fn tool_discover() -> anyhow::Result<Value> {
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "discover",
        "tool": "disky",
        "version": env!("CARGO_PKG_VERSION"),
        "cwd": std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()),
        "data_dir": snapshots::snapshot_dir().to_string_lossy(),
        "active_snapshot": snapshots::latest_snapshot(),
        "schema": schema::document(),
    }))
}

fn tool_growth(args: &Value) -> anyhow::Result<Value> {
    let since_arg = args
        .get("since")
        .and_then(Value::as_str)
        .unwrap_or("@latest~1");
    let until_arg = args
        .get("until")
        .and_then(Value::as_str)
        .unwrap_or("@latest");
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(50);
    let over = args.get("over").and_then(Value::as_str);

    let since_path = if let Some(dur) = over {
        let secs = disky::duration::parse_seconds(dur)
            .map_err(|e| DiskyError::new(ExitCode::Usage, "invalid over", e.to_string()))?;
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
            - secs;
        let snaps = snapshots::list_snapshots();
        let chosen = snaps
            .iter()
            .filter_map(|(p, _)| {
                let id = snapshots::id_for(p)?;
                let dt = snapshots::parse_id(&id)?;
                Some((p.clone(), dt.timestamp()))
            })
            .filter(|(_, ts)| *ts <= cutoff)
            .max_by_key(|(_, ts)| *ts);
        match chosen {
            Some((p, _)) => p,
            None => {
                return Err(DiskyError::new(
                    ExitCode::NotFound,
                    "no snapshot in window",
                    format!("no snapshot older than {} found", dur),
                )
                .into());
            }
        }
    } else {
        snapshots::resolve(since_arg)?
    };
    let until_path = snapshots::resolve(until_arg)?;
    let rows = disky::query::growth(&since_path, &until_path, limit)?;
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "growth",
        "since": since_arg,
        "until": until_arg,
        "records": rows,
    }))
}

fn tool_churn(args: &Value) -> anyhow::Result<Value> {
    let over = args.get("over").and_then(Value::as_str).unwrap_or("24h");
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(50);
    let secs = disky::duration::parse_seconds(over)
        .map_err(|e| DiskyError::new(ExitCode::Usage, "invalid over", e.to_string()))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cutoff = now - secs;
    let path = resolve_snapshot(args)?;
    let conn = db::open(&path)?;
    let rows = disky::query::churn(&conn, cutoff, limit)?;
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "churn",
        "over": over,
        "cutoff_unix": cutoff,
        "records": rows,
    }))
}

fn tool_predict(args: &Value) -> anyhow::Result<Value> {
    let free_bytes = args.get("free_bytes").and_then(Value::as_u64);
    let physical = args
        .get("physical")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rec = disky::predict::predict(physical, free_bytes)?;
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "predict",
        "record": rec,
    }))
}

fn tool_empty(args: &Value) -> anyhow::Result<Value> {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(100);
    let path = resolve_snapshot(args)?;
    let conn = db::open(&path)?;
    let rows = disky::query::empty_files(&conn, limit)?;
    Ok(envelope("empty", json!(rows)))
}

fn tool_old(args: &Value) -> anyhow::Result<Value> {
    let dur = args
        .get("older_than")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            DiskyError::new(
                ExitCode::Usage,
                "missing older_than",
                "older_than is required (e.g. '30d')",
            )
        })?;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(100);
    let secs = disky::duration::parse_seconds(dur)
        .map_err(|e| DiskyError::new(ExitCode::Usage, "invalid older_than", e.to_string()))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let cutoff = now - secs;
    let path = resolve_snapshot(args)?;
    let conn = db::open(&path)?;
    let rows = disky::query::old_files(&conn, cutoff, limit)?;
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "old",
        "cutoff_unix": cutoff,
        "older_than": dur,
        "records": rows,
    }))
}

fn tool_filter(args: &Value) -> anyhow::Result<Value> {
    let env_value = args.get("envelope").cloned().ok_or_else(|| {
        DiskyError::new(
            ExitCode::Usage,
            "missing envelope",
            "envelope (object) is required",
        )
    })?;
    let env = disky::envelope::parse_value(env_value)?;
    disky::envelope::require_kind(
        &env,
        &[
            "top", "find", "dirs", "ext", "empty", "old", "filter", "growth",
        ],
    )?;
    let where_clause = args.get("where").and_then(Value::as_str).unwrap_or("");
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(1000);
    let pred = disky::filter::Predicate::parse(where_clause)?;
    let kept: Vec<serde_json::Value> = env
        .records
        .into_iter()
        .filter(|r| pred.matches(r))
        .take(limit)
        .collect();
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "filter",
        "input_kind": env.kind,
        "records": kept,
    }))
}

fn tool_forget(args: &Value) -> anyhow::Result<Value> {
    let policy = Policy {
        keep_last: args
            .get("keep_last")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        keep_daily: args
            .get("keep_daily")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        keep_weekly: args
            .get("keep_weekly")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        keep_monthly: args
            .get("keep_monthly")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
        keep_yearly: args
            .get("keep_yearly")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
    };
    if policy.is_empty() {
        return Err(DiskyError::new(
            ExitCode::Usage,
            "no retention policy",
            "pass at least one keep_last / keep_daily / keep_weekly / keep_monthly / keep_yearly",
        )
        .into());
    }
    let apply = args.get("apply").and_then(Value::as_bool).unwrap_or(false);
    let snaps: Vec<SnapshotMeta> = snapshots::list_snapshots()
        .into_iter()
        .filter_map(|(path, bytes)| {
            let id = snapshots::id_for(&path)?;
            let created = snapshots::parse_id(&id).map(|d| d.to_rfc3339());
            Some(SnapshotMeta {
                id,
                path,
                bytes,
                created,
            })
        })
        .collect();
    let plan = apply_policy(&snaps, &policy);
    if apply {
        for s in &plan.removed {
            std::fs::remove_file(&s.path)
                .map_err(|e| DiskyError::io(format!("failed to remove {}: {}", s.path, e)))?;
        }
    }
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "forget",
        "applied": apply,
        "kept": plan.kept,
        "removed": plan.removed,
        "skipped_unparseable": plan.skipped_unparseable,
        "total_removed_bytes": plan.total_removed_bytes,
    }))
}
