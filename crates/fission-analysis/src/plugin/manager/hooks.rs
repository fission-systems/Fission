use super::core::PluginManager;
use crate::app::events::{FissionEvent, FissionEventType};
use crate::plugin::api::create_binary_info;
use crate::plugin::{HookPriority, PluginContext, PluginHook};

impl PluginManager {
    /// Register a hook for a plugin
    pub fn register_hook<F>(
        &mut self,
        plugin_id: &str,
        event_type: FissionEventType,
        priority: HookPriority,
        callback: F,
    ) -> Result<u64, String>
    where
        F: Fn(&FissionEvent) + Send + Sync + 'static,
    {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Plugin '{}' not found", plugin_id))?;

        let hook_id = self.next_hook_id;
        self.next_hook_id += 1;

        let hook = PluginHook {
            id: hook_id,
            plugin_id: plugin_id.to_string(),
            event_type,
            priority,
        };

        plugin.hooks.push(hook_id);
        self.hooks.insert(hook_id, (hook, Box::new(callback)));

        Ok(hook_id)
    }

    /// Unregister a hook
    pub fn unregister_hook(&mut self, hook_id: u64) -> Result<(), String> {
        let (hook, _) = self
            .hooks
            .remove(&hook_id)
            .ok_or_else(|| format!("Hook {} not found", hook_id))?;

        // Remove from plugin's hook list
        if let Some(plugin) = self.plugins.get_mut(&hook.plugin_id) {
            plugin.hooks.retain(|&id| id != hook_id);
        }

        Ok(())
    }

    /// Emit an event to all registered hooks and plugins
    pub fn emit_event(&self, event: &FissionEvent) {
        // 1. Dispatch to trait-based plugins
        if let Some(api) = &self.api {
            let extension = self
                .event_bus
                .clone()
                .map(|arc| arc as std::sync::Arc<dyn std::any::Any + Send + Sync>);
            let ctx = PluginContext::new(api.clone(), extension);

            for plugin in self.plugins.values() {
                if !plugin.info.enabled {
                    continue;
                }

                if let Some(instance) = &plugin.instance {
                    match event {
                        FissionEvent::BinaryLoaded(binary) => {
                            let info = create_binary_info(binary.as_ref());
                            instance.on_binary_loaded(&ctx, &info)
                        }
                        FissionEvent::DecompilationSuccess { address, code, .. } => {
                            instance.on_function_decompiled(&ctx, *address, code)
                        }
                        _ => {} // Other events not mapped to trait methods yet
                    }
                }
            }
        }

        // 2. Dispatch to registered hooks
        let event_type = event.event_type();

        // Collect matching hooks and sort by priority
        let mut matching_hooks: Vec<_> = self
            .hooks
            .values()
            .filter(|(hook, _)| {
                // Check if plugin is enabled
                if let Some(plugin) = self.plugins.get(&hook.plugin_id) {
                    if !plugin.info.enabled {
                        return false;
                    }
                }

                hook.event_type == event_type || hook.event_type == FissionEventType::All
            })
            .collect();

        matching_hooks.sort_by_key(|(hook, _)| hook.priority);

        // Call each hook
        for (_, callback) in matching_hooks {
            callback(event);
        }
    }
}
