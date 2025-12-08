//! Assembly view panel - displays disassembled instructions.

use eframe::egui;
use super::super::state::AppState;

/// Render the assembly view in the central panel.
pub fn render(ctx: &egui::Context, state: &AppState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("[Assembly]");
        ui.separator();

        // Use ScrollArea for proper scrolling
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Simple grid-like layout with monospace text
                egui::Grid::new("asm_grid")
                    .num_columns(4)
                    .spacing([10.0, 2.0])
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        // Header
                        ui.label(egui::RichText::new("Address").strong());
                        ui.label(egui::RichText::new("Bytes").strong());
                        ui.label(egui::RichText::new("Mnemonic").strong());
                        ui.label(egui::RichText::new("Operands").strong());
                        ui.end_row();

                        // Instructions
                        for insn in &state.asm_instructions {
                            // Address
                            ui.label(egui::RichText::new(format!("{:08X}", insn.address))
                                .color(egui::Color32::GRAY)
                                .monospace());

                            // Bytes (truncate if too long)
                            let mut bytes_str = String::new();
                            for (i, b) in insn.bytes.iter().enumerate() {
                                if i >= 8 { 
                                    bytes_str.push_str(".."); 
                                    break; 
                                }
                                use std::fmt::Write;
                                write!(bytes_str, "{:02X} ", b).unwrap();
                            }
                            ui.label(egui::RichText::new(bytes_str)
                                .color(egui::Color32::from_rgb(100, 100, 100))
                                .monospace());

                            // Mnemonic with color coding
                            let color = if insn.is_flow_control {
                                egui::Color32::from_rgb(255, 100, 100) // Red for jumps/calls
                            } else {
                                egui::Color32::from_rgb(100, 150, 255) // Blue for instructions
                            };
                            ui.label(egui::RichText::new(&insn.mnemonic)
                                .color(color)
                                .strong()
                                .monospace());

                            // Operands
                            ui.label(egui::RichText::new(&insn.operands)
                                .color(egui::Color32::WHITE)
                                .monospace());

                            ui.end_row();
                        }
                    });
            });
    });
}
