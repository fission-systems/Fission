//! Menu bar rendering with Catppuccin theme.

use eframe::egui;
use super::state::AppState;
use super::theme::catppuccin;

/// Actions triggered from menu
pub enum MenuAction {
    OpenFile,
    AttachToProcess,
    DetachProcess,
    ClearConsole,
    ClearCache,
    ShowAbout,
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
                    if ui.button(egui::RichText::new("📂 Open Binary...")
                        .color(catppuccin::BLUE)).clicked() {
                        action = MenuAction::OpenFile;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(egui::RichText::new("🚪 Exit")
                        .color(catppuccin::RED)).clicked() {
                        action = MenuAction::Exit;
                    }
                });

                ui.menu_button(egui::RichText::new("Debug").color(catppuccin::TEXT), |ui| {
                    if state.debug_state.attached_pid.is_some() {
                        if ui.button(egui::RichText::new("⏹ Detach")
                            .color(catppuccin::RED)).clicked() {
                            action = MenuAction::DetachProcess;
                            ui.close_menu();
                        }
                        ui.separator();
                        let mode_text = if state.dynamic_mode {
                            "○ Switch to Static Mode"
                        } else {
                            "● Switch to Dynamic Mode"
                        };
                        if ui.button(egui::RichText::new(mode_text)
                            .color(catppuccin::TEAL)).clicked() {
                            state.dynamic_mode = !state.dynamic_mode;
                            ui.close_menu();
                        }
                    } else {
                        if ui.button(egui::RichText::new("🔗 Attach to Process...")
                            .color(catppuccin::GREEN)).clicked() {
                            action = MenuAction::AttachToProcess;
                            ui.close_menu();
                        }
                        ui.separator();
                        let mode_text = if state.dynamic_mode {
                            "○ Switch to Static Mode"
                        } else {
                            "● Switch to Dynamic Mode"
                        };
                        if ui.button(egui::RichText::new(mode_text)
                            .color(catppuccin::TEAL)).clicked() {
                            state.dynamic_mode = !state.dynamic_mode;
                            ui.close_menu();
                        }
                    }
                });

                ui.menu_button(egui::RichText::new("View").color(catppuccin::TEXT), |ui| {
                    ui.label(egui::RichText::new("Bottom Panel:")
                        .color(catppuccin::SUBTEXT0).small());
                    use super::state::BottomTab;
                    
                    let tabs = [
                        (BottomTab::Console, "Console", catppuccin::BLUE),
                        (BottomTab::HexView, "Hex View", catppuccin::PEACH),
                        (BottomTab::Strings, "Strings", catppuccin::GREEN),
                        (BottomTab::Imports, "Imports", catppuccin::MAUVE),
                        (BottomTab::Debug, "Debug", catppuccin::RED),
                    ];
                    
                    for (tab, label, color) in tabs {
                        let is_selected = state.bottom_tab == tab;
                        let text = if is_selected {
                            egui::RichText::new(format!("● {}", label)).color(color)
                        } else {
                            egui::RichText::new(format!("  {}", label)).color(catppuccin::SUBTEXT0)
                        };
                        if ui.selectable_label(is_selected, text).clicked() {
                            state.bottom_tab = tab;
                            ui.close_menu();
                        }
                    }
                    
                    ui.separator();
                    if ui.button(egui::RichText::new("🗑 Clear Console")
                        .color(catppuccin::YELLOW)).clicked() {
                        action = MenuAction::ClearConsole;
                        ui.close_menu();
                    }
                });

                ui.menu_button(egui::RichText::new("Tools").color(catppuccin::TEXT), |ui| {
                    if ui.button(egui::RichText::new("🗑 Clear Decompile Cache")
                        .color(catppuccin::YELLOW)).clicked() {
                        action = MenuAction::ClearCache;
                        ui.close_menu();
                    }
                });

                ui.menu_button(egui::RichText::new("Help").color(catppuccin::TEXT), |ui| {
                    if ui.button(egui::RichText::new("ℹ About")
                        .color(catppuccin::SAPPHIRE)).clicked() {
                        action = MenuAction::ShowAbout;
                        ui.close_menu();
                    }
                });
                
                // Right-aligned title
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("FISSION")
                        .color(catppuccin::LAVENDER)
                        .strong());
                });
            });
        });
    
    action
}
