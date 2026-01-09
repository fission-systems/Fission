//! Cross-References (Xrefs) window panel.
//!
//! Displays references to and from a selected address.

use crate::analysis::xrefs::XrefType;
use crate::ui::gui::core::state::AppState;
use crate::ui::gui::theme::catppuccin;
use crate::ui::gui::components::widgets::empty_state;
use eframe::egui;

/// Action from xrefs window
pub enum XrefAction {
    /// Navigate to an address
    NavigateTo(u64),
    /// No action
    None,
}

/// Render the cross-references window.
/// Returns an action if user clicked on an address.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> XrefAction {
    let mut action = XrefAction::None;

    if !state.ui.show_xrefs_window {
        return action;
    }

    let mut open = state.ui.show_xrefs_window;

    egui::Window::new("🔗 Cross-References")
        .open(&mut open)
        .collapsible(true)
        .resizable(true)
        .default_width(400.0)
        .default_height(300.0)
        .show(ctx, |ui| {
            // Address input
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Address:").color(catppuccin::SUBTEXT0));

                // If we have a selected function, use its address
                if let Some(ref func) = state.analysis.selected_function {
                    let addr = func.address;
                    ui.label(
                        egui::RichText::new(format!("0x{:08X}", addr))
                            .monospace()
                            .color(catppuccin::BLUE),
                    );

                    // Update selected xref addr
                    if state.ui.selected_xref_addr != Some(addr) {
                        state.ui.selected_xref_addr = Some(addr);
                    }
                } else if let Some(addr) = state.ui.selected_xref_addr {
                    ui.label(
                        egui::RichText::new(format!("0x{:08X}", addr))
                            .monospace()
                            .color(catppuccin::BLUE),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Select a function")
                            .color(catppuccin::OVERLAY0)
                            .italics(),
                    );
                }
            });

            ui.separator();

            // Check if we have xref database
            if state.analysis.xref_db.is_none() {
                empty_state(
                    ui,
                    "No cross-references available",
                    Some("Load a binary to analyze references"),
                );
                return;
            }

            let xref_db = state.analysis.xref_db.as_ref().unwrap();

            if let Some(addr) = state.ui.selected_xref_addr {
                // Two columns: REFS TO and REFS FROM
                ui.columns(2, |columns| {
                    // REFS TO (who calls this address?)
                    columns[0].vertical(|ui| {
                        ui.label(
                            egui::RichText::new("REFS TO")
                                .strong()
                                .color(catppuccin::GREEN),
                        );
                        ui.label(
                            egui::RichText::new("(Who calls this?)")
                                .small()
                                .color(catppuccin::SUBTEXT0),
                        );
                        ui.add_space(4.0);

                        let refs_to = xref_db.get_refs_to(addr);
                        if refs_to.is_empty() {
                            ui.label(
                                egui::RichText::new("No references")
                                    .color(catppuccin::OVERLAY0)
                                    .small()
                                    .italics(),
                            );
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("refs_to")
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    for xref in refs_to {
                                        let type_icon = match xref.xref_type {
                                            XrefType::Call => "📞",
                                            XrefType::Jump => "↪",
                                            XrefType::Data => "📦",
                                        };

                                        let label = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{} 0x{:08X}",
                                                    type_icon, xref.from_addr
                                                ))
                                                .monospace()
                                                .color(catppuccin::SAPPHIRE),
                                            )
                                            .sense(egui::Sense::click()),
                                        );

                                        if label.clicked() {
                                            action = XrefAction::NavigateTo(xref.from_addr);
                                        }
                                        if label.hovered() {
                                            ui.output_mut(|o| {
                                                o.cursor_icon = egui::CursorIcon::PointingHand
                                            });
                                        }
                                    }
                                });
                        }
                    });

                    // REFS FROM (what does this address call?)
                    columns[1].vertical(|ui| {
                        ui.label(
                            egui::RichText::new("REFS FROM")
                                .strong()
                                .color(catppuccin::PEACH),
                        );
                        ui.label(
                            egui::RichText::new("(What does this call?)")
                                .small()
                                .color(catppuccin::SUBTEXT0),
                        );
                        ui.add_space(4.0);

                        // For refs from, we need to check all addresses in the function range
                        // For simplicity, just check the function start address
                        let refs_from = xref_db.get_refs_from(addr);
                        if refs_from.is_empty() {
                            ui.label(
                                egui::RichText::new("No references")
                                    .color(catppuccin::OVERLAY0)
                                    .small()
                                    .italics(),
                            );
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("refs_from")
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    for xref in refs_from {
                                        let type_icon = match xref.xref_type {
                                            XrefType::Call => "📞",
                                            XrefType::Jump => "↪",
                                            XrefType::Data => "📦",
                                        };

                                        let label = ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{} 0x{:08X}",
                                                    type_icon, xref.to_addr
                                                ))
                                                .monospace()
                                                .color(catppuccin::SAPPHIRE),
                                            )
                                            .sense(egui::Sense::click()),
                                        );

                                        if label.clicked() {
                                            action = XrefAction::NavigateTo(xref.to_addr);
                                        }
                                        if label.hovered() {
                                            ui.output_mut(|o| {
                                                o.cursor_icon = egui::CursorIcon::PointingHand
                                            });
                                        }
                                    }
                                });
                        }
                    });
                });

                // Summary
                ui.add_space(8.0);
                ui.separator();
                let refs_to_count = xref_db.get_refs_to(addr).len();
                let refs_from_count = xref_db.get_refs_from(addr).len();
                ui.label(
                    egui::RichText::new(format!(
                        "Total: {} refs to, {} refs from",
                        refs_to_count, refs_from_count
                    ))
                    .small()
                    .color(catppuccin::SUBTEXT0),
                );
            } else {
                empty_state(ui, "Select a function to view cross-references", None);
            }
        });

    state.ui.show_xrefs_window = open;
    action
}
