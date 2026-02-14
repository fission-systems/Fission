//! Listing View Panel - Full binary linear disassembly view with virtual scroll.
//!
//! Provides a continuous, scrollable view of the entire binary,
//! similar to IDA Pro, Ghidra, or x64dbg's Listing View.
//!
//! ## Key Design: Virtual Row Index = Address Offset
//! - Total virtual rows = (max_addr - min_addr) / AVG_INSTRUCTION_SIZE
//! - When a row is visible, we disassemble on-demand
//! - This gives true infinite scroll behavior

use crate::analysis::disasm::DisasmEngine;
use crate::ui::gui::components::widgets::empty_state_with_spacing;
use crate::ui::gui::core::state::AppState;
use crate::ui::gui::theme::{catppuccin, code};
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use std::collections::HashMap;

/// Estimated average instruction size for row calculations
const AVG_INSTRUCTION_SIZE: u64 = 4;

/// Cache size for disassembled instructions
const CACHE_SIZE: usize = 500;

/// Render the listing view panel with virtual scroll
pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    // Check if binary is loaded
    let binary = match state.analysis.loaded_binary() {
        Some(b) => b.clone(),
        None => {
            render_header_empty(ui);
            ui.separator();
            empty_state_with_spacing(
                ui,
                "No binary loaded",
                Some("Load a binary to view the listing"),
                40.0,
            );
            return;
        }
    };

    // Get code sections for disassembly
    let code_sections: Vec<_> = binary.sections.iter().filter(|s| s.is_executable).collect();

    if code_sections.is_empty() {
        render_header_empty(ui);
        ui.separator();
        empty_state_with_spacing(
            ui,
            "No executable sections found",
            Some("This binary has no code sections"),
            40.0,
        );
        return;
    }

    // Calculate total address range
    let min_addr = code_sections
        .iter()
        .map(|s| s.virtual_address)
        .min()
        .unwrap_or(0);
    let max_addr = code_sections
        .iter()
        .map(|s| s.virtual_address + s.virtual_size as u64)
        .max()
        .unwrap_or(0);

    // Initialize current address if not set
    if state.viewmodels.listing.current_address == 0
        || state.viewmodels.listing.current_address < min_addr
        || state.viewmodels.listing.current_address > max_addr
    {
        state.viewmodels.listing.current_address = binary.entry_point;
        state.viewmodels.listing.pending_scroll_to_current = true;
    }

    // Calculate virtual row parameters
    let address_range = max_addr.saturating_sub(min_addr);
    let total_virtual_rows = (address_range / AVG_INSTRUCTION_SIZE) as usize;

    // Header with controls
    render_header(ui, state, min_addr, max_addr, total_virtual_rows);
    ui.separator();

    // Handle keyboard navigation
    let available_height = ui.available_height();
    let row_height = 18.0;
    let visible_rows = ((available_height - 40.0) / row_height).max(10.0) as usize;

    handle_keyboard_input(ui, state, visible_rows, min_addr, max_addr);

    // Build function address set for boundary detection
    let function_addresses: std::collections::HashSet<u64> =
        binary.functions.iter().map(|f| f.address).collect();

    // Instruction cache for the visible range
    let cache = build_instruction_cache(
        &binary,
        state.viewmodels.listing.current_address,
        visible_rows * 2, // Cache a bit more than visible
    );

    // Calculate current row index from address
    let scroll_to_row = if state.viewmodels.listing.pending_scroll_to_current {
        state.viewmodels.listing.pending_scroll_to_current = false;
        let current_row = address_to_row(state.viewmodels.listing.current_address, min_addr);
        Some(current_row.saturating_sub(visible_rows / 2))
    } else {
        None
    };

    // Render virtual scroll table
    render_virtual_table(
        ui,
        state,
        &binary,
        &cache,
        &function_addresses,
        min_addr,
        total_virtual_rows,
        row_height,
        scroll_to_row,
    );
}

fn render_header_empty(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("📜 Listing View").color(catppuccin::LAVENDER));
    });
}

/// Render header with Go-to address input
fn render_header(
    ui: &mut egui::Ui,
    state: &mut AppState,
    min_addr: u64,
    max_addr: u64,
    total_rows: usize,
) {
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("📜 Listing View").color(catppuccin::LAVENDER));
        ui.separator();

        // Go to address input
        ui.label("Go to:");
        let goto_input = ui.add(
            egui::TextEdit::singleline(&mut state.viewmodels.listing.goto_address_input)
                .desired_width(100.0)
                .font(egui::TextStyle::Monospace)
                .hint_text("0x..."),
        );

        if goto_input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(addr) = parse_address(&state.viewmodels.listing.goto_address_input) {
                state.viewmodels.listing.current_address = addr.clamp(min_addr, max_addr);
                state.viewmodels.listing.pending_scroll_to_current = true;
                state.viewmodels.listing.goto_address_input.clear();
            }
        }

        // Entry point button
        if ui.button("🏠").on_hover_text("Entry point").clicked() {
            if let Some(binary) = state.analysis.loaded_binary() {
                state.viewmodels.listing.current_address = binary.entry_point;
                state.viewmodels.listing.pending_scroll_to_current = true;
            }
        }

        ui.separator();

        // Current address
        ui.label(
            egui::RichText::new(format!(
                "📍 {:08X}",
                state.viewmodels.listing.current_address
            ))
            .color(catppuccin::PEACH)
            .monospace(),
        );

        ui.separator();

        // Info
        ui.label(
            egui::RichText::new(format!(
                "{:08X}-{:08X} | {} rows",
                min_addr, max_addr, total_rows
            ))
            .color(catppuccin::SUBTEXT0)
            .small(),
        );
    });
}

