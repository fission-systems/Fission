//! Async message types for GUI communication.
//!
//! These messages are sent from background threads to the main UI thread.

use crate::debug::types::DebugEvent;
use fission_analysis::analysis::cfg::CfgSummary;
use fission_loader::loader::LoadedBinary;
use std::sync::Arc;

/// Message types for async operations between threads and UI
pub enum AsyncMessage {
    /// Binary file was loaded (success or failure)
    BinaryLoaded(Result<Arc<LoadedBinary>, String>),

    /// Decompilation completed successfully
    DecompileResult { address: u64, c_code: String },

    /// Decompilation failed
    DecompileError { address: u64, error: String },

    /// File was selected from dialog (None if cancelled)
    FileSelected(Option<String>),

    /// Save snapshot to path
    SaveSnapshot(String),
    /// Load snapshot from path
    LoadSnapshot(String),
    /// Save project to path
    SaveProject(String),
    /// Load project from path
    LoadProject(String),

    /// Debug event from debugger loop
    DebugEvent(DebugEvent),

    /// System-wide event from EventBus
    Event(crate::app::events::FissionEvent),

    /// Folder was selected from dialog (None if cancelled)
    FolderSelected(Option<String>),

    /// Project loaded from folder (path, binaries)
    ProjectLoaded {
        path: String,
        binaries: Vec<Arc<LoadedBinary>>,
    },

    /// Decompiler context initialization completed
    DecompilerContextLoaded,

    /// CFG analysis request
    CfgAnalysisRequest { address: u64 },

    /// CFG analysis completed successfully
    /// CFG analysis completed successfully
    CfgAnalysisResult(CfgSummary),

    /// CFG analysis failed
    CfgAnalysisError { address: u64, error: String },

    /// Decompiler context initialization failed (FFI error, SLA not found, etc.)
    DecompilerContextError {
        error: String,
        /// Suggested fix (e.g., "Set FISSION_SLA_DIR environment variable")
        suggestion: Option<String>,
    },

    /// Worker thread health check (sent periodically)
    WorkerHeartbeat { worker_id: usize, is_alive: bool },
}
