//! Functions panel - displays list of functions from loaded binary.

use eframe::egui;
use crate::analysis::loader::FunctionInfo;
use super::super::state::AppState;

/// Render the functions list panel on the left side.
/// 
/// Returns the clicked function if any.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> Option<FunctionInfo> {
    let mut clicked_func: Option<FunctionInfo> = None;
    
    egui::SidePanel::left("functions_panel")
        .resizable(true)
        .default_width(180.0)
        .min_width(120.0)
        .show(ctx, |ui| {
            ui.heading("[Functions]");
            ui.separator();

            if let Some(ref binary) = state.loaded_binary {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for func in &binary.functions {
                        let label = if func.name.is_empty() {
                            format!("0x{:08x}", func.address)
                        } else {
                            format!("{} (0x{:x})", func.name, func.address)
                        };
                        
                        // Highlight selected function
                        let is_selected = state.selected_function
                            .as_ref()
                            .map(|f| f.address == func.address)
                            .unwrap_or(false);
                        
                        let response = ui.selectable_label(is_selected, &label);
                        
                        if response.clicked() {
                            clicked_func = Some(func.clone());
                        }
                    }
                });
            } else {
                ui.label("Load a binary to see functions");
            }
        });
    
    clicked_func
}
