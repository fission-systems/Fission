//! Debug tab panel - Debugger controls, events, breakpoints, and registers.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use crate::ui::gui::state::{AppState, DebugAction, DebugBpAction};
use crate::ui::gui::theme::{catppuccin, code};

/// Render debug tab with improved layout
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let available_height = ui.available_height();
    
    // ═══════════════════════════════════════════════════════════════
    // TOP CONTROL BAR
    // ═══════════════════════════════════════════════════════════════
    egui::Frame::none()
        .fill(catppuccin::SURFACE0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .rounding(4.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Mode toggle with icon
                let (mode_icon, mode_text, mode_color) = if state.dynamic_mode {
                    ("⚡", "Dynamic", catppuccin::GREEN)
                } else {
                    ("📖", "Static", catppuccin::OVERLAY1)
                };
                if ui.button(egui::RichText::new(format!("{} {}", mode_icon, mode_text))
                    .color(mode_color).strong()).clicked() {
                    state.dynamic_mode = !state.dynamic_mode;
                }
                
                ui.add_space(8.0);
                
                // Status badge
                let (status_icon, status_text, status_color) = match state.debug_state.status {
                    crate::debug::types::DebugStatus::Running => ("▶", "Running", catppuccin::GREEN),
                    crate::debug::types::DebugStatus::Suspended => ("⏸", "Suspended", catppuccin::YELLOW),
                    crate::debug::types::DebugStatus::Terminated => ("⏹", "Terminated", catppuccin::RED),
                    crate::debug::types::DebugStatus::Attaching => ("🔗", "Attaching", catppuccin::BLUE),
                    _ => ("○", "Detached", catppuccin::OVERLAY0),
                };
                
                egui::Frame::none()
                    .fill(status_color.linear_multiply(0.2))
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                    .rounding(3.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(format!("{} {}", status_icon, status_text))
                            .color(status_color).strong());
                    });
                
                // PID if attached
                if let Some(pid) = state.debug_state.attached_pid {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("PID: {}", pid))
                        .color(catppuccin::SUBTEXT0).small());
                }
                
                // Last event (truncated)
                if let Some(ev) = &state.debug_state.last_event {
                    ui.add_space(8.0);
                    let display = if ev.len() > 40 { format!("{}...", &ev[..40]) } else { ev.clone() };
                    ui.label(egui::RichText::new(display)
                        .color(catppuccin::YELLOW).small().italics());
                }
                
                // Right-aligned control buttons
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Step button
                    if ui.add(egui::Button::new(
                        egui::RichText::new("⏭ Step").color(catppuccin::SAPPHIRE))
                        .fill(catppuccin::SURFACE1)
                    ).clicked() {
                        state.pending_debug_action = Some(DebugAction::Step);
                    }
                    
                    ui.add_space(4.0);
                    
                    // Continue button
                    if ui.add(egui::Button::new(
                        egui::RichText::new("▶ Continue").color(catppuccin::GREEN))
                        .fill(catppuccin::SURFACE1)
                    ).clicked() {
                        state.pending_debug_action = Some(DebugAction::Continue);
                    }
                });
            });
        });
    
    ui.add_space(4.0);
    
    // ═══════════════════════════════════════════════════════════════
    // MAIN CONTENT - 3 Column Layout
    // ═══════════════════════════════════════════════════════════════
    let content_height = (available_height - 50.0).max(80.0);
    
    ui.horizontal(|ui| {
        let panel_width = (ui.available_width() - 16.0) / 3.0;
        
        // ─────────────────────────────────────────────────────────
        // COLUMN 1: Events Log
        // ─────────────────────────────────────────────────────────
        render_events_column(ui, state, panel_width, content_height);
        
        ui.add_space(4.0);
        
        // ─────────────────────────────────────────────────────────
        // COLUMN 2: Breakpoints
        // ─────────────────────────────────────────────────────────
        render_breakpoints_column(ui, state, panel_width, content_height);
        
        ui.add_space(4.0);
        
        // ─────────────────────────────────────────────────────────
        // COLUMN 3: Registers
        // ─────────────────────────────────────────────────────────
        render_registers_column(ui, state, panel_width, content_height);
    });
}

fn render_events_column(ui: &mut egui::Ui, state: &AppState, panel_width: f32, content_height: f32) {
    egui::Frame::none()
        .fill(catppuccin::MANTLE)
        .inner_margin(6.0)
        .rounding(4.0)
        .show(ui, |ui| {
            ui.set_width(panel_width);
            
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("📋 Events")
                    .color(catppuccin::LAVENDER).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{}", state.log_buffer.len()))
                        .color(catppuccin::OVERLAY0).small());
                });
            });
            
            ui.separator();
            
            ui.push_id("events_table", |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::remainder())
                    .min_scrolled_height(0.0)
                    .max_scroll_height(content_height - 30.0)
                    .body(|body| {
                        let logs: Vec<_> = state.log_buffer.iter().rev().take(100).collect();
                        body.rows(16.0, logs.len(), |mut row| {
                            let log = logs[row.index()];
                            row.col(|ui| {
                                let (icon, color) = get_log_style(log);
                                ui.label(egui::RichText::new(format!("{} {}", icon, log))
                                    .color(color).small());
                            });
                        });
                    });
            });
        });
}

