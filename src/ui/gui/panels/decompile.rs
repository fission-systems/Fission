//! Decompiled code panel - displays C-like decompiled output (fixed right panel).

use eframe::egui;
use super::super::state::AppState;

/// Render the decompiled code as a fixed right panel.
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    egui::SidePanel::right("decompile_panel")
        .resizable(true)
        .default_width(350.0)
        .min_width(200.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("[Decompiled Code]");
                if state.decompiling {
                    ui.spinner();
                }
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut state.decompiled_code.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(40)
                );
            });
        });
}
