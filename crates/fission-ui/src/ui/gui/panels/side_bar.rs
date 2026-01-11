//! Side Bar panel - Shows content based on active activity (Explorer, Debug, etc.)

use super::functions::{self, FunctionAction};
use crate::ui::gui::components::widgets::empty_state;
use crate::ui::gui::core::state::{Activity, AppState};
use crate::ui::gui::theme::catppuccin;
use eframe::egui;

/// Result from side bar render
pub enum SideBarAction {
    /// User selected a function
    SelectFunction(fission_loader::loader::FunctionInfo),
    /// User requested function analysis
    AnalyzeFunctions,
    /// User wants to rename a function
    RenameFunction(u64),
    /// User requested deep scan functions
    DeepScanFunctions,
    /// User switched to a different binary in project
    SwitchBinary(std::sync::Arc<fission_loader::loader::LoadedBinary>),
}

/// Render the side bar panel.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> Option<SideBarAction> {
    let mut result = None;
    if !state.ui.sidebar_visible {
        return None;
    }

    egui::SidePanel::left("side_bar")
        .frame(
            egui::Frame::none()
                .fill(ctx.style().visuals.panel_fill)
                .stroke(egui::Stroke::new(1.0, catppuccin::SURFACE0)),
        )
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
                ui.label(
                    egui::RichText::new(title)
                        .size(11.0)
                        .strong()
                        .color(ui.visuals().weak_text_color()),
                );
            });
            ui.add_space(4.0);

            // Content
            match state.ui.active_activity {
                Activity::Explorer => {
                    // Show project binaries if in project mode
                    if state.analysis.domain.project_folder.is_some() {
                        if let Some(action) = render_project_explorer(ui, state) {
                            result = Some(action);
                        }
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);
                    }

                    // Use the existing functions panel logic but as part of this panel
                    if let Some(action) = functions::render_inside(ui, state) {
                        result = Some(match action {
                            FunctionAction::Select(func) => SideBarAction::SelectFunction(func),
                            FunctionAction::Analyze => SideBarAction::AnalyzeFunctions,
                            FunctionAction::Rename(addr) => SideBarAction::RenameFunction(addr),
                            FunctionAction::DeepScan => SideBarAction::DeepScanFunctions,
                        });
                    }
                }
                Activity::Search => {
                    if let Some(action) = super::search::render(ui, state) {
                        result = Some(action);
                    }
                }
                Activity::Debug => {
                    render_debug_sidebar(ui, state);
                }
                Activity::Plugins => {
                    render_plugins_sidebar(ui, state);
                }
                Activity::Settings => {
                    super::settings::render(ui, state);
                }
            }
        });

    result
}

fn render_project_explorer(ui: &mut egui::Ui, state: &mut AppState) -> Option<SideBarAction> {
    use std::collections::HashMap;
    use std::path::Path;

    ui.add_space(8.0);

    let project_folder = match &state.analysis.domain.project_folder {
        Some(p) => p.clone(),
        None => return None,
    };

    let folder_name = Path::new(&project_folder)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Project");

    // Clone project binaries to avoid borrow checker issues
    let project_binaries = state.analysis.domain.project_binaries.clone();

    // Build a map of file paths to binary info
    let mut binary_map: HashMap<
        String,
        (usize, std::sync::Arc<fission_loader::loader::LoadedBinary>),
    > = HashMap::new();
    for (idx, binary) in project_binaries.iter().enumerate() {
        binary_map.insert(binary.path.clone(), (idx, binary.clone()));
    }

    let mut result = None;

    ui.collapsing(
        egui::RichText::new(format!("📁 {}", folder_name))
            .size(12.0)
            .strong()
            .color(catppuccin::BLUE),
        |ui| {
            ui.add_space(4.0);

            // Build folder tree
            if let Ok(entries) = std::fs::read_dir(&project_folder) {
                let mut dirs = Vec::new();
                let mut files = Vec::new();

                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        dirs.push(path);
                    } else if path.is_file() {
                        files.push(path);
                    }
                }

                // Sort alphabetically
                dirs.sort();
                files.sort();

                // Render directories first
                for dir_path in dirs {
                    if let Some(action) = render_folder_tree(ui, state, &dir_path, &binary_map, 0) {
                        result = Some(action);
                    }
                }

                // Then files
                for file_path in files {
                    if let Some(action) = render_file_entry(ui, state, &file_path, &binary_map, 0) {
                        result = Some(action);
                    }
                }
            }
        },
    );

    result
}

