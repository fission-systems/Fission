//! Functions panel - displays list of functions from loaded binary with virtual scrolling.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use crate::analysis::loader::FunctionInfo;
use super::super::state::AppState;
use super::super::theme::{catppuccin, code};

/// Render the functions list panel on the left side.
/// 
/// Returns the clicked function if any.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> Option<FunctionInfo> {
    let mut clicked_func: Option<FunctionInfo> = None;
    
    egui::SidePanel::left("functions_panel")
        .resizable(true)
        .default_width(180.0)
        .min_width(120.0)
        .max_width(350.0)
        .show(ctx, |ui| {
            clicked_func = render_inside(ui, state);
        });
    
    clicked_func
}

/// Render functions list inside an existing UI.
pub fn render_inside(ui: &mut egui::Ui, state: &mut AppState) -> Option<FunctionInfo> {
    let mut clicked_func: Option<FunctionInfo> = None;
    
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("Functions").color(catppuccin::LAVENDER));
            if let Some(ref binary) = state.analysis.loaded_binary {
                ui.label(egui::RichText::new(format!("({})", binary.functions.len()))
                    .color(catppuccin::SUBTEXT0).small());
            }
        });
        ui.separator();

        if let Some(ref binary) = state.analysis.loaded_binary {
            // Search filter
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍").color(catppuccin::OVERLAY0));
                // Could add a filter input here
            });
            
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
                                ("⬇", catppuccin::PEACH)  // Import
                            } else if func.is_export {
                                ("⬆", catppuccin::GREEN)  // Export
                            } else {
                                ("◆", catppuccin::BLUE)   // Regular function
                            };
                            
                            let label = if func.name.is_empty() {
                                format!("{} sub_{:08x}", icon, func.address)
                            } else if func.name.len() > 25 {
                                format!("{} {}...", icon, &func.name[..22])
                            } else {
                                format!("{} {}", icon, func.name)
                            };
                            
                            let is_selected = state.analysis.selected_function
                                .as_ref()
                                .map(|f| f.address == func.address)
                                .unwrap_or(false);
                            
                            let text = if is_selected {
                                egui::RichText::new(&label).color(catppuccin::TEXT).strong()
                            } else {
                                egui::RichText::new(&label).color(name_color)
                            };
                            
                            if ui.selectable_label(is_selected, text).clicked() {
                                clicked_func = Some(func.clone());
                            }
                        });
                    });
                });
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(egui::RichText::new("No binary loaded")
                    .color(catppuccin::OVERLAY0)
                    .size(14.0));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("File → Open to load")
                    .color(catppuccin::OVERLAY0)
                    .small());
            });
        }
    });
    
    clicked_func
}
