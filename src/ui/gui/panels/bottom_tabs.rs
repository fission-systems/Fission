//! Bottom tabbed panel - Console, Hex View, Strings, Imports, Debug tabs.
//! Uses virtual scrolling for large data sets.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use super::super::state::{AppState, BottomTab, ExtractedString, StringEncoding, DebugAction, DebugBpAction};
use super::super::theme::{catppuccin, code};

/// Actions that can be triggered from the console
pub enum ConsoleAction {
    Command(String),
    None,
}

/// Render the bottom tabbed panel.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> ConsoleAction {
    let mut action = ConsoleAction::None;
    
    egui::TopBottomPanel::bottom("bottom_panel")
        .resizable(true)
        .default_height(200.0)
        .min_height(120.0)
        .max_height(500.0)
        .show(ctx, |ui| {
            // Force minimum height to prevent panel collapse
            ui.set_min_height(ui.available_height());
            
            // Tab bar with styled tabs
            ui.horizontal(|ui| {
                let tabs = [
                    (BottomTab::Console, "Console", catppuccin::BLUE),
                    (BottomTab::HexView, "Hex View", catppuccin::PEACH),
                    (BottomTab::Strings, "Strings", catppuccin::GREEN),
                    (BottomTab::Imports, "Imports", catppuccin::MAUVE),
                    (BottomTab::Debug, "Debug", catppuccin::RED),
                ];
                
                for (tab, label, accent) in tabs {
                    let is_selected = state.bottom_tab == tab;
                    let text = if is_selected {
                        egui::RichText::new(label).color(accent).strong()
                    } else {
                        egui::RichText::new(label).color(catppuccin::SUBTEXT0)
                    };
                    if ui.selectable_label(is_selected, text).clicked() {
                        state.bottom_tab = tab;
                    }
                }
            });
            ui.separator();

            // Tab content - allocate remaining space to prevent collapse
            let content_rect = ui.available_rect_before_wrap();
            ui.allocate_ui_at_rect(content_rect, |ui| {
            match state.bottom_tab {
                BottomTab::Console => {
                    action = render_console(ui, state);
                }
                BottomTab::HexView => {
                    render_hexview(ui, state);
                }
                BottomTab::Strings => {
                    render_strings(ui, state);
                }
                BottomTab::Imports => {
                    render_imports(ui, state);
                }
                    BottomTab::Debug => {
                        render_debug(ui, state);
                }
            }
            });
        });
    
    action
}

/// Render debug tab with improved layout
fn render_debug(ui: &mut egui::Ui, state: &mut AppState) {
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
                                    let (icon, color) = if log.contains("BP hit") || log.contains("Breakpoint") {
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
                                    };
                                    ui.label(egui::RichText::new(format!("{} {}", icon, log))
                                        .color(color).small());
                                });
                            });
                        });
                });
            });
        
        ui.add_space(4.0);
        
        // ─────────────────────────────────────────────────────────
        // COLUMN 2: Breakpoints
        // ─────────────────────────────────────────────────────────
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
        
        ui.add_space(4.0);
        
        // ─────────────────────────────────────────────────────────
        // COLUMN 3: Registers
        // ─────────────────────────────────────────────────────────
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
    });
}

