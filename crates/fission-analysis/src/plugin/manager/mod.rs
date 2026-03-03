mod core;
mod hooks;
mod loader;
mod queries;
mod types;

pub use core::PluginManager;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::events::{EventBus, FissionEvent, FissionEventType};
    use crate::plugin::api::{BinaryInfo, PluginInfo};
    use crate::plugin::{FissionPlugin, HookPriority, PluginAPI, PluginContext};
    use anyhow::Result;
    use fission_core::common::types::FunctionInfo;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use types::LoadedPlugin;

    struct MockPlugin {
        id: String,
        load_count: Arc<AtomicU32>,
    }

    impl FissionPlugin for MockPlugin {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            "Mock Plugin"
        }
        fn on_load(&mut self, _ctx: &PluginContext) -> Result<()> {
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
        assert!(pm.register_native_plugin(Box::new(plugin)).is_ok());

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
        pm.plugins.insert(
            plugin_id.clone(),
            LoadedPlugin {
                info,
                hooks: Vec::new(),
                instance: None,
                state: None,
            },
        );

        // Register a hook
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let Ok(hook_id) = pm.register_hook(
            &plugin_id,
            FissionEventType::AppStarted,
            HookPriority::Normal,
            move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            },
        ) else {
            panic!("register_hook should succeed")
        };

        assert_eq!(pm.hook_count(), 1);

        // Emit event
        pm.emit_event(&FissionEvent::AppStarted);
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Unregister hook
        assert!(pm.unregister_hook(hook_id).is_ok());
        assert_eq!(pm.hook_count(), 0);
    }

    struct EventBusPlugin {
        id: String,
    }

    impl FissionPlugin for EventBusPlugin {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            "EventBus Plugin"
        }
        fn on_load(&mut self, ctx: &PluginContext) -> Result<()> {
            if let Some(bus) = ctx.get_extension::<EventBus>() {
                bus.publish(FissionEvent::LogMessage {
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
        let event_bus = Arc::new(EventBus::new());
        pm.set_event_bus(event_bus.clone());

        // Mock API is needed for on_load to be called
        struct MockApi;
        impl PluginAPI for MockApi {
            fn get_binary(&self) -> Option<BinaryInfo> {
                None
            }
            fn get_functions(&self) -> Vec<FunctionInfo> {
                Vec::new()
            }
            fn read_binary_bytes(&self, _addr: u64, _size: usize) -> Option<Vec<u8>> {
                None
            }
            fn log(&self, _msg: &str) {}
            fn log_error(&self, _msg: &str) {}
            fn decompile(&self, _addr: u64) -> Option<String> {
                None
            }
            fn get_current_decompiled_code(&self) -> Option<String> {
                None
            }
            fn disassemble(&self, _addr: u64, _size: usize) -> Vec<String> {
                Vec::new()
            }
        }
        pm.set_api(Arc::new(MockApi));

        // Subscribe to verify event
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        event_bus.subscribe(move |event| {
            if let FissionEvent::LogMessage { message, .. } = event {
                if message == "Plugin loaded" {
                    counter_clone.fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        let plugin = EventBusPlugin {
            id: "eb_plugin".into(),
        };
        assert!(pm.register_native_plugin(Box::new(plugin)).is_ok());

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
