mod cli;
mod tui;

use clap::Parser;
use cli::{Cli, Command};
use disky::config::Config;
use disky::exit::{classify, DiskyError, ExitCode};
use disky::policy::{apply_policy, Policy, SnapshotMeta};
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
    let physical = cli.physical;
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
            let rows = if physical {
                query::top_files_physical(&conn, limit, min_size)?
            } else {
                query::top_files(&conn, limit, min_size)?
            };
            render::top_files(&rows, format)?;
        }
        Command::Ext { snapshot, limit } => {
            let conn = open_snapshot(&snapshot)?;
            let rows = if physical {
                query::by_extension_physical(&conn, limit)?
            } else {
                query::by_extension(&conn, limit)?
            };
            render::by_extension(&rows, format)?;
        }
        Command::Dirs { snapshot, limit } => {
            let conn = open_snapshot(&snapshot)?;
            let rows = if physical {
                query::top_dirs_physical(&conn, limit)?
            } else {
                query::top_dirs(&conn, limit)?
            };
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
            let s = if physical {
                query::stats_physical(&conn)?
            } else {
                query::stats(&conn)?
            };
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
        Command::Filter { where_, limit } => {
            // Read prior envelope from stdin, filter, re-emit with kind="filter".
            let env = disky::envelope::parse_json(std::io::stdin().lock())?;
            disky::envelope::require_kind(
                &env,
                &[
                    "top", "find", "dirs", "ext", "empty", "old", "filter", "growth",
                ],
            )?;
            let pred = disky::filter::Predicate::parse(where_.as_deref().unwrap_or(""))?;
            let kept: Vec<serde_json::Value> = env
                .records
                .into_iter()
                .filter(|r| pred.matches(r))
                .take(limit)
                .collect();
            if format.is_machine() {
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "kind": "filter",
                    "input_kind": env.kind,
                    "records": kept,
                });
                println!("{}", payload);
            } else {
                for r in &kept {
                    println!("{}", r);
                }
            }
        }
        Command::Growth {
            since,
            until,
            over,
            limit,
        } => {
            // --over wins over --since. Pick oldest snapshot whose age >= window.
            let resolved_since = if let Some(ref dur) = over {
                let secs = disky::duration::parse_seconds(dur).map_err(|e| {
                    DiskyError::new(ExitCode::Usage, "invalid --over", e.to_string())
                })?;
                let cutoff = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0)
                    - secs;
                let snaps = snapshots::list_snapshots();
                let oldest = snaps
                    .iter()
                    .filter_map(|(p, _)| {
                        let id = snapshots::id_for(p)?;
                        let dt = snapshots::parse_id(&id)?;
                        Some((p.clone(), dt.timestamp()))
                    })
                    .filter(|(_, ts)| *ts <= cutoff)
                    .max_by_key(|(_, ts)| *ts);
                match oldest {
                    Some((p, _)) => p,
                    None => {
                        return Err(DiskyError::new(
                            ExitCode::NotFound,
                            "no snapshot in window",
                            format!(
                                "no snapshot older than {} found (need scan history covering the window)",
                                dur
                            ),
                        )
                        .into())
                    }
                }
            } else {
                snapshots::resolve(&since)?
            };
            let path_b = snapshots::resolve(&until)?;
            let rows = query::growth(&resolved_since, &path_b, limit)?;
            let since_label = over.as_deref().unwrap_or(since.as_str());
            let since = since_label.to_string();
            if format.is_machine() {
                let payload = serde_json::json!({
                    "schema_version": SCHEMA_VERSION,
                    "kind": "growth",
                    "since": since,
                    "until": until,
                    "records": rows,
                });
                println!("{}", payload);
            } else if rows.is_empty() {
                println!("No growth detected between {} and {}.", since, until);
            } else {
                println!(
                    "{:<70} {:>14} {:>14} {:>10}",
                    "PATH", "Δ BYTES", "RATE B/DAY", "KIND"
                );
                println!("{}", "-".repeat(112));
                for r in &rows {
                    println!(
                        "{:<70} {:>14} {:>14.0} {:>10}",
                        if r.path.len() > 70 {
                            format!("...{}", &r.path[r.path.len() - 67..])
                        } else {
                            r.path.clone()
                        },
                        r.delta_bytes,
                        r.rate_bytes_per_day,
                        r.kind
                    );
                }
            }
        }
        Command::Churn {
            over,
            snapshot,
            limit,
        } => {
            let secs = disky::duration::parse_seconds(&over)
                .map_err(|e| DiskyError::new(ExitCode::Usage, "invalid --over", e.to_string()))?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let cutoff = now - secs;
            let conn = open_snapshot(&snapshot)?;
            let rows = query::churn(&conn, cutoff, limit)?;
            if format.is_machine() {
                let payload = json!({
                    "schema_version": SCHEMA_VERSION,
                    "kind": "churn",
                    "over": over,
                    "cutoff_unix": cutoff,
                    "records": rows,
                });
                println!("{}", payload);
            } else if rows.is_empty() {
                println!("No churn detected within {}.", over);
            } else {
                println!(
                    "{:<60} {:>8} {:>14} {:>10} {:>8}",
                    "PATH", "FILES", "BYTES", "OF TOTAL", "SCORE"
                );
                println!("{}", "-".repeat(108));
                for r in &rows {
                    let truncated = if r.path.len() > 60 {
                        format!("...{}", &r.path[r.path.len() - 57..])
                    } else {
                        r.path.clone()
                    };
                    println!(
                        "{:<60} {:>8} {:>14} {:>9.1}% {:>8.3}",
                        truncated,
                        r.recent_files,
                        r.recent_bytes,
                        r.churn_score * 100.0,
                        r.churn_score
                    );
                }
            }
        }
        Command::Empty { snapshot, limit } => {
            let conn = open_snapshot(&snapshot)?;
            let rows = query::empty_files(&conn, limit)?;
            // Reuse FileRow renderer; `empty` envelope kind for agents.
            if format.is_machine() {
                let payload = serde_json::json!({
                    "schema_version": SCHEMA_VERSION,
                    "kind": "empty",
                    "records": rows,
                });
                println!("{}", payload);
            } else if rows.is_empty() {
                println!("No empty files found.");
            } else {
                for r in &rows {
                    println!("{}", r.path);
                }
            }
        }
        Command::Old {
            older_than,
            snapshot,
            limit,
        } => {
            let secs = disky::duration::parse_seconds(&older_than).map_err(|e| {
                DiskyError::new(ExitCode::Usage, "invalid --older-than", e.to_string())
            })?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let cutoff = now - secs;
            let conn = open_snapshot(&snapshot)?;
            let rows = query::old_files(&conn, cutoff, limit)?;
            if format.is_machine() {
                let payload = serde_json::json!({
                    "schema_version": SCHEMA_VERSION,
                    "kind": "old",
                    "cutoff_unix": cutoff,
                    "older_than": older_than,
                    "records": rows,
                });
                println!("{}", payload);
            } else if rows.is_empty() {
                println!("No files older than {}.", older_than);
            } else {
                for r in &rows {
                    println!(
                        "{:<70} {}",
                        r.path,
                        r.mtime.as_deref().unwrap_or("(no mtime)")
                    );
                }
            }
        }
        Command::Forget {
            keep_last,
            keep_daily,
            keep_weekly,
            keep_monthly,
            keep_yearly,
            apply,
        } => {
            let p = Policy {
                keep_last,
                keep_daily,
                keep_weekly,
                keep_monthly,
                keep_yearly,
            };
            if p.is_empty() {
                return Err(DiskyError::new(
                    ExitCode::Usage,
                    "no retention policy",
                    "pass at least one --keep-last / --keep-daily / --keep-weekly / --keep-monthly / --keep-yearly",
                )
                .into());
            }
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
            let plan = apply_policy(&snaps, &p);
            if apply {
                for s in &plan.removed {
                    std::fs::remove_file(&s.path).map_err(|e| {
                        DiskyError::io(format!("failed to remove {}: {}", s.path, e))
                    })?;
                }
            }
            let payload = json!({
                "schema_version": SCHEMA_VERSION,
                "kind": "forget",
                "applied": apply,
                "kept": plan.kept,
                "removed": plan.removed,
                "skipped_unparseable": plan.skipped_unparseable,
                "total_removed_bytes": plan.total_removed_bytes,
            });
            if format.is_machine() {
                println!("{}", payload);
            } else {
                println!(
                    "{} dry-run · kept {} · would-remove {} · {} bytes",
                    if apply { "applied" } else { "DRY-RUN" },
                    plan.kept.len(),
                    plan.removed.len(),
                    plan.total_removed_bytes
                );
            }
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
