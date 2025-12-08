//! GUI - egui-based graphical interface
//!
//! This module contains GUI components and widgets for the Fission interface.
//! The main application logic is in app.rs.

use eframe::egui;

/// Disassembly view widget
pub struct DisasmView {
    /// Current base address
    pub base_address: u64,

    /// Instructions to display
    pub instructions: Vec<DisasmLine>,

    /// Currently selected line index
    pub selected_line: Option<usize>,
}

/// A single line in the disassembly view
#[derive(Debug, Clone)]
pub struct DisasmLine {
    pub address: u64,
    pub bytes: String,
    pub mnemonic: String,
    pub is_breakpoint: bool,
    pub is_current: bool,
}

impl DisasmView {
    pub fn new() -> Self {
        Self {
            base_address: 0,
            instructions: Vec::new(),
            selected_line: None,
        }
    }

    /// Render the disassembly view
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let text_style = egui::TextStyle::Monospace;
        let row_height = ui.text_style_height(&text_style);

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, self.instructions.len(), |ui, row_range| {
                for idx in row_range {
                    if let Some(line) = self.instructions.get(idx) {
                        let is_selected = self.selected_line == Some(idx);

                        ui.horizontal(|ui| {
                            // Breakpoint indicator
                            let bp_text = if line.is_breakpoint { "●" } else { " " };
                            ui.colored_label(egui::Color32::RED, bp_text);

                            // Current instruction indicator
                            let cur_text = if line.is_current { "▶" } else { " " };
                            ui.colored_label(egui::Color32::YELLOW, cur_text);

                            // Address
                            let addr_color = if is_selected {
                                egui::Color32::from_rgb(100, 200, 255)
                            } else {
                                egui::Color32::from_rgb(150, 150, 150)
                            };
                            ui.colored_label(addr_color, format!("{:016X}", line.address));

                            ui.label("  ");

                            // Bytes
                            ui.colored_label(
                                egui::Color32::from_rgb(100, 100, 100),
                                format!("{:24}", line.bytes),
                            );

                            // Mnemonic
                            let mnemonic_color = egui::Color32::from_rgb(200, 200, 200);
                            ui.colored_label(mnemonic_color, &line.mnemonic);
                        });
                    }
                }
            });
    }
}

impl Default for DisasmView {
    fn default() -> Self {
        Self::new()
    }
}

/// Register view widget
pub struct RegisterView {
    pub registers: Registers,
    pub previous_registers: Option<Registers>,
}

/// CPU register state
#[derive(Debug, Clone, Default)]
pub struct Registers {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub rip: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rflags: u64,
}

impl RegisterView {
    pub fn new() -> Self {
        Self {
            registers: Registers::default(),
            previous_registers: None,
        }
    }

