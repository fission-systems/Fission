//! Main application orchestrator for the Fission GUI.
//!
//! This module assembles all UI panels and handles the main event loop.
//! Individual panels are defined in the `panels` module.

pub mod analysis_ops;
pub mod debug_ops;
pub mod decomp_worker;
pub mod decompiler;
pub mod file_ops;
pub mod handlers;
pub mod script_ops;
pub mod titan_ops;

use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "windows")]
use crate::debug::PlatformDebugger;
use fission_loader::loader::FunctionInfo;

use super::components::status_bar;
use super::components::{MenuAction, menu};
use super::core::state::DebugAction;
use super::core::{AppState, AsyncMessage};
use super::panels::bottom_tabs::{ConsoleAction, ScriptAction};
use super::panels::{activity_bar, bottom_tabs, editor, side_bar};
use crate::app::modules::ModuleManager;
use crate::plugin::PluginManager;
use crate::plugin::module::PluginModule;

use std::sync::LazyLock;
use tokio::runtime::Runtime;

/// Global Tokio runtime for async operations
#[allow(dead_code)]
pub static TOKIO_RUNTIME: LazyLock<Runtime> =
    LazyLock::new(|| Runtime::new().expect("Failed to create global Tokio runtime"));

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
    dbg_event_rx: Option<crossbeam_channel::Receiver<crate::debug::types::DebugEvent>>,

    // Channel to send stop command to debug thread
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    dbg_stop_tx: Option<crossbeam_channel::Sender<()>>,

    /// Decompile request sender (to worker thread)
    decomp_request_tx: Sender<decomp_worker::WorkerRequest>,

    /// Latest request ID for debouncing
    latest_request_id: Arc<AtomicU64>,

    /// Theme initialization flag (legacy, now tracked by current_theme)
    theme_initialized: bool,

    /// Currently applied theme (to detect changes)
    current_theme: Option<crate::ui::gui::ThemeMode>,

    /// Track last dynamic mode to detect changes
    last_dynamic_mode: bool,

    /// Python scripting bridge
    #[cfg(feature = "python")]
    python_bridge: crate::script::PythonBridge,

    /// Module manager for lifecycle management
    module_manager: ModuleManager,

    /// System info for memory usage tracking
    sysinfo: sysinfo::System,
    /// Last memory update time
    last_mem_update: std::time::Instant,
}

impl Default for FissionApp {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        let (decomp_tx, decomp_rx) = unbounded();
        let latest_request_id = Arc::new(AtomicU64::new(0));

        // Spawn the decompiler worker thread
        decomp_worker::spawn_worker(decomp_rx, tx.clone(), latest_request_id.clone());

        // Initialize state early to access event bus
        let state = AppState::default();

        // Bridge EventBus to UI AsyncMessage channel
        let tx_clone = tx.clone();
        state.event_bus().subscribe(move |event| {
            let _ = tx_clone.send(AsyncMessage::Event(event.clone()));
        });

        // Initialize Module Manager with lifecycle management
        let mut module_manager = ModuleManager::new(state.event_bus().clone());

        // Register PluginModule
        let plugin_manager = Arc::new(Mutex::new(PluginManager::new()));
        module_manager.register_module(Box::new(PluginModule::new(plugin_manager)));

        // Run lifecycle: init -> start
        if let Err(e) = module_manager.init_all() {
            crate::core::logging::warn(&format!("[FissionApp] Module init failed: {}", e));
        }
        if let Err(e) = module_manager.start_all() {
            crate::core::logging::warn(&format!("[FissionApp] Module start failed: {}", e));
        }

        Self {
            state,
            rx,
            tx,
            #[cfg(target_os = "windows")]
            debugger: Some(PlatformDebugger::default()),
            #[cfg(target_os = "windows")]
            dbg_event_rx: None,
            dbg_stop_tx: None,
            decomp_request_tx: decomp_tx,
            latest_request_id,
            theme_initialized: false,
            current_theme: None,
            last_dynamic_mode: false,
            #[cfg(feature = "python")]
            python_bridge: crate::script::PythonBridge::new(),
            module_manager,
            sysinfo: sysinfo::System::new_all(),
            last_mem_update: std::time::Instant::now(),
        }
    }
}

