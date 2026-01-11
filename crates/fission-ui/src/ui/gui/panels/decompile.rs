//! Decompiled code panel - displays C-like decompiled output with syntax highlighting.

use super::super::components::widgets::empty_state_with_spacing;
use crate::ui::gui::core::state::AppState;
use crate::ui::gui::theme::catppuccin;
use eframe::egui;

/// Render the decompiled code as a fixed right panel.
#[allow(dead_code)]
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    let max_w = ctx.screen_rect().width() * 0.6; // Up to 60% of screen
    egui::SidePanel::right("decompile_panel")
        .resizable(true)
        .default_width(350.0)
        .min_width(150.0)
        .max_width(max_w.max(400.0))
        .show(ctx, |ui| {
            render_inside(ui, state);
        });
}

/// Render decompiled code inside an existing UI.
pub fn render_inside(ui: &mut egui::Ui, state: &mut AppState) {
    // Header row with fixed layout
    let header_height = 24.0;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), header_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.heading(egui::RichText::new("Decompiled").color(catppuccin::LAVENDER));

            if state.analysis.domain.decompiling {
                ui.spinner();
                ui.label(
                    egui::RichText::new("Processing...")
                        .color(catppuccin::YELLOW)
                        .small(),
                );
            } else if let Some(ref func) = state.analysis.domain.selected_function {
                ui.separator();
                ui.label(
                    egui::RichText::new(&func.name)
                        .color(catppuccin::BLUE)
                        .small(),
                );
            }

            // Push Copy button to the right using remaining space
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !state.analysis.domain.decompiled_code.is_empty()
                    && ui
                        .small_button(egui::RichText::new("📋 Copy").color(catppuccin::TEAL))
                        .clicked()
                {
                    ui.output_mut(|o| {
                        o.copied_text = state.analysis.domain.decompiled_code.clone()
                    });
                }
            });
        },
    );
    ui.separator();

    if state.analysis.domain.decompiled_code.is_empty() && !state.analysis.domain.decompiling {
        empty_state_with_spacing(
            ui,
            "No decompilation available",
            Some("Select a function to decompile"),
            60.0,
        );
        return;
    }

    // Code view with syntax highlighting
    let row_height = 18.0; // Estimate
    let total_rows = state.viewmodels.decompile.tokenized_lines.len();

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, total_rows, |ui, row_range| {
            render_highlighted_code(ui, state, row_range);
        });
}

/// Render code with basic C syntax highlighting
fn render_highlighted_code(
    ui: &mut egui::Ui,
    state: &mut AppState,
    row_range: std::ops::Range<usize>,
) {
    // Calculate required width (rough estimate)
    ui.set_min_width(400.0);

    for line_num in row_range {
        let tokens = match state.viewmodels.decompile.tokenized_lines.get(line_num) {
            Some(t) => t.clone(),
            None => continue,
        };

        ui.horizontal(|ui| {
            ui.style_mut().spacing.item_spacing.x = 2.0;

            // Line number
            ui.label(
                egui::RichText::new(format!("{:4}", line_num + 1))
                    .color(catppuccin::OVERLAY0)
                    .monospace(),
            );

            ui.separator();

            // Render cached tokens with indentation guides
            ui.horizontal(|ui| {
                ui.style_mut().spacing.item_spacing.x = 0.0;

                // Simple indentation guide drawing
                let mut first_token = true;
                for token in &tokens {
                    if first_token {
                        // Draw indentation guides
                        let space_per_indent = 16.0; // Estimate
                        let leading_spaces =
                            token.text.chars().take_while(|c| c.is_whitespace()).count();
                        let indent_level = leading_spaces / 4; // Assuming 4 spaces per indent

                        for i in 1..=indent_level {
                            let x = ui.cursor().min.x + (i as f32 * space_per_indent);
                            let y_top = ui.cursor().min.y;
                            let y_bottom = ui.cursor().max.y;
                            ui.painter().line_segment(
                                [egui::pos2(x, y_top), egui::pos2(x, y_bottom)],
                                egui::Stroke::new(1.0, catppuccin::SURFACE0),
                            );
                        }
                        first_token = false;
                    }

                    if token.is_clickable {
                        render_decompiler_token(
                            ui,
                            state,
                            &token.text,
                            token.color,
                            token.is_function_call,
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(&token.text)
                                .color(token.color)
                                .monospace(),
                        );
                    }
                }
            });
        });
    }
}

fn render_decompiler_token(
    ui: &mut egui::Ui,
    state: &mut AppState,
    token_text: &str,
    color: egui::Color32,
    is_function_call: bool,
) {
    let is_highlighted = state
        .ui
        .highlighted_symbol
        .as_ref()
        .map(|s| s == token_text)
        .unwrap_or(false);

    let mut rich_text = egui::RichText::new(token_text).color(color).monospace();
    if is_highlighted {
        rich_text = rich_text.background_color(catppuccin::SURFACE1);
    }
    // Underline function calls to indicate they're navigable
    if is_function_call {
        rich_text = rich_text.underline();
    }

    let resp = ui.add(egui::Label::new(rich_text).sense(egui::Sense::click()));

    // Check for Ctrl+Click for function navigation
    let ctrl_held = ui.input(|i| i.modifiers.ctrl);

    if resp.clicked() {
        if is_function_call && ctrl_held {
            // Try to navigate to the function
            if let Some(addr) = try_parse_function_target(token_text, state) {
                state.ui.pending_jump = Some(addr);
                state.log(format!("[*] Jumping to function at 0x{:x}", addr));
            }
        } else {
            // Normal highlight toggle
            if is_highlighted {
                state.ui.highlighted_symbol = None;
            } else {
                state.ui.highlighted_symbol = Some(token_text.to_string());
            }
        }
    }

    // Hover hints
    if is_function_call {
        resp.on_hover_text("Ctrl+Click to go to function");
    }
}

/// Try to parse function target from token text (either hex address or function name)
fn try_parse_function_target(token_text: &str, state: &AppState) -> Option<u64> {
    // Check if it's a hex address like 0x401000
    if token_text.starts_with("0x") {
        if let Ok(addr) = u64::from_str_radix(&token_text[2..], 16) {
            return Some(addr);
        }
    }

    // Try to find matching function by name
    if let Some(ref binary) = state.analysis.domain.loaded_binary {
        for func in &binary.functions {
            // Check original name
            if func.name == token_text {
                return Some(func.address);
            }
        }
    }

    // Check user-defined function names
    for (addr, name) in &state.analysis.domain.user_function_names {
        if name == token_text {
            return Some(*addr);
        }
    }

    None
}
