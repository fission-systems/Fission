//! TUI rendering — Codex-style single-pane linear layout.
//!
//! Layout (top to bottom):
//!   1. Chat viewport  (fills remaining space)
//!   2. Status bar     (1 line, separator)
//!   3. Input box      (3 lines with border)
//!
//! Overlays rendered on top:
//!   - Help overlay (?), Provider menu (Ctrl+P), Model menu (Ctrl+O)

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::App;
use crate::markdown::render_markdown;

// ── Palette ───────────────────────────────────────────────────────────────────
// ANSI 16-color named only (no Rgb) to avoid GPU font atlas exhaustion.
const C_ACCENT:  Color = Color::Cyan;
const C_USER:    Color = Color::Green;
const C_AI:      Color = Color::Yellow;
const C_DIM:     Color = Color::DarkGray;
const C_WHITE:   Color = Color::White;

/// Spinner frames — rotates while streaming.
const SPINNER_FRAMES: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

/// Render the full TUI layout.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let input_lines = app.input.matches('\n').count() as u16 + 1;
    let input_height = (input_lines + 2).min(10); // max 10 lines tall

    // ── Layout: chat (fill) | status (1) | input (dynamic) ───────────────────
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),      // Chat viewport (top, expands)
            Constraint::Length(1),   // Status bar / separator
            Constraint::Length(input_height), // Dynamic Input
        ])
        .split(area);

    render_chat(frame, app, outer[0]);
    render_status_bar(frame, app, outer[1]);
    render_input(frame, app, outer[2]);

    // ── Overlays (rendered last so they appear on top) ────────────────────────
    if app.show_help {
        render_help_overlay(frame, area);
    } else if app.show_provider_menu {
        render_provider_menu(frame, app, area);
    } else if app.show_model_menu {
        render_model_menu(frame, app, area);
    }
}

// ── Chat viewport ─────────────────────────────────────────────────────────────

fn render_chat(frame: &mut Frame, app: &App, area: Rect) {
    // Compute render width for markdown wrapping.
    let width = area.width.saturating_sub(2) as usize;

    let mut text_lines: Vec<Line> = Vec::new();

    if app.entries.is_empty() {
        text_lines.push(Line::from(""));
        text_lines.push(Line::from(vec![
            Span::styled(
                "  👋 Welcome to Fission AI",
                Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD),
            )
        ]));
        text_lines.push(Line::from(Span::styled("     Press ? for help or just start typing to chat.", Style::default().fg(C_DIM))));
        text_lines.push(Line::from(""));
    }

    let entries_lines: Vec<Line> = app
        .entries
        .iter()
        .flat_map(|entry| {
            let role_color = if entry.role_label == "You" { C_USER } else { C_AI };

            // ── Role header ───────────────────────────────────────────────────
            let spinner_frame = {
                use std::time::{SystemTime, UNIX_EPOCH};
                let millis = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_millis();
                (millis / 125) as usize % SPINNER_FRAMES.len()
            };
            let header = if entry.is_streaming {
                Line::from(vec![
                    Span::styled(
                        format!("{} ", entry.role_label),
                        Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        SPINNER_FRAMES[spinner_frame],
                        Style::default().fg(C_AI),
                    ),
                ])
            } else {
                Line::from(vec![Span::styled(
                    format!("{} ▸", entry.role_label),
                    Style::default().fg(role_color).add_modifier(Modifier::BOLD),
                )])
            };

            // ── Content: markdown rendered for AI, plain for user ──────────────
            let content_lines: Vec<Line> = if entry.role_label == "You" {
                entry
                    .content
                    .lines()
                    .map(|l| {
                        Line::from(vec![
                            Span::raw("  "),
                            Span::styled(l.to_string(), Style::default().fg(C_WHITE)),
                        ])
                    })
                    .collect()
            } else {
                // AI response: render as markdown
                render_markdown(&entry.content, width)
                    .into_iter()
                    .map(|mut line| {
                        // Indent the whole content area by 2 spaces.
                        line.spans.insert(0, Span::raw("  "));
                        line
                    })
                    .collect()
            };

            let mut block_lines = vec![header];
            block_lines.extend(content_lines);
            block_lines.push(Line::from("")); // blank spacer between entries
            block_lines
        })
        .collect();

    text_lines.extend(entries_lines);

    let total_lines = text_lines.len() as u16;
    let max_scroll = total_lines.saturating_sub(area.height);
    let effective_scroll = if app.scroll == u16::MAX {
        max_scroll
    } else {
        app.scroll.min(max_scroll)
    };

    let para = Paragraph::new(Text::from(text_lines))
        .wrap(Wrap { trim: false })
        .scroll((effective_scroll, 0));

    frame.render_widget(para, area);
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    // Left side: brand + provider info
    let left = Line::from(vec![
        Span::styled(
            " ⚡ Fission ",
            Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(C_DIM)),
        Span::styled(&app.status_label, Style::default().fg(C_DIM)),
        if app.streaming {
            Span::styled("  ⟳ generating…", Style::default().fg(C_AI).add_modifier(Modifier::ITALIC))
        } else {
            Span::raw("")
        },
    ]);

    // Right side: key hints
    let hint_text = " ctrl+p provider  ctrl+o model  ? help  q quit ";
    let right_width = hint_text.len() as u16;
    let left_width = area.width.saturating_sub(right_width);

    // Split the status area into left + right.
    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(1), Constraint::Length(right_width)])
        .split(area);

    let left_para = Paragraph::new(left)
        .style(Style::default().bg(Color::Black));
    frame.render_widget(left_para, status_chunks[0]);

    // Only show right hints if the terminal is wide enough.
    if left_width > 20 {
        let right_para = Paragraph::new(Line::from(Span::styled(
            hint_text,
            Style::default().fg(C_DIM),
        )))
        .style(Style::default().bg(Color::Black));
        frame.render_widget(right_para, status_chunks[1]);
    }
}

