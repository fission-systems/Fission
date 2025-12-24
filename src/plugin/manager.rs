//! Plugin Manager - Load, manage, and dispatch events to plugins.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::core::events::EventBus;
use super::hooks::{PluginEvent, PluginEventType, PluginHook, HookPriority};
use super::api::{PluginInfo, PluginType, PluginAPI, BinaryInfo};
use super::traits::{FissionPlugin, PluginContext};

/// Callback function type for plugin hooks
pub type HookCallback = Box<dyn Fn(&PluginEvent) + Send + Sync>;

/// A loaded plugin
struct LoadedPlugin {
    /// Plugin metadata
    info: PluginInfo,
    /// Registered hooks
    hooks: Vec<u64>,
    /// Native plugin instance
    instance: Option<Box<dyn FissionPlugin>>,
    /// Plugin state (opaque, for legacy/script plugins)
    #[allow(dead_code)]
    state: Option<Box<dyn std::any::Any + Send + Sync>>,
}

/// Plugin Manager - Central plugin registry and event dispatcher
pub struct PluginManager {
    /// Loaded plugins by ID
    plugins: HashMap<String, LoadedPlugin>,
    /// All registered hooks
    hooks: HashMap<u64, (PluginHook, HookCallback)>,
    /// Next hook ID
    next_hook_id: u64,
    /// Plugin search paths
    search_paths: Vec<PathBuf>,
    /// Shared API instance
    api: Option<Arc<dyn PluginAPI>>,
    /// System-wide Event Bus
    event_bus: Option<Arc<EventBus>>,
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
    
    /// Register a native Rust plugin
    pub fn register_native_plugin(&mut self, mut plugin: Box<dyn FissionPlugin>) -> Result<String, String> {
        let id = plugin.id().to_string();
        
        if self.plugins.contains_key(&id) {
            return Err(format!("Plugin '{}' already loaded", id));
        }
        
        // Initialize plugin logic
        if let Some(api) = &self.api {
            let ctx = PluginContext::new(api.clone(), self.event_bus.clone());
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

    /// Load a plugin from a file
    pub fn load_plugin<P: AsRef<Path>>(&mut self, path: P) -> Result<String, String> {
        let path = path.as_ref();
        
        // Determine plugin type from extension
        let plugin_type = match path.extension().and_then(|e| e.to_str()) {
            Some("py") => PluginType::Python,
            Some("lua") => PluginType::Lua,
            Some("so") | Some("dll") | Some("dylib") => PluginType::Native,
            _ => return Err("Unknown plugin type".into()),
        };
        
        // Generate plugin ID from filename
        let plugin_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("plugin_{}", self.plugins.len()));
        
        // Check if already loaded
        if self.plugins.contains_key(&plugin_id) {
            return Err(format!("Plugin '{}' already loaded", plugin_id));
        }
        
        // Create plugin info
        let info = PluginInfo {
            id: plugin_id.clone(),
            name: plugin_id.clone(),
            version: "0.1.0".into(),
            author: "Unknown".into(),
            description: format!("Loaded from {:?}", path),
            plugin_type,
            enabled: true,
        };
        
        // Create loaded plugin entry
        let loaded = LoadedPlugin {
            info,
            hooks: Vec::new(),
            instance: None,
            state: None,
        };
        
        self.plugins.insert(plugin_id.clone(), loaded);
        
        Ok(plugin_id)
    }
    
    /// Unload a plugin
    pub fn unload_plugin(&mut self, plugin_id: &str) -> Result<(), String> {
        if let Some(mut plugin) = self.plugins.remove(plugin_id) {
            // Call on_unload if it's a native plugin
            if let Some(mut instance) = plugin.instance.take() {
                if let Some(api) = &self.api {
                    let ctx = PluginContext::new(api.clone(), self.event_bus.clone());
                    let _ = instance.on_unload(&ctx);
                }
            }
            
            // Remove all hooks registered by this plugin
            for hook_id in plugin.hooks {
                self.hooks.remove(&hook_id);
            }
            
            Ok(())
        } else {
            Err(format!("Plugin '{}' not found", plugin_id))
        }
    }
    
    /// Register a hook for a plugin
    pub fn register_hook<F>(
        &mut self,
        plugin_id: &str,
        event_type: PluginEventType,
        priority: HookPriority,
        callback: F,
    ) -> Result<u64, String>
    where
        F: Fn(&PluginEvent) + Send + Sync + 'static,
    {
        let plugin = self.plugins.get_mut(plugin_id)
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
        let (hook, _) = self.hooks.remove(&hook_id)
            .ok_or_else(|| format!("Hook {} not found", hook_id))?;
        
        // Remove from plugin's hook list
        if let Some(plugin) = self.plugins.get_mut(&hook.plugin_id) {
            plugin.hooks.retain(|&id| id != hook_id);
        }
        
        Ok(())
    }
    
    /// Emit an event to all registered hooks and plugins
    pub fn emit_event(&self, event: &PluginEvent) {
        // 1. Dispatch to trait-based plugins
        if let Some(api) = &self.api {
            let ctx = PluginContext::new(api.clone(), self.event_bus.clone());
            
            for plugin in self.plugins.values() {
                if !plugin.info.enabled { continue; }
                
                if let Some(instance) = &plugin.instance {
                    match event {
                        PluginEvent::BinaryLoaded { binary } => {
                            let info = BinaryInfo::from(binary.as_ref());
                            instance.on_binary_loaded(&ctx, &info)
                        },
                        PluginEvent::FunctionDecompiled { address, code, name: _ } => {
                            instance.on_function_decompiled(&ctx, *address, code)
                        },
                        _ => {} // Other events not mapped to trait methods yet
                    }
                }
            }
        }

        // 2. Dispatch to registered hooks
        let event_type = event.event_type();
        
        // Collect matching hooks and sort by priority
        let mut matching_hooks: Vec<_> = self.hooks.values()
            .filter(|(hook, _)| {
                // Check if plugin is enabled
                if let Some(plugin) = self.plugins.get(&hook.plugin_id) {
                    if !plugin.info.enabled { return false; }
                }
                
                hook.event_type == event_type || hook.event_type == PluginEventType::All
            })
            .collect();
        
        matching_hooks.sort_by_key(|(hook, _)| hook.priority);
        
        // Call each hook
        for (_, callback) in matching_hooks {
            callback(event);
        }
    }
    
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
        let plugin = self.plugins.get_mut(plugin_id)
            .ok_or_else(|| format!("Plugin '{}' not found", plugin_id))?;
        plugin.info.enabled = true;
        Ok(())
    }
    
