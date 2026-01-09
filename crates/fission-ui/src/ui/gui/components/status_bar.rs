use crate::ui::gui::core::state::{Activity, AppState};
use crate::ui::gui::theme::catppuccin;
use eframe::egui::{self, Color32};

/// Render the bottom status bar
pub fn render(ctx: &egui::Context, state: &mut AppState) {
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
                }
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

                ui.label(
                    egui::RichText::new(match state.ui.active_activity {
                        Activity::Explorer => "Explorer",
                        Activity::Search => "Search",
                        Activity::Debug => "Debugging",
                        Activity::Plugins => "Extensions",
                        Activity::Settings => "Settings",
                    })
                    .color(ui.visuals().text_color())
                    .size(12.0),
                );

                ui.add_space(10.0);
                ui.separator();

                // Center: Log Message or Progress
                if let Some((percentage, message)) = &state.ui.progress {
                    // Render Progress Bar
                    let progress_width = 200.0;
                    let (rect, _response) = ui.allocate_exact_size(
                        egui::vec2(progress_width, 14.0),
                        egui::Sense::hover(),
                    );

                    // Background
                    ui.painter()
                        .rect_filled(rect, 2.0, ui.visuals().faint_bg_color);

                    // Fill
                    let fill_rect = egui::Rect::from_min_size(
                        rect.min,
                        egui::vec2(rect.width() * percentage, rect.height()),
                    );
                    ui.painter().rect_filled(fill_rect, 2.0, catppuccin::BLUE);

                    // Text
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{} {:.0}%", message, percentage * 100.0),
                        egui::FontId::proportional(10.0),
                        ui.visuals().text_color(),
                    );
                } else if let Some(last_log) = state.log_buffer.last() {
                    ui.label(
                        egui::RichText::new(last_log)
                            .color(ui.visuals().weak_text_color())
                            .size(11.0)
                            .italics(),
                    ); // could truncate if too long
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);

                    // Right: Branch
                    ui.label(
                        egui::RichText::new(&state.ui.git_branch)
                            .small()
                            .color(ui.visuals().strong_text_color()),
                    );
                    ui.label(
                        egui::RichText::new("")
                            .small()
                            .color(ui.visuals().strong_text_color()),
                    );
                    ui.add_space(10.0);

                    // Right: Cursor Position
                    if let Some((ln, col)) = state.ui.cursor_pos {
                        ui.label(
                            egui::RichText::new(format!("Ln {}, Col {}", ln, col))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("Ln 0, Col 0")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    }

                    if let Some(addr) = state.ui.selected_xref_addr {
                        ui.add_space(5.0);
                        ui.label(
                            egui::RichText::new(format!("0x{:X}", addr))
                                .monospace()
                                .size(11.0)
                                .color(ui.visuals().text_color()),
                        );
                    }

                    ui.add_space(10.0);

                    // Mode Switcher
                    let (mode_text, mode_color) = if state.ui.dynamic_mode {
                        ("DYNAMIC", catppuccin::RED)
                    } else {
                        ("STATIC", catppuccin::BLUE)
                    };

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(mode_text)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(mode_color)
                            .small(),
                        )
                        .clicked()
                    {
                        state.ui.dynamic_mode = !state.ui.dynamic_mode;
                    }

                    ui.add_space(10.0);

                    // Right: Memory
                    let mem_mb = state.ui.memory_usage as f64 / 1024.0 / 1024.0;
                    ui.label(
                        egui::RichText::new(format!("{:.1} MB", mem_mb))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.label(egui::RichText::new("💾").small());
                });
            });
        });
}
