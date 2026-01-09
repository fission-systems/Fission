//! Functions panel - displays list of functions from loaded binary with virtual scrolling.

use super::super::core::state::AppState;
use super::super::theme::catppuccin;
use super::super::components::widgets::empty_state_with_spacing;
use crate::analysis::loader::FunctionInfo;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

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
pub fn render(ctx: &egui::Context, state: &AppState) -> Option<FunctionAction> {
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
pub fn render_inside(ui: &mut egui::Ui, state: &AppState) -> Option<FunctionAction> {
    let mut action: Option<FunctionAction> = None;

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Functions").color(catppuccin::LAVENDER));
            if let Some(ref binary) = state.analysis.loaded_binary {
                ui.label(
                    egui::RichText::new(format!("({})", binary.functions.len()))
                        .color(catppuccin::SUBTEXT0)
                        .small(),
                );

                // Analyze button for discovering internal functions
                if ui
                    .small_button("🔍 Analyze")
                    .on_hover_text("Discover internal functions from CALL instructions")
                    .clicked()
                {
                    action = Some(FunctionAction::Analyze);
                }
            }
        });
        ui.separator();

        if let Some(ref binary) = state.analysis.loaded_binary {
            let available_height = ui.available_height();
            let row_height = 22.0;
            let total_rows = binary.functions.len();

            // Use TableBuilder for virtual scrolling
            TableBuilder::new(ui)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(available_height)
                .body(|body| {
                    body.rows(row_height, total_rows, |mut row| {
                        let func = &binary.functions[row.index()];

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