fn render_folder_tree(
    ui: &mut egui::Ui,
    state: &mut AppState,
    path: &std::path::Path,
    binary_map: &std::collections::HashMap<
        String,
        (usize, std::sync::Arc<fission_loader::loader::LoadedBinary>),
    >,
    depth: usize,
) -> Option<SideBarAction> {
    let folder_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");

    let path_str = path.to_string_lossy().to_string();
    let is_expanded = state.ui.expanded_folders.contains(&path_str);

    let mut result = None;

    ui.horizontal(|ui| {
        ui.add_space((depth * 16) as f32);

        let header = egui::CollapsingHeader::new(
            egui::RichText::new(format!("📁 {}", folder_name))
                .size(11.0)
                .color(catppuccin::TEXT),
        )
        .id_salt(format!("folder_{}", path_str))
        .default_open(is_expanded);

        header.show(ui, |ui| {
            // Update expanded state
            if is_expanded {
                state.ui.expanded_folders.insert(path_str.clone());
            } else {
                state.ui.expanded_folders.remove(&path_str);
            }

            // Scan directory
            if let Ok(entries) = std::fs::read_dir(path) {
                let mut dirs = Vec::new();
                let mut files = Vec::new();

                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        dirs.push(entry_path);
                    } else if entry_path.is_file() {
                        files.push(entry_path);
                    }
                }

                dirs.sort();
                files.sort();

                // Recursively render subdirectories
                for dir_path in dirs {
                    if let Some(action) =
                        render_folder_tree(ui, state, &dir_path, binary_map, depth + 1)
                    {
                        result = Some(action);
                    }
                }

                // Render files
                for file_path in files {
                    if let Some(action) =
                        render_file_entry(ui, state, &file_path, binary_map, depth + 1)
                    {
                        result = Some(action);
                    }
                }
            }
        });
    });

    result
}

fn render_file_entry(
    ui: &mut egui::Ui,
    state: &mut AppState,
    path: &std::path::Path,
    binary_map: &std::collections::HashMap<
        String,
        (usize, std::sync::Arc<fission_loader::loader::LoadedBinary>),
    >,
    depth: usize,
) -> Option<SideBarAction> {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");

    let path_str = path.to_string_lossy().to_string();
    let is_binary = binary_map.contains_key(&path_str);

    // Determine icon based on file extension
    let icon = if is_binary {
        "📦"
    } else {
        match path.extension().and_then(|e| e.to_str()) {
            Some("exe") | Some("dll") | Some("so") | Some("dylib") => "📦",
            Some("txt") | Some("md") => "📄",
            Some("json") | Some("xml") => "📋",
            _ => "📄",
        }
    };

    let selected_idx = state.analysis.domain.selected_binary_index;
    let is_selected = if let Some((idx, _)) = binary_map.get(&path_str) {
        selected_idx == Some(*idx)
    } else {
        false
    };

    let mut result = None;

    ui.horizontal(|ui| {
        ui.add_space((depth * 16 + 16) as f32);

        let button = egui::Button::new(
            egui::RichText::new(format!("{} {}", icon, file_name))
                .size(11.0)
                .color(if is_selected {
                    catppuccin::BLUE
                } else if is_binary {
                    catppuccin::GREEN
                } else {
                    catppuccin::SUBTEXT0
                }),
        )
        .fill(if is_selected {
            catppuccin::SURFACE1
        } else {
            egui::Color32::TRANSPARENT
        })
        .frame(false);

        if ui.add(button).clicked() && is_binary {
            // Return action to switch binary
            if let Some((idx, binary)) = binary_map.get(&path_str) {
                state.analysis.domain.selected_binary_index = Some(*idx);
                result = Some(SideBarAction::SwitchBinary(binary.clone()));
            }
        }
    });

    result
}

