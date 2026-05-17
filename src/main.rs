mod cli;
mod tui;

use clap::Parser;
use cli::{Cli, Command};
use disky::exit::{classify, DiskyError, ExitCode};
use disky::query::SCHEMA_VERSION;
use disky::render::{resolve_format, Format};
use disky::{db, query, render, scan, snapshots};
use serde_json::json;
use std::process::ExitCode as ProcExit;

fn main() -> ProcExit {
    let cli = Cli::parse();
    let format = resolve_format(cli.format.map(Into::into));

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
        });
        eprintln!("{}", payload);
    } else {
        eprintln!("error: {}", err);
    }
}

fn dispatch(cli: Cli, format: Format) -> anyhow::Result<()> {
    match cli.command.unwrap_or(Command::Tui { db: None }) {
        Command::Scan { path, db } => {
            let db_path = if db == "disky.db" {
                snapshots::new_snapshot_path()?
            } else {
                db
            };
            scan::run(&path, &db_path)?;
        }
        Command::Top {
            db,
            limit,
            min_size,
        } => {
            let conn = db::open(&db)?;
            let rows = query::top_files(&conn, limit, min_size)?;
            render::top_files(&rows, format)?;
        }
        Command::Ext { db, limit } => {
            let conn = db::open(&db)?;
            let rows = query::by_extension(&conn, limit)?;
            render::by_extension(&rows, format)?;
        }
        Command::Dirs { db, limit } => {
            let conn = db::open(&db)?;
            let rows = query::top_dirs(&conn, limit)?;
            render::top_dirs(&rows, format)?;
        }
        Command::Find { db, pattern, limit } => {
            let conn = db::open(&db)?;
            let rows = query::find_files(&conn, &pattern, limit)?;
            render::find_files(&rows, &pattern, format)?;
        }
        Command::Stats { db } => {
            let conn = db::open(&db)?;
            let s = query::stats(&conn)?;
            render::stats(&s, format)?;
        }
        Command::Tui { db } => {
            tui::run(db)?;
        }
        Command::List => {
            let snaps = snapshots::list_snapshots();
            if format.is_machine() {
                let records: Vec<_> = snaps
                    .iter()
                    .map(|(path, size)| json!({"path": path, "bytes": size}))
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
                    println!(
                        "{:60} {:>10}",
                        path,
                        humansize::format_size(size, humansize::BINARY)
                    );
                }
            }
        }
    }

    Ok(())
}