/// Handle keyboard navigation
fn handle_keyboard_input(
    ui: &mut egui::Ui,
    state: &mut AppState,
    visible_rows: usize,
    min_addr: u64,
    max_addr: u64,
) {
    let ctx = ui.ctx();
    let step = AVG_INSTRUCTION_SIZE;
    let page = visible_rows as u64 * step;

    if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
        state.viewmodels.listing.current_address = state
            .viewmodels
            .listing
            .current_address
            .saturating_sub(step)
            .max(min_addr);
        state.viewmodels.listing.pending_scroll_to_current = true;
        ctx.request_repaint();
    }

    if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
        state.viewmodels.listing.current_address = state
            .viewmodels
            .listing
            .current_address
            .saturating_add(step)
            .min(max_addr);
        state.viewmodels.listing.pending_scroll_to_current = true;
        ctx.request_repaint();
    }

    if ctx.input(|i| i.key_pressed(egui::Key::PageUp)) {
        state.viewmodels.listing.current_address = state
            .viewmodels
            .listing
            .current_address
            .saturating_sub(page)
            .max(min_addr);
        state.viewmodels.listing.pending_scroll_to_current = true;
        ctx.request_repaint();
    }

    if ctx.input(|i| i.key_pressed(egui::Key::PageDown)) {
        state.viewmodels.listing.current_address = state
            .viewmodels
            .listing
            .current_address
            .saturating_add(page)
            .min(max_addr);
        state.viewmodels.listing.pending_scroll_to_current = true;
        ctx.request_repaint();
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Home)) {
        state.viewmodels.listing.current_address = min_addr;
        state.viewmodels.listing.pending_scroll_to_current = true;
        ctx.request_repaint();
    }

    if ctx.input(|i| i.key_pressed(egui::Key::End)) {
        state.viewmodels.listing.current_address = max_addr.saturating_sub(page);
        state.viewmodels.listing.pending_scroll_to_current = true;
        ctx.request_repaint();
    }
}

/// Build instruction cache around the current address
fn build_instruction_cache(
    binary: &std::sync::Arc<fission_loader::loader::LoadedBinary>,
    center_addr: u64,
    count: usize,
) -> HashMap<u64, CachedInstruction> {
    let mut cache = HashMap::new();

    // Calculate start address (go back a bit)
    let start_addr = center_addr.saturating_sub((count / 2) as u64 * AVG_INSTRUCTION_SIZE);

    // Get bytes to disassemble
    let bytes_to_read = count * 15; // Max x86 instruction is 15 bytes

    // Check if address is in an executable section
    let in_code = binary.sections.iter().any(|s| {
        s.is_executable
            && start_addr >= s.virtual_address
            && start_addr < s.virtual_address + s.virtual_size as u64
    });

    if !in_code {
        return cache;
    }

    let bytes = match binary.get_bytes(start_addr, bytes_to_read) {
        Some(b) => b,
        None => return cache,
    };

    let engine = match DisasmEngine::new(binary.is_64bit) {
        Ok(e) => e,
        Err(_) => return cache,
    };

    if let Ok(insns) = engine.disassemble(&bytes, start_addr) {
        for insn in insns.into_iter().take(CACHE_SIZE) {
            cache.insert(
                insn.address,
                CachedInstruction {
                    address: insn.address,
                    bytes: insn.bytes,
                    mnemonic: insn.mnemonic,
                    operands: insn.operands,
                    is_flow_control: insn.is_flow_control,
                },
            );
        }
    }

    cache
}

#[derive(Clone)]
struct CachedInstruction {
    address: u64,
    bytes: Vec<u8>,
    mnemonic: String,
    operands: String,
    is_flow_control: bool,
}

/// Convert address to virtual row index
fn address_to_row(addr: u64, min_addr: u64) -> usize {
    ((addr.saturating_sub(min_addr)) / AVG_INSTRUCTION_SIZE) as usize
}

/// Convert virtual row index to address
fn row_to_address(row: usize, min_addr: u64) -> u64 {
    min_addr + (row as u64 * AVG_INSTRUCTION_SIZE)
}

