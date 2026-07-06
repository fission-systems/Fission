//! Markdown → ratatui `Line` renderer (Codex-style).
//!
//! Converts a markdown string into styled `ratatui` lines with support for:
//! - **Bold**, *italic*, `inline code`
//! - # Headings (H1–H3)
//! - Fenced code blocks (with lang label)
//! - Bullet/ordered lists
//! - Blockquotes
//! - Thematic rules (─────)
//!
//! Uses `pulldown-cmark` for robust event-based parsing.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::sync::LazyLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(|| SyntaxSet::load_defaults_newlines());
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(|| ThemeSet::load_defaults());

fn translate_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

fn translate_style(style: SyntectStyle) -> Style {
    let mut s = Style::default().fg(translate_color(style.foreground));
    if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) {
        s = s.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
        s = s.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(syntect::highlighting::FontStyle::UNDERLINE) {
        s = s.add_modifier(Modifier::UNDERLINED);
    }
    s
}

/// Convert `markdown` text into a `Vec<Line>` ready to render with ratatui.
///
/// `width` is the available render width; used to clip/wrap code blocks.
pub fn render_markdown(input: &str, _width: usize) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(input, options);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();

    // Style stack for nested inline styles.
    let mut style_stack: Vec<Style> = Vec::new();
    let mut list_depth: u32 = 0;
    let mut ordered_list_counters: Vec<u64> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang: Option<String> = None;
    let mut code_buf = String::new();
    let mut needs_blank = false;

    macro_rules! push_line {
        () => {{
            let spans = std::mem::take(&mut current_spans);
            lines.push(Line::from(spans));
        }};
    }

    macro_rules! blank_line_if_needed {
        () => {{
            if needs_blank {
                lines.push(Line::from(""));
                needs_blank = false;
            }
        }};
    }

    macro_rules! current_style {
        () => {
            style_stack.last().copied().unwrap_or_default()
        };
    }

    for event in parser {
        match event {
            // ── Inline text ──────────────────────────────────────────────────
            Event::Text(text) => {
                if in_code_block {
                    code_buf.push_str(&text);
                } else {
                    let style = current_style!();
                    current_spans.push(Span::styled(text.into_string(), style));
                }
            }

            Event::Code(code) => {
                // Inline `code` — cyan, like Codex
                current_spans.push(Span::styled(
                    code.into_string(),
                    Style::default().fg(Color::Cyan),
                ));
            }

            Event::SoftBreak => {
                if !in_code_block {
                    current_spans.push(Span::raw(" "));
                }
            }

            Event::HardBreak => {
                push_line!();
            }

            Event::Rule => {
                push_line!();
                lines.push(Line::from(Span::styled(
                    "─".repeat(40),
                    Style::default().fg(Color::DarkGray),
                )));
                needs_blank = true;
            }

            // ── Block starts ─────────────────────────────────────────────────
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    blank_line_if_needed!();
                }

                Tag::Heading { level, .. } => {
                    blank_line_if_needed!();
                    let (prefix, style) = match level {
                        HeadingLevel::H1 => (
                            "# ",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        HeadingLevel::H2 => (
                            "## ",
                            Style::default()
                                .fg(Color::LightBlue)
                                .add_modifier(Modifier::BOLD),
                        ),
                        HeadingLevel::H3 => (
                            "### ",
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                        ),
                        _ => (
                            "#### ",
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    };
                    style_stack.push(style);
                    current_spans.push(Span::styled(prefix, style));
                }

                Tag::Strong => {
                    let s = current_style!().add_modifier(Modifier::BOLD);
                    style_stack.push(s);
                }

                Tag::Emphasis => {
                    let s = current_style!().add_modifier(Modifier::ITALIC);
                    style_stack.push(s);
                }

                Tag::Strikethrough => {
                    let s = current_style!().add_modifier(Modifier::CROSSED_OUT);
                    style_stack.push(s);
                }

                Tag::CodeBlock(kind) => {
                    blank_line_if_needed!();
                    push_line!();
                    in_code_block = true;
                    code_buf.clear();
                    code_lang = match kind {
                        CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.into_string()),
                        _ => None,
                    };
                }

                Tag::List(start_idx) => {
                    blank_line_if_needed!();
                    list_depth += 1;
                    ordered_list_counters.push(start_idx.unwrap_or(1) - 1);
                }

                Tag::Item => {
                    push_line!(); // flush any pending line
                    let depth = list_depth.saturating_sub(1) as usize;
                    let indent = "  ".repeat(depth);
                    let marker = if let Some(counter) = ordered_list_counters.last_mut() {
                        *counter += 1;
                        format!("{}{}. ", indent, counter)
                    } else {
                        format!("{}• ", indent)
                    };
                    current_spans.push(Span::styled(marker, Style::default().fg(Color::Yellow)));
                }

                Tag::BlockQuote(_) => {
                    blank_line_if_needed!();
                    let s = Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::ITALIC);
                    style_stack.push(s);
                    current_spans.push(Span::styled("│ ", Style::default().fg(Color::Green)));
                }

                Tag::Link { .. } => {
                    let s = Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED);
                    style_stack.push(s);
                }

                _ => {}
            },

            // ── Block ends ───────────────────────────────────────────────────
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    push_line!();
                    needs_blank = true;
                }

                TagEnd::Heading(_) => {
                    style_stack.pop();
                    push_line!();
                    needs_blank = true;
                }

                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link => {
                    style_stack.pop();
                }

                TagEnd::CodeBlock => {
                    in_code_block = false;
                    let lang = code_lang.take().unwrap_or_default();
                    if !lang.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!(" {} ", lang),
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(Modifier::ITALIC),
                        )));
                    }

                    let syntax = SYNTAX_SET
                        .find_syntax_by_token(&lang)
                        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());
                    let theme = &THEME_SET.themes["base16-ocean.dark"];
                    let mut h = HighlightLines::new(syntax, theme);

                    for code_line in syntect::util::LinesWithEndings::from(&code_buf) {
                        let ranges = h.highlight_line(code_line, &SYNTAX_SET).unwrap_or_default();
                        let mut spans = vec![Span::styled("  ", Style::default())];
                        for (style, text) in ranges {
                            let trimmed = text.trim_end_matches('\n').trim_end_matches('\r');
                            if !trimmed.is_empty() {
                                spans.push(Span::styled(trimmed.to_string(), translate_style(style)));
                            }
                        }
                        lines.push(Line::from(spans));
                    }
                    code_buf.clear();
                    needs_blank = true;
                }

                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                    ordered_list_counters.pop();
                    needs_blank = true;
                }

                TagEnd::Item => {
                    push_line!();
                }

                TagEnd::BlockQuote(_) => {
                    style_stack.pop();
                    push_line!();
                    needs_blank = true;
                }

                _ => {}
            },

            _ => {}
        }
    }

    // Flush any trailing content.
    if !current_spans.is_empty() {
        push_line!();
    }

    lines
}
