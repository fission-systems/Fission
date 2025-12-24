//! Main application orchestrator for the Fission GUI.
//!
//! This module assembles all UI panels and handles the main event loop.
//! Individual panels are defined in the `panels` module.

pub mod debug_ops;
pub mod decompiler;
pub mod decomp_worker;
pub mod file_ops;
pub mod handlers;

use eframe::egui;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicU64;

use crate::analysis::loader::FunctionInfo;
use crate::debug::Debugger;
#[cfg(target_os = "windows")]
use crate::debug::PlatformDebugger;

use super::state::{AppState, EditorTab, Activity, DebugAction};
use super::messages::AsyncMessage;
use super::menu::{self, MenuAction};
use super::status_bar;
use super::panels::{functions, assembly, decompile, bottom_tabs, activity_bar, side_bar, editor};
use super::panels::bottom_tabs::{ConsoleAction, ScriptAction};
use crate::config::CONFIG;

use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

/// Global Tokio runtime for async operations
pub static TOKIO_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    Runtime::new().expect("Failed to create global Tokio runtime")
});

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

    /// Debug event receiver (Windows)
    #[cfg(target_os = "windows")]
    dbg_event_rx: Option<std::sync::mpsc::Receiver<crate::debug::types::DebugEvent>>,
    /// Debug event loop stop sender
    #[cfg(target_os = "windows")]
    dbg_stop_tx: Option<std::sync::mpsc::Sender<()>>,

    /// Native FFI decompiler (high performance)
    native_decompiler: Arc<Mutex<Option<crate::analysis::decomp::NativeDecompiler>>>,

    /// Decompile request sender (to worker thread)
    decomp_request_tx: Sender<decomp_worker::DecompileRequest>,
    
    /// Latest request ID for debouncing
    latest_request_id: Arc<AtomicU64>,

    /// Theme initialization flag
    theme_initialized: bool,

    /// Python scripting bridge
    #[cfg(feature = "python")]
    python_bridge: crate::script::PythonBridge,
}

impl Default for FissionApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        let (decomp_tx, decomp_rx) = channel();
        let native_decompiler = Arc::new(Mutex::new(None));
        let latest_request_id = Arc::new(AtomicU64::new(0));
        
        // Spawn the decompiler worker thread
        decomp_worker::spawn_worker(
            decomp_rx,
            tx.clone(),
            native_decompiler.clone(),
            latest_request_id.clone(),
        );
        
        // Initialize state early to access event bus
        let state = AppState::default();
        
        // Bridge EventBus to UI AsyncMessage channel
        let tx_clone = tx.clone();
        state.event_bus.subscribe(move |event| {
            let _ = tx_clone.send(AsyncMessage::Event(event.clone()));
        });
        
        Self {
            state,
            rx,
            tx,
            #[cfg(target_os = "windows")]
            debugger: Some(PlatformDebugger::default()),
            #[cfg(target_os = "windows")]
            dbg_event_rx: None,
            #[cfg(target_os = "windows")]
            dbg_stop_tx: None,
            native_decompiler,
            decomp_request_tx: decomp_tx,
            latest_request_id,
            theme_initialized: false,
            #[cfg(feature = "python")]
            python_bridge: crate::script::PythonBridge::new(),
        }
    }
}

