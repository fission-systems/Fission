//! Strings tab panel - Extract and display strings from binary.

use crate::config::CONFIG;
use crate::ui::gui::state::{AppState, ExtractedString, StringEncoding};
use crate::ui::gui::theme::{catppuccin, code};
use crate::ui::gui::widgets::empty_state;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// Render strings tab content with virtual scrolling
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    // Controls
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter:").color(catppuccin::SUBTEXT0));
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.analysis.strings_filter)
                .desired_width(200.0)
                .hint_text("Search strings..."),
        );

        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui
            .button(egui::RichText::new("Extract").color(catppuccin::GREEN))
            .clicked()
            || enter_pressed
        {
            extract_strings_from_binary(state);
        }

        ui.separator();
        ui.label(
            egui::RichText::new(format!(
                "{} strings",
                state.analysis.extracted_strings.len()
            ))
            .color(catppuccin::SUBTEXT0)
            .small(),
        );
    });

    if state.analysis.extracted_strings.is_empty() {
        if state.analysis.loaded_binary.is_some() {
            empty_state(ui, "Click 'Extract' to find strings", None);
        } else {
            empty_state(ui, "Load a binary first", None);
        }
        return;
    }

    // Filter strings
    let filter = state.analysis.strings_filter.to_lowercase();
    let filtered_strings: Vec<_> = state
        .analysis
        .extracted_strings
        .iter()
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
            .column(Column::exact(75.0)) // Offset
            .column(Column::exact(50.0)) // Type
            .column(Column::remainder()) // String
            .min_scrolled_height(0.0)
            .max_scroll_height(available_height)
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.label(
                        egui::RichText::new("Offset")
                            .strong()
                            .color(catppuccin::TEXT),
                    );
                });
                header.col(|ui| {
                    ui.label(egui::RichText::new("Type").strong().color(catppuccin::TEXT));
                });
                header.col(|ui| {
                    ui.label(
                        egui::RichText::new("String")
                            .strong()
                            .color(catppuccin::TEXT),
                    );
                });
            })
            .body(|body| {
                body.rows(row_height, total_rows, |mut row| {
                    let s = &filtered_strings[row.index()];

                    row.col(|ui| {
                        let _ = ui.selectable_label(
                            false,
                            egui::RichText::new(format!("{:08X}", s.offset))
                                .monospace()
                                .color(code::ADDRESS),
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
                        ui.label(
                            egui::RichText::new(display_str)
                                .color(catppuccin::GREEN)
                                .monospace(),
                        );
                    });
                });
            });
    });
}

/// Extract strings from binary
///
/// Performance optimizations:
/// - Pre-allocates string buffer with estimated capacity to reduce reallocations
/// - Uses byte-level operations instead of char conversions where possible
/// - Estimates result vector capacity based on binary size heuristics
pub fn extract_strings_from_binary(state: &mut AppState) {
    state.analysis.extracted_strings.clear();

    let Some(ref binary) = state.analysis.loaded_binary else {
        return;
    };

    let min_len = CONFIG.analysis.min_string_length;
    let data = &binary.data;

    // Pre-allocate with estimated capacity (heuristic: ~1 string per 1KB of data)
    let estimated_strings = data.len() / 1024;
    state
        .analysis
        .extracted_strings
        .reserve(estimated_strings.max(100));

    // Pre-allocate string buffer with reasonable capacity to reduce reallocations
    // Most strings are < 256 bytes, but we allow growth if needed
    let mut current_bytes: Vec<u8> = Vec::with_capacity(256);
    let mut start_offset: u64 = 0;

    for (i, &byte) in data.iter().enumerate() {
        // Check if printable ASCII (0x20-0x7E)
        if byte >= 0x20 && byte <= 0x7E {
            if current_bytes.is_empty() {
                start_offset = i as u64;
            }
            current_bytes.push(byte);
        } else {
            if current_bytes.len() >= min_len {
                // SAFETY: We only pushed bytes in 0x20-0x7E range, which are valid ASCII/UTF-8
                // Use std::mem::take to avoid clone allocation
                let bytes = std::mem::take(&mut current_bytes);
                let value = unsafe { String::from_utf8_unchecked(bytes) };
                state.analysis.extracted_strings.push(ExtractedString {
                    offset: start_offset,
                    value,
                    encoding: StringEncoding::Ascii,
                });
                // Re-allocate with same capacity for next string
                current_bytes = Vec::with_capacity(256);
            } else {
                current_bytes.clear();
            }
        }
    }

    // Handle any remaining string at end of data
    if current_bytes.len() >= min_len {
        let value = unsafe { String::from_utf8_unchecked(current_bytes) };
        state.analysis.extracted_strings.push(ExtractedString {
            offset: start_offset,
            value,
            encoding: StringEncoding::Ascii,
        });
    }

    state.analysis.extracted_strings.sort_by_key(|s| s.offset);
    state.log_buffer.push(format!(
        "[✓] Extracted {} strings",
        state.analysis.extracted_strings.len()
    ));
}
