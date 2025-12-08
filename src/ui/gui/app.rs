//! Main application orchestrator for the Fission GUI.
//!
//! This module assembles all UI panels and handles the main event loop.
//! Individual panels are defined in the `panels` module.

use eframe::egui;
use std::fs;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::analysis::decomp::client::GhidraClient;
use crate::analysis::decomp::client::BinaryId;
use crate::analysis::decomp::client::ghidra_service::FunctionMeta;
use crate::analysis::loader::{LoadedBinary, FunctionInfo};
use crate::analysis::disasm::DisasmEngine;
#[cfg(target_os = "windows")]
use crate::debug::PlatformDebugger;
#[cfg(target_os = "windows")]
use crate::debug::Debugger;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use tokio::runtime::Runtime;
use tokio::time::sleep;

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

    /// Platform debugger (Windows only)
    #[cfg(target_os = "windows")]
    debugger: Option<PlatformDebugger>,

    /// Shared Ghidra client to avoid reconnect cost
    ghidra_client: Arc<Mutex<Option<GhidraClient>>>,

    /// Cached binary id of what is loaded on the server (to skip re-load)
    current_binary_id: Option<BinaryId>,
}

static TOKIO_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Runtime::new().expect("Failed to create global Tokio runtime")
});

impl Default for FissionApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            state: AppState::default(),
            rx,
            tx,
            #[cfg(target_os = "windows")]
            debugger: Some(PlatformDebugger::default()),
            ghidra_client: Arc::new(Mutex::new(None)),
            current_binary_id: None,
        }
    }
}

impl FissionApp {
    /// Ensure server has the current binary loaded and cache functions from server metadata.
    fn preload_server_binary(&mut self) {
        let Some(binary) = self.state.loaded_binary.as_ref() else {
            return;
        };

        let arch = binary.arch_spec.clone();
        let bin_size = binary.data.len() as u64;
        let bin_mtime = fs::metadata(&binary.path).ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let bin_id = BinaryId::new(Some(binary.path.clone()), bin_size, arch.clone(), bin_mtime);
        let bin_bytes = binary.data.clone();
        let bin_base = binary.image_base;

        let shared_client = self.ghidra_client.clone();
        let funcs = TOKIO_RUNTIME.block_on(async move {
            let mut guard = shared_client.lock().unwrap();
            if guard.is_none() {
                *guard = Self::connect_with_backoff().await;
            }
            let Some(client) = guard.as_mut() else { return None; };
            match client.load_binary_if_needed(bin_bytes, bin_base, &arch, bin_id).await {
                Ok((_, metas)) => Some(metas.to_vec()),
                Err(_) => None,
            }
        });

        if let Some(server_funcs) = funcs {
            if !server_funcs.is_empty() {
                let converted: Vec<FunctionInfo> = server_funcs.into_iter().map(Self::convert_meta).collect();
                self.state.loaded_binary.as_mut().map(|b| b.functions = converted);
            }
        }
    }

    fn convert_meta(m: FunctionMeta) -> FunctionInfo {
        FunctionInfo {
            name: m.name,
            address: m.address,
            size: m.size as u64,
            is_export: false,
            is_import: m.is_import,
        }
    }

    async fn connect_with_backoff() -> Option<GhidraClient> {
        let delays = [Duration::from_millis(0), Duration::from_millis(200), Duration::from_millis(500)];
        for d in delays {
            if d.as_millis() > 0 {
                sleep(d).await;
            }
            if let Ok(c) = GhidraClient::connect().await {
                return Some(c);
            }
        }
        None
    }

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
                    self.preload_server_binary();
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
        
        let start_time = Instant::now();

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
        
        let (arch, bin_id, bin_bytes, bin_base, bytes, is_64bit) = {
            let binary = self.state.loaded_binary.as_ref().unwrap();
            let arch = binary.arch_spec.clone();
            let bin_size = binary.data.len() as u64;
            let bin_mtime = fs::metadata(&binary.path).ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let bin_id = BinaryId::new(Some(binary.path.clone()), bin_size, arch.clone(), bin_mtime);
            let bin_bytes = binary.data.clone();
            let bin_base = binary.image_base;
            
            // Get function bytes (estimate 4KB for function body)
            let func_size = if func.size > 0 { func.size as usize } else { 4096 };
            let bytes = match binary.get_bytes(address, func_size) {
                Some(b) => b,
                None => {
                    self.state.log(format!("[!] Cannot read bytes at 0x{:x}", address));
                    return;
                }
            };
            (arch, bin_id, bin_bytes, bin_base, bytes, binary.is_64bit)
        };
        
