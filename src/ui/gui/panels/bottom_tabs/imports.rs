//! Imports tab panel - Display imports and exports from binary.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use crate::ui::gui::state::AppState;
use crate::ui::gui::theme::{catppuccin, code};

/// Render imports tab content with virtual scrolling
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(ref binary) = state.loaded_binary else {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("Load a binary to view imports")
                .color(catppuccin::OVERLAY0));
        });
        return;
    };

    let imports: Vec<_> = binary.functions.iter()
        .filter(|f| f.is_import)
        .collect();
    let exports: Vec<_> = binary.functions.iter()
        .filter(|f| f.is_export)
        .collect();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Imports: {}", imports.len()))
            .color(catppuccin::PEACH));
        ui.separator();
        ui.label(egui::RichText::new(format!("Exports: {}", exports.len()))
            .color(catppuccin::GREEN));
    });

    ui.separator();

    let available_height = ui.available_height();
    
    ui.columns(2, |cols| {
        // Imports column
        cols[0].label(egui::RichText::new("Imports").color(catppuccin::PEACH).strong());
        
        let import_height = (available_height - 30.0).max(50.0);
        cols[0].push_id("imports_table", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(75.0))
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(import_height)
                .body(|body| {
                    body.rows(18.0, imports.len(), |mut row| {
                        let func = &imports[row.index()];
                        row.col(|ui| {
                            ui.label(egui::RichText::new(format!("{:08X}", func.address))
                                .monospace().color(code::ADDRESS));
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&func.name)
                                .color(catppuccin::PEACH));
                        });
                    });
                });
        });

        // Exports column
        cols[1].label(egui::RichText::new("Exports").color(catppuccin::GREEN).strong());
        
        cols[1].push_id("exports_table", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(75.0))
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(import_height)
                .body(|body| {
                    body.rows(18.0, exports.len(), |mut row| {
                        let func = &exports[row.index()];
                        row.col(|ui| {
                            ui.label(egui::RichText::new(format!("{:08X}", func.address))
                                .monospace().color(code::ADDRESS));
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&func.name)
                                .color(catppuccin::GREEN));
                        });
                    });
                });
        });
    });
}