fn render_breakpoints_column(ui: &mut egui::Ui, state: &mut AppState, panel_width: f32, content_height: f32) {
    egui::Frame::none()
        .fill(catppuccin::MANTLE)
        .inner_margin(6.0)
        .rounding(4.0)
        .show(ui, |ui| {
            ui.set_width(panel_width);
            
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🎯 Breakpoints")
                    .color(catppuccin::PEACH).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{}", state.debug_state.breakpoints.len()))
                        .color(catppuccin::OVERLAY0).small());
                });
            });
            
            ui.separator();
            
            // Add breakpoint input
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("0x").color(catppuccin::OVERLAY1).monospace());
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.breakpoint_input)
                        .id(egui::Id::new("bp_addr_input"))
                        .desired_width(ui.available_width() - 30.0)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("address...")
                );
                
                if ui.add(egui::Button::new(
                    egui::RichText::new("+").color(catppuccin::GREEN).strong())
                    .min_size(egui::vec2(24.0, 20.0))
                ).clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    if let Ok(addr) = u64::from_str_radix(
                        state.breakpoint_input.trim_start_matches("0x"), 16
                    ) {
                        state.pending_bp_action = Some(DebugBpAction::Add(addr));
                        state.breakpoint_input.clear();
                    }
                }
            });
            
            ui.add_space(4.0);
            
            // Breakpoint list
            ui.push_id("bp_list", |ui| {
                TableBuilder::new(ui)
                    .striped(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::exact(20.0))  // Status
                    .column(Column::remainder())  // Address
                    .column(Column::exact(24.0))  // Delete
                    .min_scrolled_height(0.0)
                    .max_scroll_height(content_height - 60.0)
                    .body(|body| {
                        let bps: Vec<_> = state.debug_state.breakpoints.iter().collect();
                        body.rows(20.0, bps.len(), |mut row| {
                            let (addr, bp) = bps[row.index()];
                            
                            row.col(|ui| {
                                let (icon, color) = if bp.enabled {
                                    ("●", catppuccin::RED)
                                } else {
                                    ("○", catppuccin::OVERLAY0)
                                };
                                ui.label(egui::RichText::new(icon).color(color));
                            });
                            
                            row.col(|ui| {
                                ui.label(egui::RichText::new(format!("0x{:016X}", addr))
                                    .color(catppuccin::SUBTEXT1).monospace());
                            });
                            
                            row.col(|ui| {
                                if ui.small_button(egui::RichText::new("×")
                                    .color(catppuccin::RED)).clicked() {
                                    state.pending_bp_action = Some(DebugBpAction::Remove(*addr));
                                }
                            });
                        });
                    });
            });
            
            if state.debug_state.breakpoints.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new("No breakpoints set")
                        .color(catppuccin::OVERLAY0).italics());
                });
            }
        });
}

fn render_registers_column(ui: &mut egui::Ui, state: &AppState, panel_width: f32, content_height: f32) {
    egui::Frame::none()
        .fill(catppuccin::MANTLE)
        .inner_margin(6.0)
        .rounding(4.0)
        .show(ui, |ui| {
            ui.set_width(panel_width);
            
            ui.label(egui::RichText::new("📊 Registers")
                .color(catppuccin::SAPPHIRE).strong());
            
            ui.separator();
            
            if let Some(regs) = &state.debug_state.registers {
                egui::ScrollArea::vertical()
                    .id_source("registers_scroll")
                    .max_height(content_height - 30.0)
                    .show(ui, |ui| {
                        egui::Grid::new("regs_grid")
                            .num_columns(2)
                            .spacing([8.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                let registers = [
                                    ("RAX", regs.rax), ("RBX", regs.rbx),
                                    ("RCX", regs.rcx), ("RDX", regs.rdx),
                                    ("RSI", regs.rsi), ("RDI", regs.rdi),
                                    ("RBP", regs.rbp), ("RSP", regs.rsp),
                                    ("R8 ", regs.r8),  ("R9 ", regs.r9),
                                    ("R10", regs.r10), ("R11", regs.r11),
                                    ("R12", regs.r12), ("R13", regs.r13),
                                    ("R14", regs.r14), ("R15", regs.r15),
                                    ("RIP", regs.rip), ("FLG", regs.rflags),
                                ];
                                
                                for (name, value) in registers {
                                    ui.label(egui::RichText::new(name)
                                        .color(code::REGISTER).strong().monospace());
                                    ui.label(egui::RichText::new(format!("{:016X}", value))
                                        .color(catppuccin::TEXT).monospace());
                                    ui.end_row();
                                }
                            });
                    });
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(egui::RichText::new("⏸")
                        .color(catppuccin::OVERLAY0).size(24.0));
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("No register data")
                        .color(catppuccin::OVERLAY0));
                    ui.label(egui::RichText::new("Attach to a process to view registers")
                        .color(catppuccin::OVERLAY0).small().italics());
                });
            }
        });
}

fn get_log_style(log: &str) -> (&'static str, egui::Color32) {
    if log.contains("BP hit") || log.contains("Breakpoint") {
        ("🔴", catppuccin::RED)
    } else if log.contains("Exception") {
        ("⚠", catppuccin::MAROON)
    } else if log.contains("Single step") {
        ("→", catppuccin::YELLOW)
    } else if log.contains("Process") {
        ("📦", catppuccin::BLUE)
    } else if log.contains("Thread") {
        ("🧵", catppuccin::TEAL)
    } else if log.contains("DLL") || log.contains("Loaded") {
        ("📚", catppuccin::PEACH)
    } else if log.starts_with("[✓]") {
        ("✓", catppuccin::GREEN)
    } else if log.starts_with("[✗]") || log.starts_with("[!]") {
        ("✗", catppuccin::RED)
    } else {
        ("·", catppuccin::SUBTEXT0)
    }
}

