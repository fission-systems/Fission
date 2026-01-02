use super::decomp_worker::DecompileRequest;
use crate::analysis::loader::FunctionInfo;
use crate::config::CONFIG;
use crate::ui::gui::state::{AppState, EditorTab};
use crossbeam_channel::Sender;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

pub fn analyze_functions(state: &mut AppState) {
    // Clone the Arc first to avoid borrow checker issues
    let binary_opt = state.analysis.loaded_binary.clone();

    if let Some(binary_arc) = binary_opt {
        state.log("[*] Analyzing binary for internal functions...");

        // Clone the inner LoadedBinary to get a mutable copy
        let mut binary = (*binary_arc).clone();
        let before_count = binary.functions.len();

        // Discover internal functions
        binary.discover_internal_functions();

        let after_count = binary.functions.len();
        let discovered = after_count - before_count;

        // Replace with new Arc
        state.analysis.loaded_binary = Some(std::sync::Arc::new(binary));

        state.log(format!(
            "[✓] Found {} new internal functions ({} total)",
            discovered, after_count
        ));
    } else {
        state.log("[!] No binary loaded");
    }
}

pub fn navigate_to_address(
    state: &mut AppState,
    addr: u64,
    decomp_tx: &Sender<DecompileRequest>,
    req_id: &Arc<AtomicU64>,
) {
    // Clone the functions list to avoid borrow issues
    let functions: Vec<FunctionInfo> = state
        .analysis
        .loaded_binary
        .as_ref()
        .map(|b| b.functions.clone())
        .unwrap_or_default();

    // Find function containing or starting at this address
    for func in &functions {
        // Check if address is within function range (configurable)
        let range = CONFIG.analysis.function_address_range as u64;
        if addr >= func.address && addr < func.address + range {
            state.log(format!(
                "[*] Navigating to function: {} at 0x{:08X}",
                func.name, func.address
            ));
            state.analysis.selected_function = Some(func.clone());
            state.ui.selected_xref_addr = Some(func.address);

            open_function_tabs(state, func, decomp_tx, req_id);
            return;
        }
    }

    // If no function found, just log
    if !functions.is_empty() {
        state.log(format!("[!] No function found at address 0x{:08X}", addr));
    }
}

pub fn open_function_tabs(
    state: &mut AppState,
    func: &FunctionInfo,
    decomp_tx: &Sender<DecompileRequest>,
    req_id: &Arc<AtomicU64>,
) {
    let asm_tab = EditorTab::Assembly(func.name.clone());
    let decomp_tab = EditorTab::Decompiled(func.name.clone());

    // Open Assembly tab if not open
    if !state.ui.open_tabs.contains(&asm_tab) {
        state.ui.open_tabs.push(asm_tab.clone());
    }

    // Open Decompiled tab if not open
    if !state.ui.open_tabs.contains(&decomp_tab) {
        state.ui.open_tabs.push(decomp_tab.clone());
    }

    // Focus Decompiled tab by default
    if let Some(pos) = state.ui.open_tabs.iter().position(|t| t == &decomp_tab) {
        state.ui.active_tab_index = Some(pos);
    }

    state.analysis.selected_function = Some(func.clone());
    super::decompiler::decompile_function(state, decomp_tx, req_id, func);
}
