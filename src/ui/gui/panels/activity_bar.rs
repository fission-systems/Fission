//! Activity Bar widget - Vertical bar on the left with activity icons.

use eframe::egui;
use crate::ui::gui::state::{AppState, Activity};
use crate::ui::gui::theme::catppuccin;

/// Render the vertical activity bar on the far left.
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    egui::SidePanel::left("activity_bar")
        .frame(egui::Frame::none().fill(catppuccin::MANTLE))
        .exact_width(48.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                
                // Explorer (Files/Functions)
                activity_button(ui, state, Activity::Explorer, "📁", "Explorer");
                ui.add_space(12.0);
                
                // Search
                activity_button(ui, state, Activity::Search, "🔍", "Search");
                ui.add_space(12.0);
                
                // Debug
                activity_button(ui, state, Activity::Debug, "🪲", "Run and Debug");
                ui.add_space(12.0);
                
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    // Settings at the bottom
                    activity_button(ui, state, Activity::Settings, "⚙", "Settings");
                });
            });
        });
}

fn activity_button(ui: &mut egui::Ui, state: &mut AppState, activity: Activity, icon: &str, tooltip: &str) {
    let is_active = state.active_activity == activity && state.sidebar_visible;
    
    let tint = if is_active {
        catppuccin::TEXT
    } else {
        catppuccin::OVERLAY1
    };
    
    let response = ui.add(
        egui::Button::new(egui::RichText::new(icon).size(24.0).color(tint))
            .frame(false)
            .min_size(egui::vec2(48.0, 40.0))
    );
    
    // Indicator line on the left
    if is_active {
        let rect = response.rect;
        ui.painter().line_segment(
            [egui::pos2(rect.left() + 2.0, rect.top() + 8.0), egui::pos2(rect.left() + 2.0, rect.bottom() - 8.0)],
            egui::Stroke::new(2.0, catppuccin::BLUE)
        );
    }
    
    if response.clicked() {
        if state.active_activity == activity && state.sidebar_visible {
            state.sidebar_visible = false;
        } else {
            state.active_activity = activity;
            state.sidebar_visible = true;
        }
    }
    
    response.on_hover_text(tooltip);
}