impl Drop for FissionApp {
    fn drop(&mut self) {
        // Shutdown all modules gracefully
        if let Err(e) = self.module_manager.shutdown_all() {
            crate::core::logging::warn(&format!("[FissionApp] Module shutdown failed: {}", e));
        }
    }
}

impl eframe::App for FissionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Initialize or update theme if changed
        let target_theme = self.state.settings.theme_mode;
        let dynamic_mode = self.state.ui.dynamic_mode;

        if self.current_theme != Some(target_theme) || self.last_dynamic_mode != dynamic_mode {
            super::theme::set_theme(ctx, target_theme, dynamic_mode);

            // Handle tab switching if mode changed
            if self.last_dynamic_mode != dynamic_mode {
                use crate::ui::gui::core::state::BottomTab;
                let invalid = match self.state.ui.bottom_tab {
                    BottomTab::Debug | BottomTab::Timeline => !dynamic_mode,
                    BottomTab::Strings | BottomTab::Imports | BottomTab::Cfg | BottomTab::Xrefs => {
                        dynamic_mode
                    }
                    _ => false,
                };
                if invalid {
                    self.state.ui.bottom_tab = BottomTab::Console;
                }
            }

            self.current_theme = Some(target_theme);
            self.last_dynamic_mode = dynamic_mode;

            // Re-configure fonts as they might need reloading or style update
            super::theme::configure_fonts(ctx);
            // Only load fonts once if possible, or check if needed
            if !self.theme_initialized {
                super::theme::load_jetbrains_mono(ctx);
                self.theme_initialized = true;
            }
        }

        // Apply UI Scale
        ctx.set_pixels_per_point(self.state.settings.ui_scale);

        // Clear selected function if no binary is loaded (prevents egui state restoration issues)
        if self.state.analysis.domain.loaded_binary.as_ref().is_none() {
            if self.state.analysis.domain.selected_function.is_some() {
                self.state.analysis.domain.selected_function = None;
            }
            // Reset decompiler context flag if no binary
            if self.state.analysis.domain.decompiler_context_loaded {
                self.state.analysis.domain.decompiler_context_loaded = false;
            }
            // Also clear any open tabs since they're invalid without a binary
            if !self.state.ui.open_tabs.is_empty() {
                self.state.ui.open_tabs.clear();
                self.state.ui.active_tab_index = None;
            }
        }

        // Update resource usage every 2 seconds
        if self.last_mem_update.elapsed().as_secs() >= 2 {
            use sysinfo::{Pid, ProcessesToUpdate};

            let pid = Pid::from(std::process::id() as usize);

            // Refresh processes to get current CPU/memory
            self.sysinfo
                .refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

            if let Some(process) = self.sysinfo.process(pid) {
                self.state.ui.memory_usage = process.memory();
                self.state.ui.cpu_usage = process.cpu_usage();
            }

            self.last_mem_update = std::time::Instant::now();
        }

        // Process async messages
        #[cfg(target_os = "windows")]
        handlers::process_messages(
            &mut self.state,
            &self.rx,
            &self.tx,
            &self.decomp_request_tx,
            &self.latest_request_id,
            &self.dbg_event_rx,
        );
        #[cfg(not(target_os = "windows"))]
        handlers::process_messages(
            &mut self.state,
            &self.rx,
            &self.tx,
            &self.decomp_request_tx,
            &self.latest_request_id,
        );

        // Render menu bar and handle actions
        let menu_action = menu::render(ctx, &mut self.state);
        self.handle_menu_action(menu_action);

        // Render status bar
        status_bar::render(ctx, &mut self.state);

        // VS CODE STYLE LAYOUT

        // 1. Activity Bar (Far left)
        activity_bar::render(ctx, &mut self.state);

        // 2. Side Bar (Left)
        if let Some(action) = side_bar::render(ctx, &mut self.state) {
            match action {
                side_bar::SideBarAction::SelectFunction(func) => {
                    // Record current location before jump
                    let current_addr = self
                        .state
                        .analysis
                        .domain
                        .selected_function
                        .as_ref()
                        .map(|f| f.address);

                    if let Some(addr) = current_addr {
                        analysis_ops::push_navigation(&mut self.state, addr);
                    }
                    self.open_function_tabs(&func);
                }
                side_bar::SideBarAction::AnalyzeFunctions => {
                    analysis_ops::analyze_functions(&mut self.state);
                }
                side_bar::SideBarAction::DeepScanFunctions => {
                    let count = self.state.analysis.domain.scan_for_missing_functions();
                    if count > 0 {
                        self.state.log(format!(
                            "[*] Recovered {} hidden functions via prologue scan.",
                            count
                        ));

                        // Function Identification (FID)
                        let mut fid_count = 0;
                        let mut logs = Vec::new();

                        if let Some(ref mut binary_arc) = self.state.analysis.domain.loaded_binary {
                            let binary = std::sync::Arc::make_mut(binary_arc);

                            // Initialize signature database
                            let db = fission_signatures::SignatureDatabase::new();

                            // Collect only the newly scanned functions to identify
                            let scanned_funcs: Vec<_> = binary
                                .functions
                                .iter()
                                .filter(|f| f.name.ends_with("_scanned"))
                                .map(|f| (f.address, f.name.clone()))
                                .collect();

                            if !scanned_funcs.is_empty() {
                                // Identify functions
                                let matched = db.identify_functions_in_binary(
                                    binary.data.as_slice(),
                                    &scanned_funcs,
                                    binary.image_base,
                                );

                                if !matched.is_empty() {
                                    for func in &mut binary.functions {
                                        if let Some(new_name) = matched.get(&func.address) {
                                            logs.push(format!(
                                                "    [+] Identified 0x{:x} as {}",
                                                func.address, new_name
                                            ));
                                            func.name = new_name.clone();
                                            fid_count += 1;
                                        }
                                    }
                                }
                            }
                        }

                        // Apply updates to state
                        for msg in logs {
                            self.state.log(msg);
                        }

                        if fid_count > 0 {
                            // Force UI refresh
                            self.state.viewmodels.functions.cache_key = None;
                            self.state.log(format!(
                                "[*] Identified {} functions using built-in signatures.",
                                fid_count
                            ));
                        }

                        if let (Some(ref binary), Some(ref xref_db)) = (
                            self.state.analysis.domain.loaded_binary.as_ref(),
                            self.state.analysis.domain.xref_db.as_ref(),
                        ) {
                            let call_graph =
                                crate::analysis::callgraph::CallGraph::build_from_xrefs(
                                    &binary.functions,
                                    xref_db,
                                    crate::core::config::CONFIG.analysis.function_address_range
                                        as u64,
                                );
                            self.state.analysis.domain.call_graph = Some(call_graph);
                            self.state.viewmodels.xrefs.clear();
                        }
                    } else {
                        self.state
                            .log("[*] No additional hidden functions found by signature scanning.");
                    }
                }
                side_bar::SideBarAction::RenameFunction(addr) => {
                    // Get current name for the function
                    let current_name = self
                        .state
                        .analysis
                        .user_function_names()
                        .get(&addr)
                        .cloned()
                        .unwrap_or_else(|| format!("sub_{:x}", addr));
                    self.state.viewmodels.functions.rename_dialog = Some((addr, current_name));
                }
                side_bar::SideBarAction::SwitchBinary(binary) => {
                    // Log the switch
                    let file_name = std::path::Path::new(&binary.path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&binary.path);

                    self.state
                        .log(format!("[*] Switching to binary: {}", file_name));

                    // Clear current state
                    self.state.analysis.domain.selected_function = None;
                    self.state.ui.open_tabs.clear();
                    self.state.ui.active_tab_index = None;

                    // Reinitialize decompiler context with new binary
                    handlers::message_handlers::handle_binary_loaded(
                        &mut self.state,
                        binary,
                        &self.decomp_request_tx,
                    );
                }
                side_bar::SideBarAction::OpenListing => {
                    analysis_ops::open_listing_tab(&mut self.state);
                }
            }
        }

        // 3. Bottom Panel (Terminal/Output style)
        if self.state.ui.panel_visible {
            let (console_action, script_action, cfg_action) =
                bottom_tabs::render(ctx, &mut self.state);
            match console_action {
                ConsoleAction::Command(cmd) => {
                    handlers::process_command(&mut self.state, self.tx.clone(), &cmd);
                }
                ConsoleAction::None => {}
            }

            // Handle CFG analysis requests
            use super::panels::bottom_tabs::CfgAction;
            match cfg_action {
                CfgAction::Analyze(addr) => {
                    let _ = self
                        .tx
                        .send(AsyncMessage::CfgAnalysisRequest { address: addr });
                }
                CfgAction::None => {}
            }

            #[cfg(feature = "python")]
            script_ops::handle_action(&mut self.state, script_action, &mut self.python_bridge);
            #[cfg(not(feature = "python"))]
            if let ScriptAction::Execute(_) = script_action {
                self.state
                    .script
                    .script_output
                    .push("[Error] Python support not enabled".into());
            }
        }

        // 4. Editor Area (Central)
        editor::render(ctx, &mut self.state);

        // Update debug state (registers, memory) if suspended
        self.update_debug_state();

        // Process pending debug control requests
        self.handle_pending_debug_actions();

        // Render attach dialog
        self.render_attach_dialog(ctx);

        // Render xrefs window
        use super::panels::xrefs;
        if let xrefs::XrefAction::NavigateTo(addr) = xrefs::render(ctx, &mut self.state) {
            // Navigate to address - find function containing this address
            analysis_ops::navigate_to_address(
                &mut self.state,
                addr,
                &self.decomp_request_tx,
                &self.latest_request_id,
            );
        }

        // Render string xrefs window
        use super::panels::string_xrefs;
        string_xrefs::render(ctx, &mut self.state);

        // Render input dialogs
        self.render_rename_dialog(ctx);
        self.render_comment_dialog(ctx);
        self.render_goto_address_dialog(ctx);

        // Handle navigation actions
        self.handle_navigation_actions(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(e) = crate::core::config_store::save(&self.state.settings) {
            crate::core::logging::error(&format!("Failed to save settings: {}", e));
        } else {
            crate::core::logging::info("Settings saved successfully");
        }
    }
}

