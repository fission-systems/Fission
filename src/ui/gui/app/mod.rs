//! Main application orchestrator for the Fission GUI.
//!
//! This module assembles all UI panels and handles the main event loop.
//! Individual panels are defined in the `panels` module.

pub mod debug_ops;
pub mod decompiler;
pub mod file_ops;
pub mod handlers;

use eframe::egui;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::analysis::decomp::client::GhidraClient;
use crate::analysis::loader::FunctionInfo;
#[cfg(target_os = "windows")]
use crate::debug::PlatformDebugger;

use super::state::AppState;
use super::messages::AsyncMessage;
use super::menu::{self, MenuAction};
use super::status_bar;
use super::panels::{functions, assembly, decompile, bottom_tabs};
use super::panels::bottom_tabs::ConsoleAction;

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

    /// Shared Ghidra client to avoid reconnect cost
    ghidra_client: Arc<Mutex<Option<GhidraClient>>>,

    /// Theme initialization flag
    theme_initialized: bool,
}

impl Default for FissionApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            state: AppState::default(),
            rx,
            tx,
            #[cfg(target_os = "windows")]
            debugger: Some(PlatformDebugger::default()),
            #[cfg(target_os = "windows")]
            dbg_event_rx: None,
            #[cfg(target_os = "windows")]
            dbg_stop_tx: None,
            ghidra_client: Arc::new(Mutex::new(None)),
            theme_initialized: false,
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
            self.ghidra_client.clone(),
            &self.dbg_event_rx,
        );
        #[cfg(not(target_os = "windows"))]
        handlers::process_messages(
            &mut self.state,
            &self.rx,
            &self.tx,
            self.ghidra_client.clone(),
        );

        // Render menu bar and handle actions
        let menu_action = menu::render(ctx, &mut self.state);
        self.handle_menu_action(menu_action);

        // Render status bar
        status_bar::render(ctx, &self.state);

        // Render panels
        let clicked_func = functions::render(ctx, &mut self.state);
        
        // Bottom tabbed panel (Console, Hex View, Strings, Debug)
        match bottom_tabs::render(ctx, &mut self.state) {
            ConsoleAction::Command(cmd) => {
                handlers::process_command(&mut self.state, self.tx.clone(), &cmd);
            }
            ConsoleAction::None => {}
        }

        // Process pending debug control requests
        self.handle_pending_debug_actions();
        
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
    fn handle_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::OpenFile => file_ops::open_file_dialog(self.tx.clone()),
            MenuAction::AttachToProcess => {
                self.state.show_attach_dialog = true;
                self.state.process_list = crate::debug::enumerate_processes();
            }
            MenuAction::DetachProcess => self.detach_process(),
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
    }

    fn handle_pending_debug_actions(&mut self) {
        if let Some(action) = self.state.pending_debug_action.take() {
            #[cfg(target_os = "windows")]
            debug_ops::handle_debug_action(&mut self.state, &mut self.debugger, action);
            #[cfg(not(target_os = "windows"))]
            debug_ops::handle_debug_action(&mut self.state, action);
        }
        if let Some(bp_action) = self.state.pending_bp_action.take() {
            #[cfg(target_os = "windows")]
            debug_ops::handle_bp_action(&mut self.state, &mut self.debugger, bp_action);
            #[cfg(not(target_os = "windows"))]
            debug_ops::handle_bp_action(&mut self.state, bp_action);
        }
    }

    fn decompile_function(&mut self, func: &FunctionInfo) {
        decompiler::decompile_function(
            &mut self.state,
            self.tx.clone(),
            self.ghidra_client.clone(),
            func,
        );
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
        if let Some(pid) = debug_ops::render_attach_dialog(&mut self.state, ctx) {
            self.state.show_attach_dialog = false;
            self.attach_to_process(pid);
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
}

