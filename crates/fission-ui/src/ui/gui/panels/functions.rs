//! Functions panel - displays list of functions from loaded binary with virtual scrolling.

use super::super::components::widgets::empty_state_with_spacing;
use super::super::core::state::AppState;
use super::super::theme::catppuccin;
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use fission_loader::loader::FunctionInfo;

/// Action returned from functions panel
pub enum FunctionAction {
    /// User clicked on a function
    Select(FunctionInfo),
    /// User clicked Analyze button
    Analyze,
    /// User wants to rename a function
    Rename(u64), // function address
}

/// Render the functions list panel on the left side.
///
/// Returns the action if any.
#[allow(dead_code)]
pub fn render(ctx: &egui::Context, state: &mut AppState) -> Option<FunctionAction> {
    let mut action: Option<FunctionAction> = None;

    egui::SidePanel::left("functions_panel")
        .resizable(true)
        .default_width(180.0)
        .min_width(120.0)
        .max_width(350.0)
        .show(ctx, |ui| {
            action = render_inside(ui, state);
        });

    action
}

/// Render functions list inside an existing UI.
pub fn render_inside(ui: &mut egui::Ui, state: &mut AppState) -> Option<FunctionAction> {
    let mut action: Option<FunctionAction> = None;

    ui.vertical(|ui| {
        // Header with count
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Functions").color(catppuccin::LAVENDER));
            if let Some(ref binary) = state.analysis.domain.loaded_binary {
                ui.label(
                    egui::RichText::new(format!("({})", binary.functions.len()))
                        .color(catppuccin::SUBTEXT0)
                        .small(),
                );

                // Analyze button for discovering internal functions
                if ui
                    .small_button("🔍")
                    .on_hover_text("Discover internal functions from CALL instructions")
                    .clicked()
                {
                    action = Some(FunctionAction::Analyze);
                }
            }
        });

        // Search bar
        ui.horizontal(|ui| {
            ui.label("🔎");
            let filter = &mut state.viewmodels.functions.filter;
            ui.add(
                egui::TextEdit::singleline(filter)
                    .hint_text("Filter...")
                    .desired_width(ui.available_width() - 10.0),
            );
        });

        // Category filter toggles
        ui.horizontal(|ui| {
            let vm = &mut state.viewmodels.functions;

            // Import toggle
            let import_text = if vm.show_imports {
                egui::RichText::new("⬇ Imp").color(catppuccin::PEACH)
            } else {
                egui::RichText::new("⬇ Imp").color(catppuccin::SURFACE2)
            };
            if ui.selectable_label(vm.show_imports, import_text).clicked() {
                vm.show_imports = !vm.show_imports;
            }

            // Export toggle
            let export_text = if vm.show_exports {
                egui::RichText::new("⬆ Exp").color(catppuccin::GREEN)
            } else {
                egui::RichText::new("⬆ Exp").color(catppuccin::SURFACE2)
            };
            if ui.selectable_label(vm.show_exports, export_text).clicked() {
                vm.show_exports = !vm.show_exports;
            }

            // Internal toggle
            let internal_text = if vm.show_internals {
                egui::RichText::new("◆ Int").color(catppuccin::BLUE)
            } else {
                egui::RichText::new("◆ Int").color(catppuccin::SURFACE2)
            };
            if ui
                .selectable_label(vm.show_internals, internal_text)
                .clicked()
            {
                vm.show_internals = !vm.show_internals;
            }
        });

        ui.separator();

        if let Some(ref binary) = state.analysis.domain.loaded_binary {
            // Apply filters to get visible functions
            let filter_lower = state.viewmodels.functions.filter.to_lowercase();
            let show_imports = state.viewmodels.functions.show_imports;
            let show_exports = state.viewmodels.functions.show_exports;
            let show_internals = state.viewmodels.functions.show_internals;

            let filtered_functions: Vec<&FunctionInfo> = binary
                .functions
                .iter()
                .filter(|func| {
                    // Category filter
                    let category_match = if func.is_import {
                        show_imports
                    } else if func.is_export {
                        show_exports
                    } else {
                        show_internals
                    };

                    // Name filter (case-insensitive)
                    let name_match = if filter_lower.is_empty() {
                        true
                    } else {
                        func.name.to_lowercase().contains(&filter_lower)
                            || format!("{:x}", func.address).contains(&filter_lower)
                    };

                    category_match && name_match
                })
                .collect();

            let available_height = ui.available_height();
            let row_height = 22.0;
            let total_rows = filtered_functions.len();

            // Show filtered count if different from total
            if total_rows != binary.functions.len() {
                ui.label(
                    egui::RichText::new(format!(
                        "Showing {} of {}",
                        total_rows,
                        binary.functions.len()
                    ))
                    .color(catppuccin::SUBTEXT0)
                    .small(),
                );
            }

            // Use TableBuilder for virtual scrolling
            TableBuilder::new(ui)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(available_height)
                .body(|body| {
                    body.rows(row_height, total_rows, |mut row| {
                        let func = filtered_functions[row.index()];

                        row.col(|ui| {
                            // Determine icon and color based on function type
                            let (icon, name_color) = if func.is_import {
                                ("⬇", catppuccin::PEACH) // Import
                            } else if func.is_export {
                                ("⬆", catppuccin::GREEN) // Export
                            } else {
                                ("◆", catppuccin::BLUE) // Regular function
                            };

                            let label = if func.name.is_empty() {
                                format!("{} sub_{:08x}", icon, func.address)
                            } else if func.name.len() > 25 {
                                format!("{} {}...", icon, &func.name[..22])
                            } else {
                                format!("{} {}", icon, func.name)
                            };

                            let is_selected = state
                                .analysis
                                .domain
                                .selected_function
                                .as_ref()
                                .map(|f| f.address == func.address)
                                .unwrap_or(false);

                            let text = if is_selected {
                                egui::RichText::new(&label).color(catppuccin::TEXT).strong()
                            } else {
                                egui::RichText::new(&label).color(name_color)
                            };

                            let response = ui.selectable_label(is_selected, text);

                            if response.clicked() {
                                action = Some(FunctionAction::Select(func.clone()));
                            }

                            // Right-click context menu
                            response.context_menu(|ui| {
                                if ui.button("✏️ Rename").clicked() {
                                    action = Some(FunctionAction::Rename(func.address));
                                    ui.close_menu();
                                }
                                if ui.button("📋 Copy Address").clicked() {
                                    ui.output_mut(|o| {
                                        o.copied_text = format!("0x{:x}", func.address)
                                    });
                                    ui.close_menu();
                                }
                            });
                        });
                    });
                });
        } else {
            empty_state_with_spacing(ui, "No binary loaded", Some("File → Open to load"), 40.0);
        }
    });

    action
}
