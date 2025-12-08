//! Bottom tabbed panel - Console, Hex View, Strings, Imports, Debug tabs.
//!
//! This module organizes the bottom panel into separate sub-modules for each tab.

mod console;
mod debug;
mod hexview;
mod imports;
mod strings;

use eframe::egui;
use crate::ui::gui::state::{AppState, BottomTab};
use crate::ui::gui::theme::catppuccin;

// Re-export ConsoleAction for external use
pub use console::ConsoleAction;

/// Render the bottom tabbed panel.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> ConsoleAction {
    let mut action = ConsoleAction::None;
    
    egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(true)
        .default_height(200.0)
        .min_height(120.0)
        .max_height(500.0)
        .show(ctx, |ui| {
            // Force minimum height to prevent panel collapse
            ui.set_min_height(ui.available_height());
            
            // Tab bar with styled tabs
            ui.horizontal(|ui| {
                let tabs = [
                    (BottomTab::Console, "Console", catppuccin::BLUE),
                    (BottomTab::HexView, "Hex View", catppuccin::PEACH),
                    (BottomTab::Strings, "Strings", catppuccin::GREEN),
                    (BottomTab::Imports, "Imports", catppuccin::MAUVE),
                    (BottomTab::Debug, "Debug", catppuccin::RED),
                ];
                
                for (tab, label, accent) in tabs {
                    let is_selected = state.bottom_tab == tab;
                    let text = if is_selected {
                        egui::RichText::new(label).color(accent).strong()
                    } else {
                        egui::RichText::new(label).color(catppuccin::SUBTEXT0)
                    };
                    if ui.selectable_label(is_selected, text).clicked() {
                        state.bottom_tab = tab;
                    }
                }
            });
            ui.separator();

            // Tab content - allocate remaining space to prevent collapse
            let content_rect = ui.available_rect_before_wrap();
            ui.allocate_ui_at_rect(content_rect, |ui| {
                match state.bottom_tab {
                    BottomTab::Console => {
                        action = console::render(ui, state);
                    }
                    BottomTab::HexView => {
                        hexview::render(ui, state);
                    }
                    BottomTab::Strings => {
                        strings::render(ui, state);
                    }
                    BottomTab::Imports => {
                        imports::render(ui, state);
                    }
                    BottomTab::Debug => {
                        debug::render(ui, state);
                    }
                }
            });
        });
    
    action
}