fn render_debug_sidebar(ui: &mut egui::Ui, state: &AppState) {
    // Basic debug info moved from bottom panel if desired,
    // or specialized debug views like breakpoints list
    ui.add_space(8.0);
    ui.collapsing(egui::RichText::new("BREAKPOINTS").small().strong(), |ui| {
        // Breakpoint list (abbreviated)
        if state.debug.domain.debug_state.breakpoints.is_empty() {
            ui.label(
                egui::RichText::new("No breakpoints")
                    .color(ui.visuals().weak_text_color())
                    .small(),
            );
        } else {
            for addr in state.debug.domain.debug_state.breakpoints.keys() {
                ui.label(
                    egui::RichText::new(format!("0x{:016x}", addr))
                        .small()
                        .monospace(),
                );
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
        if ui
            .button(
                egui::RichText::new("📦 Load Plugin...")
                    .color(catppuccin::GREEN)
                    .small(),
            )
            .clicked()
        {
            state.log("[*] Plugin loading UI - coming soon".to_string());
        }
    });

    ui.add_space(8.0);
    ui.separator();

    // Installed Plugins section
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("INSTALLED")
                .size(10.0)
                .strong()
                .color(ui.visuals().weak_text_color()),
        );
    });
    ui.add_space(4.0);

    // List plugins - clone to avoid borrow issues
    let plugins: Vec<_> = if let Ok(mgr) = state.plugin_manager().read() {
        mgr.list_plugins().into_iter().cloned().collect()
    } else {
        Vec::new()
    };
    let mut toggle_action: Option<(String, bool, String)> = None; // (id, was_enabled, name)

    if plugins.is_empty() {
        empty_state(
            ui,
            "No plugins installed",
            Some("Load a plugin to extend Fission's functionality"),
        );
    } else {
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for plugin in &plugins {
                    ui.push_id(&plugin.id, |ui| {
                        egui::Frame::none()
                            .fill(ui.visuals().faint_bg_color)
                            .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                            .rounding(4.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Plugin type icon
                                    let icon = match plugin.plugin_type {
                                        PluginType::Python => "Py",
                                        PluginType::Lua => "Lu",
                                        PluginType::Native => "Na",
                                    };
                                    ui.label(
                                        egui::RichText::new(icon)
                                            .size(14.0)
                                            .strong()
                                            .color(ui.visuals().text_color()),
                                    );
                                    ui.add_space(4.0);

                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(&plugin.name)
                                                .color(ui.visuals().strong_text_color())
                                                .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(&plugin.version)
                                                .color(ui.visuals().weak_text_color())
                                                .small(),
                                        );
                                    });

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let enabled = plugin.enabled;
                                            let toggle_text = if enabled { "●" } else { "○" };
                                            let toggle_color = if enabled {
                                                catppuccin::GREEN
                                            } else {
                                                catppuccin::OVERLAY0
                                            };

                                            if ui
                                                .button(
                                                    egui::RichText::new(toggle_text)
                                                        .color(toggle_color),
                                                )
                                                .clicked()
                                            {
                                                toggle_action = Some((
                                                    plugin.id.clone(),
                                                    enabled,
                                                    plugin.name.clone(),
                                                ));
                                            }
                                        },
                                    );
                                });

                                if !plugin.description.is_empty() {
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(&plugin.description)
                                            .color(catppuccin::OVERLAY1)
                                            .small(),
                                    );
                                }
                            });
                        ui.add_space(4.0);
                    });
                }
            });
    }

    // Apply toggle action after rendering
    if let Some((plugin_id, was_enabled, name)) = toggle_action {
        let result = if let Ok(mut mgr) = state.plugin_manager().write() {
            if was_enabled {
                let _ = mgr.disable_plugin(&plugin_id);
                Some(format!("[*] Disabled plugin: {}", name))
            } else {
                let _ = mgr.enable_plugin(&plugin_id);
                Some(format!("[*] Enabled plugin: {}", name))
            }
        } else {
            None
        };

        if let Some(msg) = result {
            state.log(msg);
        } else if state.plugin_manager().write().is_err() {
            state.log(format!(
                "[!] Failed to acquire write lock for plugin manager to toggle {}",
                name
            ));
        }
    }

    // Recommended section
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("RECOMMENDED")
                .size(10.0)
                .strong()
                .color(catppuccin::SUBTEXT0),
        );
    });
    ui.add_space(4.0);

    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("• Yara Rules Scanner")
                .color(ui.visuals().weak_text_color())
                .small(),
        );
        ui.label(
            egui::RichText::new("• Crypto Detector")
                .color(ui.visuals().weak_text_color())
                .small(),
        );
        ui.label(
            egui::RichText::new("• IDA Script Importer")
                .color(ui.visuals().weak_text_color())
                .small(),
        );
    });
}
