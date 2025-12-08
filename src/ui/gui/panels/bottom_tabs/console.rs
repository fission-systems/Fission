//! Console tab panel - Command input and log output.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use crate::ui::gui::state::AppState;
use crate::ui::gui::theme::catppuccin;

/// Actions that can be triggered from the console
pub enum ConsoleAction {
    Command(String),
    None,
}

/// Render console tab content using TableBuilder for stable scrolling
pub fn render(ui: &mut egui::Ui, state: &mut AppState) -> ConsoleAction {
    let mut action = ConsoleAction::None;
    
    // Header buttons
    ui.horizontal(|ui| {
        if ui.small_button(egui::RichText::new("Clear").color(catppuccin::RED)).clicked() {
            state.log_buffer.clear();
        }
        if ui.small_button(egui::RichText::new("📋 Copy").color(catppuccin::BLUE)).clicked() {
            let all_logs = state.log_buffer.join("\n");
            ui.output_mut(|o| o.copied_text = all_logs);
        }
        ui.separator();
        ui.label(egui::RichText::new(format!("{} lines", state.log_buffer.len()))
            .color(catppuccin::SUBTEXT0).small());
    });

    // Virtual scrolling table for console logs
    let available_height = ui.available_height() - 35.0;
    let num_logs = state.log_buffer.len();
    
    TableBuilder::new(ui)
        .striped(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder())
        .min_scrolled_height(available_height.max(50.0))
        .max_scroll_height(available_height.max(50.0))
        .body(|body| {
            body.rows(16.0, num_logs, |mut row| {
                let idx = row.index();
                row.col(|ui| {
                    if let Some(log) = state.log_buffer.get(idx) {
                        let color = get_log_color(log);
                        ui.label(egui::RichText::new(log).color(color).monospace());
                    }
                });
            });
        });

    // CLI input
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(">").color(catppuccin::MAUVE).strong());
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

        if ui.button(egui::RichText::new("Run").color(catppuccin::GREEN)).clicked() {
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

fn get_log_color(log: &str) -> egui::Color32 {
    if log.starts_with("[✓]") {
        catppuccin::GREEN
    } else if log.starts_with("[✗]") || log.starts_with("[!]") {
        catppuccin::RED
    } else if log.starts_with("[*]") || log.starts_with("[>]") {
        catppuccin::BLUE
    } else if log.starts_with(">") {
        catppuccin::MAUVE
    } else {
        catppuccin::SUBTEXT0
    }
}

