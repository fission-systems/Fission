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

use crate::app::{App, ViewMode, ActivePanel};
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

    match app.view_mode {
        ViewMode::Chat => render_chat_layout(frame, app, area),
        ViewMode::CodeExplorer => render_code_explorer(frame, app, area),
    }
}

// ── Chat layout (top-level shell) ────────────────────────────────────────────

fn render_chat_layout(frame: &mut Frame, app: &App, area: Rect) {
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

    if app.mention_state.is_some() {
        render_mention_popup(frame, app, outer[2]);
    } else if app.slash_state.is_some() {
        render_slash_popup(frame, app, outer[2]);
    }

    if app.session_history.is_some() {
        render_session_history(frame, app, area);
    }
}

// ── Code Explorer (Disasm + Decomp dual-pane) ─────────────────────────────

fn render_code_explorer(frame: &mut Frame, app: &App, area: Rect) {
    // ── Outer layout: header (1) | panes (fill) | status (1) ──────────────
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),   // View-mode header bar
            Constraint::Min(4),      // Two stacked panels
            Constraint::Length(1),   // Status / key-hint bar
        ])
        .split(area);

    // ── Header bar ──────────────────────────────────────────────────────────
    let label = app.explorer_label.as_deref().unwrap_or("No function selected");
    let header = Line::from(vec![
        Span::styled(" 🔬 Code Explorer ", Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("│ ", Style::default().fg(C_DIM)),
        Span::styled(label, Style::default().fg(C_WHITE)),
        Span::styled("  F2 back to Chat", Style::default().fg(C_DIM)),
    ]);
    frame.render_widget(
        Paragraph::new(header).style(Style::default().bg(Color::Black)),
        outer[0],
    );

    // ── Split pane area: top (disasm) | bottom (decomp) ────────────────────
    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // Disassembly
            Constraint::Percentage(50), // Decompiled C
        ])
        .split(outer[1]);

    // ── Disassembly panel ──────────────────────────────────────────────────
    let disasm_focused = app.active_panel == ActivePanel::Disasm;
    let disasm_border_color = if disasm_focused { C_ACCENT } else { C_DIM };
    let disasm_title_mod = if disasm_focused {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };

    let disasm_block = Block::default()
        .title(if disasm_focused {
            " ▶ Disassembly (Tab to switch) "
        } else {
            "   Disassembly "
        })
        .title_style(Style::default().fg(disasm_border_color).add_modifier(disasm_title_mod))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(disasm_border_color));

    let disasm_text: Vec<Line> = if app.disasm_content.is_empty() {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No disassembly loaded. Ask the AI: 'disassemble <address>'",
                Style::default().fg(C_DIM),
            )),
        ]
    } else {
        app.disasm_content
            .lines()
            .map(|l| render_disasm_line(l))
            .collect()
    };

    let disasm_para = Paragraph::new(Text::from(disasm_text))
        .block(disasm_block)
        .wrap(Wrap { trim: false })
        .scroll((app.disasm_scroll, 0));
    frame.render_widget(disasm_para, panes[0]);

    // ── Decompiled C panel ─────────────────────────────────────────────────
    let decomp_focused = app.active_panel == ActivePanel::Decomp;
    let decomp_border_color = if decomp_focused { C_ACCENT } else { C_DIM };
    let decomp_title_mod = if decomp_focused {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };

    let decomp_block = Block::default()
        .title(if decomp_focused {
            " ▶ Decompiled C (Tab to switch) "
        } else {
            "   Decompiled C "
        })
        .title_style(Style::default().fg(decomp_border_color).add_modifier(decomp_title_mod))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(decomp_border_color));

    let decomp_text: Vec<Line> = if app.decomp_content.is_empty() {
        vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No decompiled output. Ask the AI: 'decompile <address>'",
                Style::default().fg(C_DIM),
            )),
        ]
    } else {
        let width = panes[1].width.saturating_sub(4) as usize;
        render_markdown(&app.decomp_content, width)
    };

    let decomp_para = Paragraph::new(Text::from(decomp_text))
        .block(decomp_block)
        .wrap(Wrap { trim: false })
        .scroll((app.decomp_scroll, 0));
    frame.render_widget(decomp_para, panes[1]);

    // ── Bottom key-hint bar ────────────────────────────────────────────────
    let hint = Line::from(vec![
        Span::styled(" Tab ", Style::default().fg(C_ACCENT)),
        Span::styled("focus  ", Style::default().fg(C_DIM)),
        Span::styled("↑↓/PgUp/PgDn ", Style::default().fg(C_ACCENT)),
        Span::styled("scroll  ", Style::default().fg(C_DIM)),
        Span::styled("F2 ", Style::default().fg(C_ACCENT)),
        Span::styled("back to Chat  ", Style::default().fg(C_DIM)),
        Span::styled("q ", Style::default().fg(C_ACCENT)),
        Span::styled("quit", Style::default().fg(C_DIM)),
    ]);
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().bg(Color::Black)),
        outer[2],
    );
}