    /// Render the register view
    pub fn show(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("registers_grid")
            .num_columns(2)
            .spacing([20.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                self.show_register(
                    ui,
                    "RAX",
                    self.registers.rax,
                    self.previous_registers.as_ref().map(|r| r.rax),
                );
                self.show_register(
                    ui,
                    "RBX",
                    self.registers.rbx,
                    self.previous_registers.as_ref().map(|r| r.rbx),
                );
                ui.end_row();

                self.show_register(
                    ui,
                    "RCX",
                    self.registers.rcx,
                    self.previous_registers.as_ref().map(|r| r.rcx),
                );
                self.show_register(
                    ui,
                    "RDX",
                    self.registers.rdx,
                    self.previous_registers.as_ref().map(|r| r.rdx),
                );
                ui.end_row();

                self.show_register(
                    ui,
                    "RSI",
                    self.registers.rsi,
                    self.previous_registers.as_ref().map(|r| r.rsi),
                );
                self.show_register(
                    ui,
                    "RDI",
                    self.registers.rdi,
                    self.previous_registers.as_ref().map(|r| r.rdi),
                );
                ui.end_row();

                self.show_register(
                    ui,
                    "RBP",
                    self.registers.rbp,
                    self.previous_registers.as_ref().map(|r| r.rbp),
                );
                self.show_register(
                    ui,
                    "RSP",
                    self.registers.rsp,
                    self.previous_registers.as_ref().map(|r| r.rsp),
                );
                ui.end_row();

                self.show_register(
                    ui,
                    "RIP",
                    self.registers.rip,
                    self.previous_registers.as_ref().map(|r| r.rip),
                );
                self.show_register(
                    ui,
                    "RFLAGS",
                    self.registers.rflags,
                    self.previous_registers.as_ref().map(|r| r.rflags),
                );
                ui.end_row();

                self.show_register(
                    ui,
                    "R8",
                    self.registers.r8,
                    self.previous_registers.as_ref().map(|r| r.r8),
                );
                self.show_register(
                    ui,
                    "R9",
                    self.registers.r9,
                    self.previous_registers.as_ref().map(|r| r.r9),
                );
                ui.end_row();

                self.show_register(
                    ui,
                    "R10",
                    self.registers.r10,
                    self.previous_registers.as_ref().map(|r| r.r10),
                );
                self.show_register(
                    ui,
                    "R11",
                    self.registers.r11,
                    self.previous_registers.as_ref().map(|r| r.r11),
                );
                ui.end_row();

                self.show_register(
                    ui,
                    "R12",
                    self.registers.r12,
                    self.previous_registers.as_ref().map(|r| r.r12),
                );
                self.show_register(
                    ui,
                    "R13",
                    self.registers.r13,
                    self.previous_registers.as_ref().map(|r| r.r13),
                );
                ui.end_row();

                self.show_register(
                    ui,
                    "R14",
                    self.registers.r14,
                    self.previous_registers.as_ref().map(|r| r.r14),
                );
                self.show_register(
                    ui,
                    "R15",
                    self.registers.r15,
                    self.previous_registers.as_ref().map(|r| r.r15),
                );
                ui.end_row();
            });
    }

    fn show_register(&self, ui: &mut egui::Ui, name: &str, value: u64, previous: Option<u64>) {
        let changed = previous.map(|p| p != value).unwrap_or(false);
        let color = if changed {
            egui::Color32::from_rgb(255, 100, 100) // Red for changed
        } else {
            egui::Color32::from_rgb(200, 200, 200) // Default
        };

        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(100, 200, 255),
                format!("{:6}", name),
            );
            ui.colored_label(color, format!("{:#018x}", value));
        });
    }
}

impl Default for RegisterView {
    fn default() -> Self {
        Self::new()
    }
}

/// Hex view widget for memory display
pub struct HexView {
    pub base_address: u64,
    pub data: Vec<u8>,
    pub bytes_per_line: usize,
}

impl HexView {
    pub fn new() -> Self {
        Self {
            base_address: 0,
            data: Vec::new(),
            bytes_per_line: 16,
        }
    }

    /// Render the hex view
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let text_style = egui::TextStyle::Monospace;
        let row_height = ui.text_style_height(&text_style);
        let num_rows = (self.data.len() + self.bytes_per_line - 1) / self.bytes_per_line;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, num_rows, |ui, row_range| {
                for row in row_range {
                    let start = row * self.bytes_per_line;
                    let end = (start + self.bytes_per_line).min(self.data.len());
                    let row_data = &self.data[start..end];

                    ui.horizontal(|ui| {
                        // Address
                        let addr = self.base_address + start as u64;
                        ui.colored_label(
                            egui::Color32::from_rgb(100, 200, 255),
                            format!("{:016X}  ", addr),
                        );

                        // Hex bytes
                        let mut hex_str = String::new();
                        for (i, byte) in row_data.iter().enumerate() {
                            hex_str.push_str(&format!("{:02X} ", byte));
                            if i == 7 {
                                hex_str.push(' '); // Extra space in middle
                            }
                        }
                        // Pad if needed
                        for _ in row_data.len()..self.bytes_per_line {
                            hex_str.push_str("   ");
                        }
                        ui.monospace(&hex_str);

                        ui.label(" |");

                        // ASCII representation
                        let ascii: String = row_data
                            .iter()
                            .map(|&b| {
                                if b.is_ascii_graphic() || b == b' ' {
                                    b as char
                                } else {
                                    '.'
                                }
                            })
                            .collect();
                        ui.monospace(&ascii);

                        ui.label("|");
                    });
                }
            });
    }
}

impl Default for HexView {
    fn default() -> Self {
        Self::new()
    }
}
