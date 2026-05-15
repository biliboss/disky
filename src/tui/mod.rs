mod app;
mod tree;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};
use std::io;
use std::time::Duration;

use crate::db;
use crate::display;
use crate::snapshots;
use app::App;
use tree::load_root;

pub fn run(db_path: Option<String>) -> Result<()> {
    let path = db_path
        .or_else(|| snapshots::latest_snapshot())
        .ok_or_else(|| anyhow::anyhow!("No snapshot found. Run `disky scan` first."))?;

    let conn = db::open(&path)?;

    // detect scan root from DB (smallest depth path)
    let root_path: String = conn
        .query_row("SELECT path FROM files WHERE depth = 0 LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap_or_else(|_| "/".to_string());

    eprintln!("Loading snapshot {}...", path);
    let root = load_root(&conn, &root_path)?;

    let mut app = App::new(path.clone(), root);
    let mut list_state = ListState::default();
    list_state.select(Some(0));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        list_state.select(Some(app.selected));
        terminal.draw(|f| ui::render(f, &app, &mut list_state))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            // ctrl-c / ctrl-d
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
            {
                break;
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,

                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),

                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    app.toggle_expand(&conn)?;
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    // collapse current or go to parent
                    if let Some(item) = app.flat.get(app.selected).cloned() {
                        if item.is_dir && item.expanded {
                            app.toggle_expand(&conn)?;
                        } else if item.depth > 1 {
                            // select parent
                            let parent = parent_path(&item.path);
                            if let Some(idx) = app.flat.iter().position(|f| f.path == parent) {
                                app.selected = idx;
                            }
                        }
                    }
                }

                KeyCode::Char('o') => {
                    if let Err(e) = app.open_finder() {
                        app.status = format!("Error: {e}");
                    }
                }
                KeyCode::Char('c') => {
                    if let Err(e) = app.copy_path() {
                        app.status = format!("Error: {e}");
                    }
                }
                KeyCode::Char('e') => {
                    terminal.clear()?;
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    display::export_html_report(&conn, &path)?;
                    enable_raw_mode()?;
                    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                    app.status = "HTML report exported.".into();
                }
                KeyCode::Char('r') => {
                    app.status = "Run `disky scan` to create new snapshot.".into();
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => path[..i].to_string(),
        None => "/".to_string(),
    }
}
