//! Editor area panel - Tabbed interface for assembly, decompiled code, etc.

use eframe::egui;
use crate::ui::gui::state::{AppState, EditorTab};
use crate::ui::gui::theme::catppuccin;
use super::{assembly, decompile};

/// Render the central editor area with tabs.
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(catppuccin::BASE))
        .show(ctx, |ui| {
            render_tabs(ui, state);
            
            ui.separator();
            
            render_active_tab_content(ui, state);
        });
}

fn render_tabs(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        ui.style_mut().spacing.item_spacing.x = 0.0;
        
        let mut close_tab = None;
        
        for (i, tab) in state.open_tabs.iter().enumerate() {
            let is_active = state.active_tab_index == Some(i);
            
            let bg = if is_active { catppuccin::BASE } else { catppuccin::MANTLE };
            let text_color = if is_active { catppuccin::TEXT } else { catppuccin::OVERLAY1 };
            
            let response = egui::Frame::none()
                .fill(bg)
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.selectable_label(is_active, egui::RichText::new(tab.title()).color(text_color)).clicked() {
                            state.active_tab_index = Some(i);
                        }
                        
                        if ui.add(egui::Button::new(egui::RichText::new(" × ").small()).frame(false)).clicked() {
                            close_tab = Some(i);
                        }
                    });
                }).response;
            
            if is_active {
                // Active tab top indicator
                let rect = response.rect;
                ui.painter().line_segment(
                    [egui::pos2(rect.left(), rect.top() + 1.0), egui::pos2(rect.right(), rect.top() + 1.0)],
                    egui::Stroke::new(2.0, catppuccin::BLUE)
                );
            }
            
            ui.separator();
        }
        
        if let Some(i) = close_tab {
            state.open_tabs.remove(i);
            if state.open_tabs.is_empty() {
                state.active_tab_index = None;
            } else if state.active_tab_index == Some(i) {
                state.active_tab_index = Some(i.saturating_sub(1));
            } else if let Some(idx) = state.active_tab_index {
                if idx > i {
                    state.active_tab_index = Some(idx - 1);
                }
            }
        }
    });
}

fn render_active_tab_content(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(idx) = state.active_tab_index else {
        render_empty_state(ui);
        return;
    };
    
    let tab = &state.open_tabs[idx].clone(); // Clone to avoid borrow issues
    
    match tab {
        EditorTab::Assembly(_name) => {
            assembly::render_inside(ui, state);
        }
        EditorTab::Decompiled(_name) => {
            decompile::render_inside(ui, state);
        }
        EditorTab::HexView => {
            ui.label("Hex View in Editor Tab - Coming Soon");
        }
        EditorTab::Welcome => {
            render_welcome(ui);
        }
    }
}

fn render_empty_state(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() / 3.0);
        ui.label(egui::RichText::new("FISSION").size(40.0).strong().color(catppuccin::SURFACE1));
        ui.add_space(20.0);
        ui.label(egui::RichText::new("Open a binary to start analyzing").color(catppuccin::OVERLAY0));
    });
}

fn render_welcome(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(60.0);
        ui.label(egui::RichText::new("🔬").size(80.0));
        ui.add_space(20.0);
        ui.heading(egui::RichText::new("FISSION").size(32.0).strong().color(catppuccin::LAVENDER));
        ui.label(egui::RichText::new("Split the Binary, Fuse the Power.").italics().color(catppuccin::SUBTEXT0));
        
        ui.add_space(40.0);
        
        ui.group(|ui| {
            ui.set_width(300.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Quick Start").strong());
                ui.add_space(8.0);
                if ui.button("📂 Open Binary...").clicked() {
                    // This would need to trigger an action
                }
                ui.add_space(4.0);
                if ui.button("🪲 Attach to Process...").clicked() {
                    // This would need to trigger an action
                }
            });
        });
    });
}

