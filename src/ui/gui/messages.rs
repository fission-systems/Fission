//! Async message types for GUI communication.
//!
//! These messages are sent from background threads to the main UI thread.

use std::sync::Arc;
use crate::analysis::loader::LoadedBinary;
use crate::debug::types::DebugEvent;

/// Message types for async operations between threads and UI
pub enum AsyncMessage {
    /// Binary file was loaded (success or failure)
    BinaryLoaded(Result<Arc<LoadedBinary>, String>),
    
    /// Decompilation completed successfully
    DecompileResult { 
        address: u64, 
        c_code: String,
    },
    
    /// Decompilation failed
    DecompileError { 
        address: u64, 
        error: String,
    },
    
    /// File was selected from dialog (None if cancelled)
    FileSelected(Option<String>),
    
    /// Debug event from debugger loop
    DebugEvent(DebugEvent),
}
