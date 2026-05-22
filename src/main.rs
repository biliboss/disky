mod cli;
mod tui;

use clap::Parser;
use cli::{Cli, Command};
use disky::config::Config;
use disky::exit::{classify, DiskyError, ExitCode};
use disky::query::SCHEMA_VERSION;
use disky::render::{resolve_format, Format};
use disky::{cleanup, db, query, render, scan, schema, snapshots};
use serde_json::json;
use std::process::ExitCode as ProcExit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() -> ProcExit {
    let cli = Cli::parse();
    // Layered config: file → env → CLI. Malformed file fails fast so the
    // user sees the parse error rather than silently falling back.
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {:#}", e);
            return ProcExit::from(ExitCode::Usage as u8);
        }
    };
    // CLI flag wins; config supplies the default when CLI is silent.
    let cli_format = cli.format.map(Into::into).or_else(|| {
        config
            .format()
            .and_then(|s| match s.to_ascii_lowercase().as_str() {
                "text" => Some(Format::Text),
                "json" => Some(Format::Json),
                "ndjson" => Some(Format::Ndjson),
                _ => None,
            })
    });
    let format = resolve_format(cli_format);

    match dispatch(cli, format) {
        Ok(()) => ProcExit::from(ExitCode::Ok as u8),
        Err(err) => {
            let derr = classify(err);
            emit_error(&derr, format);
            ProcExit::from(derr.code as u8)
        }
    }
}

fn emit_error(err: &DiskyError, format: Format) {
    if format.is_machine() {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "type": format!("https://disky.dev/errors/{}", err.code.slug()),
            "title": err.title,
            "status": err.code as i32,
            "detail": err.detail,
            "retryable": err.retryable,
            "instance": err.instance,
        });
        eprintln!("{}", payload);
    } else {
        eprintln!("error: {}", err);
    }
}

fn open_snapshot(spec: &str) -> anyhow::Result<duckdb::Connection> {
    let path = snapshots::resolve(spec)?;
    db::open(&path)
}

fn dispatch(cli: Cli, format: Format) -> anyhow::Result<()> {
    match cli.command.unwrap_or(Command::Tui { snapshot: None }) {
        Command::Scan {
            path,
            db,
            emit_top,
            emit_dirs,
            emit_ext,
            emit_stats,
        } => {
            let db_path = match db {
                Some(p) => p,
                None => snapshots::new_snapshot_path()?,
            };
            let cancel = Arc::new(AtomicBool::new(false));
            let cancel_handler = Arc::clone(&cancel);
            let _ = ctrlc::set_handler(move || {
                cancel_handler.store(true, Ordering::Relaxed);
            });
            let outcome = scan::run_cancellable(&path, &db_path, cancel)?;

            let want_bundle =
                emit_top.is_some() || emit_dirs.is_some() || emit_ext.is_some() || emit_stats;
            if want_bundle {
                let conn = db::open(&db_path)?;
                let mut bundle = serde_json::Map::new();
                bundle.insert("schema_version".into(), json!(disky::query::SCHEMA_VERSION));
                bundle.insert("kind".into(), json!("scan_bundle"));
                bundle.insert("snapshot".into(), json!(db_path));
                bundle.insert("complete".into(), json!(outcome.complete));
                bundle.insert("stats".into(), json!(query::stats(&conn)?));
                if let Some(n) = emit_top {
                    bundle.insert("top".into(), json!(query::top_files(&conn, n, 0)?));
                }
                if let Some(n) = emit_dirs {
                    bundle.insert("dirs".into(), json!(query::top_dirs(&conn, n)?));
                }
                if let Some(n) = emit_ext {
                    bundle.insert("ext".into(), json!(query::by_extension(&conn, n)?));
                }
                println!("{}", serde_json::to_string(&bundle)?);
            }

            if !outcome.complete {
                return Err(disky::exit::DiskyError::new(
                    ExitCode::PartialScan,
                    "scan cancelled",
                    format!(
                        "interrupted at {} entries ({} bytes); snapshot left partial at {}",
                        outcome.entries, outcome.bytes, db_path
                    ),
                )
                .into());
            }
        }
        Command::Top {
            snapshot,
            limit,
            min_size,
        } => {
            let conn = open_snapshot(&snapshot)?;
            let rows = query::top_files(&conn, limit, min_size)?;
            render::top_files(&rows, format)?;
        }
        Command::Ext { snapshot, limit } => {
            let conn = open_snapshot(&snapshot)?;
            let rows = query::by_extension(&conn, limit)?;
            render::by_extension(&rows, format)?;
        }
        Command::Dirs { snapshot, limit } => {
            let conn = open_snapshot(&snapshot)?;
            let rows = query::top_dirs(&conn, limit)?;
            render::top_dirs(&rows, format)?;
        }
        Command::Find {
            snapshot,
            pattern,
            limit,
        } => {
            let conn = open_snapshot(&snapshot)?;
            let rows = query::find_files(&conn, &pattern, limit)?;
            render::find_files(&rows, &pattern, format)?;
        }
        Command::Stats {
            snapshot,
            summarize,
            raw,
        } => {
            let conn = open_snapshot(&snapshot)?;
            let s = query::stats(&conn)?;
            if raw {
                println!("{}", s.total_bytes);
            } else if summarize {
                render::stats_scalar(&s)?;
            } else {
                render::stats(&s, format)?;
            }
        }
        Command::Query {
            sql,
            snapshot,
            limit,
        } => {
            let conn = open_snapshot(&snapshot)?;
            let rows = query::raw_query(&conn, &sql, limit)?;
            render::raw_query(&rows, format)?;
        }
        Command::Cleanup {
            target,
            snapshot,
            limit,
            apply,
            reversible,
        } => {
            let conn = open_snapshot(&snapshot)?;
            let targets: Vec<String> = if target.is_empty() {
                cleanup::default_target_names()
                    .into_iter()
                    .map(String::from)
                    .collect()
            } else {
                target
            };
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
            render::cleanup(&hits, removed.as_deref(), format)?;
        }
        Command::Diff { a, b, limit } => {
            let path_a = snapshots::resolve(&a)?;
            let path_b = snapshots::resolve(&b)?;
            let rows = query::diff(&path_a, &path_b, limit)?;
            render::diff(&rows, format)?;
        }
        Command::Schema => {
            println!("{}", serde_json::to_string_pretty(&schema::document())?);
        }
        Command::Tui { snapshot } => {
            tui::run(snapshot)?;
        }
        Command::List => {
            let snaps = snapshots::list_snapshots();
            if format.is_machine() {
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
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "kind": "snapshots",
                    "records": records,
                });
                println!("{}", payload);
            } else if snaps.is_empty() {
                println!("No snapshots found. Run `disky scan` first.");
            } else {
                for (path, size) in snaps {
                    let id = snapshots::id_for(&path).unwrap_or_default();
                    println!(
                        "{:24} {:>10} {}",
                        id,
                        humansize::format_size(size, humansize::BINARY),
                        path,
                    );
                }
            }
        }
    }

    Ok(())
}
