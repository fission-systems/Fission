use super::core::PluginManager;
use crate::plugin::api::PluginInfo;

impl PluginManager {
    /// List all loaded plugins
    pub fn list_plugins(&self) -> Vec<&PluginInfo> {
        self.plugins.values().map(|p| &p.info).collect()
    }

    /// Get plugin info by ID
    pub fn get_plugin(&self, plugin_id: &str) -> Option<&PluginInfo> {
        self.plugins.get(plugin_id).map(|p| &p.info)
    }

    /// Enable a plugin
    pub fn enable_plugin(&mut self, plugin_id: &str) -> Result<(), String> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Plugin '{}' not found", plugin_id))?;
        plugin.info.enabled = true;
        Ok(())
    }

    /// Disable a plugin
    pub fn disable_plugin(&mut self, plugin_id: &str) -> Result<(), String> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Plugin '{}' not found", plugin_id))?;
        plugin.info.enabled = false;
        Ok(())
    }

    /// Get plugin count
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Get hook count
    pub fn hook_count(&self) -> usize {
        self.hooks.len()
    }
}
