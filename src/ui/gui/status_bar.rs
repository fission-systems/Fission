use eframe::egui::{self, Color32};
use crate::ui::gui::state::{AppState, Activity};
use crate::ui::gui::theme::catppuccin;

/// Render the bottom status bar
pub fn render(ctx: &egui::Context, state: &AppState) {
    egui::TopBottomPanel::bottom("status_bar")
        .default_height(24.0)
        .max_height(24.0)
        .show(ctx, |ui| {
            // Apply status bar background style
            // Apply status bar background style
            let bg_color = match state.ui.active_activity {
                Activity::Debug => {
                    if ctx.style().visuals.dark_mode {
                        catppuccin::MAROON 
                    } else {
                        Color32::from_rgb(200, 50, 50) // Reddish for light mode debug
                    }
                },
                _ => ctx.style().visuals.extreme_bg_color, // Use theme's bottom/extreme background
            };
            
            ui.painter().rect_filled(ui.max_rect(), 0.0, bg_color);
            
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                
                // Left: Activity Indicator
                let mode_icon = match state.ui.active_activity {
                    Activity::Explorer => "📂",
                    Activity::Search => "🔍",
                    Activity::Debug => "🐞",
                    Activity::Plugins => "🔌",
                    Activity::Settings => "⚙️",
                };
                ui.label(egui::RichText::new(mode_icon).size(12.0));
                
                ui.label(egui::RichText::new(match state.ui.active_activity {
                    Activity::Explorer => "Explorer",
                    Activity::Search => "Search",
                    Activity::Debug => "Debugging",
                    Activity::Plugins => "Extensions",
                    Activity::Settings => "Settings",
                }).color(ui.visuals().text_color()).size(12.0));

                ui.add_space(10.0);
                ui.separator();
                
                // Center: Log Message (Most recent)
                if let Some(last_log) = state.log_buffer.last() {
                     ui.label(egui::RichText::new(last_log)
                        .color(ui.visuals().weak_text_color())
                        .size(11.0)
                        .italics()); // could truncate if too long
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                     ui.add_space(8.0);
                     
                     // Right: Branch
                     ui.label(egui::RichText::new("main*").small().color(ui.visuals().strong_text_color()));
                     ui.label(egui::RichText::new("").small().color(ui.visuals().strong_text_color()));
                     ui.add_space(10.0);

                     // Right: Cursor Position
                     if let Some(addr) = state.ui.selected_xref_addr {
                         ui.label(egui::RichText::new(format!("Ln {}, Col 1", 0)).small().color(ui.visuals().weak_text_color())); // Placeholder
                         ui.label(egui::RichText::new(format!("0x{:X}", addr)).monospace().size(11.0).color(ui.visuals().text_color()));
                     } else {
                         ui.label(egui::RichText::new("--").small().color(ui.visuals().weak_text_color()));
                     }

                     ui.add_space(10.0);
                     
                     // Right: Memory (Mock)
                     ui.label(egui::RichText::new("128 MB").small().color(ui.visuals().weak_text_color()));
                     ui.label(egui::RichText::new("💾").small());
                });
            });
        });
}
