//! Bottom tabbed panel - Console, Hex View, Strings, Imports, Debug, Script, Timeline, Plugins, CFG tabs.
//!
//! This module organizes the bottom panel into separate sub-modules for each tab.

pub mod cfg;
mod console;
mod debug;
mod hexview;
mod imports;
// Plugin management is now only in the left sidebar (Activity::Plugins)
// Keep this module for PluginPanelState export
pub(crate) mod plugins;
mod script;
mod strings;
mod timeline;

use crate::ui::gui::core::state::{AppState, BottomTab};
use crate::ui::gui::theme::catppuccin;
use eframe::egui;

// Re-export actions for external use
pub use cfg::CfgAction;
pub use console::ConsoleAction;
pub use script::ScriptAction;

/// Render the bottom tabbed panel.
pub fn render(
    ctx: &egui::Context,
    state: &mut AppState,
) -> (ConsoleAction, ScriptAction, CfgAction) {
    let mut console_action = ConsoleAction::None;
    let mut script_action = ScriptAction::None;
    let mut cfg_action = CfgAction::None;

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
                    (BottomTab::Cfg, "CFG", catppuccin::FLAMINGO),
                    (BottomTab::Debug, "Debug", catppuccin::RED),
                    (BottomTab::Script, "Script", catppuccin::YELLOW),
                    (BottomTab::Timeline, "Timeline", catppuccin::TEAL),
                ];

                for (tab, label, accent) in tabs {
                    // Filter tabs based on mode
                    let visible = match tab {
                        BottomTab::Debug | BottomTab::Timeline => state.ui.dynamic_mode,
                        BottomTab::Strings | BottomTab::Imports | BottomTab::Cfg => {
                            !state.ui.dynamic_mode
                        }
                        _ => true,
                    };

                    if !visible {
                        continue;
                    }

                    let is_selected = state.ui.bottom_tab == tab;
                    let text = if is_selected {
                        egui::RichText::new(label).color(accent).strong()
                    } else {
                        egui::RichText::new(label).color(catppuccin::SUBTEXT0)
                    };
                    if ui.selectable_label(is_selected, text).clicked() {
                        state.ui.bottom_tab = tab;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(egui::RichText::new(" × ").small()).frame(false))
                        .clicked()
                    {
                        state.ui.panel_visible = false;
                    }
                });
            });
            ui.separator();

            // Tab content - allocate remaining space to prevent collapse
            let content_rect = ui.available_rect_before_wrap();
            ui.allocate_new_ui(
                egui::UiBuilder::new().max_rect(content_rect),
                |ui| match state.ui.bottom_tab {
                    BottomTab::Console => {
                        console_action = console::render(ui, state);
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
                    BottomTab::Cfg => {
                        cfg_action = cfg::render(ui, state);
                    }
                    BottomTab::Debug => {
                        debug::render(ui, state);
                    }
                    BottomTab::Script => {
                        script_action = script::render(ui, state);
                    }
                    BottomTab::Timeline => {
                        if let Some(action) = timeline::render(ui, &mut state.debug.domain.timeline)
                        {
                            state.debug.pending_debug_action = Some(action);
                        }
                    }
                },
            );
        });

    (console_action, script_action, cfg_action)
}
