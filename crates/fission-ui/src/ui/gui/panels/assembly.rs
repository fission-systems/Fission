//! Assembly view panel - displays disassembled instructions with virtual scrolling.

use super::super::components::widgets::empty_state_with_spacing;
use super::super::core::state::AppState;
use super::super::theme::{catppuccin, code};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// Render the assembly view in the central panel with virtualized scrolling.
#[allow(dead_code)]
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        render_inside(ui, state);
    });
}

/// Render assembly view inside an existing UI.
pub fn render_inside(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("Assembly").color(catppuccin::LAVENDER));
        ui.separator();
        ui.label(
            egui::RichText::new(format!(
                "{} instructions",
                state.analysis.domain.asm_instructions.len()
            ))
            .color(catppuccin::SUBTEXT0)
            .small(),
        );
    });
    ui.separator();

    if state.analysis.domain.asm_instructions.is_empty() {
        empty_state_with_spacing(
            ui,
            "No disassembly available",
            Some("Select a function to view assembly"),
            40.0,
        );
        return;
    }

    let available_height = ui.available_height();
    let row_height = 20.0;
    let total_rows = state.analysis.domain.asm_instructions.len();

    // Use TableBuilder for efficient virtual scrolling
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(90.0)) // Address
        .column(Column::initial(120.0).at_least(80.0)) // Bytes
        .column(Column::initial(70.0).at_least(50.0)) // Mnemonic
        .column(Column::initial(160.0).at_least(100.0)) // Operands
        .column(Column::remainder()) // Comment
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height)
        .header(22.0, |mut header| {
            header.col(|ui| {
                ui.label(
                    egui::RichText::new("Address")
                        .strong()
                        .color(catppuccin::TEXT),
                );
            });
            header.col(|ui| {
                ui.label(
                    egui::RichText::new("Bytes")
                        .strong()
                        .color(catppuccin::TEXT),
                );
            });
            header.col(|ui| {
                ui.label(
                    egui::RichText::new("Mnemonic")
                        .strong()
                        .color(catppuccin::TEXT),
                );
            });
            header.col(|ui| {
                ui.label(
                    egui::RichText::new("Operands")
                        .strong()
                        .color(catppuccin::TEXT),
                );
            });
            header.col(|ui| {
                ui.label(
                    egui::RichText::new("Comment")
                        .strong()
                        .color(catppuccin::TEXT),
                );
            });
        })
        .body(|body| {
            body.rows(row_height, total_rows, |mut row| {
                let row_index = row.index();
                let insn = &state.analysis.domain.asm_instructions[row_index];

                // Address column
                row.col(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{:08X}", insn.address))
                            .color(code::ADDRESS)
                            .monospace(),
                    );
                });

                // Bytes column (truncate if too long)
                row.col(|ui| {
                    let mut bytes_str = String::with_capacity(32);
                    for (i, b) in insn.bytes.iter().enumerate() {
                        if i >= 8 {
                            bytes_str.push_str("..");
                            break;
                        }
                        use std::fmt::Write;
                        write!(bytes_str, "{:02X} ", b).unwrap();
                    }
                    ui.label(
                        egui::RichText::new(bytes_str)
                            .color(code::HEX_BYTE)
                            .monospace(),
                    );
                });

                let insn_addr = insn.address;
                let insn_mnemonic = insn.mnemonic.clone();
                let insn_operands = insn.operands.clone();
                let insn_is_flow = insn.is_flow_control;

                // Mnemonic column with color coding
                row.col(|ui| {
                    let color = if insn_is_flow {
                        code::MNEMONIC_FLOW
                    } else {
                        code::MNEMONIC_NORMAL
                    };
                    render_clickable_text(ui, state, &insn_mnemonic, color, true);
                });

                // Operands column with syntax highlighting
                row.col(|ui| {
                    render_operands(ui, state, &insn_operands);
                });

                // Comment column - inline editable
                row.col(|ui| {
                    let comment = state.analysis.domain.user_comments.get(&insn_addr);
                    let has_comment = comment.is_some();

                    let display_text = if let Some(c) = comment {
                        c.as_str()
                    } else {
                        "" // Empty placeholder
                    };

                    // Style based on whether comment exists
                    let (text_color, bg_color) = if has_comment {
                        (catppuccin::GREEN, Some(catppuccin::SURFACE0))
                    } else {
                        (catppuccin::OVERLAY1, None)
                    };

                    let mut rich_text = egui::RichText::new(if display_text.is_empty() {
                        "  ; ..." // Placeholder hint
                    } else {
                        display_text
                    })
                    .color(if has_comment {
                        text_color
                    } else {
                        catppuccin::SURFACE2
                    })
                    .monospace()
                    .italics();

                    if let Some(bg) = bg_color {
                        rich_text = rich_text.background_color(bg);
                    }

                    let resp = ui.add(egui::Label::new(rich_text).sense(egui::Sense::click()));

                    // Double-click to edit comment
                    if resp.double_clicked() {
                        let current = state
                            .analysis
                            .domain
                            .user_comments
                            .get(&insn_addr)
                            .cloned()
                            .unwrap_or_default();
                        state.viewmodels.functions.comment_dialog = Some((insn_addr, current));
                    }

                    // Hover tooltip (clone resp since context_menu consumes it)
                    if has_comment {
                        resp.clone()
                            .on_hover_text("Double-click to edit • Right-click for menu");
                    } else {
                        resp.clone().on_hover_text("Double-click to add comment");
                    }

                    // Add context menu to show/edit comment or rename
                    resp.context_menu(|ui| {
                        if ui.button("✏️ Edit Comment").clicked() {
                            let current = state
                                .analysis
                                .domain
                                .user_comments
                                .get(&insn_addr)
                                .cloned()
                                .unwrap_or_default();
                            state.viewmodels.functions.comment_dialog = Some((insn_addr, current));
                            ui.close_menu();
                        }
                        if ui.button("🏷️ Rename Label").clicked() {
                            let current = state
                                .analysis
                                .domain
                                .user_function_names
                                .get(&insn_addr)
                                .cloned()
                                .unwrap_or_else(|| format!("sub_{:x}", insn_addr));
                            state.viewmodels.functions.rename_dialog = Some((insn_addr, current));
                            ui.close_menu();
                        }
                        if ui.button("📌 Add Bookmark").clicked() {
                            let label = state
                                .analysis
                                .domain
                                .user_function_names
                                .get(&insn_addr)
                                .cloned()
                                .unwrap_or_else(|| format!("loc_{:x}", insn_addr));
                            state.analysis.domain.bookmarks.insert(insn_addr, label);
                            state.log(format!("[*] Bookmark added at 0x{:08X}", insn_addr));
                            ui.close_menu();
                        }
                    });
                });
            });
        });
}

