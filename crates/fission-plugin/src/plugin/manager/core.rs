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

    pub(super) fn context_for_plugin(
        &self,
        api: &Arc<dyn PluginAPI>,
        plugin_id: &str,
    ) -> crate::contracts::traits::PluginContext {
        let extension = self
            .event_bus
            .as_ref()
            .map(|bus| Arc::new(bus.scoped(plugin_id)) as Arc<dyn std::any::Any + Send + Sync>);
        crate::contracts::traits::PluginContext::new(api.clone(), extension)
    }

    pub fn set_api(&mut self, api: Arc<dyn PluginAPI>) {
        self.api = Some(api.clone());

        // Registration only calls `on_load` once there is an API to hand the
        // plugin, so a plugin registered earlier has not been loaded yet. Do it
        // now, once. One whose `on_load` fails ends disabled and marked failed
        // without marking `on_load_called = true`.
        let plugin_ids: Vec<String> = self.plugins.keys().cloned().collect();
        for plugin_id in plugin_ids {
            let should_load = self
                .plugins
                .get(&plugin_id)
                .is_some_and(|p| !p.on_load_called && !p.on_load_failed && p.instance.is_some());
            if !should_load {
                continue;
            }
            let ctx = self.context_for_plugin(&api, &plugin_id);
            let load_result = self
                .plugins
                .get_mut(&plugin_id)
                .and_then(|p| p.instance.as_mut())
                .map(|instance| instance.on_load(&ctx));
            match load_result {
                Some(Ok(())) => {
                    if let Some(plugin) = self.plugins.get_mut(&plugin_id) {
                        plugin.on_load_called = true;
                        plugin.on_load_failed = false;
                    }
                }
                Some(Err(_)) => {
                    if let Some(bus) = &self.event_bus {
                        bus.unsubscribe_owner(&plugin_id);
                    }
                    if let Some(plugin) = self.plugins.get_mut(&plugin_id) {
                        plugin.on_load_called = false;
                        plugin.on_load_failed = true;
                        plugin.info.enabled = false;
                    }
                }
                None => {}
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

impl Drop for PluginManager {
    fn drop(&mut self) {
        // Drop callbacks (both hooks and EventBus closures) before `self.plugins`
        // releases plugin instances and unmaps dynamic libraries.
        self.hooks.clear();
        if let Some(bus) = &self.event_bus {
            for plugin_id in self.plugins.keys() {
                bus.unsubscribe_owner(plugin_id);
            }
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
