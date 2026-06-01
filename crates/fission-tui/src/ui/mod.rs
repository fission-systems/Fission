//! TUI rendering — composes status bar, chat viewport, and input box.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::app::App;

// ── Colour palette ────────────────────────────────────────────────────────────
const C_ACCENT:  Color = Color::Rgb(100, 180, 255);  // Fission blue
const C_USER:    Color = Color::Rgb(120, 220, 140);  // User green
const C_AI:      Color = Color::Rgb(220, 170, 100);  // AI amber
const C_DIM:     Color = Color::Rgb(120, 120, 130);  // dim grey
const C_BG:      Color = Color::Rgb(18, 18, 24);     // near-black background
const C_SURFACE: Color = Color::Rgb(28, 28, 36);     // slightly lighter surface
const C_BORDER:  Color = Color::Rgb(55, 55, 70);     // subtle border

/// Render the full TUI layout.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // ── Layout: status (1) | chat (fill) | input (3) ─────────────────────────
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),   // status bar
            Constraint::Min(5),      // chat viewport
            Constraint::Length(3),   // input box
        ])
        .split(area);

    render_status_bar(frame, app, chunks[0]);
    render_chat(frame, app, chunks[1]);
    render_input(frame, app, chunks[2]);

    if app.show_help {
        render_help_overlay(frame, area);
    }
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let streaming_indicator = if app.streaming { "  ⟳ streaming…" } else { "" };
    let line = Line::from(vec![
        Span::styled(
            " ⚡ Fission AI ",
            Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(C_BORDER)),
        Span::styled(&app.status_label, Style::default().fg(C_DIM)),
        Span::styled(streaming_indicator, Style::default().fg(C_AI).add_modifier(Modifier::ITALIC)),
        Span::styled(
            "  [?] help  [q] quit",
            Style::default().fg(C_DIM),
        ),
    ]);
    let paragraph = Paragraph::new(line)
        .style(Style::default().bg(C_SURFACE));
    frame.render_widget(paragraph, area);
}

// ── Chat viewport ─────────────────────────────────────────────────────────────

fn render_chat(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(C_BORDER))
        .style(Style::default().bg(C_BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build list items from chat entries.
    let items: Vec<ListItem> = app
        .entries
        .iter()
        .flat_map(|entry| {
            let role_color = if entry.role_label == "You" { C_USER } else { C_AI };
            let header = ListItem::new(Line::from(vec![
                Span::styled(
                    format!("╔ {} ", entry.role_label),
                    Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                ),
            ]));

            // Wrap content lines.
            let content_lines: Vec<ListItem> = entry
                .content
                .lines()
                .map(|l| {
                    ListItem::new(Line::from(vec![
                        Span::styled("║ ", Style::default().fg(C_DIM)),
                        Span::raw(l.to_string()),
                    ]))
                })
                .collect();

            let separator = ListItem::new(Line::from(
                Span::styled("╚═══", Style::default().fg(C_DIM)),
            ));

            let mut items = vec![header];
            items.extend(content_lines);
            items.push(separator);
            items.push(ListItem::new(Line::from(""))); // blank spacer
            items
        })
        .collect();

    // Compute scroll: clamp to visible area.
    let total = items.len() as u16;
    let visible = inner.height;
    let max_scroll = total.saturating_sub(visible);
    let _scroll = if app.scroll == u16::MAX {
        max_scroll
    } else {
        app.scroll.min(max_scroll)
    };

    let _list = List::new(items)
        .style(Style::default().fg(Color::White));

    // ratatui List doesn't support scroll directly; use Paragraph for now.
    // We render a Paragraph of collected text with scroll offset.
    let text_lines: Vec<Line> = app
        .entries
        .iter()
        .flat_map(|entry| {
            let role_color = if entry.role_label == "You" { C_USER } else { C_AI };
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("╔ {} ", entry.role_label),
                    Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                ),
                if entry.is_streaming {
                    Span::styled("●", Style::default().fg(C_AI).add_modifier(Modifier::SLOW_BLINK))
                } else {
                    Span::raw("")
                },
            ])];
            for l in entry.content.lines() {
                lines.push(Line::from(vec![
                    Span::styled("║ ", Style::default().fg(C_DIM)),
                    Span::raw(l.to_string()),
                ]));
            }
            lines.push(Line::from(Span::styled("╚════", Style::default().fg(C_DIM))));
            lines.push(Line::from(""));
            lines
        })
        .collect();

    let total_lines = text_lines.len() as u16;
    let max_scroll = total_lines.saturating_sub(inner.height);
    let effective_scroll = if app.scroll == u16::MAX { max_scroll } else { app.scroll.min(max_scroll) };

    let para = Paragraph::new(Text::from(text_lines))
        .wrap(Wrap { trim: false })
        .scroll((effective_scroll, 0))
        .style(Style::default().bg(C_BG));

    frame.render_widget(para, inner);
}

// ── Input box ─────────────────────────────────────────────────────────────────

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let prompt_hint = if app.streaming { " (waiting…)" } else { "" };
    let block = Block::default()
        .title(format!(" Message{prompt_hint} "))
        .title_style(Style::default().fg(C_ACCENT))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.streaming { C_DIM } else { C_ACCENT }))
        .style(Style::default().bg(C_SURFACE));

    let input_text = Paragraph::new(app.input.as_str())
        .block(block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(input_text, area);

    // Show cursor inside the input box.
    if !app.streaming {
        let inner_x = area.x + 1 + app.input_cursor as u16;
        let inner_y = area.y + 1;
        if inner_x < area.x + area.width - 1 {
            frame.set_cursor_position((inner_x, inner_y));
        }
    }
}

// ── Help overlay ──────────────────────────────────────────────────────────────

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let width = 50u16.min(area.width);
    let height = 12u16.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let overlay_area = Rect::new(x, y, width, height);

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Key Bindings", Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD))
        ]),
        Line::from(""),
        Line::from("  Enter       Send message"),
        Line::from("  Backspace   Delete character"),
        Line::from("  ↑ / ↓       Scroll chat"),
        Line::from("  q / Ctrl+C  Quit"),
        Line::from("  ?           Toggle help"),
        Line::from(""),
        Line::from(vec![Span::styled("  Press ? to close", Style::default().fg(C_DIM))]),
    ];

    let block = Block::default()
        .title(" Help ")
        .title_style(Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(C_ACCENT))
        .style(Style::default().bg(C_SURFACE));

    let para = Paragraph::new(Text::from(text)).block(block);
    frame.render_widget(para, overlay_area);
}
