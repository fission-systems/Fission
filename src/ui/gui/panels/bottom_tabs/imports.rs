//! Imports tab panel - Display imports and exports from binary.

use crate::ui::gui::state::AppState;
use crate::ui::gui::theme::{catppuccin, code};
use crate::ui::gui::widgets::empty_state;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

/// Render imports tab content with virtual scrolling
pub fn render(ui: &mut egui::Ui, state: &AppState) {
    // Dynamic Mode: Show Reconstructed Imports
    if state.ui.dynamic_mode {
        let imports = &state.analysis.reconstructed_imports;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Reconstructed Imports: {}", imports.len()))
                    .color(catppuccin::MAUVE),
            );
        });

        ui.separator();

        let available_height = ui.available_height();

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(100.0)) // RVA
            .column(Column::exact(100.0)) // Target
            .column(Column::exact(150.0)) // Module
            .column(Column::remainder()) // Function
            .min_scrolled_height(0.0)
            .max_scroll_height(available_height)
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("RVA");
                });
                header.col(|ui| {
                    ui.strong("Target");
                });
                header.col(|ui| {
                    ui.strong("Module");
                });
                header.col(|ui| {
                    ui.strong("Function");
                });
            })
            .body(|body| {
                body.rows(18.0, imports.len(), |mut row| {
                    let imp = &imports[row.index()];
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:08X}", imp.rva))
                                .monospace()
                                .color(code::ADDRESS),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:08X}", imp.target_address))
                                .monospace()
                                .color(code::NUMBER),
                        );
                    });
                    row.col(|ui| {
                        ui.label(egui::RichText::new(&imp.module_name).color(catppuccin::BLUE));
                    });
                    row.col(|ui| {
                        let name = imp.function_name.as_deref().unwrap_or("?");
                        ui.label(egui::RichText::new(name).color(catppuccin::MAUVE));
                    });
                });
            });

        return;
    }

    let Some(ref binary) = state.analysis.loaded_binary else {
        empty_state(ui, "Load a binary to view imports", None);
        return;
    };

    let imports: Vec<_> = binary.functions.iter().filter(|f| f.is_import).collect();
    let exports: Vec<_> = binary.functions.iter().filter(|f| f.is_export).collect();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("Imports: {}", imports.len())).color(catppuccin::PEACH),
        );
        ui.separator();
        ui.label(
            egui::RichText::new(format!("Exports: {}", exports.len())).color(catppuccin::GREEN),
        );
    });

    ui.separator();

    let available_height = ui.available_height();

    ui.columns(2, |cols| {
        // Imports column
        cols[0].label(
            egui::RichText::new("Imports")
                .color(catppuccin::PEACH)
                .strong(),
        );

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
                            ui.label(
                                egui::RichText::new(format!("{:08X}", func.address))
                                    .monospace()
                                    .color(code::ADDRESS),
                            );
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&func.name).color(catppuccin::PEACH));
                        });
                    });
                });
        });

        // Exports column
        cols[1].label(
            egui::RichText::new("Exports")
                .color(catppuccin::GREEN)
                .strong(),
        );

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
                            ui.label(
                                egui::RichText::new(format!("{:08X}", func.address))
                                    .monospace()
                                    .color(code::ADDRESS),
                            );
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&func.name).color(catppuccin::GREEN));
                        });
                    });
                });
        });
    });
}
