//! Minimal MCP server over stdio for disky.
//!
//! Implements just enough of the Model Context Protocol (JSON-RPC 2.0,
//! newline-delimited) to expose disky's query layer as typed tool calls:
//! `initialize`, `tools/list`, `tools/call`. No external MCP SDK dependency.

use disky::exit::{classify, DiskyError, ExitCode};
use disky::query::{self, SCHEMA_VERSION};
use disky::{db, scan, snapshots};
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
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory to scan", "default": "/" },
                        "db": { "type": "string", "description": "Snapshot DB path (default: auto-named in ~/Library/Application Support/disky/)" }
                    }
                }
            },
            {
                "name": "disky_top",
                "description": "Largest files in a snapshot. Bytes are u64, paths absolute, mtime RFC 3339 UTC.",
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
                "name": "disky_list_snapshots",
                "description": "List available DuckDB snapshots in ~/Library/Application Support/disky/.",
                "inputSchema": { "type": "object", "properties": {} }
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
        "disky_list_snapshots" => tool_list_snapshots(),
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
    if snap == "@latest" {
        snapshots::latest_snapshot()
            .ok_or_else(|| anyhow::anyhow!("no snapshot found; run disky_scan first (not found)"))
    } else {
        Ok(snap.to_string())
    }
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
    scan::run(&path, &db_path)?;
    let conn = db::open(&db_path)?;
    let stats = query::stats(&conn)?;
    Ok(json!({
        "schema_version": SCHEMA_VERSION,
        "kind": "scan_result",
        "snapshot": db_path,
        "stats": stats,
    }))
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

fn tool_list_snapshots() -> anyhow::Result<Value> {
    let snaps = snapshots::list_snapshots();
    let records: Vec<_> = snaps
        .iter()
        .map(|(path, size)| json!({ "path": path, "bytes": size }))
        .collect();
    Ok(envelope("snapshots", json!(records)))
}
