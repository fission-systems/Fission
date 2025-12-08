//! Status bar rendering at the bottom of the window.

use eframe::egui;
use super::state::AppState;
use super::theme::catppuccin;

/// Render the status bar at the very bottom.
pub fn render(ctx: &egui::Context, state: &AppState) {
    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(24.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Server status (with recovery indicator)
                let (server_color, server_icon, server_text) = if state.recovering {
                    (catppuccin::YELLOW, "◉", "Recovering")
                } else if state.server_connected {
                    (catppuccin::GREEN, "●", "Connected")
                } else {
                    (catppuccin::OVERLAY0, "○", "Offline")
                };
                ui.label(egui::RichText::new(server_icon).color(server_color).small());
                ui.label(egui::RichText::new(server_text).color(server_color).small());
                
                ui.separator();

                // Debugger status indicator
                let (debug_color, debug_icon, debug_text) = if state.is_debugging {
                    (catppuccin::GREEN, "▶", "Debugging")
                } else {
                    (catppuccin::OVERLAY0, "■", "Idle")
                };
                ui.label(egui::RichText::new(debug_icon).color(debug_color).small());
                ui.label(egui::RichText::new(debug_text).color(debug_color).small());

                // Mode indicator
                ui.separator();
                if state.dynamic_mode {
                    ui.label(egui::RichText::new("● Dynamic").color(catppuccin::TEAL).small());
                } else {
                    ui.label(egui::RichText::new("○ Static").color(catppuccin::OVERLAY0).small());
                }

                ui.separator();

                // Loaded binary info
                if let Some(ref binary) = state.loaded_binary {
                    let arch = if binary.is_64bit { "x64" } else { "x86" };
                    ui.label(egui::RichText::new(format!("{} | {} | {} funcs", 
                        truncate_path(&binary.path, 30), arch, binary.functions.len()))
                        .color(catppuccin::SUBTEXT0).small());
                } else {
                    ui.label(egui::RichText::new("No binary").color(catppuccin::OVERLAY0).small());
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("Fission v0.1.0")
                        .color(catppuccin::LAVENDER).small());
                    ui.separator();
                    ui.label(egui::RichText::new(format!("Cache: {}", state.decompile_cache.len()))
                        .color(catppuccin::SUBTEXT0).small());
                });
            });
        });
}

/// Truncate a path for display
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        // Try to show the filename
        if let Some(filename) = std::path::Path::new(path).file_name() {
            if let Some(name) = filename.to_str() {
                if name.len() <= max_len {
                    return name.to_string();
                }
            }
        }
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}
