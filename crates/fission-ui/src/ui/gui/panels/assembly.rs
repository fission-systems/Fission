//! Assembly view panel - displays disassembled instructions with virtual scrolling.

use super::super::components::widgets::empty_state_with_spacing;
use super::super::core::state::AppState;
use super::super::theme::{catppuccin, code};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// Render the assembly view in the central panel with virtualized scrolling.
#[allow(dead_code)]
pub fn render(ctx: &egui::Context, state: &AppState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        render_inside(ui, state);
    });
}

/// Render assembly view inside an existing UI.
pub fn render_inside(ui: &mut egui::Ui, state: &AppState) {
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
        .column(Column::initial(140.0).at_least(80.0)) // Bytes
        .column(Column::initial(80.0).at_least(50.0)) // Mnemonic
        .column(Column::remainder()) // Operands
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

                // Mnemonic column with color coding
                row.col(|ui| {
                    let color = if insn.is_flow_control {
                        code::MNEMONIC_FLOW
                    } else {
                        code::MNEMONIC_NORMAL
                    };
                    ui.label(
                        egui::RichText::new(&insn.mnemonic)
                            .color(color)
                            .strong()
                            .monospace(),
                    );
                });

                // Operands column with syntax highlighting
                row.col(|ui| {
                    let text = highlight_operands(&insn.operands);
                    ui.label(text);
                });
            });
        });
}

/// Apply syntax highlighting to operands
fn highlight_operands(operands: &str) -> egui::RichText {
    // Optimized highlighting: Check tokens for registers or numbers
    // This is faster and more accurate than "contains" which hits false positives
    let mut is_reg = false;
    let mut is_num = false;

    // Quick check using split iterator logic without allocating
    for token in operands.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if token.is_empty() {
            continue;
        }

        if matches!(
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
        ) {
            is_reg = true;
            break; // Prioritize register color
        }

        if token.starts_with("0x") || token.chars().all(|c| c.is_ascii_digit()) {
            is_num = true;
        }
    }

    let color = if is_reg {
        code::REGISTER
    } else if is_num {
        code::NUMBER
    } else {
        catppuccin::TEXT
    };

    egui::RichText::new(operands).color(color).monospace()
}
