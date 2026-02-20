//! Fission Tauri — Application state managed by Tauri.

use fission_loader::loader::LoadedBinary;
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(feature = "native_decomp")]
use fission_analysis::analysis::decomp::CachingDecompiler;

/// Inner mutable state behind a Mutex.
pub struct InnerState {
    /// Currently loaded binary
    pub loaded_binary: Option<Arc<LoadedBinary>>,

    /// Native decompiler instance (persistent context)
    #[cfg(feature = "native_decomp")]
    pub decompiler: Option<CachingDecompiler>,

    /// Whether the decompiler context has been loaded with a binary
    pub decompiler_loaded: bool,

    /// Log messages
    pub logs: Vec<String>,
}

impl Default for InnerState {
    fn default() -> Self {
        Self {
            loaded_binary: None,
            #[cfg(feature = "native_decomp")]
            decompiler: None,
            decompiler_loaded: false,
            logs: vec!["[Fission] Tauri backend initialized.".to_string()],
        }
    }
}

/// Thread-safe application state wrapper.
pub struct AppState {
    pub inner: Mutex<InnerState>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(InnerState::default()),
        }
    }
}
