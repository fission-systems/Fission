//! Hex View tab panel - Binary hex dump viewer with patching support.

use crate::analysis::patch::QuickPatch;
use crate::ui::gui::components::widgets::empty_state;
use crate::ui::gui::core::state::AppState;
use crate::ui::gui::theme::{catppuccin, code};
use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// Pending patch action to apply after UI rendering
enum PatchAction {
    None,
    ApplyBytes { offset: u64, bytes: Vec<u8> },
    QuickPatch { offset: u64, patch_type: QuickPatch },
    SaveAs,
}

/// Render hex view tab content with virtual scrolling
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    // Check if binary is loaded and get data length
    let (data_len, total_rows) = if let Some(ref binary) = state.analysis.domain.loaded_binary {
        let len = binary.data.len() as u64;
        let rows = (len / 16) + if len.is_multiple_of(16) { 0 } else { 1 };
        (len, rows)
    } else {
        empty_state(ui, "No binary loaded", None);
        return;
    };

    // Navigation Controls
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Go to Offset:").color(catppuccin::SUBTEXT0));
        let mut offset_str = format!("{:08X}", state.analysis.domain.hex_offset);
        if ui
            .add(
                egui::TextEdit::singleline(&mut offset_str)
                    .desired_width(80.0)
                    .font(egui::TextStyle::Monospace),
            )
            .lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Ok(new_offset) = u64::from_str_radix(&offset_str, 16)
        {
            state.analysis.domain.hex_offset = (new_offset as usize / 16) * 16;
            state.analysis.domain.hex_offset = state
                .analysis
                .domain
                .hex_offset
                .min((data_len.saturating_sub(16)) as usize);
            // Note: To scroll to this offset programmatically with TableBuilder requires scroll_to_row
            // which might need specific egui context handling or scroll area wrapping.
            // For now, updating the state is a start, but the table needs to read it.
            // This hybrid approach (native scroll + jump) needs care.
            // Simplified: We just update the display logic below to respect scroll OR jump?
            // Actually, native scroll means 'scroll' is the truth.
            // 'Jump' needs to force scroll.
        }

        ui.separator();
        ui.label(
            egui::RichText::new(format!("Total: {} bytes", data_len))
                .color(catppuccin::SUBTEXT0)
                .small(),
        );
    });

    // Patch Controls - collect action first, apply later
    let mut patch_action = PatchAction::None;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("📝 Patch:").color(catppuccin::MAUVE));

        // Patch offset input
        ui.label("@");
        ui.add(
            egui::TextEdit::singleline(&mut state.viewmodels.hex.patch_offset_input)
                .desired_width(70.0)
                .hint_text("offset")
                .font(egui::TextStyle::Monospace),
        );

        // Patch bytes input
        ui.label("→");
        ui.add(
            egui::TextEdit::singleline(&mut state.viewmodels.hex.patch_bytes_input)
                .desired_width(100.0)
                .hint_text("90 90 90")
                .font(egui::TextStyle::Monospace),
        );

        // Patch button
        if ui.button("Apply").clicked()
            && let Ok(offset) = u64::from_str_radix(
                state
                    .viewmodels
                    .hex
                    .patch_offset_input
                    .trim()
                    .trim_start_matches("0x"),
                16,
            )
        {
            let bytes: Vec<u8> = state
                .viewmodels
                .hex
                .patch_bytes_input
                .split_whitespace()
                .filter_map(|s| u8::from_str_radix(s, 16).ok())
                .collect();

            if !bytes.is_empty() {
                patch_action = PatchAction::ApplyBytes { offset, bytes };
            }
        }

        ui.separator();

        // Quick patches dropdown
        egui::ComboBox::from_id_salt("quick_patch")
            .selected_text("Quick Patch")
            .width(100.0)
            .show_ui(ui, |ui| {
                let offset_result = u64::from_str_radix(
                    state
                        .viewmodels
                        .hex
                        .patch_offset_input
                        .trim()
                        .trim_start_matches("0x"),
                    16,
                );

                if let Ok(offset) = offset_result {
                    if ui.selectable_label(false, "NOP (0x90)").clicked() {
                        patch_action = PatchAction::QuickPatch {
                            offset,
                            patch_type: QuickPatch::Nop,
                        };
                    }
                    if ui.selectable_label(false, "JE→JNE (0x74→0x75)").clicked() {
                        patch_action = PatchAction::QuickPatch {
                            offset,
                            patch_type: QuickPatch::JeToJne,
                        };
                    }
                    if ui.selectable_label(false, "JNE→JE (0x75→0x74)").clicked() {
                        patch_action = PatchAction::QuickPatch {
                            offset,
                            patch_type: QuickPatch::JneToJe,
                        };
                    }
                    if ui.selectable_label(false, "JMP short (0xEB)").clicked() {
                        patch_action = PatchAction::QuickPatch {
                            offset,
                            patch_type: QuickPatch::JmpShort,
                        };
                    }
                    if ui.selectable_label(false, "RET (0xC3)").clicked() {
                        patch_action = PatchAction::QuickPatch {
                            offset,
                            patch_type: QuickPatch::Ret,
                        };
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Enter offset first")
                            .color(catppuccin::OVERLAY0)
                            .small(),
                    );
                }
            });

        ui.separator();

        // Save button
        if ui
            .button(egui::RichText::new("💾 Save As...").color(catppuccin::GREEN))
            .clicked()
        {
            patch_action = PatchAction::SaveAs;
        }
    });

    // Apply patch action - use Arc::make_mut for mutable access
    match patch_action {
        PatchAction::ApplyBytes { offset, bytes } => {
            if let Some(ref mut binary_arc) = state.analysis.domain.loaded_binary {
                let binary = std::sync::Arc::make_mut(binary_arc);
                if binary.patch_bytes(offset, &bytes).is_some() {
                    state.log(format!(
                        "✅ Patched {} bytes at 0x{:X}",
                        bytes.len(),
                        offset
                    ));
                    state.viewmodels.hex.patch_bytes_input.clear();
                } else {
                    state.log(format!("❌ Patch failed: invalid offset 0x{:X}", offset));
                }
            }
        }
        PatchAction::QuickPatch { offset, patch_type } => {
            if let Some(ref mut binary_arc) = state.analysis.domain.loaded_binary {
                let binary = std::sync::Arc::make_mut(binary_arc);
                let bytes = patch_type.bytes();
                if binary.patch_bytes(offset, &bytes).is_some() {
                    state.log(format!(
                        "✅ Applied {} at 0x{:X}",
                        patch_type.description(),
                        offset
                    ));
                }
            }
        }
        PatchAction::SaveAs => {
            if let Some(ref binary) = state.analysis.domain.loaded_binary {
                let original_path = std::path::Path::new(&binary.path);
                let stem = original_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("binary");
                let ext = original_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("exe");
                let patched_name = format!("{}_patched.{}", stem, ext);
                let output_path = original_path.with_file_name(&patched_name);

                match binary.save_as(&output_path) {
                    Ok(()) => state.log(format!(
                        "💾 Saved patched binary to: {}",
                        output_path.display()
                    )),
                    Err(e) => state.log(format!("❌ Save failed: {}", e)),
                }
            }
        }
        PatchAction::None => {}
    }

    ui.separator();

    let available_height = ui.available_height();
    let row_height = 18.0;

    // Get binary data reference for table
    let Some(ref binary) = state.analysis.domain.loaded_binary else {
        return;
    };

    // Use TableBuilder for efficient virtual scrolling
    let mut builder = TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(75.0)) // Offset
        .column(Column::exact(380.0)) // Hex bytes
        .column(Column::remainder()) // ASCII
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height);

    // Handle programatic scroll jump if offset changed significantly
    // (This is a simplified approach; proper scroll control requires egui::ScrollArea::from_id_source handling)
    if state.analysis.domain.hex_offset > 0 {
        let row = (state.analysis.domain.hex_offset / 16) as usize;
        builder = builder.scroll_to_row(row, Some(egui::Align::Center));
        // Reset so we don't lock scrolling
        state.analysis.domain.hex_offset = 0;
    }

    builder
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.label(
                    egui::RichText::new("Offset")
                        .strong()
                        .color(catppuccin::TEXT),
                );
            });
            header.col(|ui| {
                ui.label(
                    egui::RichText::new("00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F")
                        .strong()
                        .color(catppuccin::TEXT)
                        .monospace(),
                );
            });
            header.col(|ui| {
                ui.label(
                    egui::RichText::new("ASCII")
                        .strong()
                        .color(catppuccin::TEXT),
                );
            });
        })
        .body(|body| {
            body.rows(row_height, total_rows as usize, |mut row| {
                let row_index = row.index();
                let row_offset = (row_index as u64) * 16;

                if row_offset >= data_len {
                    return;
                }

                // Offset column
                row.col(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{:08X}", row_offset))
                            .color(code::ADDRESS)
                            .monospace(),
                    );
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
                            if i == 7 {
                                hex_str.push(' ');
                            }
                        }
                        // Pad remaining
                        for i in bytes.len()..16 {
                            hex_str.push_str("   ");
                            if i == 7 {
                                hex_str.push(' ');
                            }
                        }
                    }
                    ui.label(
                        egui::RichText::new(&hex_str)
                            .color(code::HEX_BYTE)
                            .monospace(),
                    );
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
                    ui.label(
                        egui::RichText::new(&ascii_str)
                            .color(code::ASCII_PRINTABLE)
                            .monospace(),
                    );
                });
            });
        });
}
