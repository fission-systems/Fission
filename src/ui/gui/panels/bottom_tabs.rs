//! Bottom tabbed panel - Console, Hex View, Strings tabs.

use eframe::egui;
use super::super::state::{AppState, BottomTab, ExtractedString, StringEncoding};

/// Actions that can be triggered from the console
pub enum ConsoleAction {
    Command(String),
    None,
}

/// Render the bottom tabbed panel.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> ConsoleAction {
    let mut action = ConsoleAction::None;
    
    egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(true)
        .default_height(180.0)
        .min_height(100.0)
        .max_height(400.0)
        .show(ctx, |ui| {
            // Tab bar
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.bottom_tab, BottomTab::Console, "Console");
                ui.selectable_value(&mut state.bottom_tab, BottomTab::HexView, "Hex View");
                ui.selectable_value(&mut state.bottom_tab, BottomTab::Strings, "Strings");
            });
            ui.separator();

            // Tab content
            match state.bottom_tab {
                BottomTab::Console => {
                    action = render_console(ui, state);
                }
                BottomTab::HexView => {
                    render_hexview(ui, state);
                }
                BottomTab::Strings => {
                    render_strings(ui, state);
                }
            }
        });
    
    action
}

/// Render console tab content
fn render_console(ui: &mut egui::Ui, state: &mut AppState) -> ConsoleAction {
    let mut action = ConsoleAction::None;
    
    // Header buttons
    ui.horizontal(|ui| {
        if ui.small_button("Clear").clicked() {
            state.log_buffer.clear();
        }
        if ui.small_button("📋 Copy All").clicked() {
            let all_logs = state.log_buffer.join("\n");
            ui.output_mut(|o| o.copied_text = all_logs);
        }
    });

    // Scrollable log area
    let available_height = ui.available_height() - 30.0;
    egui::ScrollArea::vertical()
        .max_height(available_height.max(50.0))
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for log in &state.log_buffer {
                let color = if log.starts_with("[✓]") {
                    egui::Color32::from_rgb(100, 200, 100)
                } else if log.starts_with("[✗]") || log.starts_with("[!]") {
                    egui::Color32::from_rgb(255, 100, 100)
                } else if log.starts_with("[*]") || log.starts_with("[>]") {
                    egui::Color32::from_rgb(100, 150, 255)
                } else {
                    egui::Color32::GRAY
                };
                ui.colored_label(color, log);
            }
        });

    // CLI input
    ui.horizontal(|ui| {
        ui.label(">");
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.cli_input)
                .desired_width(ui.available_width() - 60.0)
                .font(egui::TextStyle::Monospace)
                .hint_text("Enter command..."),
        );

        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let cmd = state.cli_input.trim().to_string();
            if !cmd.is_empty() {
                state.log_buffer.push(format!("> {}", cmd));
                action = ConsoleAction::Command(cmd);
                state.cli_input.clear();
            }
            response.request_focus();
        }

        if ui.button("Run").clicked() {
            let cmd = state.cli_input.trim().to_string();
            if !cmd.is_empty() {
                state.log_buffer.push(format!("> {}", cmd));
                action = ConsoleAction::Command(cmd);
                state.cli_input.clear();
            }
        }
    });
    
    action
}