/// Syntax-highlight a single disassembly line with ANSI palette colors.
fn render_disasm_line(line: &str) -> Line<'static> {
    // Typical format: "  0x00401234  push    rbp"
    // We colorise: address (cyan), mnemonic (yellow), operands (white), comments (dark gray).
    let line = line.to_string();

    // Split on semicolon for inline comments.
    let (code_part, comment_part) = if let Some(idx) = line.find(';') {
        (&line[..idx], Some(&line[idx..]))
    } else {
        (line.as_str(), None)
    };

    let mut spans: Vec<Span<'static>> = Vec::new();

    // Try to detect an address token at the start.
    let trimmed = code_part.trim_start();
    let leading_ws = &code_part[..code_part.len() - trimmed.len()];
    if !leading_ws.is_empty() {
        spans.push(Span::raw(leading_ws.to_string()));
    }

    // Address token (hex starting with 0x or pure hex followed by ':').
    let rest = if trimmed.starts_with("0x") || (trimmed.len() > 8 && trimmed.chars().next().map(|c| c.is_ascii_hexdigit()).unwrap_or(false)) {
        let addr_end = trimmed.find(|c: char| c.is_whitespace()).unwrap_or(trimmed.len());
        let addr = &trimmed[..addr_end];
        spans.push(Span::styled(addr.to_string(), Style::default().fg(C_DIM)));
        &trimmed[addr_end..]
    } else {
        trimmed
    };

    // Mnemonic (first non-whitespace word after address).
    let rest2 = rest.trim_start();
    let leading2 = &rest[..rest.len() - rest2.len()];
    if !leading2.is_empty() {
        spans.push(Span::raw(leading2.to_string()));
    }

    if !rest2.is_empty() {
        let mnem_end = rest2.find(|c: char| c.is_whitespace()).unwrap_or(rest2.len());
        let mnem = &rest2[..mnem_end];
        // Keywords / interesting mnemonics highlighted brighter.
        let mnem_color = match mnem {
            "call" | "ret" | "jmp" | "je" | "jne" | "jz" | "jnz" | "jl" | "jle"
            | "jg" | "jge" | "ja" | "jae" | "jb" | "jbe" => Color::Magenta,
            "push" | "pop" => Color::Green,
            "mov" | "lea" | "movsx" | "movzx" | "movaps" | "movups" => Color::Yellow,
            "add" | "sub" | "imul" | "idiv" | "xor" | "and" | "or" | "not"
            | "shl" | "shr" | "sar" => Color::Cyan,
            _ => C_WHITE,
        };
        spans.push(Span::styled(mnem.to_string(), Style::default().fg(mnem_color).add_modifier(Modifier::BOLD)));
        let operands = &rest2[mnem_end..];
        if !operands.is_empty() {
            spans.push(Span::styled(operands.to_string(), Style::default().fg(C_WHITE)));
        }
    }

    // Comment part.
    if let Some(comment) = comment_part {
        spans.push(Span::styled(comment.to_string(), Style::default().fg(C_DIM).add_modifier(Modifier::ITALIC)));
    }

    Line::from(spans)
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

    let total_lines: u16 = text_lines.iter().map(|l| {
        let w = l.width() as u16;
        if w == 0 { 1 } else { (w + width as u16 - 1) / (width as u16) }
    }).sum();

    let max_scroll = total_lines.saturating_sub(area.height);
    let effective_scroll = max_scroll.saturating_sub(app.offset_from_bottom);

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

// ── Mention Popup ─────────────────────────────────────────────────────────────

