//! UI rendering components

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::app::App;

/// Render the main UI
pub fn render_ui(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_main_content(frame, app, chunks[1]);
    render_status_bar(frame, app, chunks[2]);
}

fn render_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let header = Paragraph::new(format!(
        " Fission TUI │ {} │ {} functions │ {} ({}-bit)",
        app.binary().path,
        app.functions().len(),
        app.binary().format,
        if app.binary().is_64bit { 64 } else { 32 }
    ))
    .block(Block::default().borders(Borders::ALL).title("Fission"))
    .style(Style::default().fg(Color::Cyan));

    frame.render_widget(header, area);
}

fn render_main_content(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    render_function_list(frame, app, main_chunks[0]);
    render_code_view(frame, app, main_chunks[1]);
}

fn render_function_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = app
        .functions()
        .iter()
        .map(|f| ListItem::new(format!("0x{:x} {}", f.address, f.name)))
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Functions"))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(list, area, app.list_state_mut());
}

fn render_code_view(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let code_lines: Vec<&str> = app.decompiled_code().lines().collect();
    let visible_start = app.scroll() as usize;
    let visible_lines: String = code_lines
        .iter()
        .skip(visible_start)
        .take(area.height as usize - 2)
        .copied()
        .collect::<Vec<&str>>()
        .join("\n");

    let code_block = Paragraph::new(visible_lines)
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Decompiled Code (line {}/{})",
            visible_start + 1,
            code_lines.len()
        )))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    frame.render_widget(code_block, area);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let status = Paragraph::new(format!(" {}", app.status()))
        .style(Style::default().bg(Color::Blue).fg(Color::White));

    frame.render_widget(status, area);
}
