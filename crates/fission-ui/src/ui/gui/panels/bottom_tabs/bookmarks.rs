//! Bookmarks panel - Manage saved locations in the binary.

use crate::ui::gui::core::state::AppState;
use crate::ui::gui::theme::catppuccin;
use eframe::egui;

/// Render the bookmarks list tab.
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new("📌 Bookmarks").color(catppuccin::LAVENDER));
            ui.separator();
            ui.label(
                egui::RichText::new(format!("{} items", state.analysis.domain.bookmarks.len()))
                    .small()
                    .color(catppuccin::OVERLAY0),
            );
        });

        ui.separator();

        if state.analysis.domain.bookmarks.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new(
                        "No bookmarks set.\nRight-click in Assembly view to add one.",
                    )
                    .color(catppuccin::OVERLAY0)
                    .italics(),
                );
            });
            return;
        }

        let mut delete_req = None;
        let mut jump_req = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("bookmarks_grid")
                .num_columns(3)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    // Header
                    ui.label(egui::RichText::new("Address").strong());
                    ui.label(egui::RichText::new("Label").strong());
                    ui.label(""); // Actions
                    ui.end_row();

                    // Sort addresses for consistent display
                    let mut addrs: Vec<_> =
                        state.analysis.domain.bookmarks.keys().cloned().collect();
                    addrs.sort_unstable();

                    for addr in addrs {
                        let label = state.analysis.domain.bookmarks.get(&addr).unwrap();

                        // Address
                        if ui
                            .link(egui::RichText::new(format!("0x{:08X}", addr)).monospace())
                            .clicked()
                        {
                            jump_req = Some(addr);
                        }

                        // Label
                        ui.label(label);

                        // Actions
                        ui.horizontal(|ui| {
                            if ui.button("🗑").on_hover_text("Remove Bookmark").clicked() {
                                delete_req = Some(addr);
                            }
                        });

                        ui.end_row();
                    }
                });
        });

        // Handle requests
        if let Some(addr) = delete_req {
            state.analysis.domain.bookmarks.remove(&addr);
            state.log(format!("[*] Bookmark removed: 0x{:08X}", addr));
        }

        if let Some(addr) = jump_req {
            state.ui.pending_jump = Some(addr);
        }
    });
}