impl eframe::App for FissionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Initialize theme on first frame
        if !self.theme_initialized {
            super::theme::init(ctx);
            self.theme_initialized = true;
        }

        // Process async messages
        #[cfg(target_os = "windows")]
        handlers::process_messages(
            &mut self.state,
            &self.rx,
            &self.tx,
            self.native_decompiler.clone(),
            &self.dbg_event_rx,
        );
        #[cfg(not(target_os = "windows"))]
        handlers::process_messages(
            &mut self.state,
            &self.rx,
            &self.tx,
            self.native_decompiler.clone(),
        );

        // Render menu bar and handle actions
        let menu_action = menu::render(ctx, &mut self.state);
        self.handle_menu_action(menu_action);

        // Render status bar
        status_bar::render(ctx, &self.state);

        // VS CODE STYLE LAYOUT
        
        // 1. Activity Bar (Far left)
        activity_bar::render(ctx, &mut self.state);
        
        // 2. Side Bar (Left)
        if let Some(action) = side_bar::render(ctx, &mut self.state) {
            match action {
                side_bar::SideBarAction::SelectFunction(func) => {
                    self.open_function_tabs(&func);
                }
                side_bar::SideBarAction::AnalyzeFunctions => {
                    self.analyze_functions();
                }
            }
        }
        
        // 3. Bottom Panel (Terminal/Output style)
        if self.state.ui.panel_visible {
            let (console_action, script_action) = bottom_tabs::render(ctx, &mut self.state);
            match console_action {
                ConsoleAction::Command(cmd) => {
                    handlers::process_command(&mut self.state, self.tx.clone(), &cmd);
                }
                ConsoleAction::None => {}
            }
            self.handle_script_action(script_action);
        }

        // 4. Editor Area (Central)
        editor::render(ctx, &mut self.state);

        // Update debug state (registers, memory) if suspended
        self.update_debug_state();

        // Process pending debug control requests
        self.handle_pending_debug_actions();
        
        // Handle function click from Explorer/Functions panel
        // (functions::render is now called inside side_bar)
        
        // Render attach dialog
        self.render_attach_dialog(ctx);
        
        // Render xrefs window
        use super::panels::xrefs;
        if let xrefs::XrefAction::NavigateTo(addr) = xrefs::render(ctx, &mut self.state) {
            // Navigate to address - find function containing this address
            self.navigate_to_address(addr);
        }
    }
}

