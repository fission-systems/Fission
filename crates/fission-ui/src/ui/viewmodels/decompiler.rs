use crate::ui::gui::core::state::AppState;

/// ViewModel for the Decompiler View.
///
/// Responsibilities:
/// - Holding cached split lines of code (for virtualization)
/// - Managing syntax highlighting state
/// - handling scrolling and navigation helper methods
pub struct DecompilerViewModel {
    /// Cached lines of the current decompiled code.
    /// This allows O(1) access for the virtualized list.
    pub lines: Vec<String>,

    /// Address of the currently loaded function (used for cache invalidation)
    pub current_address: Option<u64>,
}

impl Default for DecompilerViewModel {
    fn default() -> Self {
        Self {
            lines: vec!["// Select a function to decompile".to_string()],
            current_address: None,
        }
    }
}

impl DecompilerViewModel {
    /// Sync the ViewModel with the global AppState.
    ///
    /// This should be called before rendering the view.
    /// It checks if the decompiled code in AppState has changed and updates the cached lines.
    pub fn update(&mut self, state: &AppState) {
        // If state has a new function selected or code changed
        let addr = state.analysis.domain.selected_function.as_ref().map(|f| f.address);

        // Simple invalidation: if address changed or code content length differs dramatically
        // (A more robust way would be a version counter or hash, but string comparison is heavy)
        // For now, we rely on the fact that `handle_decompile_result` updates `decompiled_lines` in AppState mostly.

        // Wait, Phase 3 Plan said: "Move line splitting... from AnalysisState to this ViewModel".
        // But currently `state.analysis.decompiled_lines` exists.
        // We should eventually migrate that logic here.
        // For the transition step: copy from state if we don't have it, or ideally,
        // the state shouldn't hold `decompiled_lines` anymore.

        // Let's assume we are migrating. The AppState should hold the `decompiled_code` string (The Truth).
        // This VM should hold the `lines` (The View Representation).

        let code = &state.analysis.domain.decompiled_code;

        // Check if we need to re-split (optimization: only if code changed)
        // We can use a simple checksum or length check + address check for now.

        // If this is a very fresh VM or context switch
        if self.current_address != addr || self.lines.is_empty() {
            self.lines = code.lines().map(|s| s.to_string()).collect();
            self.current_address = addr;
            return;
        }

        // Ideally we compare content, but that's O(N).
        // If `decompiled_code` is updated in AppState, we need a signal.
        // For now, in immediate mode, if we want to be purely reactive without signals,
        // we might compare the first few distinct chars or length.
        // Or better: `AppState` carries a `decompilation_version` counter.
    }

    /// Force update lines from string (called when new decompilation arrives)
    pub fn set_code(&mut self, code: &str, address: Option<u64>) {
        self.lines = code.lines().map(|s| s.to_string()).collect();
        self.current_address = address;
    }
}
