//! Application state management for the Fission GUI.
//!
//! This module contains the main application struct that holds all UI state,
//! loaded binary state, and manages the egui rendering loop.

use eframe::egui;
use std::sync::mpsc::{channel, Receiver, Sender};

use crate::loader::{LoadedBinary, FunctionInfo};

// Message types for async operations
enum AsyncMessage {
    BinaryLoaded(Result<LoadedBinary, String>),
    DecompileResult { address: u64, c_code: String },
    DecompileError { address: u64, error: String },
    ServerStatus(bool),
}

/// Main application state container
pub struct FissionApp {
    /// Log buffer for the output console
    log_buffer: Vec<String>,

    /// Current command input in the integrated CLI
    cli_input: String,

    /// Currently loaded binary (if any)
    loaded_binary: Option<LoadedBinary>,

    /// Debugger running state
    is_debugging: bool,

    /// Selected function (for decompilation view)
    selected_function: Option<FunctionInfo>,

    /// Current decompiled C code
    decompiled_code: String,

    /// Is decompilation in progress?
    decompiling: bool,

    /// Server connection status
    server_connected: bool,

    /// Channel for receiving async messages
    rx: Receiver<AsyncMessage>,
    
    /// Channel sender (cloned for async tasks)
    tx: Sender<AsyncMessage>,

    /// File dialog path
    file_dialog_path: String,
}

impl Default for FissionApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            log_buffer: vec![
                "==============================================================".into(),
                "  Fission - Next-Gen Dynamic Instrumentation Platform".into(),
                "  \"Split the Binary, Fuse the Power.\"".into(),
                "==============================================================".into(),
                "".into(),
                "[*] Ready. Load a binary to begin analysis.".into(),
            ],
            cli_input: String::new(),
            loaded_binary: None,
            is_debugging: false,
            selected_function: None,
            decompiled_code: "// Select a function to decompile".into(),
            decompiling: false,
            server_connected: false,
            rx,
            tx,
            file_dialog_path: String::new(),
        }
    }
}

impl FissionApp {
    /// Add a log message to the output buffer
    pub fn log(&mut self, message: impl Into<String>) {
        self.log_buffer.push(message.into());
    }

    /// Clear the log buffer
    pub fn clear_logs(&mut self) {
        self.log_buffer.clear();
    }

