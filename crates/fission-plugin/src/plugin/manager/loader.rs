use super::core::PluginManager;
use super::types::LoadedPlugin;
use crate::plugin::api::{PluginInfo, PluginType};
use crate::plugin::{FissionPlugin, PluginContext};
use fission_core::FISSION_VERSION;
use std::path::Path;

impl PluginManager {
    pub fn register_native_plugin(
        &mut self,
        mut plugin: Box<dyn FissionPlugin>,
    ) -> Result<String, String> {
        let id = plugin.id().to_string();

        if self.plugins.contains_key(&id) {
            return Err(format!("Plugin '{}' already loaded", id));
        }

        if let Some(api) = &self.api {
            let extension = self
                .event_bus
                .clone()
                .map(|e| e as std::sync::Arc<dyn std::any::Any + Send + Sync>);
            let ctx = PluginContext::new(api.clone(), extension);
            if let Err(e) = plugin.on_load(&ctx) {
                return Err(format!("Failed to load plugin '{}': {:?}", id, e));
            }
        }

        let info = PluginInfo {
            id: id.clone(),
            name: plugin.name().to_string(),
            version: plugin.version().to_string(),
            author: "Unknown".into(),
            description: plugin.description().to_string(),
            plugin_type: PluginType::Native,
            enabled: true,
        };

        let loaded = LoadedPlugin {
            info,
            hooks: Vec::new(),
            instance: Some(plugin),
            state: None,
        };

        self.plugins.insert(id.clone(), loaded);
        Ok(id)
    }

    pub fn load_plugin<P: AsRef<Path>>(&mut self, path: P) -> Result<String, String> {
        let path = path.as_ref();

        let plugin_type = match path.extension().and_then(|e| e.to_str()) {
            Some("so") | Some("dll") | Some("dylib") => PluginType::Native,
            Some(ext) => return Err(format!("Unsupported plugin type: .{ext}")),
            _ => return Err("Unknown plugin type".into()),
        };

        let plugin_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("plugin_{}", self.plugins.len()));

        if self.plugins.contains_key(&plugin_id) {
            return Err(format!("Plugin '{}' already loaded", plugin_id));
        }

        let info = PluginInfo {
            id: plugin_id.clone(),
            name: plugin_id.clone(),
            version: FISSION_VERSION.into(),
            author: "Unknown".into(),
            description: format!("Loaded from {:?}", path),
            plugin_type,
            enabled: true,
        };

        let loaded = LoadedPlugin {
            info,
            hooks: Vec::new(),
            instance: None,
            state: None,
        };

        self.plugins.insert(plugin_id.clone(), loaded);
        Ok(plugin_id)
    }

    pub fn unload_plugin(&mut self, plugin_id: &str) -> Result<(), String> {
        if let Some(mut plugin) = self.plugins.remove(plugin_id) {
            if let Some(mut instance) = plugin.instance.take()
                && let Some(api) = &self.api
            {
                let extension = self
                    .event_bus
                    .clone()
                    .map(|e| e as std::sync::Arc<dyn std::any::Any + Send + Sync>);
                let ctx = PluginContext::new(api.clone(), extension);
                let _ = instance.on_unload(&ctx);
            }

            for hook_id in plugin.hooks {
                self.hooks.remove(&hook_id);
            }

            Ok(())
        } else {
            Err(format!("Plugin '{}' not found", plugin_id))
        }
    }
}
