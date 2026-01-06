//! Application state and logic

use crate::analysis::loader::{FunctionInfo, LoadedBinary};
use ratatui::widgets::ListState;

#[cfg(feature = "native_decomp")]
use crate::analysis::decomp::ffi::DecompilerNative;

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
            status: "Ready. ↑/↓:Navigate  Enter:Decompile  q:Quit".to_string(),
            scroll: 0,
            should_quit: false,
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
}