    /// Disable a plugin
    pub fn disable_plugin(&mut self, plugin_id: &str) -> Result<(), String> {
        let plugin = self.plugins.get_mut(plugin_id)
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

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    
    struct MockPlugin {
        id: String,
        load_count: Arc<AtomicU32>,
    }
    
    impl FissionPlugin for MockPlugin {
        fn id(&self) -> &str { &self.id }
        fn name(&self) -> &str { "Mock Plugin" }
        fn on_load(&mut self, _ctx: &PluginContext) -> crate::core::prelude::Result<()> {
            self.load_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    
    #[test]
    fn test_trait_plugin() {
        let mut pm = PluginManager::new();
        let count = Arc::new(AtomicU32::new(0));
        
        let plugin = MockPlugin {
            id: "mock".into(),
            load_count: count.clone(),
        };
        
        // Note: register_native_plugin calls on_load ONLY if API is set.
        // For this test we just register it and check it exists.
        pm.register_native_plugin(Box::new(plugin)).unwrap();
        
        assert_eq!(pm.plugin_count(), 1);
        assert!(pm.get_plugin("mock").is_some());
    }

    #[test]
    fn test_plugin_manager_basic() {
        let mut pm = PluginManager::new();
        
        // Register a "fake" plugin manually
        let plugin_id = "test_plugin".to_string();
        let info = PluginInfo {
            id: plugin_id.clone(),
            name: "Test Plugin".into(),
            ..Default::default()
        };
        pm.plugins.insert(plugin_id.clone(), LoadedPlugin {
            info,
            hooks: Vec::new(),
            instance: None,
            state: None,
        });
        
        // Register a hook
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let hook_id = pm.register_hook(
            &plugin_id,
            PluginEventType::AppStarted,
            HookPriority::Normal,
            move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            },
        ).unwrap();
        
        assert_eq!(pm.hook_count(), 1);
        
        // Emit event
        pm.emit_event(&PluginEvent::AppStarted);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        
        // Unregister hook
        pm.unregister_hook(hook_id).unwrap();
        assert_eq!(pm.hook_count(), 0);
    }

    struct EventBusPlugin {
        id: String,
    }

    impl FissionPlugin for EventBusPlugin {
        fn id(&self) -> &str { &self.id }
        fn name(&self) -> &str { "EventBus Plugin" }
        fn on_load(&mut self, ctx: &PluginContext) -> crate::core::prelude::Result<()> {
            if let Some(bus) = &ctx.event_bus {
                bus.publish(crate::core::events::FissionEvent::LogMessage { 
                    level: "info".into(),
                    message: "Plugin loaded".into(),
                    target: "plugin".into(),
                });
            }
            Ok(())
        }
    }

    #[test]
    fn test_plugin_event_bus() {
        let mut pm = PluginManager::new();
        let event_bus = Arc::new(crate::core::events::EventBus::new());
        pm.set_event_bus(event_bus.clone());
        
        // Mock API is needed for on_load to be called
        struct MockApi;
        impl PluginAPI for MockApi {
            fn get_binary(&self) -> Option<BinaryInfo> { None }
            fn get_functions(&self) -> Vec<crate::analysis::loader::FunctionInfo> { Vec::new() }
            fn read_binary_bytes(&self, _addr: u64, _size: usize) -> Option<Vec<u8>> { None }
            fn log(&self, _msg: &str) {}
            fn log_error(&self, _msg: &str) {}
            fn decompile(&self, _addr: u64) -> Option<String> { None }
            fn get_current_decompiled_code(&self) -> Option<String> { None }
            fn disassemble(&self, _addr: u64, _size: usize) -> Vec<String> { Vec::new() }
        }
        pm.set_api(Arc::new(MockApi));

        // Subscribe to verify event
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        event_bus.subscribe(move |event| {
            if let crate::core::events::FissionEvent::LogMessage { message, .. } = event {
                if message == "Plugin loaded" {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        let plugin = EventBusPlugin { id: "eb_plugin".into() };
        pm.register_native_plugin(Box::new(plugin)).unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
