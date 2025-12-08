//! Menu bar rendering.

use eframe::egui;
use super::state::AppState;

/// Actions triggered from menu
pub enum MenuAction {
    OpenFile,
    ToggleDebug,
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
    
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open Binary...").clicked() {
                    action = MenuAction::OpenFile;
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    action = MenuAction::Exit;
                }
            });

            ui.menu_button("Debug", |ui| {
                let attach_text = if state.is_debugging { "[Stop]" } else { "[Start]" };
                if ui.button(attach_text).clicked() {
                    action = MenuAction::ToggleDebug;
                    ui.close_menu();
                }
            });

            ui.menu_button("View", |ui| {
                ui.label("Bottom Panel Tab:");
                use super::state::BottomTab;
                ui.selectable_value(&mut state.bottom_tab, BottomTab::Console, "Console");
                ui.selectable_value(&mut state.bottom_tab, BottomTab::HexView, "Hex View");
                ui.selectable_value(&mut state.bottom_tab, BottomTab::Strings, "Strings");
                ui.separator();
                if ui.button("Clear Console").clicked() {
                    action = MenuAction::ClearConsole;
                    ui.close_menu();
                }
            });

            ui.menu_button("Tools", |ui| {
                if ui.button("Clear Decompile Cache").clicked() {
                    action = MenuAction::ClearCache;
                    ui.close_menu();
                }
            });

            ui.menu_button("Help", |ui| {
                if ui.button("About").clicked() {
                    action = MenuAction::ShowAbout;
                    ui.close_menu();
                }
            });
        });
    });
    
    action
}