impl FissionApp {
    fn handle_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::OpenFile => file_ops::open_file_dialog(self.tx.clone()),
            MenuAction::OpenFolder => file_ops::open_folder_dialog(self.tx.clone()),
            MenuAction::SaveSnapshot => file_ops::save_snapshot_dialog(self.tx.clone()),
            MenuAction::LoadSnapshot => file_ops::load_snapshot_dialog(self.tx.clone()),
            MenuAction::SaveProject => file_ops::save_project_dialog(self.tx.clone()),
            MenuAction::LoadProject => file_ops::load_project_dialog(self.tx.clone()),
            MenuAction::AttachToProcess => {
                self.state.ui.show_attach_dialog = true;
                self.state.debug.domain.process_list = crate::debug::enumerate_processes();
            }
            MenuAction::DetachProcess => self.detach_process(),
            MenuAction::ClearConsole => {
                self.state.clear_logs();
                self.state.log("[*] Console cleared");
            }
            MenuAction::ClearCache => {
                let _ = self
                    .decomp_request_tx
                    .send(decomp_worker::WorkerRequest::clear_cache(String::new()));
                self.state.log("[*] Decompiler cache clear requested");
            }
            MenuAction::ShowAbout => {
                self.state
                    .log("[*] Fission v0.1.0 - Ghidra-Powered Analysis Platform");
            }
            MenuAction::ShowXrefs => {
                self.state.ui.show_xrefs_window = true;
            }
            MenuAction::ShowStringXrefs => {
                self.state.ui.show_string_xrefs_window = true;
            }
            MenuAction::BatchDecompile => {
                analysis_ops::batch_decompile_project(
                    &mut self.state,
                    &self.decomp_request_tx,
                    &self.latest_request_id,
                );
            }
            MenuAction::ExportResults => {
                file_ops::export_results_dialog(self.tx.clone());
            }
            MenuAction::Exit => std::process::exit(0),
            MenuAction::None => {}
        }
    }

    fn open_function_tabs(&mut self, func: &FunctionInfo) {
        // Skip if no binary is loaded
        if self.state.analysis.domain.loaded_binary.as_ref().is_none() {
            self.state
                .log("[!] Cannot open function: No binary loaded".to_string());
        }

        analysis_ops::open_function_tabs(
            &mut self.state,
            func,
            &self.decomp_request_tx,
            &self.latest_request_id,
        );
    }

    fn handle_pending_debug_actions(&mut self) {
        if let Some(action) = self.state.debug.pending_debug_action.take() {
            // TitanEngine Actions (Dynamic Mode)
            if titan_ops::handle_debug_action(&mut self.state, action) {
                return;
            }

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

    #[allow(dead_code)]
    fn decompile_function(&mut self, func: &FunctionInfo) {
        decompiler::decompile_function(
            &mut self.state,
            &self.decomp_request_tx,
            &self.latest_request_id,
            func,
        );
    }

    #[allow(clippy::needless_pass_by_ref_mut)] // Needs mut on Windows platform
    fn update_debug_state(&mut self) {
        // TitanEngine Integration (Dynamic Mode)
        if self.state.ui.dynamic_mode {
            #[cfg(target_os = "windows")]
            if let Some(engine_lock) = &self.state.debug.domain.titan_engine {
                if let Ok(engine) = engine_lock.read() {
                    if engine.active_process.is_some() {
                        if let Ok(ctx) = engine.get_context() {
                            let regs = crate::debug::types::RegisterState {
                                rax: ctx.rax(),
                                rbx: ctx.rbx(),
                                rcx: ctx.rcx(),
                                rdx: ctx.rdx(),
                                rsi: ctx.rsi(),
                                rdi: ctx.rdi(),
                                rbp: ctx.rbp(),
                                rsp: ctx.rsp(),
                                r8: ctx.r8(),
                                r9: ctx.r9(),
                                r10: ctx.r10(),
                                r11: ctx.r11(),
                                r12: ctx.r12(),
                                r13: ctx.r13(),
                                r14: ctx.r14(),
                                r15: ctx.r15(),
                                rip: ctx.rip(),
                                rflags: ctx.rflags(),
                            };
                            self.state.debug.domain.debug_state().registers = Some(regs);
                        }
                    }
                }
            }
        } else {
            #[cfg(target_os = "windows")]
            {
                if let Some(dbg) = self.debugger.as_mut() {
                    // Update registers if suspended
                    if self.state.debug.domain.debug_state().status
                        == crate::debug::types::DebugStatus::Suspended
                    {
                        if let Some(tid) =
                            self.state.debug.domain.debug_state().last_thread_id.or(self
                                .state
                                .debug
                                .debug_state
                                .main_thread_id)
                        {
                            if let Ok(regs) = dbg.fetch_registers(tid) {
                                self.state.debug.domain.debug_state().registers = Some(regs);
                            }
                        }
                    }

                    // Handle pending memory read
                    if let Some((addr, len)) = self.state.debug.pending_mem_read.take() {
                        match dbg.read_memory(addr, len) {
                            Ok(data) => {
                                self.state.debug.domain.mem_dump = format_hexdump(addr, &data);
                            }
                            Err(e) => {
                                self.state.debug.domain.mem_dump =
                                    format!("Error reading memory: {}", e);
                            }
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
                self.state
                    .log(format!("[*] Loading binary from: {}", exe_path));
                file_ops::load_binary(&mut self.state, self.tx.clone(), exe_path);
            }

            // Attach to process
            self.attach_to_process(process.pid);
        }
    }

    #[cfg(target_os = "windows")]
    fn attach_to_process(&mut self, pid: u32) {
        if titan_ops::attach(&mut self.state, pid) {
            return;
        }

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

    fn render_rename_dialog(&mut self, ctx: &egui::Context) {
        let mut rename_result = None;
        if let Some((addr, ref mut name)) = self.state.viewmodels.functions.rename_dialog {
            egui::Window::new("Rename Symbol")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(format!("Rename symbol at 0x{:x}", addr)).strong(),
                    );
                    ui.add_space(8.0);
                    let res = ui.add(
                        egui::TextEdit::singleline(name)
                            .hint_text("New name...")
                            .desired_width(200.0),
                    );
                    if res.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        rename_result = Some((addr, name.clone()));
                    }
                    res.request_focus();

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Confirm").clicked() {
                            rename_result = Some((addr, name.clone()));
                        }
                        if ui.button("Cancel").clicked() {
                            rename_result = Some((0, String::new())); // Close signal
                        }
                    });
                });
        }

        if let Some((addr, name)) = rename_result {
            if addr != 0 {
                use crate::ui::gui::core::state::EditorTab;

                // 1. Determine the previous display name to find existing tabs
                let old_name = self
                    .state
                    .analysis
                    .domain
                    .user_function_names
                    .get(&addr)
                    .cloned()
                    .unwrap_or_else(|| {
                        self.state
                            .analysis
                            .domain
                            .loaded_binary
                            .as_ref()
                            .and_then(|b| b.functions.iter().find(|f| f.address == addr))
                            .map(|f| f.name.clone())
                            .unwrap_or_else(|| format!("sub_{:x}", addr))
                    });

                // 2. Update the name map
                self.state
                    .analysis
                    .domain
                    .user_function_names
                    .insert(addr, name.clone());
                self.state.viewmodels.xrefs.clear();
                self.state
                    .log(format!("[*] Renamed symbol at 0x{:x} to {}", addr, name));

                // 3. Update any open tabs with the old name
                for tab in self.state.ui.open_tabs.iter_mut() {
                    match tab {
                        EditorTab::Assembly(n) if *n == old_name => *n = name.clone(),
                        EditorTab::Decompiled(n) if *n == old_name => *n = name.clone(),
                        _ => {}
                    }
                }

                // 4. Trigger re-decompilation if the current function is affected
                if let Some(ref selected) = self.state.analysis.domain.selected_function {
                    self.open_function_tabs(&selected.clone());
                }
            }
            self.state.viewmodels.functions.rename_dialog = None;
        }
    }

    fn render_comment_dialog(&mut self, ctx: &egui::Context) {
        use crate::ui::gui::theme::catppuccin;

        let mut comment_result: Option<(u64, Option<String>)> = None;

        if let Some((addr, ref mut comment)) = self.state.viewmodels.functions.comment_dialog {
            egui::Window::new("💬 Edit Comment")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(format!("Address: 0x{:x}", addr))
                            .color(catppuccin::BLUE)
                            .strong(),
                    );
                    ui.add_space(8.0);

                    let res = ui.add(
                        egui::TextEdit::multiline(comment)
                            .hint_text("Enter your comment here...")
                            .desired_width(320.0)
                            .desired_rows(3)
                            .font(egui::TextStyle::Monospace),
                    );
                    res.request_focus();

                    // Handle Enter (with Ctrl) to save, Escape to cancel
                    let enter_pressed =
                        ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl);
                    let escape_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Ctrl+Enter to save • Escape to cancel")
                            .small()
                            .color(catppuccin::OVERLAY0),
                    );

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("💾 Save").clicked() || enter_pressed {
                            comment_result = Some((addr, Some(comment.clone())));
                        }
                        if ui
                            .button(egui::RichText::new("🗑 Delete").color(catppuccin::RED))
                            .clicked()
                        {
                            comment_result = Some((addr, None)); // Delete comment
                        }
                        if ui.button("Cancel").clicked() || escape_pressed {
                            comment_result = Some((0, None)); // Close without saving
                        }
                    });
                });
        }

        if let Some((addr, maybe_comment)) = comment_result {
            if addr != 0 {
                match maybe_comment {
                    Some(comment) if !comment.is_empty() => {
                        self.state
                            .analysis
                            .domain
                            .user_comments
                            .insert(addr, comment);
                        self.state.log(format!("[*] Comment saved at 0x{:x}", addr));
                    }
                    _ => {
                        self.state.analysis.domain.user_comments.remove(&addr);
                        self.state
                            .log(format!("[*] Comment removed from 0x{:x}", addr));
                    }
                }

                // Refresh view
                if let Some(ref selected) = self.state.analysis.domain.selected_function {
                    self.open_function_tabs(&selected.clone());
                }
            }
            self.state.viewmodels.functions.comment_dialog = None;
        }
    }

    pub fn handle_navigation_actions(&mut self, ctx: &egui::Context) {
        // 1. Check Keyboard Shortcuts (Alt + Left / Alt + Right)
        let back_pressed = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)
                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowLeft)
        });

        let forward_pressed = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight)
                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowRight)
        });

        // 2. Go to Address (G)
        let g_pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::G));
        if g_pressed
            && self
                .state
                .viewmodels
                .navigation
                .goto_address_input
                .is_none()
        {
            self.state.viewmodels.navigation.goto_address_input = Some(String::new());
        }

        if back_pressed || self.state.ui.pending_nav_back {
            self.state.ui.pending_nav_back = false;
            analysis_ops::navigate_back(
                &mut self.state,
                &self.decomp_request_tx,
                &self.latest_request_id,
            );
        }

        if forward_pressed || self.state.ui.pending_nav_forward {
            self.state.ui.pending_nav_forward = false;
            analysis_ops::navigate_forward(
                &mut self.state,
                &self.decomp_request_tx,
                &self.latest_request_id,
            );
        }

        // 3. Toggle Bookmark (F2)
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
            if let Some(addr) = self.state.ui.selected_xref_addr {
                if self.state.analysis.domain.bookmarks.contains_key(&addr) {
                    self.state.analysis.domain.bookmarks.remove(&addr);
                    self.state
                        .log(format!("[*] Bookmark removed at 0x{:08X}", addr));
                } else {
                    let label = self
                        .state
                        .analysis
                        .domain
                        .user_function_names
                        .get(&addr)
                        .cloned()
                        .unwrap_or_else(|| format!("loc_{:x}", addr));
                    self.state.analysis.domain.bookmarks.insert(addr, label);
                    self.state
                        .log(format!("[*] Bookmark added at 0x{:08X}", addr));
                }
            }
        }

        if let Some(addr) = self.state.ui.pending_jump.take() {
            analysis_ops::navigate_to_address(
                &mut self.state,
                addr,
                &self.decomp_request_tx,
                &self.latest_request_id,
            );
        }
    }

    /// Render Go to Address (G) dialog
    fn render_goto_address_dialog(&mut self, ctx: &egui::Context) {
        let mut jump_target = None;
        let mut close_dialog = false;

        if let Some(ref mut input) = self.state.viewmodels.navigation.goto_address_input {
            egui::Window::new("🚀 Go to Address")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -100.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Target:");
                        let text_edit = egui::TextEdit::singleline(input)
                            .hint_text("0x1234, 4660, function_name")
                            .desired_width(200.0);

                        let res = ui.add(text_edit);
                        res.request_focus();

                        if ui.button("Go").clicked()
                            || (res.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            jump_target = Some(input.clone());
                        }
                    });

                    ui.label(
                        egui::RichText::new("Enter hex (0x...), decimal, or function name")
                            .small()
                            .color(super::theme::catppuccin::OVERLAY0),
                    );

                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        close_dialog = true;
                    }
                });
        }

        if close_dialog {
            self.state.viewmodels.navigation.goto_address_input = None;
        }

        if let Some(target) = jump_target {
            self.state.viewmodels.navigation.goto_address_input = None;

            // Try to parse as hex
            let addr = if target.starts_with("0x") {
                u64::from_str_radix(&target[2..], 16).ok()
            } else if let Ok(val) = u64::from_str_radix(&target, 16) {
                Some(val)
            } else {
                target.parse::<u64>().ok()
            };

            if let Some(a) = addr {
                analysis_ops::navigate_to_address(
                    &mut self.state,
                    a,
                    &self.decomp_request_tx,
                    &self.latest_request_id,
                );
            } else {
                // Try to find function by name
                let functions = self
                    .state
                    .analysis
                    .loaded_binary()
                    .as_ref()
                    .map(|b| b.functions.clone())
                    .unwrap_or_default();

                // Search user names first, then original names
                let user_match = self
                    .state
                    .analysis
                    .domain
                    .user_function_names
                    .iter()
                    .find(|(_, name)| **name == target)
                    .map(|(addr, _)| *addr);

                if let Some(a) = user_match {
                    analysis_ops::navigate_to_address(
                        &mut self.state,
                        a,
                        &self.decomp_request_tx,
                        &self.latest_request_id,
                    );
                } else if let Some(f) = functions.iter().find(|f| f.name == target) {
                    analysis_ops::navigate_to_address(
                        &mut self.state,
                        f.address,
                        &self.decomp_request_tx,
                        &self.latest_request_id,
                    );
                } else {
                    self.state
                        .log(format!("[!] Invalid jump target: {}", target));
                }
            }
        }
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn format_hexdump(addr: u64, data: &[u8]) -> String {
    let mut output = String::new();
    for chunk in data.chunks(16) {
        output.push_str(&format!(
            "{:016X}: ",
            addr + (output.len() as u64 / 75 * 16)
        ));
        for b in chunk {
            output.push_str(&format!("{:02X} ", b));
        }
        if chunk.len() < 16 {
            for _ in 0..(16 - chunk.len()) {
                output.push_str("   ");
            }
        }
        output.push_str(" | ");
        for b in chunk {
            output.push(if *b >= 0x20 && *b <= 0x7E {
                *b as char
            } else {
                '.'
            });
        }
        output.push('\n');
    }
    output
}
