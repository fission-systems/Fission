use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fission_core::{APP_DIR_NAME, PLUGIN_DIR_NAME};

use super::super::api::PluginAPI;
use super::super::hooks::PluginHook;
use super::types::{HookCallback, LoadedPlugin};
use crate::events::EventBus;

pub struct PluginManager {
    pub(super) hooks: HashMap<u64, (PluginHook, HookCallback)>,
    pub(super) next_hook_id: u64,
    pub(super) search_paths: Vec<PathBuf>,
    pub(super) api: Option<Arc<dyn PluginAPI>>,
    pub(super) event_bus: Option<Arc<EventBus>>,
    /// Declared last on purpose. Struct fields drop in declaration order, and
    /// a plugin's hook closures and event subscriptions run (and are dropped
    /// by) code that lives in its dynamic library. `LoadedPlugin` unmaps the
    /// library when it drops, so every field holding such a callback must be
    /// declared before this one.
    pub(super) plugins: HashMap<String, LoadedPlugin>,
}

impl PluginManager {
    pub fn new() -> Self {
        let mut search_paths = vec![PathBuf::from(PLUGIN_DIR_NAME)];
        if let Some(home) = dirs::home_dir() {
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

    pub fn set_api(&mut self, api: Arc<dyn PluginAPI>) {
        self.api = Some(api.clone());

        // Registration only calls `on_load` once there is an API to hand the
        // plugin, so a plugin registered earlier has not been loaded yet. Do it
        // now, once. One whose `on_load` fails ends as registration would have
        // ended it -- not running -- so it is disabled rather than left active.
        let extension = self
            .event_bus
            .clone()
            .map(|e| e as std::sync::Arc<dyn std::any::Any + Send + Sync>);
        let ctx = crate::contracts::traits::PluginContext::new(api, extension);
        for plugin in self.plugins.values_mut() {
            if plugin.on_load_called {
                continue;
            }
            let Some(instance) = plugin.instance.as_mut() else {
                continue;
            };
            plugin.on_load_called = true;
            if instance.on_load(&ctx).is_err() {
                plugin.info.enabled = false;
            }
        }
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub fn add_search_path<P: AsRef<Path>>(&mut self, path: P) {
        self.search_paths.push(path.as_ref().to_path_buf());
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