    /// Process pending async messages
    fn process_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AsyncMessage::BinaryLoaded(Ok(binary)) => {
                    self.log(format!("[✓] Loaded: {}", binary.path));
                    self.log(format!("    {} {} | Entry: 0x{:x}", 
                        if binary.is_64bit { "64-bit" } else { "32-bit" },
                        binary.format,
                        binary.entry_point));
                    self.log(format!("    {} functions found", binary.functions.len()));
                    self.loaded_binary = Some(binary);
                }
                AsyncMessage::BinaryLoaded(Err(e)) => {
                    self.log(format!("[✗] Failed to load binary: {}", e));
                }
                AsyncMessage::DecompileResult { address, c_code } => {
                    self.decompiled_code = c_code;
                    self.decompiling = false;
                    self.log(format!("[✓] Decompiled 0x{:x}", address));
                }
                AsyncMessage::DecompileError { address, error } => {
                    self.decompiled_code = format!("// Error decompiling 0x{:x}\n// {}", address, error);
                    self.decompiling = false;
                    self.log(format!("[✗] Decompile error: {}", error));
                }
                AsyncMessage::ServerStatus(connected) => {
                    self.server_connected = connected;
                }
            }
        }
    }

    /// Load a binary file
    fn load_binary(&mut self, path: &str) {
        let path = path.to_string();
        let tx = self.tx.clone();
        
        self.log(format!("[*] Loading {}...", path));
        
        std::thread::spawn(move || {
            match LoadedBinary::from_file(&path) {
                Ok(binary) => { let _ = tx.send(AsyncMessage::BinaryLoaded(Ok(binary))); }
                Err(e) => { let _ = tx.send(AsyncMessage::BinaryLoaded(Err(e.to_string()))); }
            }
        });
    }

    /// Decompile a function (async)
    fn decompile_function(&mut self, func: &FunctionInfo) {
        if self.loaded_binary.is_none() {
            self.log("[!] No binary loaded");
            return;
        }
        
        let binary = self.loaded_binary.as_ref().unwrap();
        let address = func.address;
        let arch = binary.arch_spec.clone();
        
        // Get function bytes (estimate 4KB for function body)
        let func_size = if func.size > 0 { func.size as usize } else { 4096 };
        let bytes = match binary.get_bytes(address, func_size) {
            Some(b) => b,
            None => {
                self.log(format!("[!] Cannot read bytes at 0x{:x}", address));
                return;
            }
        };
        
        let tx = self.tx.clone();
        self.decompiling = true;
        self.decompiled_code = format!("// Decompiling 0x{:x}...", address);
        self.log(format!("[*] Decompiling 0x{:x} ({} bytes)", address, bytes.len()));
        
        // Spawn async task for decompilation
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                match crate::decomp::client::GhidraClient::connect().await {
                    Ok(mut client) => {
                        // Load the binary bytes
                        if let Err(e) = client.load_binary(bytes, address, &arch).await {
                            let _ = tx.send(AsyncMessage::DecompileError { 
                                address, 
                                error: e.to_string() 
                            });
                            return;
                        }
                        
                        // Decompile
                        match client.decompile_function(address).await {
                            Ok(result) => {
                                let _ = tx.send(AsyncMessage::DecompileResult { 
                                    address, 
                                    c_code: result.c_code 
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(AsyncMessage::DecompileError { 
                                    address, 
                                    error: e.to_string() 
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(AsyncMessage::DecompileError { 
                            address, 
                            error: format!("Server connection failed: {}", e) 
                        });
                    }
                }
            });
        });
    }

    /// Render the top menu bar
    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Binary...").clicked() {
                        // Simple path input for now
                        self.log("[*] Enter path in console: load <path>");
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        std::process::exit(0);
                    }
                });

                ui.menu_button("Debug", |ui| {
                    let attach_text = if self.is_debugging { "[Stop]" } else { "[Start]" };
                    if ui.button(attach_text).clicked() {
                        self.is_debugging = !self.is_debugging;
                        let status = if self.is_debugging { "started" } else { "stopped" };
                        self.log(format!("[*] Debugging {}", status));
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Clear Console").clicked() {
                        self.clear_logs();
                        self.log("[*] Console cleared");
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.log("[*] Fission v0.1.0 - Ghidra-Powered Analysis Platform");
                        ui.close_menu();
                    }
                });
            });
        });
    }

    /// Render the status bar at the bottom
    fn render_status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Server status
                let (server_color, server_text) = if self.server_connected {
                    (egui::Color32::from_rgb(100, 200, 100), "[+] Server")
                } else {
                    (egui::Color32::from_rgb(150, 150, 150), "[-] Server")
                };
                ui.colored_label(server_color, server_text);
                
                ui.separator();

                // Debugger status indicator
                let (status_color, status_text) = if self.is_debugging {
                    (egui::Color32::from_rgb(100, 200, 100), "[*] DEBUGGING")
                } else {
                    (egui::Color32::from_rgb(150, 150, 150), "[ ] IDLE")
                };
                ui.colored_label(status_color, status_text);

                ui.separator();

                // Loaded binary info
                if let Some(ref binary) = self.loaded_binary {
                    ui.label(format!("File: {} | {} functions", binary.path, binary.functions.len()));
                } else {
                    ui.label("No binary loaded");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Fission v0.1.0");
                });
            });
        });
    }

    /// Render the main content area
    fn render_main_content(&mut self, ctx: &egui::Context) {
        // Left Panel: Function List
        let mut clicked_func: Option<FunctionInfo> = None;
        
        egui::SidePanel::left("functions_panel")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("[Functions]");
                ui.separator();

                if let Some(ref binary) = self.loaded_binary {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for func in &binary.functions {
                            let label = if func.name.is_empty() {
                                format!("0x{:08x}", func.address)
                            } else {
                                format!("{} (0x{:x})", func.name, func.address)
                            };
                            
                            let is_selected = self.selected_function.as_ref()
                                .map(|f| f.address == func.address)
                                .unwrap_or(false);
                            
                            let response = ui.selectable_label(is_selected, &label);
                            
                            if response.clicked() {
                                clicked_func = Some(func.clone());
                            }
                        }
                    });
                } else {
                    ui.label("Load a binary to see functions");
                }
            });

        // Handle function click outside of closure
        if let Some(func) = clicked_func {
            self.selected_function = Some(func.clone());
            self.decompile_function(&func);
        }

        // Right Panel: Decompiled Code
        egui::SidePanel::right("decompile_panel")
            .resizable(true)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("[Decompiled Code]");
                    if self.decompiling {
                        ui.spinner();
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Syntax highlighted C code would be nice, but for now just monospace
                    ui.add(
                        egui::TextEdit::multiline(&mut self.decompiled_code.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(30)
                    );
                });
            });

        // Center Panel: Console
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("[Console]");
            ui.separator();

            // Scrollable log area
            let text_style = egui::TextStyle::Monospace;
            let row_height = ui.text_style_height(&text_style);

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .max_height(ui.available_height() - 35.0)
                .show_rows(ui, row_height, self.log_buffer.len(), |ui, row_range| {
                    for row in row_range {
                        if let Some(log) = self.log_buffer.get(row) {
                            // Color code log messages
                            let color = if log.starts_with("[✓]") {
                                egui::Color32::from_rgb(100, 200, 100)
                            } else if log.starts_with("[✗]") || log.starts_with("[!]") {
                                egui::Color32::from_rgb(255, 100, 100)
                            } else if log.starts_with("[*]") {
                                egui::Color32::from_rgb(100, 150, 255)
                            } else {
                                egui::Color32::GRAY
                            };
                            ui.colored_label(color, log);
                        }
                    }
                });

            ui.separator();

            // CLI input at the bottom
            ui.horizontal(|ui| {
                ui.label(">");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.cli_input)
                        .desired_width(ui.available_width() - 60.0)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("Enter command..."),
                );

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let cmd = self.cli_input.trim().to_string();
                    if !cmd.is_empty() {
                        self.log(format!("> {}", cmd));
                        self.process_command(&cmd);
                        self.cli_input.clear();
                    }
                    response.request_focus();
                }

                if ui.button("Run").clicked() {
                    let cmd = self.cli_input.trim().to_string();
                    if !cmd.is_empty() {
                        self.log(format!("> {}", cmd));
                        self.process_command(&cmd);
                        self.cli_input.clear();
                    }
                }
            });
        });
    }

    /// Process a CLI command entered in the GUI
    fn process_command(&mut self, cmd: &str) {
        match cmd {
            "help" | "?" => {
                self.log("Available commands:");
                self.log("  load <path>  : Load a binary for analysis");
                self.log("  funcs        : List functions");
                self.log("  decompile <addr> : Decompile function at address");
                self.log("  start        : Start debugging session");
                self.log("  stop         : Stop debugging session");
                self.log("  clear        : Clear console");
                self.log("  exit         : Quit Fission");
            }
            "clear" => {
                self.clear_logs();
                self.log("[*] Console cleared");
            }
            "exit" | "quit" | "q" => {
                std::process::exit(0);
            }
            "funcs" => {
                if let Some(ref binary) = self.loaded_binary {
                    let count = binary.functions.len();
                    let funcs: Vec<_> = binary.functions.iter()
                        .take(20)
                        .map(|f| (f.address, f.name.clone()))
                        .collect();
                    
                    self.log(format!("Functions ({}):", count));
                    for (addr, name) in &funcs {
                        self.log(format!("  0x{:08x}: {}", addr, name));
                    }
                    if count > 20 {
                        self.log(format!("  ... and {} more", count - 20));
                    }
                } else {
                    self.log("[!] No binary loaded");
                }
            }
            "start" => {
                if self.loaded_binary.is_some() {
                    self.is_debugging = true;
                    self.log("[*] Debugging session started");
                } else {
                    self.log("[!] No binary loaded. Use 'load <path>' first.");
                }
            }
            "stop" => {
                self.is_debugging = false;
                self.log("[*] Debugging session stopped");
            }
            _ if cmd.starts_with("load ") => {
                let path = cmd.strip_prefix("load ").unwrap().trim();
                self.load_binary(path);
            }
            _ if cmd.starts_with("decompile ") => {
                let addr_str = cmd.strip_prefix("decompile ").unwrap().trim();
                if let Some(addr) = parse_address(addr_str) {
                    if let Some(ref binary) = self.loaded_binary {
                        if let Some(func) = binary.function_at(addr) {
                            let func = func.clone();
                            self.selected_function = Some(func.clone());
                            self.decompile_function(&func);
                        } else {
                            // Create temporary function info
                            let func = FunctionInfo {
                                name: format!("func_{:x}", addr),
                                address: addr,
                                size: 0,
                                is_export: false,
                                is_import: false,
                            };
                            self.selected_function = Some(func.clone());
                            self.decompile_function(&func);
                        }
                    } else {
                        self.log("[!] No binary loaded");
                    }
                } else {
                    self.log("[!] Invalid address format");
                }
            }
            _ => {
                self.log(format!("[!] Unknown command: '{}'", cmd));
                self.log("    Type 'help' for available commands.");
            }
        }
    }
}

/// Parse an address from hex or decimal string
fn parse_address(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.starts_with("0x") || s.starts_with("0X") {
        u64::from_str_radix(&s[2..], 16).ok()
    } else if s.chars().all(|c| c.is_ascii_hexdigit()) && s.len() > 4 {
        u64::from_str_radix(s, 16).ok()
    } else {
        s.parse().ok()
    }
}

impl eframe::App for FissionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process any pending async messages
        self.process_messages();
        
        // Request repaint periodically for async updates
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        
        self.render_menu_bar(ctx);
        self.render_status_bar(ctx);
        self.render_main_content(ctx);
    }
}
