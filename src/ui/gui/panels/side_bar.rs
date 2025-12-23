//! Side Bar panel - Shows content based on active activity (Explorer, Debug, etc.)

use eframe::egui;
use crate::ui::gui::state::{AppState, Activity};
use crate::ui::gui::theme::catppuccin;
use super::functions::{self, FunctionAction};

/// Result from side bar render
pub enum SideBarAction {
    /// User selected a function
    SelectFunction(crate::analysis::loader::FunctionInfo),
    /// User requested function analysis
    AnalyzeFunctions,
}

/// Render the side bar panel.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> Option<SideBarAction> {
    let mut result = None;
    if !state.ui.sidebar_visible {
        return None;
    }

    egui::SidePanel::left("side_bar")
        .frame(egui::Frame::none().fill(catppuccin::BASE))
        .default_width(240.0)
        .min_width(150.0)
        .resizable(true)
        .show(ctx, |ui| {
            // Header
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let title = match state.ui.active_activity {
                    Activity::Explorer => "EXPLORER",
                    Activity::Search => "SEARCH",
                    Activity::Debug => "RUN AND DEBUG",
                    Activity::Settings => "SETTINGS",
                };
                ui.label(egui::RichText::new(title).size(11.0).strong().color(catppuccin::SUBTEXT0));
            });
            ui.add_space(4.0);
            
            // Content
            match state.ui.active_activity {
                Activity::Explorer => {
                    // Use the existing functions panel logic but as part of this panel
                    if let Some(action) = functions::render_inside(ui, state) {
                        result = Some(match action {
                            FunctionAction::Select(func) => SideBarAction::SelectFunction(func),
                            FunctionAction::Analyze => SideBarAction::AnalyzeFunctions,
                        });
                    }
                }
                Activity::Search => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new("Search functionality coming soon")
                            .color(catppuccin::OVERLAY0).small());
                    });
                }
                Activity::Debug => {
                    render_debug_sidebar(ui, state);
                }
                Activity::Settings => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(20.0);
                        ui.label(egui::RichText::new("Settings coming soon")
                            .color(catppuccin::OVERLAY0).small());
                    });
                }
            }
        });
    
    result
}

fn render_debug_sidebar(ui: &mut egui::Ui, state: &mut AppState) {
    // Basic debug info moved from bottom panel if desired, 
    // or specialized debug views like breakpoints list
    ui.add_space(8.0);
    ui.collapsing(egui::RichText::new("BREAKPOINTS").small().strong(), |ui| {
        // Breakpoint list (abbreviated)
        if state.debug.debug_state.breakpoints.is_empty() {
            ui.label(egui::RichText::new("No breakpoints").color(catppuccin::OVERLAY0).small());
        } else {
            for (addr, _bp) in &state.debug.debug_state.breakpoints {
                ui.label(egui::RichText::new(format!("0x{:016x}", addr)).small().monospace());
            }
        }
    });
}
