//! Status bar rendering at the bottom of the window.

use eframe::egui;
use super::state::AppState;

/// Render the status bar at the very bottom.
pub fn render(ctx: &egui::Context, state: &AppState) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Server status (with recovery indicator)
            let (server_color, server_text) = if state.recovering {
                (egui::Color32::YELLOW, "🔄 Recovering...")
            } else if state.server_connected {
                (egui::Color32::from_rgb(100, 200, 100), "⚡ Server")
            } else {
                (egui::Color32::from_rgb(150, 150, 150), "🔌 Offline")
            };
            ui.colored_label(server_color, server_text);
            
            ui.separator();

            // Debugger status indicator
            let (status_color, status_text) = if state.is_debugging {
                (egui::Color32::from_rgb(100, 200, 100), "[*] DEBUGGING")
            } else {
                (egui::Color32::from_rgb(150, 150, 150), "[ ] IDLE")
            };
            ui.colored_label(status_color, status_text);

            ui.separator();

            // Loaded binary info
            if let Some(ref binary) = state.loaded_binary {
                ui.label(format!("File: {} | {} functions", binary.path, binary.functions.len()));
            } else {
                ui.label("No binary loaded");
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("Fission v0.1.0");
                ui.separator();
                ui.label(format!("Cache: {}", state.decompile_cache.len()));
            });
        });
    });
}
