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
    /// User wants to scan for hidden functions
    DeepScan,
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

                // Deep scan button
                if ui
                    .small_button("🕵")
                    .on_hover_text("Deep scan for hidden functions (Prologue Search)")
                    .clicked()
                {
                    action = Some(FunctionAction::DeepScan);
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
            let func_count = binary.functions.len();

            // Use cached filter if valid, otherwise recompute
            if state.viewmodels.functions.needs_refresh(func_count) {
                let filter_lower = state.viewmodels.functions.filter.to_lowercase();
                let show_imports = state.viewmodels.functions.show_imports;
                let show_exports = state.viewmodels.functions.show_exports;
                let show_internals = state.viewmodels.functions.show_internals;

                let indices: Vec<usize> = binary
                    .functions
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, func)| {
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

                        if category_match && name_match {
                            Some(idx)
                        } else {
                            None
                        }
                    })
                    .collect();

                state.viewmodels.functions.cached_indices = indices;
                state.viewmodels.functions.cache_key =
                    Some(state.viewmodels.functions.current_cache_key(func_count));
            }

            let cached_indices = &state.viewmodels.functions.cached_indices;
            let available_height = ui.available_height();
            let row_height = 22.0;
            let total_rows = cached_indices.len();

            // Show filtered count if different from total
            if total_rows != func_count {
                ui.label(
                    egui::RichText::new(format!("Showing {} of {}", total_rows, func_count))
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
                        let func_idx = cached_indices[row.index()];
                        let func = &binary.functions[func_idx];

                        row.col(|ui| {
                            // Determine icon and color based on function type
                            let (icon, name_color) = if func.is_import {
                                ("⬇", catppuccin::PEACH) // Import
                            } else if func.is_export {
                                ("⬆", catppuccin::GREEN) // Export
                            } else {
                                ("◆", catppuccin::BLUE) // Regular function
                            };

                            let display_name = state
                                .analysis
                                .domain
                                .user_function_names
                                .get(&func.address)
                                .cloned()
                                .unwrap_or_else(|| func.name.clone());

                            let label = if display_name.is_empty() {
                                format!("{} sub_{:08x}", icon, func.address)
                            } else if display_name.len() > 25 {
                                format!("{} {}...", icon, &display_name[..22])
                            } else {
                                format!("{} {}", icon, display_name)
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