// ── Input box ─────────────────────────────────────────────────────────────────

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let (prompt_hint, border_color) = if app.streaming {
        (" waiting for response… ", C_DIM)
    } else {
        ("", C_ACCENT)
    };

    let block = Block::default()
        .title(format!(" Message{prompt_hint}"))
        .title_style(Style::default().fg(border_color))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let text_area_height = area.height.saturating_sub(2);
    let cursor_line = app.input[..app.input_cursor].matches('\n').count() as u16;
    
    // Scroll input box if cursor is below visible area
    let scroll_y = if cursor_line >= text_area_height {
        cursor_line - text_area_height + 1
    } else {
        0
    };

    let input_text = Paragraph::new(app.input.as_str())
        .block(block)
        .style(Style::default().fg(C_WHITE))
        .scroll((scroll_y, 0));

    frame.render_widget(input_text, area);

    // Show blinking block cursor inside the input box when not streaming.
    if !app.streaming {
        let text_before = &app.input[..app.input_cursor];
        let last_nl = text_before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let cursor_col = app.input[last_nl..app.input_cursor].chars().count() as u16;
        
        let inner_x = area.x + 1 + cursor_col;
        let inner_y = area.y + 1 + cursor_line.saturating_sub(scroll_y);
        
        if inner_x < area.x + area.width.saturating_sub(1) && inner_y < area.y + area.height.saturating_sub(1) {
            frame.set_cursor_position((inner_x, inner_y));
        }
    }
}

// ── Help overlay ──────────────────────────────────────────────────────────────

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let width = 54u16.min(area.width);
    let height = 16u16.min(area.height);
    let overlay_area = centered_rect(width, height, area);

    frame.render_widget(Clear, overlay_area);

    let text = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Keyboard Shortcuts",
            Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        hint_line("Enter", "Send message"),
        hint_line("Backspace", "Delete character"),
        hint_line("← / →", "Move cursor left/right"),
        hint_line("↑ / ↓ / PgUp / PgDn", "Scroll chat"),
        hint_line("Ctrl+P", "Choose AI provider"),
        hint_line("Ctrl+O", "Choose model"),
        hint_line("Tab", "Toggle agent mode"),
        hint_line("Esc", "Close menu/overlay"),
        hint_line("q / Ctrl+C", "Quit"),
        hint_line("?", "Toggle this help"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Press Esc or ? to close",
            Style::default().fg(C_DIM),
        )]),
    ];

    let block = Block::default()
        .title(" Help ")
        .title_style(Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(C_ACCENT))
        .style(Style::default().bg(Color::Black));

    let para = Paragraph::new(text).block(block);
    frame.render_widget(para, overlay_area);
}

// ── Provider menu overlay ─────────────────────────────────────────────────────

fn render_provider_menu(frame: &mut Frame, app: &App, area: Rect) {
    let height = (app.provider_options.len() as u16 + 4).min(area.height);
    let width = 64u16.min(area.width);
    let overlay_area = centered_rect(width, height, area);

    frame.render_widget(Clear, overlay_area);

    let mut items = vec![Line::from("")];
    for (i, opt) in app.provider_options.iter().enumerate() {
        let selected = i == app.selected_provider_idx;
        let (prefix, style) = if selected {
            (" ▶ ", Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD))
        } else {
            ("   ", Style::default().fg(C_WHITE))
        };
        items.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("{:<22}", opt.title), style),
            Span::styled(opt.description, Style::default().fg(C_DIM)),
        ]));
    }
    items.push(Line::from(""));

    let block = Block::default()
        .title(" Select AI Provider  ↑/↓ navigate  Enter select  Esc cancel ")
        .title_style(Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(C_ACCENT))
        .style(Style::default().bg(Color::Black));

    let para = Paragraph::new(items).block(block);
    frame.render_widget(para, overlay_area);
}

// ── Model menu overlay ────────────────────────────────────────────────────────

fn render_model_menu(frame: &mut Frame, app: &App, area: Rect) {
    let height = (app.model_options.len() as u16 + 4).max(6).min(area.height);
    let width = 52u16.min(area.width);
    let overlay_area = centered_rect(width, height, area);

    frame.render_widget(Clear, overlay_area);

    let mut items = vec![Line::from("")];
    if app.is_fetching_models {
        items.push(Line::from(Span::styled(
            "   Fetching models…",
            Style::default().fg(C_AI).add_modifier(Modifier::SLOW_BLINK),
        )));
    } else if app.model_options.is_empty() {
        items.push(Line::from(Span::styled(
            "   No models available",
            Style::default().fg(C_DIM),
        )));
    } else {
        for (i, model) in app.model_options.iter().enumerate() {
            let selected = i == app.selected_model_idx;
            let (prefix, style) = if selected {
                (" ▶ ", Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD))
            } else {
                ("   ", Style::default().fg(C_WHITE))
            };
            items.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(model.clone(), style),
            ]));
        }
    }
    items.push(Line::from(""));

    let block = Block::default()
        .title(" Select Model  ↑/↓ navigate  Enter select  Esc cancel ")
        .title_style(Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(C_ACCENT))
        .style(Style::default().bg(Color::Black));

    let para = Paragraph::new(items).block(block);
    frame.render_widget(para, overlay_area);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Center a rect of `width × height` inside `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Build a two-column key/description hint line.
fn hint_line(key: &str, description: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {:<22}", key),
            Style::default().fg(C_ACCENT),
        ),
        Span::styled(description.to_string(), Style::default().fg(C_WHITE)),
    ])
}