impl FissionApp {
    fn handle_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::OpenFile => file_ops::open_file_dialog(self.tx.clone()),
            MenuAction::AttachToProcess => {
                self.state.ui.show_attach_dialog = true;
                self.state.debug.process_list = crate::debug::enumerate_processes();
            }
            MenuAction::DetachProcess => self.detach_process(),
            MenuAction::ClearConsole => {
                self.state.clear_logs();
                self.state.log("[*] Console cleared");
            }
            MenuAction::ClearCache => {
                let count = self.state.analysis.decompile_cache.len();
                self.state.analysis.decompile_cache.clear();
                self.state.log(format!("[*] Cleared {} cached items", count));
            }
            MenuAction::ShowAbout => {
                self.state.log("[*] Fission v0.1.0 - Ghidra-Powered Analysis Platform");
            }
            MenuAction::ShowXrefs => {
                self.state.ui.show_xrefs_window = true;
            }
            MenuAction::Exit => std::process::exit(0),
            MenuAction::None => {}
        }
    }

    fn handle_script_action(&mut self, action: ScriptAction) {
        match action {
            ScriptAction::Execute(code) => {
                self.execute_python_script(&code);
            }
            ScriptAction::Load => {
                self.load_script_file();
            }
            ScriptAction::Save => {
                self.save_script_file();
            }
            ScriptAction::None => {}
        }
    }

    fn load_script_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Python", &["py"])
            .add_filter("All Files", &["*"])
            .pick_file()
        {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    self.state.script.script_code = content;
                    self.state.script.script_path = Some(path.display().to_string());
                    self.state.script.script_output.push(format!("[✓] Loaded: {}", path.display()));
                }
                Err(e) => {
                    self.state.script.script_output.push(format!("[Error] Failed to load: {}", e));
                }
            }
        }
    }

    fn save_script_file(&mut self) {
        let default_path = self.state.script.script_path.as_ref()
            .map(|p| std::path::PathBuf::from(p))
            .unwrap_or_else(|| std::path::PathBuf::from("script.py"));
        
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Python", &["py"])
            .set_file_name(default_path.file_name().unwrap_or_default().to_str().unwrap_or("script.py"))
            .save_file()
        {
            match std::fs::write(&path, &self.state.script.script_code) {
                Ok(_) => {
                    self.state.script.script_path = Some(path.display().to_string());
                    self.state.script.script_output.push(format!("[✓] Saved: {}", path.display()));
                }
                Err(e) => {
                    self.state.script.script_output.push(format!("[Error] Failed to save: {}", e));
                }
            }
        }
    }

    #[cfg(feature = "python")]
    fn execute_python_script(&mut self, code: &str) {
        self.state.script.script_running = true;
        self.state.script.script_output.push(format!(">>> Executing script..."));
        
        // Initialize Python if needed
        if let Err(e) = self.python_bridge.initialize() {
            self.state.script.script_output.push(format!("[Error] Failed to initialize Python: {}", e));
            self.state.script.script_running = false;
            return;
        }

        // Sync loaded binary to Python bridge
        self.python_bridge.set_binary(self.state.analysis.loaded_binary.clone());
        
        // Execute the script
        match self.python_bridge.run(code) {
            Ok(_) => {
                self.state.script.script_output.push("[✓] Script executed successfully".into());
            }
            Err(e) => {
                self.state.script.script_output.push(format!("[Error] {}", e));
            }
        }
        
        self.state.script.script_running = false;
    }

    #[cfg(not(feature = "python"))]
    fn execute_python_script(&mut self, _code: &str) {
        self.state.script.script_output.push("[Error] Python support not enabled. Build with --features python".into());
    }

    fn open_function_tabs(&mut self, func: &FunctionInfo) {
        let asm_tab = EditorTab::Assembly(func.name.clone());
        let decomp_tab = EditorTab::Decompiled(func.name.clone());
        
        // Open Assembly tab if not open
        if !self.state.ui.open_tabs.contains(&asm_tab) {
            self.state.ui.open_tabs.push(asm_tab.clone());
        }
        
        // Open Decompiled tab if not open
        if !self.state.ui.open_tabs.contains(&decomp_tab) {
            self.state.ui.open_tabs.push(decomp_tab.clone());
        }
        
        // Focus Decompiled tab by default
        if let Some(pos) = self.state.ui.open_tabs.iter().position(|t| t == &decomp_tab) {
            self.state.ui.active_tab_index = Some(pos);
        }
        
        self.state.analysis.selected_function = Some(func.clone());
        self.decompile_function(func);
    }

    fn handle_pending_debug_actions(&mut self) {
        if let Some(action) = self.state.debug.pending_debug_action.take() {
            match action {
                DebugAction::Detach => {
                    #[cfg(target_os = "windows")]
                    debug_ops::detach_process(
                        &mut self.state,
                        &mut self.debugger,
                        &mut self.dbg_stop_tx,
                    );
                    #[cfg(not(target_os = "windows"))]
                    debug_ops::detach_process(&mut self.state);
                }
                _ => {
                    #[cfg(target_os = "windows")]
                    debug_ops::handle_debug_action(&mut self.state, &mut self.debugger, action);
                    #[cfg(not(target_os = "windows"))]
                    debug_ops::handle_debug_action(&mut self.state, action);
                }
            }
        }
        if let Some(bp_action) = self.state.debug.pending_bp_action.take() {
            #[cfg(target_os = "windows")]
            debug_ops::handle_bp_action(&mut self.state, &mut self.debugger, bp_action);
            #[cfg(not(target_os = "windows"))]
            debug_ops::handle_bp_action(&mut self.state, bp_action);
        }
    }

    fn decompile_function(&mut self, func: &FunctionInfo) {
        decompiler::decompile_function(
            &mut self.state,
            &self.decomp_request_tx,
            &self.latest_request_id,
            func,
        );
    }

    fn update_debug_state(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(dbg) = self.debugger.as_mut() {
                // Update registers if suspended
                if self.state.debug.debug_state.status == crate::debug::types::DebugStatus::Suspended {
                    if let Some(tid) = self.state.debug.debug_state.last_thread_id.or(self.state.debug.debug_state.main_thread_id) {
                        if let Ok(regs) = dbg.fetch_registers(tid) {
                            self.state.debug.debug_state.registers = Some(regs);
                        }
                    }
                }

                // Handle pending memory read
                if let Some((addr, len)) = self.state.debug.pending_mem_read.take() {
                    match dbg.read_memory(addr, len) {
                        Ok(data) => {
                            self.state.debug.mem_dump = format_hexdump(addr, &data);
                        }
                        Err(e) => {
                            self.state.debug.mem_dump = format!("Error reading memory: {}", e);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn detach_process(&mut self) {
        debug_ops::detach_process(&mut self.state, &mut self.debugger, &mut self.dbg_stop_tx);
    }

    #[cfg(not(target_os = "windows"))]
    fn detach_process(&mut self) {
        debug_ops::detach_process(&mut self.state);
    }

    fn render_attach_dialog(&mut self, ctx: &egui::Context) {
        if let Some(process) = debug_ops::render_attach_dialog(&mut self.state, ctx) {
            self.state.ui.show_attach_dialog = false;
            
            // Load binary if exe_path is available
            if let Some(ref exe_path) = process.exe_path {
                self.state.log(format!("[*] Loading binary from: {}", exe_path));
                file_ops::load_binary(&mut self.state, self.tx.clone(), exe_path);
            }
            
            // Attach to process
            self.attach_to_process(process.pid);
        }
    }

    #[cfg(target_os = "windows")]
    fn attach_to_process(&mut self, pid: u32) {
        debug_ops::attach_to_process(
            &mut self.state,
            &mut self.debugger,
            &mut self.dbg_event_rx,
            &mut self.dbg_stop_tx,
            pid,
        );
    }

    #[cfg(not(target_os = "windows"))]
    fn attach_to_process(&mut self, pid: u32) {
        debug_ops::attach_to_process(&mut self.state, pid);
    }

    /// Navigate to an address - find function containing this address and select it
    fn navigate_to_address(&mut self, addr: u64) {
        // Clone the functions list to avoid borrow issues
        let functions: Vec<FunctionInfo> = self.state.analysis.loaded_binary
            .as_ref()
            .map(|b| b.functions.clone())
            .unwrap_or_default();
        
        // Find function containing or starting at this address
        for func in &functions {
            // Check if address is within function range (configurable)
            let range = CONFIG.analysis.function_address_range as u64;
            if addr >= func.address && addr < func.address + range {
                self.state.log(format!("[*] Navigating to function: {} at 0x{:08X}", func.name, func.address));
                self.state.analysis.selected_function = Some(func.clone());
                self.state.ui.selected_xref_addr = Some(func.address);
                self.decompile_function(func);
                return;
            }
        }
        
        // If no function found, just log
        if !functions.is_empty() {
            self.state.log(format!("[!] No function found at address 0x{:08X}", addr));
        }
    }
}

fn format_hexdump(addr: u64, data: &[u8]) -> String {
    let mut output = String::new();
    for chunk in data.chunks(16) {
        output.push_str(&format!("{:016X}: ", addr + (output.len() as u64 / 75 * 16)));
        for b in chunk {
            output.push_str(&format!("{:02X} ", b));
        }
        if chunk.len() < 16 {
            for _ in 0..(16 - chunk.len()) { output.push_str("   "); }
        }
        output.push_str(" | ");
        for b in chunk {
            output.push(if *b >= 0x20 && *b <= 0x7E { *b as char } else { '.' });
        }
        output.push('\n');
    }
    output
}

impl FissionApp {
    /// Analyze the loaded binary to discover internal functions from CALL instructions
    fn analyze_functions(&mut self) {
        // Clone the Arc first to avoid borrow checker issues
        let binary_opt = self.state.analysis.loaded_binary.clone();
        
        if let Some(binary_arc) = binary_opt {
            self.state.log("[*] Analyzing binary for internal functions...");
            
            // Clone the inner LoadedBinary to get a mutable copy
            let mut binary = (*binary_arc).clone();
            let before_count = binary.functions.len();
            
            // Discover internal functions
            binary.discover_internal_functions();
            
            let after_count = binary.functions.len();
            let discovered = after_count - before_count;
            
            // Replace with new Arc
            self.state.analysis.loaded_binary = Some(std::sync::Arc::new(binary));
            
            self.state.log(format!("[✓] Found {} new internal functions ({} total)", 
                discovered, after_count));
        } else {
            self.state.log("[!] No binary loaded");
        }
    }
}
