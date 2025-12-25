use eframe::egui;
use crate::ui::gui::state::AppState;
use crate::ui::gui::theme::catppuccin;

/// Render the search panel in the side bar
pub fn render(ui: &mut egui::Ui, state: &mut AppState) -> Option<super::side_bar::SideBarAction> {
    let mut action = None;

    ui.add_space(8.0);
    
    // Search Box
    let response = ui.add(egui::TextEdit::singleline(&mut state.analysis.strings_filter) // Reusing filter field for now or add new one
        .hint_text("Search functions, strings... (Ctrl+F)")
        .desired_width(f32::INFINITY));
    
    if response.changed() {
        // Trigger generic search update if needed
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    let query = state.analysis.strings_filter.to_lowercase();
    if query.is_empty() {
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("Type to search...")
                .color(catppuccin::OVERLAY0).italics());
        });
        return None;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        // 1. Search Functions
        if let Some(binary) = &state.analysis.loaded_binary {
            let mut func_matches = 0;
            ui.heading(egui::RichText::new("Functions").size(12.0).strong());
            
            for func in &binary.functions {
                if func.name.to_lowercase().contains(&query) {
                    func_matches += 1;
                    if func_matches > 50 {
                        ui.label(egui::RichText::new("... too many results").small());
                        break;
                    }

                    if ui.button(egui::RichText::new(&func.name).color(ui.visuals().strong_text_color())).clicked() {
                         action = Some(super::side_bar::SideBarAction::SelectFunction(func.clone()));
                    }
                }
            }
            if func_matches == 0 {
                ui.label(egui::RichText::new("No matching functions").small().color(ui.visuals().weak_text_color()));
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        
        // 2. Search Strings
        // (Accessing string state might be tricky if it is deeply nested or requires lock.
        //  Assuming extracted_strings is available in AnalysisState)
        
        let mut string_matches = 0;
        ui.heading(egui::RichText::new("Strings").size(12.0).strong());
        for s in &state.analysis.extracted_strings {
            if s.value.to_lowercase().contains(&query) {
                string_matches += 1;
                if string_matches > 50 {
                    ui.label(egui::RichText::new("... too many results").small());
                    break;
                }
                
                let label = format!("0x{:x}: {}", s.offset, s.value);
                if ui.button(egui::RichText::new(label).color(ui.visuals().text_color())).clicked() {
                     // Creating a dummy function info to navigate or generic navigation
                     // For now, let's abuse SelectFunction or add Navigate Action
                     // Since SideBarAction only has SelectFunction, we might need to expand it or map it.
                     // Mapping to a dummy function for navigation:
                     let dummy = crate::analysis::loader::FunctionInfo {
                         address: s.offset,
                         name: format!("String_{:x}", s.offset),
                         size: 0,
                         is_export: false,
                         is_import: false,
                     };
                     action = Some(super::side_bar::SideBarAction::SelectFunction(dummy));
                }
            }
        }
    });

    action
}