        // Disassemble bytes
        let disasm_start = Instant::now();
        match DisasmEngine::new(is_64bit) {
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
        let shared_client = self.ghidra_client.clone();
        let bin_bytes_clone = bin_bytes.clone();
        let bin_id_clone = bin_id.clone();
        let arch_clone = arch.clone();
        let handle = TOKIO_RUNTIME.handle().clone();
        std::thread::spawn(move || {
            handle.block_on(async {

                let mut guard = shared_client.lock().unwrap();

                // Try reuse; if missing or failed ensure, reconnect with short backoff
                let mut need_new = guard.is_none();
                if !need_new {
                    if let Some(client) = guard.as_mut() {
                        if client.ensure_connected().await.is_err() {
                            need_new = true;
                        }
                    }
                }

                if need_new {
                    let (prev_id, prev_funcs) = guard
                        .as_ref()
                        .map(|c| c.snapshot_state())
                        .unwrap_or((None, Vec::new()));

                    let mut new_client = None;
                    let delays = [Duration::from_millis(0), Duration::from_millis(200), Duration::from_millis(500)];
                    for d in delays {
                        if d.as_millis() > 0 {
                            sleep(d).await;
                        }
                        match GhidraClient::connect().await {
                            Ok(mut c) => {
                                c.restore_state(prev_id.clone(), prev_funcs.clone());
                                new_client = Some(c);
                                break;
                            }
                            Err(_) => continue,
                        }
                    }

                    if let Some(c) = new_client {
                        *guard = Some(c);
                    } else {
                        let _ = tx.send(AsyncMessage::DecompileError { 
                            address, 
                            error: "Server reconnection failed".to_string() 
                        });
                        return;
                    }
                }

                let client = guard.as_mut().unwrap();

                // Load the binary bytes only if needed
                if let Err(e) = match client.load_binary_if_needed(bin_bytes_clone.clone(), bin_base, &arch_clone, bin_id_clone.clone()).await {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err),
                } {
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
            MenuAction::AttachToProcess => {
                self.state.show_attach_dialog = true;
                // Refresh process list
                self.state.process_list = crate::debug::enumerate_processes();
            }
            MenuAction::DetachProcess => {
                self.detach_process();
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
        // Render attach dialog
        self.render_attach_dialog(ctx);
    }


}

impl FissionApp {
    /// Attach to a process (Windows builds only)
    fn attach_to_process(&mut self, pid: u32) {
        #[cfg(target_os = "windows")]
        {
            let dbg = self.debugger.get_or_insert_with(PlatformDebugger::default);
            self.state.log(format!("[*] Attaching to PID {}...", pid));
            match dbg.attach(pid) {
                Ok(_) => {
                    self.state.is_debugging = true;
                    self.state.debug_state = dbg.state().clone();
                    self.state.log(format!("[✓] Attached to PID {}", pid));
                }
                Err(e) => {
                    self.state.is_debugging = false;
                    self.state.log(format!("[✗] Attach failed: {}", e));
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = pid;
            self.state
                .log("[!] Debug attach is only supported on Windows builds right now.");
        }
    }

    /// Detach from the current process (Windows builds only)
    fn detach_process(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(dbg) = self.debugger.as_mut() {
                if let Some(pid) = dbg.attached_pid() {
                    self.state.log(format!("[*] Detaching from PID {}...", pid));
                } else {
                    self.state.log("[!] Not attached to any process");
                    return;
                }

                match dbg.detach() {
                    Ok(_) => {
                        self.state.is_debugging = false;
                        self.state.debug_state = dbg.state().clone();
                        self.state.show_attach_dialog = false;
                        self.state.log("[*] Detached from process");
                    }
                    Err(e) => {
                        self.state.log(format!("[✗] Detach failed: {}", e));
                    }
                }
            } else {
                self.state.log("[!] Debugger not initialized");
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.state
                .log("[!] Debug detach is only supported on Windows builds right now.");
        }
    }

    /// Render "Attach to Process" dialog
    fn render_attach_dialog(&mut self, ctx: &egui::Context) {
        if !self.state.show_attach_dialog {
            return;
        }

        let mut open = self.state.show_attach_dialog;
        let mut attached_pid = None;

        egui::Window::new("Attach to Process")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(400.0)
            .default_height(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("🔄 Refresh").clicked() {
                        self.state.process_list = crate::debug::enumerate_processes();
                    }
                    ui.label(format!("{} processes found", self.state.process_list.len()));
                });
                
                ui.separator();
                
                // Process list
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("process_list")
                        .striped(true)
                        .num_columns(3)
                        .show(ui, |ui| {
                            ui.strong("PID");
                            ui.strong("Name");
                            ui.strong("Action");
                            ui.end_row();

                            for process in &self.state.process_list {
                                ui.label(format!("{}", process.pid));
                                ui.label(&process.name);
                                if ui.button("Attach").clicked() {
                                    attached_pid = Some(process.pid);
                                }
                                ui.end_row();
                            }
                        });
                });
            });

        self.state.show_attach_dialog = open;

        if let Some(pid) = attached_pid {
            self.state.show_attach_dialog = false;
            self.attach_to_process(pid);
        }
    }
}
