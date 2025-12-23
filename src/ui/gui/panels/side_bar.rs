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
                    Activity::Plugins => "EXTENSIONS",
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
                Activity::Plugins => {
                    render_plugins_sidebar(ui, state);
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

fn render_plugins_sidebar(ui: &mut egui::Ui, state: &mut AppState) {
    use crate::plugin::api::PluginType;
    
    ui.add_space(4.0);
    
    // Load plugin button
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        if ui.button(egui::RichText::new("📦 Load Plugin...").color(catppuccin::GREEN).small()).clicked() {
            state.log("[*] Plugin loading UI - coming soon".to_string());
        }
    });
    
    ui.add_space(8.0);
    ui.separator();
    
    // Installed Plugins section
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("INSTALLED").size(10.0).strong().color(catppuccin::SUBTEXT0));
    });
    ui.add_space(4.0);
    
    // List plugins - clone to avoid borrow issues
    let plugins: Vec<_> = state.plugin_manager.list_plugins().iter().cloned().collect();
    let mut toggle_action: Option<(String, bool, String)> = None; // (id, was_enabled, name)
    
    if plugins.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("No plugins installed")
                .color(catppuccin::OVERLAY0).small().italics());
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Load a plugin to extend\nFission's functionality")
                .color(catppuccin::OVERLAY0).small());
        });
    } else {
        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            for plugin in &plugins {
                ui.push_id(&plugin.id, |ui| {
                    egui::Frame::none()
                        .fill(catppuccin::SURFACE0)
                        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                        .rounding(4.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Plugin type icon
                                let icon = match plugin.plugin_type {
                                    PluginType::Python => "🐍",
                                    PluginType::Lua => "🌙",
                                    PluginType::Native => "⚡",
                                };
                                ui.label(egui::RichText::new(icon).size(16.0));
                                
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&plugin.name)
                                        .color(catppuccin::TEXT).strong());
                                    ui.label(egui::RichText::new(&plugin.version)
                                        .color(catppuccin::SUBTEXT0).small());
                                });
                                
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let enabled = plugin.enabled;
                                    let toggle_text = if enabled { "●" } else { "○" };
                                    let toggle_color = if enabled { catppuccin::GREEN } else { catppuccin::OVERLAY0 };
                                    
                                    if ui.button(egui::RichText::new(toggle_text).color(toggle_color)).clicked() {
                                        toggle_action = Some((plugin.id.clone(), enabled, plugin.name.clone()));
                                    }
                                });
                            });
                            
                            if !plugin.description.is_empty() {
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(&plugin.description)
                                    .color(catppuccin::OVERLAY1).small());
                            }
                        });
                    ui.add_space(4.0);
                });
            }
        });
    }
    
    // Apply toggle action after rendering
    if let Some((plugin_id, was_enabled, name)) = toggle_action {
        if was_enabled {
            let _ = state.plugin_manager.disable_plugin(&plugin_id);
            state.log(format!("[*] Disabled plugin: {}", name));
        } else {
            let _ = state.plugin_manager.enable_plugin(&plugin_id);
            state.log(format!("[*] Enabled plugin: {}", name));
        }
    }
    
    // Recommended section
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("RECOMMENDED").size(10.0).strong().color(catppuccin::SUBTEXT0));
    });
    ui.add_space(4.0);
    
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new("• Yara Rules Scanner")
            .color(catppuccin::SUBTEXT1).small());
        ui.label(egui::RichText::new("• Crypto Detector")
            .color(catppuccin::SUBTEXT1).small());
        ui.label(egui::RichText::new("• IDA Script Importer")
            .color(catppuccin::SUBTEXT1).small());
    });
}
