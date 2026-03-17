//! Fission Tauri — Application state managed by Tauri.

use crate::dto::BookmarkDto;
use crate::dto::DebugStateDto;
use crate::menu::MenuHandles;
use crate::services::cross_image::{AutoRenameKind, PropagationReason};
use fission_dynamic::debug::ttd::Timeline;
use fission_dynamic::plugin::PluginManager;
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::{FactProvenance, FactStore};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(feature = "native_decomp")]
use fission_static::analysis::decomp::CachingDecompiler;

#[cfg(target_os = "windows")]
use crossbeam_channel::{Receiver, Sender};
#[cfg(target_os = "windows")]
use fission_dynamic::debug::{types::DebugEvent, windows::WindowsDebugger};

/// Inner mutable state behind a Mutex.
/// Does NOT contain the decompiler — that lives in its own Mutex to
/// prevent long-running decompilations from blocking all other commands.
#[derive(Default)]
pub struct InnerState {
    /// Currently loaded binary
    pub loaded_binary: Option<Arc<LoadedBinary>>,

    /// Whether the decompiler context has been loaded with a binary
    pub decompiler_loaded: bool,

    /// User comments keyed by address
    pub comments: HashMap<u64, String>,

    /// User-defined function renames keyed by address
    pub renamed_functions: HashMap<u64, String>,

    /// Addresses explicitly renamed by the user or restored from a project.
    pub manual_renamed_functions: HashSet<u64>,

    /// In-memory provenance for auto-propagated names.
    pub auto_renamed_functions: HashMap<u64, AutoRenameKind>,

    /// Session-scoped source of truth for symbol/type/name facts.
    pub fact_store: Option<FactStore>,

    /// User bookmarks
    pub bookmarks: Vec<BookmarkDto>,
}

fn fact_provenance_from_auto(reason: AutoRenameKind) -> FactProvenance {
    match reason {
        AutoRenameKind::StrongFid => FactProvenance::StrongFid,
        AutoRenameKind::CrossImage(PropagationReason::ImportExport) => {
            FactProvenance::CrossImageImportExport
        }
        AutoRenameKind::CrossImage(PropagationReason::Thunk) => FactProvenance::CrossImageThunk,
    }
}

impl InnerState {
    pub fn rebuild_fact_store(&mut self) {
        let Some(binary) = self.loaded_binary.as_ref() else {
            self.fact_store = None;
            return;
        };

        let mut store = FactStore::from_binary(binary);
        for (addr, name) in &self.renamed_functions {
            if self.manual_renamed_functions.contains(addr) {
                store.ingest_name_fact(*addr, name.clone(), FactProvenance::UserRename);
            } else if let Some(reason) = self.auto_renamed_functions.get(addr).copied() {
                store.ingest_name_fact(*addr, name.clone(), fact_provenance_from_auto(reason));
            }
        }
        self.fact_store = Some(store);
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

    /// Native menu item handles for dynamic enable/disable
    pub menu_handles: std::sync::OnceLock<MenuHandles>,
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
            menu_handles: std::sync::OnceLock::new(),
        }
    }
}
