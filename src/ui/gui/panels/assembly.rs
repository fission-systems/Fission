//! Assembly view panel - displays disassembled instructions with virtual scrolling.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use super::super::state::AppState;
use super::super::theme::{catppuccin, code};

/// Render the assembly view in the central panel with virtualized scrolling.
pub fn render(ctx: &egui::Context, state: &AppState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Assembly").color(catppuccin::LAVENDER));
            ui.separator();
            ui.label(egui::RichText::new(format!("{} instructions", state.asm_instructions.len()))
                .color(catppuccin::SUBTEXT0)
                .small());
        });
        ui.separator();

        if state.asm_instructions.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(egui::RichText::new("No disassembly available")
                    .color(catppuccin::OVERLAY0)
                    .size(16.0));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Select a function to view assembly")
                    .color(catppuccin::OVERLAY0)
                    .small());
            });
            return;
        }

        let available_height = ui.available_height();
        let row_height = 20.0;
        let total_rows = state.asm_instructions.len();

        // Use TableBuilder for efficient virtual scrolling
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(90.0))  // Address
            .column(Column::initial(140.0).at_least(80.0))  // Bytes
            .column(Column::initial(80.0).at_least(50.0))   // Mnemonic
            .column(Column::remainder())  // Operands
            .min_scrolled_height(0.0)
            .max_scroll_height(available_height)
            .header(22.0, |mut header| {
                header.col(|ui| {
                    ui.label(egui::RichText::new("Address")
                        .strong()
                        .color(catppuccin::TEXT));
                });
                header.col(|ui| {
                    ui.label(egui::RichText::new("Bytes")
                        .strong()
                        .color(catppuccin::TEXT));
                });
                header.col(|ui| {
                    ui.label(egui::RichText::new("Mnemonic")
                        .strong()
                        .color(catppuccin::TEXT));
                });
                header.col(|ui| {
                    ui.label(egui::RichText::new("Operands")
                        .strong()
                        .color(catppuccin::TEXT));
                });
            })
            .body(|body| {
                body.rows(row_height, total_rows, |mut row| {
                    let row_index = row.index();
                    let insn = &state.asm_instructions[row_index];
                    
                    // Address column
                    row.col(|ui| {
                        ui.label(egui::RichText::new(format!("{:08X}", insn.address))
                            .color(code::ADDRESS)
                            .monospace());
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
                        ui.label(egui::RichText::new(bytes_str)
                            .color(code::HEX_BYTE)
                            .monospace());
                    });
                    
                    // Mnemonic column with color coding
                    row.col(|ui| {
                        let color = if insn.is_flow_control {
                            code::MNEMONIC_FLOW
                        } else {
                            code::MNEMONIC_NORMAL
                        };
                        ui.label(egui::RichText::new(&insn.mnemonic)
                            .color(color)
                            .strong()
                            .monospace());
                    });
                    
                    // Operands column with syntax highlighting
                    row.col(|ui| {
                        let text = highlight_operands(&insn.operands);
                        ui.label(text);
                    });
                });
            });
    });
}

/// Apply syntax highlighting to operands
fn highlight_operands(operands: &str) -> egui::RichText {
    // Simple highlighting - in a full implementation you'd parse and color each token
    // For now, color registers differently
    let color = if operands.contains("rax") || operands.contains("rbx") || 
                   operands.contains("rcx") || operands.contains("rdx") ||
                   operands.contains("rsi") || operands.contains("rdi") ||
                   operands.contains("rbp") || operands.contains("rsp") ||
                   operands.contains("eax") || operands.contains("ebx") {
        code::REGISTER
    } else if operands.starts_with("0x") || operands.contains("0x") {
        code::NUMBER
    } else {
        catppuccin::TEXT
    };
    
    egui::RichText::new(operands)
        .color(color)
        .monospace()
}
