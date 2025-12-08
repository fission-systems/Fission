//! Hex View tab panel - Binary hex dump viewer.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use crate::ui::gui::state::AppState;
use crate::ui::gui::theme::{catppuccin, code};

/// Render hex view tab content with virtual scrolling
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(ref binary) = state.loaded_binary else {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("No binary loaded")
                .color(catppuccin::OVERLAY0)
                .size(14.0));
        });
        return;
    };

    let data_len = binary.data.len() as u64;
    let rows_per_page = 64;
    let total_rows = (data_len / 16) + if data_len % 16 != 0 { 1 } else { 0 };
    
    // Controls
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Offset:").color(catppuccin::SUBTEXT0));
        let mut offset_str = format!("{:08X}", state.hex_offset);
        if ui.add(
            egui::TextEdit::singleline(&mut offset_str)
                .desired_width(80.0)
                .font(egui::TextStyle::Monospace)
        ).changed() {
            if let Ok(new_offset) = u64::from_str_radix(&offset_str, 16) {
                state.hex_offset = (new_offset / 16) * 16;
                state.hex_offset = state.hex_offset.min(data_len.saturating_sub(16));
            }
        }
        
        ui.separator();
        
        if ui.small_button("⬆").clicked() && state.hex_offset >= 0x100 {
            state.hex_offset -= 0x100;
        }
        if ui.small_button("⬇").clicked() {
            state.hex_offset = (state.hex_offset + 0x100).min(data_len.saturating_sub(16));
        }
        if ui.small_button("Top").clicked() {
            state.hex_offset = 0;
        }
        if ui.small_button("End").clicked() {
            state.hex_offset = (total_rows.saturating_sub(rows_per_page as u64)) * 16;
        }
        
        ui.separator();
        ui.label(egui::RichText::new(format!("{} / {} bytes", state.hex_offset, data_len))
            .color(catppuccin::SUBTEXT0).small());
    });

    ui.separator();

    let available_height = ui.available_height();
    let row_height = 18.0;
    
    // Calculate which rows to show based on current offset
    let start_row = (state.hex_offset / 16) as usize;
    let visible_rows = ((available_height / row_height) as usize).min(rows_per_page).max(8);
    let end_row = (start_row + visible_rows).min(total_rows as usize);
    let display_rows = end_row - start_row;

    // Use TableBuilder for virtual scrolling hex view
    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(75.0))   // Offset
        .column(Column::exact(380.0))  // Hex bytes
        .column(Column::remainder())   // ASCII
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height)
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.label(egui::RichText::new("Offset")
                    .strong().color(catppuccin::TEXT));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F")
                    .strong().color(catppuccin::TEXT).monospace());
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("ASCII")
                    .strong().color(catppuccin::TEXT));
            });
        })
        .body(|body| {
            body.rows(row_height, display_rows, |mut row| {
                let row_index = start_row + row.index();
                let row_offset = (row_index as u64) * 16;
                
                if row_offset >= data_len {
                    return;
                }
                
                // Offset column
                row.col(|ui| {
                    ui.label(egui::RichText::new(format!("{:08X}", row_offset))
                        .color(code::ADDRESS).monospace());
                });
                
                // Hex bytes column
                row.col(|ui| {
                    let mut hex_str = String::with_capacity(50);
                    let start = row_offset as usize;
                    let end = (row_offset + 16).min(data_len) as usize;
                    
                    if start < binary.data.len() {
                        let bytes = &binary.data[start..end.min(binary.data.len())];
                        for (i, byte) in bytes.iter().enumerate() {
                            use std::fmt::Write;
                            write!(hex_str, "{:02X} ", byte).unwrap();
                            if i == 7 { hex_str.push(' '); }
                        }
                        // Pad remaining
                        for i in bytes.len()..16 {
                            hex_str.push_str("   ");
                            if i == 7 { hex_str.push(' '); }
                        }
                    }
                    ui.label(egui::RichText::new(&hex_str)
                        .color(code::HEX_BYTE).monospace());
                });
                
                // ASCII column
                row.col(|ui| {
                    let mut ascii_str = String::with_capacity(16);
                    let start = row_offset as usize;
                    let end = (row_offset + 16).min(data_len) as usize;
                    
                    if start < binary.data.len() {
                        let bytes = &binary.data[start..end.min(binary.data.len())];
                        for byte in bytes {
                            ascii_str.push(if *byte >= 0x20 && *byte <= 0x7E { 
                                *byte as char 
                            } else {
                                '.' 
                            });
                        }
                    }
                    ui.label(egui::RichText::new(&ascii_str)
                        .color(code::ASCII_PRINTABLE).monospace());
                });
            });
        });
}