/// Render the virtual scroll table
fn render_virtual_table(
    ui: &mut egui::Ui,
    state: &mut AppState,
    binary: &std::sync::Arc<fission_loader::loader::LoadedBinary>,
    cache: &HashMap<u64, CachedInstruction>,
    function_addresses: &std::collections::HashSet<u64>,
    min_addr: u64,
    total_rows: usize,
    row_height: f32,
    scroll_to_row: Option<usize>,
) {
    let highlight_addr = state.viewmodels.listing.current_address;

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(18.0)) // Indicator
        .column(Column::exact(75.0)) // Address
        .column(Column::initial(90.0).at_least(50.0)) // Bytes
        .column(Column::exact(60.0)) // Mnemonic
        .column(Column::remainder()) // Operands
        .min_scrolled_height(0.0);

    // Scroll to current address row
    if let Some(row) = scroll_to_row {
        table = table.scroll_to_row(row, Some(egui::Align::Center));
    }

    table
        .header(20.0, |mut header| {
            header.col(|_ui| {});
            header.col(|ui| {
                ui.label(egui::RichText::new("Address").small().strong());
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Bytes").small().strong());
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Opcode").small().strong());
            });
            header.col(|ui| {
                ui.label(egui::RichText::new("Operands").small().strong());
            });
        })
        .body(|body| {
            body.rows(row_height, total_rows, |mut row| {
                let row_idx = row.index();
                let row_addr = row_to_address(row_idx, min_addr);

                let is_current = row_addr == highlight_addr;
                let is_function_start = function_addresses.contains(&row_addr);

                // Try to get cached instruction
                let insn = cache.get(&row_addr);

                // Find nearest cached instruction if exact match not found
                let display = if let Some(i) = insn {
                    Some(i.clone())
                } else {
                    // Find instruction that contains this address
                    cache
                        .values()
                        .find(|i| {
                            i.address <= row_addr && i.address + i.bytes.len() as u64 > row_addr
                        })
                        .cloned()
                };

                // Indicator column
                row.col(|ui| {
                    if is_function_start {
                        ui.label(egui::RichText::new("▸").color(catppuccin::GREEN).strong())
                            .on_hover_text("Function");
                    }
                });

                // Address column
                row.col(|ui| {
                    let color = if is_current {
                        catppuccin::PEACH
                    } else if is_function_start {
                        catppuccin::GREEN
                    } else {
                        code::ADDRESS
                    };

                    let resp = ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("{:08X}", row_addr))
                                .color(color)
                                .monospace(),
                        )
                        .sense(egui::Sense::click()),
                    );

                    if resp.clicked() {
                        state.viewmodels.listing.current_address = row_addr;
                        state.viewmodels.listing.pending_scroll_to_current = true;
                    }
                    if resp.double_clicked() {
                        state.ui.pending_jump = Some(row_addr);
                    }
                });

                if let Some(ref insn) = display {
                    // Only show if this is the start of the instruction
                    let show_content = insn.address == row_addr;

                    // Bytes column
                    row.col(|ui| {
                        if show_content {
                            let bytes_str: String = insn
                                .bytes
                                .iter()
                                .take(6)
                                .map(|b| format!("{:02X}", b))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let suffix = if insn.bytes.len() > 6 { ".." } else { "" };

                            ui.label(
                                egui::RichText::new(format!("{}{}", bytes_str, suffix))
                                    .color(code::HEX_BYTE)
                                    .monospace()
                                    .small(),
                            );
                        }
                    });

                    // Mnemonic column
                    row.col(|ui| {
                        if show_content {
                            let mnemonic_color = if insn.is_flow_control {
                                code::MNEMONIC_FLOW
                            } else {
                                code::MNEMONIC_NORMAL
                            };

                            ui.label(
                                egui::RichText::new(&insn.mnemonic)
                                    .color(mnemonic_color)
                                    .monospace()
                                    .strong(),
                            );
                        }
                    });

                    // Operands column
                    row.col(|ui| {
                        if show_content {
                            let operand_text = resolve_symbols(
                                &insn.operands,
                                &binary.iat_symbols,
                                &binary.functions,
                            );

                            ui.label(
                                egui::RichText::new(operand_text)
                                    .color(catppuccin::TEXT)
                                    .monospace(),
                            );
                        }
                    });
                } else {
                    // No instruction at this address - show as data or padding
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new("??")
                                .color(catppuccin::OVERLAY0)
                                .monospace()
                                .small(),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new("db")
                                .color(catppuccin::OVERLAY0)
                                .monospace(),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new("(no code)")
                                .color(catppuccin::OVERLAY0)
                                .small(),
                        );
                    });
                }
            });
        });
}

/// Resolve symbols in operands
fn resolve_symbols(
    operands: &str,
    iat_symbols: &std::collections::HashMap<u64, String>,
    functions: &[fission_loader::loader::FunctionInfo],
) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static HEX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"0x([0-9a-fA-F]{6,16})").unwrap());

    HEX_RE
        .replace_all(operands, |caps: &regex::Captures| {
            if let Ok(addr) = u64::from_str_radix(&caps[1], 16) {
                if let Some(name) = iat_symbols.get(&addr) {
                    return name.clone();
                }
                if let Some(func) = functions.iter().find(|f| f.address == addr) {
                    return func.name.clone();
                }
            }
            caps[0].to_string()
        })
        .to_string()
}

/// Parse an address string
fn parse_address(input: &str) -> Option<u64> {
    let trimmed = input.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        u64::from_str_radix(&trimmed[2..], 16).ok()
    } else {
        trimmed.parse().ok()
    }
}