fn render_mention_popup(frame: &mut Frame, app: &App, input_area: Rect) {
    if let Some(ref state) = app.mention_state {
        let max_items = 8;
        let item_count = state.options.len().min(max_items) as u16;
        
        let width = 40;
        let height = item_count + 2; // +2 for borders
        
        // Position it right above the input cursor
        let cursor_line = app.input[..app.input_cursor].matches('\n').count() as u16;
        let text_before = &app.input[..app.input_cursor];
        let last_nl = text_before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let cursor_col = app.input[last_nl..app.input_cursor].chars().count() as u16;
        
        let x = (input_area.x + 1 + cursor_col).min(input_area.x + input_area.width.saturating_sub(width));
        
        // Calculate Y so it sits just above the current cursor line
        let y = input_area.y
            .saturating_add(cursor_line)
            .saturating_sub(height);
            
        let area = Rect {
            x,
            y,
            width,
            height,
        };

        frame.render_widget(Clear, area);

        let items: Vec<Line> = if state.options.is_empty() {
            vec![Line::from(Span::styled("  No results found", Style::default().fg(C_DIM)))]
        } else {
            state.options.iter().enumerate().take(max_items).map(|(i, opt)| {
                let (prefix, style) = if i == state.selected_idx {
                    ("> ", Style::default().fg(Color::Black).bg(C_ACCENT))
                } else {
                    ("  ", Style::default().fg(C_WHITE).bg(Color::Black))
                };
                Line::from(Span::styled(format!("{prefix}{opt}"), style))
            }).collect()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(C_ACCENT))
            .title(format!(" Mentions (@{}) ", state.query))
            .style(Style::default().bg(Color::Black));
            
        let paragraph = Paragraph::new(items).block(block);
        frame.render_widget(paragraph, area);
    }
}

// ── Slash Command Popup ───────────────────────────────────────────────────────

fn render_slash_popup(frame: &mut Frame, app: &App, input_area: Rect) {
    if let Some(ref state) = app.slash_state {
        let max_items = 8;
        let item_count = state.options.len().min(max_items) as u16;
        
        let width = 30;
        let height = item_count + 2; // +2 for borders
        
        let cursor_line = app.input[..app.input_cursor].matches('\n').count() as u16;
        let text_before = &app.input[..app.input_cursor];
        let last_nl = text_before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let cursor_col = app.input[last_nl..app.input_cursor].chars().count() as u16;
        
        let x = (input_area.x + 1 + cursor_col).min(input_area.x + input_area.width.saturating_sub(width));
        
        let y = input_area.y
            .saturating_add(cursor_line)
            .saturating_sub(height);
            
        let area = Rect { x, y, width, height };

        frame.render_widget(Clear, area);

        let items: Vec<Line> = if state.options.is_empty() {
            vec![Line::from(Span::styled("  No commands found", Style::default().fg(C_DIM)))]
        } else {
            state.options.iter().enumerate().take(max_items).map(|(i, opt)| {
                let (prefix, style) = if i == state.selected_idx {
                    ("> ", Style::default().fg(Color::Black).bg(C_ACCENT))
                } else {
                    ("  ", Style::default().fg(C_WHITE).bg(Color::Black))
                };
                Line::from(Span::styled(format!("{prefix}/{opt}"), style))
            }).collect()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(C_ACCENT))
            .title(" Commands ")
            .style(Style::default().bg(Color::Black));
            
        let paragraph = Paragraph::new(items).block(block);
        frame.render_widget(paragraph, area);
    }
}

// ── Session History Popup ─────────────────────────────────────────────────────

fn render_session_history(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(ref state) = app.session_history {
        let block = Block::default()
            .title(" Session History ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(C_ACCENT))
            .style(Style::default().bg(Color::Black));
            
        let popup_area = centered_rect(60, 50, area);
        frame.render_widget(Clear, popup_area);

        let items: Vec<Line> = if state.options.is_empty() {
            vec![Line::from(Span::styled("  No saved sessions found", Style::default().fg(C_DIM)))]
        } else {
            state.options.iter().enumerate().map(|(i, (_path, name))| {
                let (prefix, style) = if i == state.selected_idx {
                    ("> ", Style::default().fg(Color::Black).bg(C_ACCENT).add_modifier(Modifier::BOLD))
                } else {
                    ("  ", Style::default().fg(C_WHITE).bg(Color::Black))
                };
                Line::from(Span::styled(format!("{prefix}{name}"), style))
            }).collect()
        };

        let list = Paragraph::new(items).block(block);
        frame.render_widget(list, popup_area);
    }
}

// ── Help overlay ──────────────────────────────────────────────────────────────

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let width = 60u16.min(area.width);
    let height = 18u16.min(area.height);
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
        hint_line("F2", "Toggle Code Explorer view"),
        hint_line("Tab (in explorer)", "Switch Disasm/Decomp focus"),
        hint_line("Ctrl+P", "Choose AI provider"),
        hint_line("Ctrl+O", "Choose model"),
        hint_line("Tab (in chat)", "Toggle agent mode"),
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
