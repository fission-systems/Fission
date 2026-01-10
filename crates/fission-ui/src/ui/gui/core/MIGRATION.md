//! Migration Guide for UI State Refactoring
//!
//! This guide helps migrate from the old state structure to the new domain/viewmodel separation.
//!
//! ## Old Structure (Before)
//!
//! ```rust,ignore
//! // UI inputs mixed with domain data
//! state.analysis.patch_offset_input  // UI input
//! state.analysis.loaded_binary       // Domain data
//! state.debug.breakpoint_input       // UI input
//! state.debug.debug_state            // Domain data
//! ```
//!
//! ## New Structure (After)
//!
//! ```rust,ignore
//! // Pure domain data
//! state.analysis.domain.loaded_binary
//! state.debug.domain.debug_state
//!
//! // UI inputs in ViewModels
//! state.viewmodels.hex.patch_offset_input
//! state.viewmodels.debug.breakpoint_input
//! state.viewmodels.strings.filter
//! state.viewmodels.functions.rename_dialog
//! ```
//!
//! ## Migration Path
//!
//! ### 1. Hex Panel
//! - OLD: `state.analysis.patch_offset_input`
//! - NEW: `state.viewmodels.hex.patch_offset_input`
//! - OLD: `state.analysis.patch_bytes_input`
//! - NEW: `state.viewmodels.hex.patch_bytes_input`
//! - OLD: `state.analysis.hex_offset`
//! - NEW: `state.viewmodels.hex.current_offset`
//!
//! ### 2. Strings Panel
//! - OLD: `state.analysis.strings_filter`
//! - NEW: `state.viewmodels.strings.filter`
//! - Domain data: `state.analysis.domain.extracted_strings`
//!
//! ### 3. Debug Panel
//! - OLD: `state.debug.breakpoint_input`
//! - NEW: `state.viewmodels.debug.breakpoint_input`
//! - OLD: `state.debug.mem_addr_input`
//! - NEW: `state.viewmodels.debug.mem_addr_input`
//! - OLD: `state.debug.mem_len_input`
//! - NEW: `state.viewmodels.debug.mem_len_input`
//! - OLD: `state.debug.process_filter`
//! - NEW: `state.viewmodels.debug.process_filter`
//! - Domain data: `state.debug.domain.debug_state`
//!
//! ### 4. Functions Panel
//! - OLD: `state.analysis.rename_dialog`
//! - NEW: `state.viewmodels.functions.rename_dialog`
//! - Domain data: `state.analysis.domain.selected_function`
//!
//! ### 5. String Xrefs Panel
//! - OLD: `state.analysis.string_xref_search`
//! - NEW: `state.viewmodels.string_xrefs.search_term`
//! - OLD: `state.analysis.string_xref_min_len`
//! - NEW: `state.viewmodels.string_xrefs.min_length`
//! - Domain data: `state.analysis.domain.string_xref_results`
//!
//! ## Benefits
//!
//! 1. **Testability**: Domain logic can be tested without UI state
//! 2. **Lock Granularity**: UI inputs don't require global state lock
//! 3. **Clarity**: Clear separation between business logic and presentation
//! 4. **Performance**: Reduced lock contention when updating UI fields
//!
//! ## Example Refactoring
//!
//! ### Before:
//! ```rust,ignore
//! egui::TextEdit::singleline(&mut state.analysis.patch_offset_input)
//!     .hint_text("0x1000")
//!     .show(ui);
//! ```
//!
//! ### After:
//! ```rust,ignore
//! egui::TextEdit::singleline(&mut state.viewmodels.hex.patch_offset_input)
//!     .hint_text("0x1000")
//!     .show(ui);
//! ```

// This file is documentation only - no code