/// Render clickable and highlightable text
fn render_clickable_text(
    ui: &mut egui::Ui,
    state: &mut AppState,
    text: &str,
    color: egui::Color32,
    strong: bool,
) {
    let is_highlighted = state
        .ui
        .highlighted_symbol
        .as_ref()
        .map(|s| s == text)
        .unwrap_or(false);

    let mut rich_text = egui::RichText::new(text).color(color).monospace();
    if strong {
        rich_text = rich_text.strong();
    }

    if is_highlighted {
        rich_text = rich_text.background_color(catppuccin::SURFACE2);
    }

    let resp = ui.add(egui::Label::new(rich_text).sense(egui::Sense::click()));
    if resp.clicked() {
        if is_highlighted {
            state.ui.highlighted_symbol = None;
        } else {
            state.ui.highlighted_symbol = Some(text.to_string());
        }
    }
}

/// Render operands with individual token highlighting and clicking
fn render_operands(ui: &mut egui::Ui, state: &mut AppState, operands: &str) {
    ui.horizontal(|ui| {
        ui.style_mut().spacing.item_spacing.x = 0.0;

        // Split operands into tokens (registers, numbers, delimiters)
        let mut last_pos = 0;
        for (start, part) in operands.match_indices(|c: char| !c.is_alphanumeric() && c != '_') {
            if start > last_pos {
                let token = &operands[last_pos..start];
                render_token(ui, state, token);
            }
            // Render delimiter
            ui.label(
                egui::RichText::new(part)
                    .color(catppuccin::TEXT)
                    .monospace(),
            );
            last_pos = start + part.len();
        }

        if last_pos < operands.len() {
            let token = &operands[last_pos..];
            render_token(ui, state, token);
        }
    });
}

fn render_token(ui: &mut egui::Ui, state: &mut AppState, token: &str) {
    let is_reg = is_register(token);
    let is_num = token.starts_with("0x") || token.chars().all(|c| c.is_ascii_digit());

    let color = if is_reg {
        code::REGISTER
    } else if is_num {
        code::NUMBER
    } else {
        catppuccin::TEXT
    };

    render_clickable_text(ui, state, token, color, false);
}

fn is_register(token: &str) -> bool {
    matches!(
        token,
        "rax"
            | "rbx"
            | "rcx"
            | "rdx"
            | "rsi"
            | "rdi"
            | "rbp"
            | "rsp"
            | "r8"
            | "r9"
            | "r10"
            | "r11"
            | "r12"
            | "r13"
            | "r14"
            | "r15"
            | "eax"
            | "ebx"
            | "ecx"
            | "edx"
            | "esi"
            | "edi"
            | "ebp"
            | "esp"
            | "ax"
            | "bx"
            | "cx"
            | "dx"
            | "si"
            | "di"
            | "bp"
            | "sp"
            | "al"
            | "bl"
            | "cl"
            | "dl"
            | "sil"
            | "dil"
            | "bpl"
            | "spl"
            | "rip"
    )
}
