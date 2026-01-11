//! Settings Panel - Application configuration and preferences.
//!
//! Provides UI controls for:
//! - Theme selection (Light/Dark/System)
//! - UI scale adjustment
//! - Decompiler configuration
//! - Other application preferences

use crate::ui::gui::{AppState, ThemeMode};
use eframe::egui;

/// Render the settings panel inside the sidebar or a dialog
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(10.0);

    // Title
    ui.label(
        egui::RichText::new("SETTINGS")
            .size(12.0)
            .size(12.0)
            .strong()
            .color(ui.visuals().weak_text_color()),
    );

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Appearance Section
    ui.collapsing(egui::RichText::new("Appearance").strong(), |ui| {
        ui.add_space(4.0);

        // Theme Mode
        ui.horizontal(|ui| {
            ui.label("Theme:");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::ComboBox::from_id_salt("theme_combo")
                    .selected_text(match state.settings.theme_mode {
                        ThemeMode::Dark => "Dark (Catppuccin)",
                        ThemeMode::Light => "Light",
                        ThemeMode::System => "System",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.settings.theme_mode,
                            ThemeMode::Dark,
                            "Dark (Catppuccin)",
                        );
                        ui.selectable_value(
                            &mut state.settings.theme_mode,
                            ThemeMode::Light,
                            "Light",
                        );
                        ui.selectable_value(
                            &mut state.settings.theme_mode,
                            ThemeMode::System,
                            "System",
                        );
                    });
            });
        });

        ui.add_space(4.0);

        // UI Scale
        ui.horizontal(|ui| {
            ui.label("UI Scale:");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Slider::new(&mut state.settings.ui_scale, 0.5..=3.0).text(""))
                    .changed()
                {
                    // Note: This takes effect next frame by applying to ctx
                }
            });
        });

        ui.add_space(4.0);

        // Editor Font Size
        ui.horizontal(|ui| {
            ui.label("Editor Font Size:");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::DragValue::new(&mut state.settings.editor_font_size)
                        .speed(1)
                        .range(8..=32),
                );
            });
        });

        ui.add_space(8.0);
    });

    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    // Advanced Section
    ui.collapsing(egui::RichText::new("Developer").strong(), |ui| {
        ui.add_space(4.0);

        ui.checkbox(&mut state.settings.show_dev_tools, "Show Developer Tools");
        ui.label(
            egui::RichText::new("Enables advanced debugging overlays")
                .small()
                .color(ui.visuals().weak_text_color()),
        );

        ui.add_space(8.0);

        if ui.button("Clear Cache").clicked() {
            // Send clear cache request via some mechanism or trigger a flag
            // For now, we'll need a way to access the decomp_tx from here or via AppState
            state.log("[*] Decompiler cache clear requested (Use Menu -> Analysis -> Clear Cache)");
        }
    });

    ui.add_space(20.0);

    // About / Info
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("Fission")
                .strong()
                .size(16.0)
                .color(ui.visuals().strong_text_color()),
        );

        ui.label(
            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .small()
                .color(ui.visuals().weak_text_color()),
        );

        ui.add_space(4.0);
        ui.hyperlink("https://github.com/sjkim1127/Fission");
    });
}
