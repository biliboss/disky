use super::app::App;
use humansize::{format_size, BINARY};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, app: &App, list_state: &mut ListState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(0),    // tree
            Constraint::Length(1), // status
            Constraint::Length(1), // keys help
        ])
        .split(area);

    // header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " disky ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(&app.db_path, Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(header, chunks[0]);

    // tree list
    let max_size = app.flat.iter().map(|f| f.size).max().unwrap_or(1).max(1);
    let bar_width = 12usize;

    let items: Vec<ListItem> = app
        .flat
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.depth.saturating_sub(1));
            let icon = if item.is_dir {
                if item.expanded {
                    "▼ "
                } else {
                    "▶ "
                }
            } else {
                "  "
            };

            let size_str = if item.size > 0 {
                format_size(item.size as u64, BINARY)
            } else {
                "-".to_string()
            };

            let bar = size_bar(item.size, max_size, bar_width);

            let name_style = if item.is_dir {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let _size_col_width = 10;
            let name_part = format!("{}{}{}", indent, icon, item.name);
            let name_truncated = truncate(&name_part, 40);

            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<40}", name_truncated), name_style),
                Span::styled(
                    format!(" {:>10} ", size_str),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(bar, Style::default().fg(Color::Green)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ");

    frame.render_stateful_widget(list, chunks[1], list_state);

    // status
    let status_text = if app.status.is_empty() {
        format!(" {} items", app.flat.len())
    } else {
        format!(" {}", app.status)
    };
    let status = Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status, chunks[2]);

    // keys help
    let help = Paragraph::new(Line::from(vec![
        key("↑↓"),
        Span::raw(" nav  "),
        key("Enter"),
        Span::raw(" expand  "),
        key("o"),
        Span::raw(" Finder  "),
        key("c"),
        Span::raw(" copy path  "),
        key("e"),
        Span::raw(" HTML report  "),
        key("r"),
        Span::raw(" rescan  "),
        key("q"),
        Span::raw(" quit"),
    ]));
    frame.render_widget(help, chunks[3]);
}

fn key(s: &str) -> Span<'static> {
    Span::styled(
        format!("[{}]", s),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
}

fn size_bar(size: i64, max: i64, width: usize) -> String {
    let filled = if max > 0 {
        (size as f64 / max as f64 * width as f64) as usize
    } else {
        0
    };
    let filled = filled.min(width);
    let empty = width - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
