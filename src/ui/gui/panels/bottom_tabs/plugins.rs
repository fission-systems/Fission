//! Plugin Panel - UI for managing plugins.
//!
//! Displays loaded plugins, allows loading new plugins, and shows plugin status.

use eframe::egui;
use crate::plugin::PluginManager;
use crate::plugin::api::PluginType;
use crate::ui::gui::theme::catppuccin;

/// Plugin panel state
#[derive(Default)]
pub struct PluginPanelState {
    /// Path input for loading new plugin
    pub load_path: String,
    /// Selected plugin ID
    pub selected_plugin: Option<String>,
    /// Last error message
    pub last_error: Option<String>,
}

/// Render the plugin management panel
pub fn render(ui: &mut egui::Ui, manager: &mut PluginManager, state: &mut PluginPanelState) {
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("🔌 Plugins").color(catppuccin::MAUVE));
        
        ui.separator();
        
        // Plugin count
        ui.label(egui::RichText::new(format!(
            "{} loaded | {} hooks",
            manager.plugin_count(),
            manager.hook_count()
        )).color(catppuccin::SUBTEXT0).small());
    });
    
    ui.separator();
    
    // Two-column layout
    ui.columns(2, |columns| {
        // Left column: Plugin list
        render_plugin_list(&mut columns[0], manager, state);
        
        // Right column: Plugin details
        render_plugin_details(&mut columns[1], manager, state);
    });
}

/// Render the plugin list
fn render_plugin_list(ui: &mut egui::Ui, manager: &mut PluginManager, state: &mut PluginPanelState) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("Loaded Plugins").color(catppuccin::LAVENDER));
        
        // Load plugin input
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut state.load_path)
                .hint_text("Plugin path...")
                .desired_width(150.0));
            
            if ui.button("Load").clicked() && !state.load_path.is_empty() {
                match manager.load_plugin(&state.load_path) {
                    Ok(id) => {
                        state.last_error = None;
                        state.selected_plugin = Some(id);
                    }
                    Err(e) => {
                        state.last_error = Some(e);
                    }
                }
                state.load_path.clear();
            }
        });
        
        // Error display
        if let Some(error) = &state.last_error {
            ui.label(egui::RichText::new(format!("⚠ {}", error))
                .color(catppuccin::RED).small());
        }
        
        ui.separator();
        
        // Plugin list
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                let plugins: Vec<_> = manager.list_plugins()
                    .into_iter()
                    .map(|p| p.clone())
                    .collect();
                
                if plugins.is_empty() {
                    ui.label(egui::RichText::new("No plugins loaded")
                        .color(catppuccin::OVERLAY0).italics());
                } else {
                    for plugin in plugins {
                        let is_selected = state.selected_plugin.as_ref() == Some(&plugin.id);
                        let type_icon = match plugin.plugin_type {
                            PluginType::Python => "🐍",
                            PluginType::Lua => "🌙",
                            PluginType::Native => "⚙️",
                        };
                        
                        let label_text = format!("{} {}", type_icon, plugin.name);
                        let color = if plugin.enabled {
                            catppuccin::TEXT
                        } else {
                            catppuccin::OVERLAY0
                        };
                        
                        if ui.selectable_label(
                            is_selected,
                            egui::RichText::new(label_text).color(color)
                        ).clicked() {
                            state.selected_plugin = Some(plugin.id.clone());
                        }
                    }
                }
            });
    });
}

/// Render plugin details panel
fn render_plugin_details(ui: &mut egui::Ui, manager: &mut PluginManager, state: &mut PluginPanelState) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("Plugin Details").color(catppuccin::LAVENDER));
        ui.separator();
        
        // Clone the selected plugin ID to avoid borrow issues
        let selected_id = state.selected_plugin.clone();
        
        if let Some(plugin_id) = selected_id {
            if let Some(plugin) = manager.get_plugin(&plugin_id) {
                let plugin = plugin.clone();
                let plugin_id_clone = plugin_id.clone();
                
                // Plugin info grid
                egui::Grid::new("plugin_details_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("ID:");
                        ui.label(egui::RichText::new(&plugin.id).color(catppuccin::PEACH));
                        ui.end_row();
                        
                        ui.label("Name:");
                        ui.label(&plugin.name);
                        ui.end_row();
                        
                        ui.label("Version:");
                        ui.label(&plugin.version);
                        ui.end_row();
                        
                        ui.label("Author:");
                        ui.label(&plugin.author);
                        ui.end_row();
                        
                        ui.label("Type:");
                        let type_str = match plugin.plugin_type {
                            PluginType::Python => "Python",
                            PluginType::Lua => "Lua",
                            PluginType::Native => "Native",
                        };
                        ui.label(type_str);
                        ui.end_row();
                        
                        ui.label("Status:");
                        let (status_text, status_color) = if plugin.enabled {
                            ("Enabled", catppuccin::GREEN)
                        } else {
                            ("Disabled", catppuccin::OVERLAY0)
                        };
                        ui.label(egui::RichText::new(status_text).color(status_color));
                        ui.end_row();
                    });
                
                ui.separator();
                
                // Description
                if !plugin.description.is_empty() {
                    ui.label(egui::RichText::new("Description:").small());
                    ui.label(egui::RichText::new(&plugin.description)
                        .color(catppuccin::SUBTEXT0).small());
                }
                
                ui.separator();
                
                // Actions
                let mut should_unload = false;
                ui.horizontal(|ui| {
                    if plugin.enabled {
                        if ui.button("Disable").clicked() {
                            let _ = manager.disable_plugin(&plugin_id_clone);
                        }
                    } else {
                        if ui.button("Enable").clicked() {
                            let _ = manager.enable_plugin(&plugin_id_clone);
                        }
                    }
                    
                    if ui.button(egui::RichText::new("Unload").color(catppuccin::RED)).clicked() {
                        let _ = manager.unload_plugin(&plugin_id_clone);
                        should_unload = true;
                    }
                });
                
                if should_unload {
                    state.selected_plugin = None;
                }
            } else {
                state.selected_plugin = None;
            }
        } else {
            ui.label(egui::RichText::new("Select a plugin to view details")
                .color(catppuccin::OVERLAY0).italics());
        }
    });
}