/// Render console tab content using TableBuilder for stable scrolling
fn render_console(ui: &mut egui::Ui, state: &mut AppState) -> ConsoleAction {
    let mut action = ConsoleAction::None;
    
    // Header buttons
    ui.horizontal(|ui| {
        if ui.small_button(egui::RichText::new("Clear").color(catppuccin::RED)).clicked() {
            state.log_buffer.clear();
        }
        if ui.small_button(egui::RichText::new("📋 Copy").color(catppuccin::BLUE)).clicked() {
            let all_logs = state.log_buffer.join("\n");
            ui.output_mut(|o| o.copied_text = all_logs);
        }
        ui.separator();
        ui.label(egui::RichText::new(format!("{} lines", state.log_buffer.len()))
            .color(catppuccin::SUBTEXT0).small());
    });

    // Virtual scrolling table for console logs
    let available_height = ui.available_height() - 35.0;
    let num_logs = state.log_buffer.len();
    
    TableBuilder::new(ui)
        .striped(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder())
        .min_scrolled_height(available_height.max(50.0))
        .max_scroll_height(available_height.max(50.0))
        .body(|body| {
            body.rows(16.0, num_logs, |mut row| {
                let idx = row.index();
                row.col(|ui| {
                    if let Some(log) = state.log_buffer.get(idx) {
                let color = if log.starts_with("[✓]") {
                            catppuccin::GREEN
                } else if log.starts_with("[✗]") || log.starts_with("[!]") {
                            catppuccin::RED
                } else if log.starts_with("[*]") || log.starts_with("[>]") {
                            catppuccin::BLUE
                        } else if log.starts_with(">") {
                            catppuccin::MAUVE
                } else {
                            catppuccin::SUBTEXT0
                };
                        ui.label(egui::RichText::new(log).color(color).monospace());
            }
                });
            });
        });

    // CLI input
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(">").color(catppuccin::MAUVE).strong());
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.cli_input)
                .desired_width(ui.available_width() - 60.0)
                .font(egui::TextStyle::Monospace)
                .hint_text("Enter command..."),
        );

        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let cmd = state.cli_input.trim().to_string();
            if !cmd.is_empty() {
                state.log_buffer.push(format!("> {}", cmd));
                action = ConsoleAction::Command(cmd);
                state.cli_input.clear();
            }
            response.request_focus();
        }

        if ui.button(egui::RichText::new("Run").color(catppuccin::GREEN)).clicked() {
            let cmd = state.cli_input.trim().to_string();
            if !cmd.is_empty() {
                state.log_buffer.push(format!("> {}", cmd));
                action = ConsoleAction::Command(cmd);
                state.cli_input.clear();
            }
        }
    });
    
    action
}

