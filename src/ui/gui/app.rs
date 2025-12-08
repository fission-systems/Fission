//! Main application orchestrator for the Fission GUI.
//!
//! This module assembles all UI panels and handles the main event loop.
//! Individual panels are defined in the `panels` module.

use eframe::egui;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::analysis::loader::{LoadedBinary, FunctionInfo};
use crate::analysis::disasm::DisasmEngine;

use super::state::{AppState, CachedDecompile};
use super::messages::AsyncMessage;
use super::menu::{self, MenuAction};
use super::status_bar;
use super::panels::{functions, assembly, decompile, bottom_tabs};
use super::panels::bottom_tabs::ConsoleAction;

/// Main application struct that implements eframe::App
pub struct FissionApp {
    /// Shared application state
    state: AppState,
    
    /// Channel for receiving async messages
    rx: Receiver<AsyncMessage>,
    
    /// Channel sender (cloned for async tasks)
    tx: Sender<AsyncMessage>,
}

impl Default for FissionApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            state: AppState::default(),
            rx,
            tx,
        }
    }
}

impl FissionApp {
    /// Process pending async messages from background threads
    fn process_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AsyncMessage::BinaryLoaded(Ok(binary)) => {
                    self.state.log(format!("[✓] Loaded: {}", binary.path));
                    self.state.log(format!("    {} {} | Entry: 0x{:x}", 
                        if binary.is_64bit { "64-bit" } else { "32-bit" },
                        binary.format,
                        binary.entry_point));
                    self.state.log(format!("    {} functions found", binary.functions.len()));
                    self.state.loaded_binary = Some(binary);
                }
                AsyncMessage::BinaryLoaded(Err(e)) => {
                    self.state.log(format!("[✗] Failed to load binary: {}", e));
                }
                AsyncMessage::DecompileResult { address, c_code } => {
                    // Store in cache
                    if let Some(func) = &self.state.selected_function {
                        if func.address == address {
                            self.state.decompile_cache.insert(address, CachedDecompile {
                                c_code: c_code.clone(),
                                asm_instructions: self.state.asm_instructions.clone(),
                                timestamp: Instant::now(),
                            });
                        }
                    }
                    self.state.decompiled_code = c_code;
                    self.state.decompiling = false;
                    self.state.log(format!("[✓] Decompiled 0x{:x} (cached)", address));
                }
                AsyncMessage::DecompileError { address: _, error } => {
                    self.state.decompiled_code = format!("// Error: {}", error);
                    self.state.decompiling = false;
                    self.state.log(format!("[✗] Decompile error: {}", error));
                    
                    // Check if this is a connection error
                    if error.contains("transport") || error.contains("connection") {
                        let _ = self.tx.send(AsyncMessage::ServerDisconnected);
                    }
                }
                AsyncMessage::ServerStatus(connected) => {
                    self.state.server_connected = connected;
                }
                AsyncMessage::FileSelected(Some(path)) => {
                    self.load_binary(&path);
                }
                AsyncMessage::FileSelected(None) => {
                    // User cancelled
                }
                AsyncMessage::ServerDisconnected => {
                    self.state.server_connected = false;
                    self.state.log("[!] Server disconnected. Attempting recovery...");
                    self.attempt_server_recovery();
                }
                AsyncMessage::ServerRecovered => {
                    self.state.server_connected = true;
                    self.state.recovering = false;
                    self.state.log("[✓] Server reconnected successfully");
                    
                    // Reload binary if we had one loaded
                    if let Some(path) = self.state.last_binary_path.clone() {
                        self.state.log("[*] Reloading binary...");
                        self.load_binary(&path);
                    }
                }
                AsyncMessage::RecoveryFailed(reason) => {
                    self.state.recovering = false;
                    self.state.log(format!("[✗] Server recovery failed: {}", reason));
                }
            }
        }
    }

    /// Attempt to recover server connection with exponential backoff
    fn attempt_server_recovery(&mut self) {
        if self.state.recovering {
            return; // Already recovering
        }
        
        self.state.recovering = true;
        let tx = self.tx.clone();
        
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // Max 3 retries with exponential backoff (1s, 2s, 4s)
                for attempt in 0..3 {
                    let wait_time = Duration::from_secs(1 << attempt);
                    tokio::time::sleep(wait_time).await;
                    
                    match crate::analysis::decomp::client::GhidraClient::connect().await {
                        Ok(_client) => {
                            let _ = tx.send(AsyncMessage::ServerRecovered);
                            return;
                        }
                        Err(e) => {
                            let _ = tx.send(AsyncMessage::ServerStatus(false));
                            if attempt == 2 {
                                let _ = tx.send(AsyncMessage::RecoveryFailed(e.to_string()));
                            }
                        }
                    }
                }
            });
        });
    }

    /// Open native file dialog to select a binary
    fn open_file_dialog(&mut self) {
        let tx = self.tx.clone();
        
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Open Binary")
                .add_filter("Executables", &["exe", "dll", "so", "dylib", "bin"])
                .add_filter("All Files", &["*"])
                .pick_file();
            
            let path = file.map(|p| p.to_string_lossy().to_string());
            let _ = tx.send(AsyncMessage::FileSelected(path));
        });
    }

    /// Load a binary file
    fn load_binary(&mut self, path: &str) {
        let path = path.to_string();
        let tx = self.tx.clone();
        
        // Clear cache on new binary load
        self.state.decompile_cache.clear();
        // Save path for recovery reload
        self.state.last_binary_path = Some(path.clone());
        
        self.state.log(format!("[*] Loading {}...", path));
        
        std::thread::spawn(move || {
            match LoadedBinary::from_file(&path) {
                Ok(binary) => { let _ = tx.send(AsyncMessage::BinaryLoaded(Ok(binary))); }
                Err(e) => { let _ = tx.send(AsyncMessage::BinaryLoaded(Err(e.to_string()))); }
            }
        });
    }

    /// Decompile a function
    fn decompile_function(&mut self, func: &FunctionInfo) {
        // Skip import functions
        if func.is_import {
            self.state.log(format!("[!] {} is an import function (no code to decompile)", func.name));
            self.state.decompiled_code = format!(
                "// {} is an imported function\n// Address: 0x{:x}\n// No code available - this is a stub pointing to external library",
                func.name, func.address
            );
            return;
        }
        
        // Check cache first
        let address = func.address;
        if let Some(cached) = self.state.decompile_cache.get(&address) {
            let c_code = cached.c_code.clone();
            let asm = cached.asm_instructions.clone();
            self.state.log(format!("[*] Using cached result for 0x{:x}", address));
            self.state.decompiled_code = c_code;
            self.state.asm_instructions = asm;
            return;
        }
        
        if self.state.loaded_binary.is_none() {
            self.state.log("[!] No binary loaded");
            return;
        }
        
        let binary = self.state.loaded_binary.as_ref().unwrap();
        let arch = binary.arch_spec.clone();
        
        // Get function bytes (estimate 4KB for function body)
        let func_size = if func.size > 0 { func.size as usize } else { 4096 };
        let bytes = match binary.get_bytes(address, func_size) {
            Some(b) => b,
            None => {
                self.state.log(format!("[!] Cannot read bytes at 0x{:x}", address));
                return;
            }
        };
        
        // Disassemble bytes
        match DisasmEngine::new(binary.is_64bit) {
            Ok(engine) => {
                match engine.disassemble(&bytes, address) {
                    Ok(insns) => {
                        self.state.asm_instructions = insns;
                    }
                    Err(e) => {
                        self.state.log(format!("[!] Disassembly error: {}", e));
                        self.state.asm_instructions.clear();
                    }
                }
            }
            Err(e) => {
                self.state.log(format!("[!] Failed to initialize disassembler: {}", e));
                self.state.asm_instructions.clear();
            }
        }

        let tx = self.tx.clone();
        self.state.decompiling = true;
        self.state.decompiled_code = format!("// Decompiling 0x{:x}...", address);
        self.state.log(format!("[*] Decompiling 0x{:x} ({} bytes)", address, bytes.len()));
        
        // Spawn async task for decompilation
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                match crate::analysis::decomp::client::GhidraClient::connect().await {
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

    /// Process a CLI command
    fn process_command(&mut self, cmd: &str) {
        match cmd {
            "help" | "?" => {
                self.state.log("Available commands:");
                self.state.log("  load <path>  : Load a binary for analysis");
                self.state.log("  funcs        : List functions");
                self.state.log("  clear        : Clear console");
                self.state.log("  exit         : Quit Fission");
            }
            "funcs" | "functions" => {
                if let Some(ref binary) = self.state.loaded_binary {
                    let funcs: Vec<_> = binary.functions.iter()
                        .map(|f| (f.address, f.name.clone()))
                        .collect();
                    self.state.log(format!("[*] {} functions:", funcs.len()));
                    for (addr, name) in funcs {
                        self.state.log(format!("  0x{:08x} {}", addr, name));
                    }
                } else {
                    self.state.log("[!] No binary loaded");
                }
            }
            "clear" => {
                self.state.clear_logs();
                self.state.log("[*] Console cleared");
            }
            "exit" | "quit" => {
                std::process::exit(0);
            }
            _ if cmd.starts_with("load ") => {
                let path = cmd.trim_start_matches("load ").trim();
                self.load_binary(path);
            }
            _ => {
                self.state.log(format!("[!] Unknown command: {}", cmd));
            }
        }
    }
}

impl eframe::App for FissionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process async messages
        self.process_messages();

        // Render menu bar and handle actions
        match menu::render(ctx, &mut self.state) {
            MenuAction::OpenFile => self.open_file_dialog(),
            MenuAction::ToggleDebug => {
                self.state.is_debugging = !self.state.is_debugging;
                let status = if self.state.is_debugging { "started" } else { "stopped" };
                self.state.log(format!("[*] Debugging {}", status));
            }
            MenuAction::ClearConsole => {
                self.state.clear_logs();
                self.state.log("[*] Console cleared");
            }
            MenuAction::ClearCache => {
                let count = self.state.decompile_cache.len();
                self.state.decompile_cache.clear();
                self.state.log(format!("[*] Cleared {} cached items", count));
            }
            MenuAction::ShowAbout => {
                self.state.log("[*] Fission v0.1.0 - Ghidra-Powered Analysis Platform");
            }
            MenuAction::Exit => std::process::exit(0),
            MenuAction::None => {}
        }

        // Render status bar
        status_bar::render(ctx, &self.state);

        // Render panels
        let clicked_func = functions::render(ctx, &mut self.state);
        
        // Bottom tabbed panel (Console, Hex View, Strings)
        match bottom_tabs::render(ctx, &mut self.state) {
            ConsoleAction::Command(cmd) => self.process_command(&cmd),
            ConsoleAction::None => {}
        }
        
        // Fixed right panel - Decompile
        decompile::render(ctx, &mut self.state);
        
        // Main content - Assembly
        assembly::render(ctx, &self.state);

        // Handle function click
        if let Some(func) = clicked_func {
            self.state.selected_function = Some(func.clone());
            self.decompile_function(&func);
        }
    }
}
