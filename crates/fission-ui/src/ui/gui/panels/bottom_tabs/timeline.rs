use crate::debug::ttd::Timeline;
use crate::ui::gui::core::state::DebugAction;
use crate::ui::gui::theme::catppuccin;
use eframe::egui;

/// Render the timeline panel for Time Travel Debugging
pub fn render(ui: &mut egui::Ui, timeline: &mut Timeline) -> Option<DebugAction> {
    let mut action = None;

    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("⏱ Time Travel").color(catppuccin::MAUVE));

        ui.separator();

        // Recording controls
        let is_recording = timeline.is_recording();
        let is_replay = timeline.is_replay_mode();

        // Record button
        if is_recording {
            if ui
                .button(egui::RichText::new("⏹ Stop").color(catppuccin::RED))
                .clicked()
            {
                timeline.stop_recording();
            }
        } else if !is_replay
            && ui
                .button(egui::RichText::new("⏺ Record").color(catppuccin::PEACH))
                .clicked()
        {
            timeline.start_recording();
        }

        // Replay mode controls
        if timeline.snapshot_count() > 0 && !is_recording {
            ui.separator();

            if !is_replay {
                if ui.button("▶ Replay Mode").clicked() {
                    timeline.enter_replay_mode();
                }
            } else if ui.button("✕ Exit Replay").clicked() {
                timeline.exit_replay_mode();
            }
        }

        // Stats
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let stats = timeline.stats();
            ui.label(
                egui::RichText::new(format!(
                    "Steps: {} | Memory: {:.1} KB",
                    stats.count,
                    stats.memory_bytes as f64 / 1024.0
                ))
                .color(catppuccin::SUBTEXT0)
                .small(),
            );
        });
    });

    ui.separator();

    // Show content based on state
    if timeline.snapshot_count() == 0 {
        render_empty(ui, timeline.is_recording());
    } else if timeline.is_replay_mode() {
        action = render_replay_controls(ui, timeline);
    } else {
        render_recording_info(ui, timeline);
    }

    action
}

/// Render empty state
fn render_empty(ui: &mut egui::Ui, is_recording: bool) {
    ui.vertical_centered(|ui| {
        ui.add_space(20.0);
        if is_recording {
            ui.label(egui::RichText::new("Recording...").color(catppuccin::PEACH));
            ui.label(
                egui::RichText::new("Step through the debugger to capture execution")
                    .color(catppuccin::SUBTEXT0)
                    .small(),
            );
        } else {
            ui.label(egui::RichText::new("No recording available").color(catppuccin::OVERLAY0));
            ui.label(
                egui::RichText::new("Start recording while debugging to enable Time Travel")
                    .color(catppuccin::SUBTEXT0)
                    .small(),
            );
        }
    });
}

/// Render recording info (not in replay mode)
fn render_recording_info(ui: &mut egui::Ui, timeline: &Timeline) {
    let stats = timeline.stats();

    ui.horizontal(|ui| {
        ui.label(format!("📊 {} snapshots recorded", stats.count));
        if let Some(duration) = timeline.duration() {
            ui.label(format!("| Duration: {:.1}s", duration.as_secs_f64()));
        }
    });

    // Show latest snapshot info
    if let Some(snap) = timeline.latest_snapshot() {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Latest Snapshot").color(catppuccin::LAVENDER));
            ui.horizontal(|ui| {
                ui.label(format!("Step: {}", snap.step_index));
                ui.separator();
                ui.label(format!("RIP: 0x{:016x}", snap.registers.rip));
                ui.separator();
                ui.label(format!("Thread: {}", snap.thread_id));
            });
        });
    }
}

/// Render replay mode controls
fn render_replay_controls(ui: &mut egui::Ui, timeline: &mut Timeline) -> Option<DebugAction> {
    let mut action = None;
    let (min_step, max_step) = timeline.step_range().unwrap_or((0, 0));
    let current = timeline.current_position().unwrap_or(0);

    // Timeline slider
    ui.horizontal(|ui| {
        // Seek to start
        if ui.button("⏮").on_hover_text("Go to start").clicked() {
            action = Some(DebugAction::Seek(min_step));
        }

        // Reverse Continue
        if ui.button("⟪").on_hover_text("Reverse Continue").clicked() {
            action = Some(DebugAction::ReverseContinue);
        }

        // Rewind
        if ui
            .button("⏪")
            .on_hover_text("Rewind 1 step (Reverse Step)")
            .clicked()
        {
            action = Some(DebugAction::ReverseStep);
        }

        // Slider
        let mut slider_pos = current as f64;
        let response = ui.add(
            egui::Slider::new(&mut slider_pos, min_step as f64..=max_step as f64).show_value(false),
        );

        if response.changed() {
            action = Some(DebugAction::Seek(slider_pos as u64));
        }

        // Forward
        if ui.button("⏩").on_hover_text("Forward 1 step").clicked() {
            action = Some(DebugAction::Step);
        }

        // Forward Continue
        if ui.button("⟫").on_hover_text("Continue").clicked() {
            action = Some(DebugAction::Continue);
        }

        // Seek to end
        if ui.button("⏭").on_hover_text("Go to end").clicked() {
            action = Some(DebugAction::Seek(max_step));
        }

        // Position indicator
        ui.label(format!("Step {}/{}", current, max_step));
    });

    ui.separator();

    // Current snapshot details
    if let Some(snap) = timeline.current_snapshot() {
        egui::Grid::new("ttd_snapshot_grid")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                // Row 1: Basic info
                ui.label(egui::RichText::new("RIP").color(catppuccin::MAUVE));
                ui.label(format!("0x{:016x}", snap.registers.rip));
                ui.label(egui::RichText::new("RSP").color(catppuccin::MAUVE));
                ui.label(format!("0x{:016x}", snap.registers.rsp));
                ui.end_row();

                // Row 2: More registers
                ui.label(egui::RichText::new("RAX").color(catppuccin::PEACH));
                ui.label(format!("0x{:016x}", snap.registers.rax));
                ui.label(egui::RichText::new("RBX").color(catppuccin::PEACH));
                ui.label(format!("0x{:016x}", snap.registers.rbx));
                ui.end_row();

                ui.label(egui::RichText::new("RCX").color(catppuccin::PEACH));
                ui.label(format!("0x{:016x}", snap.registers.rcx));
                ui.label(egui::RichText::new("RDX").color(catppuccin::PEACH));
                ui.label(format!("0x{:016x}", snap.registers.rdx));
                ui.end_row();
            });

        // Memory deltas
        if !snap.memory_deltas.is_empty() {
            ui.separator();
            ui.label(
                egui::RichText::new(format!("Memory Changes ({})", snap.memory_deltas.len()))
                    .color(catppuccin::YELLOW),
            );

            egui::ScrollArea::vertical()
                .max_height(100.0)
                .show(ui, |ui| {
                    for delta in &snap.memory_deltas {
                        ui.horizontal(|ui| {
                            ui.label(format!("0x{:x}:", delta.address));
                            ui.label(
                                egui::RichText::new(format!("{:02x?}", &delta.old_value))
                                    .color(catppuccin::RED)
                                    .small(),
                            );
                            ui.label("→");
                            ui.label(
                                egui::RichText::new(format!("{:02x?}", &delta.new_value))
                                    .color(catppuccin::GREEN)
                                    .small(),
                            );
                        });
                    }
                });
        }
    }

    action
}
