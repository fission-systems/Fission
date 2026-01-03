//! Plugin Manager Core

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::types::{HookCallback, LoadedPlugin};
use super::super::api::PluginAPI;
use super::super::hooks::PluginHook;
use crate::core::events::EventBus;

/// Plugin Manager - Central plugin registry and event dispatcher
pub struct PluginManager {
    /// Loaded plugins by ID
    pub(super) plugins: HashMap<String, LoadedPlugin>,
    /// All registered hooks
    pub(super) hooks: HashMap<u64, (PluginHook, HookCallback)>,
    /// Next hook ID
    pub(super) next_hook_id: u64,
    /// Plugin search paths
    pub(super) search_paths: Vec<PathBuf>,
    /// Shared API instance
    pub(super) api: Option<Arc<dyn PluginAPI>>,
    /// System-wide Event Bus
    pub(super) event_bus: Option<Arc<EventBus>>,
    /// Python runtime (if enabled)
    #[cfg(feature = "python")]
    pub(super) python_runtime: super::super::python::PythonRuntime,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            hooks: HashMap::new(),
            next_hook_id: 1,
            search_paths: vec![
                PathBuf::from("plugins"),
                PathBuf::from("~/.fission/plugins"),
            ],
            api: None,
            event_bus: None,
            #[cfg(feature = "python")]
            python_runtime: super::super::python::PythonRuntime::default(),
        }
    }

    /// Set the API instance for plugins to use
    pub fn set_api(&mut self, api: Arc<dyn PluginAPI>) {
        self.api = Some(api);
    }

    /// Set the Event Bus for plugins to use
    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Add a plugin search path
    pub fn add_search_path<P: AsRef<Path>>(&mut self, path: P) {
        self.search_paths.push(path.as_ref().to_path_buf());
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
