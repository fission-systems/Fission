//! Console panel - log output and CLI input.

use eframe::egui;
use super::super::state::AppState;

/// Actions that can be triggered from the console panel
pub enum ConsoleAction {
    /// User entered a command
    Command(String),
    /// No action
    None,
}

/// Render the console panel at the bottom.
/// 
/// Returns any command entered by the user.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> ConsoleAction {
    let mut action = ConsoleAction::None;
    
    egui::TopBottomPanel::bottom("console_panel")
        .resizable(true)
        .default_height(150.0)
        .min_height(80.0)
        .max_height(300.0)
        .show(ctx, |ui| {
            // Header with buttons
            ui.horizontal(|ui| {
                ui.heading("[Console]");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Clear").clicked() {
                        state.log_buffer.clear();
                    }
                    if ui.small_button("📋 Copy All").clicked() {
                        let all_logs = state.log_buffer.join("\n");
                        ui.output_mut(|o| o.copied_text = all_logs);
                    }
                });
            });
            ui.separator();

            // Scrollable log area
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 30.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for log in &state.log_buffer {
                        // Color code log messages
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

            ui.separator();

            // CLI input at the bottom
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
                        state.log(format!("> {}", cmd));
                        action = ConsoleAction::Command(cmd);
                        state.cli_input.clear();
                    }
                    response.request_focus();
                }

                if ui.button("Run").clicked() {
                    let cmd = state.cli_input.trim().to_string();
                    if !cmd.is_empty() {
                        state.log(format!("> {}", cmd));
                        action = ConsoleAction::Command(cmd);
                        state.cli_input.clear();
                    }
                }
            });
        });
    
    action
}