/// Render hex view tab content
fn render_hexview(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(ref binary) = state.loaded_binary else {
        ui.label("No binary loaded");
        return;
    };

    let data_len = binary.data.len() as u64;
    
    // Controls
    ui.horizontal(|ui| {
        ui.label("File Offset:");
        let mut offset_str = format!("{:X}", state.hex_offset);
        if ui.add(
            egui::TextEdit::singleline(&mut offset_str)
                .desired_width(100.0)
                .font(egui::TextStyle::Monospace)
        ).changed() {
            if let Ok(new_offset) = u64::from_str_radix(&offset_str, 16) {
                state.hex_offset = new_offset.min(data_len.saturating_sub(1));
            }
        }
        
        if ui.button("⬆").clicked() && state.hex_offset >= 0x100 {
            state.hex_offset -= 0x100;
        }
        if ui.button("⬇").clicked() {
            state.hex_offset = (state.hex_offset + 0x100).min(data_len.saturating_sub(1));
        }
        if ui.button("⬆⬆ Top").clicked() {
            state.hex_offset = 0;
        }
        if ui.button("⬇⬇ End").clicked() {
            state.hex_offset = data_len.saturating_sub(256);
        }
        
        ui.label(format!("/ {:X} ({} bytes)", data_len, data_len));
    });

    // Hex grid with scroll
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("hex_grid")
                .num_columns(3)
                .spacing([15.0, 2.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Offset").strong().monospace());
                    ui.label(egui::RichText::new("00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F").strong().monospace());
                    ui.label(egui::RichText::new("ASCII").strong().monospace());
                    ui.end_row();

                    // Show 16 rows
                    for row in 0u64..16 {
                        let row_offset = state.hex_offset + (row * 16);
                        if row_offset >= data_len {
                            break;
                        }
                        
                        ui.label(egui::RichText::new(format!("{:08X}", row_offset))
                            .color(egui::Color32::GRAY).monospace());
                        
                        let mut hex_str = String::with_capacity(50);
                        let mut ascii_str = String::with_capacity(16);
                        
                        // Direct access to binary data using file offset
                        let start = row_offset as usize;
                        let end = (row_offset + 16).min(data_len) as usize;
                        
                        if start < binary.data.len() {
                            let bytes = &binary.data[start..end.min(binary.data.len())];
                            for (i, byte) in bytes.iter().enumerate() {
                                use std::fmt::Write;
                                write!(hex_str, "{:02X} ", byte).unwrap();
                                if i == 7 { hex_str.push(' '); }
                                ascii_str.push(if *byte >= 0x20 && *byte <= 0x7E { *byte as char } else { '.' });
                            }
                            // Pad remaining
                            for i in bytes.len()..16 {
                                hex_str.push_str("   ");
                                if i == 7 { hex_str.push(' '); }
                                ascii_str.push(' ');
                            }
                        } else {
                            hex_str = "-- -- -- -- -- -- -- --  -- -- -- -- -- -- -- --".into();
                            ascii_str = "................".into();
                        }
                        
                        ui.label(egui::RichText::new(&hex_str).color(egui::Color32::from_rgb(200, 200, 200)).monospace());
                        ui.label(egui::RichText::new(&ascii_str).color(egui::Color32::from_rgb(100, 200, 100)).monospace());
                        ui.end_row();
                    }
                });
        });
}

/// Render strings tab content
fn render_strings(ui: &mut egui::Ui, state: &mut AppState) {
    // Controls
    ui.horizontal(|ui| {
        ui.label("Filter:");
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.strings_filter)
                .desired_width(200.0)
                .hint_text("Search strings...")
        );
        
        // Extract on button click OR Enter key
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Extract").clicked() || enter_pressed {
            extract_strings_from_binary(state);
        }
        
        ui.label(format!("({} strings)", state.extracted_strings.len()));
    });

    if state.extracted_strings.is_empty() {
        if state.loaded_binary.is_some() {
            ui.label("Click 'Extract' to find strings.");
        } else {
            ui.label("Load a binary first.");
        }
        return;
    }

    // Strings grid
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("strings_grid")
            .num_columns(3)
            .spacing([10.0, 2.0])
            .striped(true)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Offset").strong());
                ui.label(egui::RichText::new("Type").strong());
                ui.label(egui::RichText::new("String").strong());
                ui.end_row();

                let filter = state.strings_filter.to_lowercase();
                
                for s in &state.extracted_strings {
                    if !filter.is_empty() && !s.value.to_lowercase().contains(&filter) {
                        continue;
                    }
                    
                    if ui.selectable_label(false, 
                        egui::RichText::new(format!("{:08X}", s.offset)).monospace().color(egui::Color32::GRAY)
                    ).clicked() {
                        state.hex_offset = s.offset;
                        state.bottom_tab = BottomTab::HexView;
                    }
                    
                    let type_str = match s.encoding {
                        StringEncoding::Ascii => "ASCII",
                        StringEncoding::Utf16Le => "UTF16",
                    };
                    ui.label(egui::RichText::new(type_str).color(egui::Color32::from_rgb(150, 150, 200)));
                    
                    let display_str = if s.value.len() > 60 {
                        format!("{}...", &s.value[..60])
                    } else {
                        s.value.clone()
                    };
                    ui.label(egui::RichText::new(display_str).color(egui::Color32::from_rgb(100, 200, 100)));
                    
                    ui.end_row();
                }
            });
    });
}

/// Extract strings from binary
fn extract_strings_from_binary(state: &mut AppState) {
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
    state.log_buffer.push(format!("[*] Extracted {} strings", state.extracted_strings.len()));
}