/// Render hex view tab content with virtual scrolling
fn render_hexview(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(ref binary) = state.loaded_binary else {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("No binary loaded")
                .color(catppuccin::OVERLAY0)
                .size(14.0));
        });
        return;
    };

    let data_len = binary.data.len() as u64;
    let rows_per_page = 64; // Virtual rows to show
    let total_rows = (data_len / 16) + if data_len % 16 != 0 { 1 } else { 0 };
    
    // Controls
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Offset:").color(catppuccin::SUBTEXT0));
        let mut offset_str = format!("{:08X}", state.hex_offset);
        if ui.add(
            egui::TextEdit::singleline(&mut offset_str)
                .desired_width(80.0)
                .font(egui::TextStyle::Monospace)
        ).changed() {
            if let Ok(new_offset) = u64::from_str_radix(&offset_str, 16) {
                state.hex_offset = (new_offset / 16) * 16; // Align to 16
                state.hex_offset = state.hex_offset.min(data_len.saturating_sub(16));
            }
        }
        
        ui.separator();
        
        if ui.small_button("⬆").clicked() && state.hex_offset >= 0x100 {
            state.hex_offset -= 0x100;
        }
        if ui.small_button("⬇").clicked() {
            state.hex_offset = (state.hex_offset + 0x100).min(data_len.saturating_sub(16));
        }
        if ui.small_button("Top").clicked() {
            state.hex_offset = 0;
        }
        if ui.small_button("End").clicked() {
            state.hex_offset = (total_rows.saturating_sub(rows_per_page as u64)) * 16;
        }
        
        ui.separator();
        ui.label(egui::RichText::new(format!("{} / {} bytes", state.hex_offset, data_len))
            .color(catppuccin::SUBTEXT0).small());
    });

    ui.separator();

    let available_height = ui.available_height();
    let row_height = 18.0;
    
    // Calculate which rows to show based on current offset
    let start_row = (state.hex_offset / 16) as usize;
    let visible_rows = ((available_height / row_height) as usize).min(rows_per_page).max(8);
    let end_row = (start_row + visible_rows).min(total_rows as usize);
    let display_rows = end_row - start_row;

    // Use TableBuilder for virtual scrolling hex view
    TableBuilder::new(ui)
                .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(75.0))   // Offset
        .column(Column::exact(380.0))  // Hex bytes
        .column(Column::remainder())   // ASCII
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height)
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.label(egui::RichText::new("Offset")
                    .strong().color(catppuccin::TEXT));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F")
                    .strong().color(catppuccin::TEXT).monospace());
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("ASCII")
                    .strong().color(catppuccin::TEXT));
            });
        })
        .body(|body| {
            body.rows(row_height, display_rows, |mut row| {
                let row_index = start_row + row.index();
                let row_offset = (row_index as u64) * 16;
                
                        if row_offset >= data_len {
                    return;
                        }
                        
                // Offset column
                row.col(|ui| {
                        ui.label(egui::RichText::new(format!("{:08X}", row_offset))
                        .color(code::ADDRESS).monospace());
                });
                        
                // Hex bytes column
                row.col(|ui| {
                        let mut hex_str = String::with_capacity(50);
                        let start = row_offset as usize;
                        let end = (row_offset + 16).min(data_len) as usize;
                        
                        if start < binary.data.len() {
                            let bytes = &binary.data[start..end.min(binary.data.len())];
                            for (i, byte) in bytes.iter().enumerate() {
                                use std::fmt::Write;
                                write!(hex_str, "{:02X} ", byte).unwrap();
                                if i == 7 { hex_str.push(' '); }
                            }
                            // Pad remaining
                            for i in bytes.len()..16 {
                                hex_str.push_str("   ");
                                if i == 7 { hex_str.push(' '); }
                        }
                    }
                    ui.label(egui::RichText::new(&hex_str)
                        .color(code::HEX_BYTE).monospace());
                });
                
                // ASCII column
                row.col(|ui| {
                    let mut ascii_str = String::with_capacity(16);
                    let start = row_offset as usize;
                    let end = (row_offset + 16).min(data_len) as usize;
                    
                    if start < binary.data.len() {
                        let bytes = &binary.data[start..end.min(binary.data.len())];
                        for byte in bytes {
                            ascii_str.push(if *byte >= 0x20 && *byte <= 0x7E { 
                                *byte as char 
                        } else {
                                '.' 
                            });
                        }
                    }
                    ui.label(egui::RichText::new(&ascii_str)
                        .color(code::ASCII_PRINTABLE).monospace());
                });
                });
        });
}

/// Render strings tab content with virtual scrolling
fn render_strings(ui: &mut egui::Ui, state: &mut AppState) {
    // Controls
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter:").color(catppuccin::SUBTEXT0));
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.strings_filter)
                .desired_width(200.0)
                .hint_text("Search strings...")
        );
        
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button(egui::RichText::new("Extract").color(catppuccin::GREEN)).clicked() || enter_pressed {
            extract_strings_from_binary(state);
        }
        
        ui.separator();
        ui.label(egui::RichText::new(format!("{} strings", state.extracted_strings.len()))
            .color(catppuccin::SUBTEXT0).small());
    });

    if state.extracted_strings.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
        if state.loaded_binary.is_some() {
                ui.label(egui::RichText::new("Click 'Extract' to find strings")
                    .color(catppuccin::OVERLAY0));
        } else {
                ui.label(egui::RichText::new("Load a binary first")
                    .color(catppuccin::OVERLAY0));
        }
        });
        return;
    }

    // Filter strings
    let filter = state.strings_filter.to_lowercase();
    let filtered_strings: Vec<_> = state.extracted_strings.iter()
        .filter(|s| filter.is_empty() || s.value.to_lowercase().contains(&filter))
        .collect();

    let available_height = ui.available_height();
    let row_height = 20.0;
    let total_rows = filtered_strings.len();

    // Virtual scrolling table for strings
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(75.0))   // Offset
        .column(Column::exact(50.0))   // Type
        .column(Column::remainder())   // String
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height)
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.label(egui::RichText::new("Offset").strong().color(catppuccin::TEXT));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Type").strong().color(catppuccin::TEXT));
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("String").strong().color(catppuccin::TEXT));
            });
        })
        .body(|body| {
            body.rows(row_height, total_rows, |mut row| {
                let s = &filtered_strings[row.index()];
                
                row.col(|ui| {
                    if ui.selectable_label(false, 
                        egui::RichText::new(format!("{:08X}", s.offset))
                            .monospace().color(code::ADDRESS)
                    ).clicked() {
                        // Navigate to hex view at this offset
                        // This requires mutable access which we don't have here
                        // Would need to return an action
                    }
                });
                
                row.col(|ui| {
                    let (type_str, color) = match s.encoding {
                        StringEncoding::Ascii => ("ASCII", catppuccin::BLUE),
                        StringEncoding::Utf16Le => ("UTF16", catppuccin::MAUVE),
                    };
                    ui.label(egui::RichText::new(type_str).color(color).small());
                });
                
                row.col(|ui| {
                    let display_str = if s.value.len() > 80 {
                        format!("{}...", &s.value[..80])
                    } else {
                        s.value.clone()
                    };
                    ui.label(egui::RichText::new(display_str)
                        .color(catppuccin::GREEN).monospace());
                });
            });
    });
}

