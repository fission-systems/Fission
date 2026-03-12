//! Plugin Manager Core

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fission_core::{APP_DIR_NAME, PLUGIN_DIR_NAME};

use super::super::api::PluginAPI;
use super::super::hooks::PluginHook;
use super::types::{HookCallback, LoadedPlugin};
use crate::app::events::EventBus;

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
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new() -> Self {
        // Build search paths: relative "plugins/" dir, then user's home config dir
        let mut search_paths = vec![PathBuf::from(PLUGIN_DIR_NAME)];
        if let Some(home) = dirs::home_dir() {
            // ~/.fission/plugins (properly expanded – PathBuf does NOT auto-expand ~)
            search_paths.push(
                home.join(format!(".{}", APP_DIR_NAME))
                    .join(PLUGIN_DIR_NAME),
            );
        }

        Self {
            plugins: HashMap::new(),
            hooks: HashMap::new(),
            next_hook_id: 1,
            search_paths,
            api: None,
            event_bus: None,
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
