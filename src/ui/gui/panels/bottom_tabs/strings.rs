//! Strings tab panel - Extract and display strings from binary.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use crate::ui::gui::state::{AppState, ExtractedString, StringEncoding};
use crate::ui::gui::theme::{catppuccin, code};

/// Render strings tab content with virtual scrolling
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    // Controls
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter:").color(catppuccin::SUBTEXT0));
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.strings_filter)
                .desired_width(200.0)
                .hint_text("Search strings...")
        );
        
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button(egui::RichText::new("Extract").color(catppuccin::GREEN)).clicked() || enter_pressed {
            extract_strings_from_binary(state);
        }
        
        ui.separator();
        ui.label(egui::RichText::new(format!("{} strings", state.extracted_strings.len()))
            .color(catppuccin::SUBTEXT0).small());
    });

    if state.extracted_strings.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            if state.loaded_binary.is_some() {
                ui.label(egui::RichText::new("Click 'Extract' to find strings")
                    .color(catppuccin::OVERLAY0));
            } else {
                ui.label(egui::RichText::new("Load a binary first")
                    .color(catppuccin::OVERLAY0));
            }
        });
        return;
    }

    // Filter strings
    let filter = state.strings_filter.to_lowercase();
    let filtered_strings: Vec<_> = state.extracted_strings.iter()
        .filter(|s| filter.is_empty() || s.value.to_lowercase().contains(&filter))
        .collect();

    let available_height = ui.available_height();
    let row_height = 20.0;
    let total_rows = filtered_strings.len();

    // Virtual scrolling table for strings
    ui.push_id("strings_table", |ui| {
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(75.0))   // Offset
        .column(Column::exact(50.0))   // Type
        .column(Column::remainder())   // String
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height)
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.label(egui::RichText::new("Offset").strong().color(catppuccin::TEXT));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Type").strong().color(catppuccin::TEXT));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("String").strong().color(catppuccin::TEXT));
            });
        })
        .body(|body| {
            body.rows(row_height, total_rows, |mut row| {
                let s = &filtered_strings[row.index()];
                
                row.col(|ui| {
                    let _ = ui.selectable_label(false, 
                        egui::RichText::new(format!("{:08X}", s.offset))
                            .monospace().color(code::ADDRESS)
                    );
                });
                
                row.col(|ui| {
                    let (type_str, color) = match s.encoding {
                        StringEncoding::Ascii => ("ASCII", catppuccin::BLUE),
                        StringEncoding::Utf16Le => ("UTF16", catppuccin::MAUVE),
                    };
                    ui.label(egui::RichText::new(type_str).color(color).small());
                });
                
                row.col(|ui| {
                    let display_str = if s.value.len() > 80 {
                        format!("{}...", &s.value[..80])
                    } else {
                        s.value.clone()
                    };
                    ui.label(egui::RichText::new(display_str)
                        .color(catppuccin::GREEN).monospace());
                });
            });
        });
    });
}

/// Extract strings from binary
pub fn extract_strings_from_binary(state: &mut AppState) {
    state.extracted_strings.clear();
    
    let Some(ref binary) = state.loaded_binary else { return; };
    
    let min_len = 4;
    let data = &binary.data;
    
    // ASCII strings
    let mut current_string = String::new();
    let mut start_offset: u64 = 0;
    
    for (i, &byte) in data.iter().enumerate() {
        if byte >= 0x20 && byte <= 0x7E {
            if current_string.is_empty() { start_offset = i as u64; }
            current_string.push(byte as char);
        } else {
            if current_string.len() >= min_len {
                state.extracted_strings.push(ExtractedString {
                    offset: start_offset,
                    value: current_string.clone(),
                    encoding: StringEncoding::Ascii,
                });
            }
            current_string.clear();
        }
    }
    
    state.extracted_strings.sort_by_key(|s| s.offset);
    state.log_buffer.push(format!("[✓] Extracted {} strings", state.extracted_strings.len()));
}

