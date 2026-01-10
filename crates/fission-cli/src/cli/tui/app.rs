//! Application state and logic

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use ratatui::widgets::ListState;

#[cfg(feature = "native_decomp")]
use fission_ffi::DecompilerNative;

#[cfg(feature = "native_decomp")]
use crate::analysis::cfg::{CfgAnalysis, CfgVisualizer, DotOptions};
#[cfg(feature = "native_decomp")]
use crate::analysis::pcode::PcodeFunction;

/// TUI Application state
pub struct App {
    /// Loaded binary
    binary: LoadedBinary,
    /// Binary data for decompiler
    binary_data: Vec<u8>,
    /// List of non-import functions
    functions: Vec<FunctionInfo>,
    /// Selected function index
    list_state: ListState,
    /// Decompiled code for selected function
    decompiled_code: String,
    /// Decompiler instance
    #[cfg(feature = "native_decomp")]
    decompiler: Option<DecompilerNative>,
    /// Status message
    status: String,
    /// Scroll position for code view
    scroll: u16,
    /// Should quit flag
    should_quit: bool,
    /// CFG analysis summary for selected function
    cfg_summary: Option<String>,
}

impl App {
    pub fn new(binary: LoadedBinary, binary_data: Vec<u8>) -> Self {
        let functions: Vec<FunctionInfo> = binary
            .functions
            .iter()
            .filter(|f| !f.is_import)
            .cloned()
            .collect();

        let mut list_state = ListState::default();
        if !functions.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            binary,
            binary_data,
            functions,
            list_state,
            decompiled_code: "// Select a function and press Enter to decompile".to_string(),
            #[cfg(feature = "native_decomp")]
            decompiler: None,
            status: "Ready. ↑/↓:Navigate  Enter:Decompile  c:CFG  q:Quit".to_string(),
            scroll: 0,
            should_quit: false,
            cfg_summary: None,
        }
    }

    // Getters
    pub fn binary(&self) -> &LoadedBinary {
        &self.binary
    }

    pub fn functions(&self) -> &[FunctionInfo] {
        &self.functions
    }

    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    pub fn list_state_mut(&mut self) -> &mut ListState {
        &mut self.list_state
    }

    pub fn decompiled_code(&self) -> &str {
        &self.decompiled_code
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn scroll(&self) -> u16 {
        self.scroll
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn selected_function(&self) -> Option<&FunctionInfo> {
        self.list_state
            .selected()
            .and_then(|i| self.functions.get(i))
    }

    // Navigation
    pub fn select_next(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected < self.functions.len() - 1 {
                self.list_state.select(Some(selected + 1));
            }
        }
    }

    pub fn select_previous(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if selected > 0 {
                self.list_state.select(Some(selected - 1));
            }
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn page_down(&mut self) {
        self.scroll = self.scroll.saturating_add(10);
    }

    pub fn page_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(10);
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    // Decompilation
    #[cfg(feature = "native_decomp")]
    pub fn decompile_selected(&mut self) {
        let func = match self.selected_function() {
            Some(f) => f.clone(),
            None => return,
        };

        self.status = format!("Decompiling {} @ 0x{:x}...", func.name, func.address);

        // Initialize decompiler if needed
        if self.decompiler.is_none() {
            let sla_dir = std::env::current_dir()
                .unwrap()
                .join("ghidra_decompiler")
                .to_string_lossy()
                .into_owned();

            match DecompilerNative::new(&sla_dir) {
                Ok(mut decomp) => {
                    // Load binary
                    if let Err(e) = decomp.load_binary(
                        &self.binary_data,
                        self.binary.image_base,
                        self.binary.is_64bit,
                    ) {
                        self.decompiled_code = format!("// Error loading binary: {}", e);
                        self.status = "Error loading binary".to_string();
                        return;
                    }
                    decomp.add_symbols(&self.binary.iat_symbols);
                    decomp.add_global_symbols(&self.binary.global_symbols);
                    decomp.set_symbol_provider(
                        &self.binary.functions,
                        &self.binary.global_symbols,
                        &self.binary.sections,
                    );
                    self.decompiler = Some(decomp);
                }
                Err(e) => {
                    self.decompiled_code = format!("// Error creating decompiler: {}", e);
                    self.status = "Error creating decompiler".to_string();
                    return;
                }
            }
        }

        // Decompile
        if let Some(ref decomp) = self.decompiler {
            match decomp.decompile(func.address) {
                Ok(code) => {
                    self.decompiled_code = code;
                    self.scroll = 0;
                    self.status = format!(
                        "Decompiled {} ({} bytes)",
                        func.name,
                        self.decompiled_code.len()
                    );
                }
                Err(e) => {
                    self.decompiled_code = format!("// Error: {}", e);
                    self.status = format!("Error decompiling: {}", e);
                }
            }
        }
    }

    #[cfg(not(feature = "native_decomp"))]
    pub fn decompile_selected(&mut self) {
        self.decompiled_code =
            "// Decompilation requires native_decomp feature\n// Run with: cargo run --bin fission_tui --features \"tui,native_decomp\"".to_string();
        self.status = "native_decomp feature required".to_string();
    }

    // CFG Getters
    pub fn cfg_summary(&self) -> Option<&str> {
        self.cfg_summary.as_deref()
    }

    // CFG Analysis
    #[cfg(feature = "native_decomp")]
    pub fn analyze_cfg_selected(&mut self) {
        let func = match self.selected_function() {
            Some(f) => f.clone(),
            None => return,
        };

        self.status = format!("Analyzing CFG for {} @ 0x{:x}...", func.name, func.address);

        // Initialize decompiler if needed
        if self.decompiler.is_none() {
            let sla_dir = std::env::current_dir()
                .unwrap()
                .join("ghidra_decompiler")
                .to_string_lossy()
                .into_owned();

            match DecompilerNative::new(&sla_dir) {
                Ok(mut decomp) => {
                    if let Err(e) = decomp.load_binary(
                        &self.binary_data,
                        self.binary.image_base,
                        self.binary.is_64bit,
                    ) {
                        self.cfg_summary = Some(format!("Error loading binary: {}", e));
                        self.status = "Error loading binary".to_string();
                        return;
                    }
                    decomp.add_symbols(&self.binary.iat_symbols);
                    decomp.add_global_symbols(&self.binary.global_symbols);
                    decomp.set_symbol_provider(
                        &self.binary.functions,
                        &self.binary.global_symbols,
                        &self.binary.sections,
                    );
                    self.decompiler = Some(decomp);
                }
                Err(e) => {
                    self.cfg_summary = Some(format!("Error creating decompiler: {}", e));
                    self.status = "Error creating decompiler".to_string();
                    return;
                }
            }
        }

        // Get Pcode and analyze CFG
        if let Some(ref decomp) = self.decompiler {
            match decomp.get_pcode(func.address) {
                Ok(pcode_json) => {
                    match PcodeFunction::from_json(&pcode_json) {
                        Ok(pcode_func) => {
                            match CfgAnalysis::from_pcode(&pcode_func) {
                                Ok(analysis) => {
                                    // Build summary string
                                    let mut summary = String::new();
                                    summary.push_str(&format!(
                                        "=== CFG Analysis: {} ===\n\n",
                                        func.name
                                    ));
                                    summary.push_str(&format!(
                                        "Blocks: {}\n",
                                        analysis.cfg.block_count()
                                    ));
                                    summary.push_str(&format!(
                                        "Edges: {}\n",
                                        analysis.cfg.edge_count()
                                    ));
                                    summary.push_str(&format!(
                                        "Cyclomatic Complexity: {}\n",
                                        analysis.metrics.cyclomatic_complexity
                                    ));
                                    summary.push_str(&format!(
                                        "Max Nesting Depth: {}\n",
                                        analysis.metrics.max_nesting_depth
                                    ));
                                    summary
                                        .push_str(&format!("Loops: {}\n\n", analysis.loops.len()));

                                    if !analysis.loops.is_empty() {
                                        summary.push_str("Loop Details:\n");
                                        for (i, l) in analysis.loops.iter().enumerate() {
                                            summary.push_str(&format!(
                                                "  Loop {}: Header=BB{}, Kind={:?}, Body={:?}\n",
                                                i,
                                                l.header,
                                                l.kind,
                                                l.body.iter().collect::<Vec<_>>()
                                            ));
                                        }
                                        summary.push_str("\n");
                                    }

                                    summary.push_str("Blocks:\n");
                                    for block in &analysis.cfg.blocks {
                                        let marker = if block.is_entry {
                                            " [ENTRY]"
                                        } else if block.is_exit {
                                            " [EXIT]"
                                        } else {
                                            ""
                                        };
                                        summary.push_str(&format!(
                                            "  BB{} @ 0x{:x}{}\n",
                                            block.index, block.start_address, marker
                                        ));
                                    }

                                    self.cfg_summary = Some(summary);
                                    self.scroll = 0;
                                    self.status = format!(
                                        "CFG: {} blocks, {} edges, complexity: {}",
                                        analysis.cfg.block_count(),
                                        analysis.cfg.edge_count(),
                                        analysis.metrics.cyclomatic_complexity
                                    );
                                }
                                Err(e) => {
                                    self.cfg_summary = Some(format!("CFG analysis error: {}", e));
                                    self.status = format!("CFG error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            self.cfg_summary = Some(format!("Pcode parse error: {}", e));
                            self.status = format!("Pcode error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    self.cfg_summary = Some(format!("Error getting Pcode: {}", e));
                    self.status = format!("Pcode error: {}", e);
                }
            }
        }
    }

    #[cfg(not(feature = "native_decomp"))]
    pub fn analyze_cfg_selected(&mut self) {
        self.cfg_summary = Some(
            "CFG analysis requires native_decomp feature\nRun with: cargo run --bin fission_tui --features \"tui,native_decomp\"".to_string()
        );
        self.status = "native_decomp feature required".to_string();
    }
}
