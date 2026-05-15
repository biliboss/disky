mod cli;
mod db;
mod display;
mod scan;
mod snapshots;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();

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
            display::top_files(&conn, limit, min_size)?;
        }
        Command::Ext { db, limit } => {
            let conn = db::open(&db)?;
            display::by_extension(&conn, limit)?;
        }
        Command::Dirs { db, limit } => {
            let conn = db::open(&db)?;
            display::top_dirs(&conn, limit)?;
        }
        Command::Find { db, pattern, limit } => {
            let conn = db::open(&db)?;
            display::find_files(&conn, &pattern, limit)?;
        }
        Command::Stats { db } => {
            let conn = db::open(&db)?;
            display::stats(&conn)?;
        }
        Command::Tui { db } => {
            tui::run(db)?;
        }
        Command::List => {
            let snaps = snapshots::list_snapshots();
            if snaps.is_empty() {
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
