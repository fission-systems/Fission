//! Menu bar rendering with Catppuccin theme.

use super::super::core::state::AppState;
use super::super::theme::catppuccin;
use eframe::egui;

/// Actions triggered from menu
#[allow(dead_code)]
pub enum MenuAction {
    OpenFile,
    OpenFolder,
    SaveSnapshot,
    LoadSnapshot,
    AttachToProcess,
    DetachProcess,
    ClearConsole,
    ClearCache,
    ShowAbout,
    ShowXrefs,
    ShowStringXrefs,
    BatchDecompile,
    ExportResults,
    Exit,
    None,
}

/// Render the top menu bar.
///
/// Returns any action triggered by menu clicks.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> MenuAction {
    let mut action = MenuAction::None;

    egui::TopBottomPanel::top("menu_bar")
        .exact_height(28.0)
        .show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(egui::RichText::new("File").color(catppuccin::TEXT), |ui| {
                    if ui
                        .button(egui::RichText::new("📂 Open Binary...").color(catppuccin::BLUE))
                        .clicked()
                    {
                        action = MenuAction::OpenFile;
                        ui.close_menu();
                    }
                    if ui
                        .button(egui::RichText::new("📁 Open Folder...").color(catppuccin::GREEN))
                        .clicked()
                    {
                        action = MenuAction::OpenFolder;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .button(egui::RichText::new("💾 Save Snapshot...").color(catppuccin::MAUVE))
                        .clicked()
                    {
                        action = MenuAction::SaveSnapshot;
                        ui.close_menu();
                    }
                    if ui
                        .button(egui::RichText::new("📂 Load Snapshot...").color(catppuccin::PINK))
                        .clicked()
                    {
                        action = MenuAction::LoadSnapshot;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .button(egui::RichText::new("🚪 Exit").color(catppuccin::RED))
                        .clicked()
                    {
                        action = MenuAction::Exit;
                    }
                });

                ui.menu_button(egui::RichText::new("Debug").color(catppuccin::TEXT), |ui| {
                    // macOS: Static analysis only (no dynamic debugging)
                    #[cfg(target_os = "macos")]
                    {
                        ui.label(
                            egui::RichText::new("⚠ macOS: Static Analysis Only")
                                .color(catppuccin::YELLOW),
                        );
                        ui.separator();
                        ui.add_enabled(
                            false,
                            egui::Button::new(
                                egui::RichText::new("🔗 Attach to Process...")
                                    .color(catppuccin::OVERLAY0),
                            ),
                        );
                        ui.label(
                            egui::RichText::new("(Dynamic debugging requires Windows)")
                                .color(catppuccin::SUBTEXT0)
                                .small(),
                        );
                    }

                    #[cfg(not(target_os = "macos"))]
                    {
                        if state.debug.domain.debug_state().attached_pid.is_some() {
                            if ui
                                .button(egui::RichText::new("⏹ Detach").color(catppuccin::RED))
                                .clicked()
                            {
                                action = MenuAction::DetachProcess;
                                ui.close_menu();
                            }
                            ui.separator();
                            let mode_text = if state.ui.dynamic_mode {
                                "○ Switch to Static Mode"
                            } else {
                                "● Switch to Dynamic Mode"
                            };
                            if ui
                                .button(egui::RichText::new(mode_text).color(catppuccin::TEAL))
                                .clicked()
                            {
                                state.ui.dynamic_mode = !state.ui.dynamic_mode;
                                ui.close_menu();
                            }
                        } else {
                            if ui
                                .button(
                                    egui::RichText::new("🔗 Attach to Process...")
                                        .color(catppuccin::GREEN),
                                )
                                .clicked()
                            {
                                action = MenuAction::AttachToProcess;
                                ui.close_menu();
                            }
                            ui.separator();
                            let mode_text = if state.ui.dynamic_mode {
                                "○ Switch to Static Mode"
                            } else {
                                "● Switch to Dynamic Mode"
                            };
                            if ui
                                .button(egui::RichText::new(mode_text).color(catppuccin::TEAL))
                                .clicked()
                            {
                                state.ui.dynamic_mode = !state.ui.dynamic_mode;
                                ui.close_menu();
                            }
                        }
                    }
                });

                ui.menu_button(egui::RichText::new("View").color(catppuccin::TEXT), |ui| {
                    if ui
                        .button(
                            egui::RichText::new(if state.ui.sidebar_visible {
                                "Hide Side Bar"
                            } else {
                                "Show Side Bar"
                            })
                            .color(catppuccin::TEXT),
                        )
                        .clicked()
                    {
                        state.ui.sidebar_visible = !state.ui.sidebar_visible;
                        ui.close_menu();
                    }
                    if ui
                        .button(
                            egui::RichText::new(if state.ui.panel_visible {
                                "Hide Panel"
                            } else {
                                "Show Panel"
                            })
                            .color(catppuccin::TEXT),
                        )
                        .clicked()
                    {
                        state.ui.panel_visible = !state.ui.panel_visible;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .button(
                            egui::RichText::new("🔗 Cross-References").color(catppuccin::LAVENDER),
                        )
                        .clicked()
                    {
                        action = MenuAction::ShowXrefs;
                        ui.close_menu();
                    }
                    if ui
                        .button(
                            egui::RichText::new("🔤 String Cross-References").color(catppuccin::PEACH),
                        )
                        .clicked()
                    {
                        action = MenuAction::ShowStringXrefs;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new("Bottom Panel:")
                            .color(catppuccin::SUBTEXT0)
                            .small(),
                    );
                    use super::super::core::state::BottomTab;

                    let tabs = [
                        (BottomTab::Console, "Console", catppuccin::BLUE),
                        (BottomTab::HexView, "Hex View", catppuccin::PEACH),
                        (BottomTab::Strings, "Strings", catppuccin::GREEN),
                        (BottomTab::Imports, "Imports", catppuccin::MAUVE),
                        (BottomTab::Debug, "Debug", catppuccin::RED),
                    ];

                    for (tab, label, color) in tabs {
                        let is_selected = state.ui.bottom_tab == tab;
                        let text = if is_selected {
                            egui::RichText::new(format!("● {}", label)).color(color)
                        } else {
                            egui::RichText::new(format!("  {}", label)).color(catppuccin::SUBTEXT0)
                        };
                        if ui.selectable_label(is_selected, text).clicked() {
                            state.ui.bottom_tab = tab;
                            ui.close_menu();
                        }
                    }

                    ui.separator();
                    if ui
                        .button(egui::RichText::new("🗑 Clear Console").color(catppuccin::YELLOW))
                        .clicked()
                    {
                        action = MenuAction::ClearConsole;
                        ui.close_menu();
                    }
                });

                ui.menu_button(egui::RichText::new("Tools").color(catppuccin::TEXT), |ui| {
                    // Project-related tools
                    if state.analysis.domain.project_folder.is_some() {
                        if ui
                            .button(
                                egui::RichText::new("⚡ Batch Decompile Project")
                                    .color(catppuccin::GREEN),
                            )
                            .clicked()
                        {
                            action = MenuAction::BatchDecompile;
                            ui.close_menu();
                        }
                        if ui
                            .button(
                                egui::RichText::new("💾 Export Results (JSON/CSV)")
                                    .color(catppuccin::PEACH),
                            )
                            .clicked()
                        {
                            action = MenuAction::ExportResults;
                            ui.close_menu();
                        }
                        ui.separator();
                    }
                    
                    if ui
                        .button(
                            egui::RichText::new("🗑 Clear Decompile Cache")
                                .color(catppuccin::YELLOW),
                        )
                        .clicked()
                    {
                        action = MenuAction::ClearCache;
                        ui.close_menu();
                    }
                });

                ui.menu_button(egui::RichText::new("Help").color(catppuccin::TEXT), |ui| {
                    if ui
                        .button(egui::RichText::new("ℹ About").color(catppuccin::SAPPHIRE))
                        .clicked()
                    {
                        action = MenuAction::ShowAbout;
                        ui.close_menu();
                    }
                });

                // Right-aligned title
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("FISSION")
                            .color(catppuccin::LAVENDER)
                            .strong(),
                    );
                });
            });
        });

    action
}