/// Extract strings from binary
fn extract_strings_from_binary(state: &mut AppState) {
    state.extracted_strings.clear();
    
    let Some(ref binary) = state.loaded_binary else { return; };
    
    let min_len = 4;
    let data = &binary.data;
    
    // ASCII strings
    let mut current_string = String::new();
    let mut start_offset: u64 = 0;
    
    for (i, &byte) in data.iter().enumerate() {
        if byte >= 0x20 && byte <= 0x7E {
            if current_string.is_empty() { start_offset = i as u64; }
            current_string.push(byte as char);
        } else {
            if current_string.len() >= min_len {
                state.extracted_strings.push(ExtractedString {
                    offset: start_offset,
                    value: current_string.clone(),
                    encoding: StringEncoding::Ascii,
                });
            }
            current_string.clear();
        }
    }
    
    state.extracted_strings.sort_by_key(|s| s.offset);
    state.log_buffer.push(format!("[✓] Extracted {} strings", state.extracted_strings.len()));
}

/// Render imports tab content with virtual scrolling
fn render_imports(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(ref binary) = state.loaded_binary else {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("Load a binary to view imports")
                .color(catppuccin::OVERLAY0));
        });
        return;
    };

    let imports: Vec<_> = binary.functions.iter()
        .filter(|f| f.is_import)
        .collect();
    let exports: Vec<_> = binary.functions.iter()
        .filter(|f| f.is_export)
        .collect();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Imports: {}", imports.len()))
            .color(catppuccin::PEACH));
        ui.separator();
        ui.label(egui::RichText::new(format!("Exports: {}", exports.len()))
            .color(catppuccin::GREEN));
    });

    ui.separator();

    let available_height = ui.available_height();
    
    ui.columns(2, |cols| {
        // Imports column
        cols[0].label(egui::RichText::new("Imports").color(catppuccin::PEACH).strong());
        
        let import_height = (available_height - 30.0).max(50.0);
        cols[0].push_id("imports_table", |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(75.0))
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(import_height)
                .body(|body| {
                    body.rows(18.0, imports.len(), |mut row| {
                        let func = &imports[row.index()];
                        row.col(|ui| {
                            ui.label(egui::RichText::new(format!("{:08X}", func.address))
                                .monospace().color(code::ADDRESS));
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&func.name)
                                .color(catppuccin::PEACH));
                        });
                    });
                    });
            });

        // Exports column
        cols[1].label(egui::RichText::new("Exports").color(catppuccin::GREEN).strong());
        
        cols[1].push_id("exports_table", |ui| {
            TableBuilder::new(ui)
                    .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(75.0))
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(import_height)
                .body(|body| {
                    body.rows(18.0, exports.len(), |mut row| {
                        let func = &exports[row.index()];
                        row.col(|ui| {
                            ui.label(egui::RichText::new(format!("{:08X}", func.address))
                                .monospace().color(code::ADDRESS));
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(&func.name)
                                .color(catppuccin::GREEN));
                        });
                    });
                    });
            });
        });
}
