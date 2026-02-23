//! Fission Tauri — Application state managed by Tauri.

use crate::dto::BookmarkDto;
use crate::dto::DebugStateDto;
use fission_analysis::debug::ttd::Timeline;
use fission_analysis::plugin::PluginManager;
use fission_loader::loader::LoadedBinary;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(feature = "native_decomp")]
use fission_analysis::analysis::decomp::CachingDecompiler;

#[cfg(target_os = "windows")]
use fission_analysis::debug::{types::DebugEvent, windows::WindowsDebugger};
#[cfg(target_os = "windows")]
use crossbeam_channel::{Receiver, Sender};

/// Inner mutable state behind a Mutex.
/// Does NOT contain the decompiler — that lives in its own Mutex to
/// prevent long-running decompilations from blocking all other commands.
pub struct InnerState {
    /// Currently loaded binary
    pub loaded_binary: Option<Arc<LoadedBinary>>,

    /// Whether the decompiler context has been loaded with a binary
    pub decompiler_loaded: bool,

    /// User comments keyed by address
    pub comments: HashMap<u64, String>,

    /// User-defined function renames keyed by address
    pub renamed_functions: HashMap<u64, String>,

    /// User bookmarks
    pub bookmarks: Vec<BookmarkDto>,
}

impl Default for InnerState {
    fn default() -> Self {
        Self {
            loaded_binary: None,
            decompiler_loaded: false,
            comments: HashMap::new(),
            renamed_functions: HashMap::new(),
            bookmarks: Vec::new(),
        }
    }
}

/// Thread-safe application state wrapper.
///
/// The decompiler has its own separate Mutex so that a slow/hanging
/// decompile call does not block assembly, hex, search, etc.
pub struct AppState {
    pub inner: Mutex<InnerState>,

    /// Native decompiler — separate lock so it never blocks other commands
    #[cfg(feature = "native_decomp")]
    pub decompiler: Mutex<Option<CachingDecompiler>>,

    /// Debugger session state — separate lock to avoid blocking other commands
    pub debug_state: Mutex<DebugStateDto>,

    /// Active Windows debugger instance (None when not attached).
    /// Uses tokio::sync::Mutex so async commands can await without blocking the executor.
    #[cfg(target_os = "windows")]
    pub debugger: Mutex<Option<WindowsDebugger>>,

    /// Receives OS debug events produced by `start_event_loop`.
    /// Uses std::sync::Mutex because try_recv() is non-blocking and lock is held briefly.
    #[cfg(target_os = "windows")]
    pub debug_event_rx: std::sync::Mutex<Option<Receiver<DebugEvent>>>,

    /// Send `()` on this channel to stop the background event-loop thread.
    #[cfg(target_os = "windows")]
    pub debug_stop_tx: std::sync::Mutex<Option<Sender<()>>>,

    /// Plugin manager — separate lock, always available
    pub plugin_manager: Mutex<PluginManager>,

    /// TTD (Time Travel Debugging) timeline — separate lock
    pub timeline: Mutex<Timeline>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(InnerState::default()),
            #[cfg(feature = "native_decomp")]
            decompiler: Mutex::new(None),
            debug_state: Mutex::new(DebugStateDto::default()),
            #[cfg(target_os = "windows")]
            debugger: Mutex::new(None),
            #[cfg(target_os = "windows")]
            debug_event_rx: std::sync::Mutex::new(None),
            #[cfg(target_os = "windows")]
            debug_stop_tx: std::sync::Mutex::new(None),
            plugin_manager: Mutex::new(PluginManager::new()),
            timeline: Mutex::new(Timeline::new()),
        }
    }
}
