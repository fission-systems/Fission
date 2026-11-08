mod core;
mod hooks;
mod loader;
mod queries;
mod types;

pub use core::PluginManager;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventBus, FissionEvent, FissionEventType};
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

        assert!(pm.register_native_plugin(Box::new(plugin)).is_ok());
        assert_eq!(pm.plugin_count(), 1);
        assert!(pm.get_plugin("mock").is_some());
    }

    #[test]
    fn test_plugin_manager_basic() {
        let mut pm = PluginManager::new();

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
                library: None,
                state: None,
            },
        );

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
        pm.emit_event(&FissionEvent::AppStarted);
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        assert!(pm.unregister_hook(hook_id).is_ok());
        assert_eq!(pm.hook_count(), 0);
    }

    /// Records its own drop into a shared log.
    struct DropLog(&'static str, Arc<std::sync::Mutex<Vec<&'static str>>>);

    impl Drop for DropLog {
        fn drop(&mut self) {
            self.1.lock().unwrap().push(self.0);
        }
    }

    /// The field is never read; it exists to be dropped with the plugin.
    struct DropRecordingPlugin(#[allow(dead_code)] DropLog);

    impl FissionPlugin for DropRecordingPlugin {
        fn id(&self) -> &str {
            "drop_order"
        }
        fn name(&self) -> &str {
            "Drop Order"
        }
    }

    /// A plugin's callbacks run code that lives in the plugin's library, so
    /// they have to be dropped before the plugin (and its `Library`, which is a
    /// field of the same `LoadedPlugin`) is. Fields drop in declaration order,
    /// and `plugins` was declared before `hooks`, so the library went first.
    #[test]
    fn hooks_are_dropped_before_the_plugins_whose_code_they_run() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let mut pm = PluginManager::new();
            pm.plugins.insert(
                "drop_order".into(),
                LoadedPlugin {
                    info: PluginInfo {
                        id: "drop_order".into(),
                        name: "Drop Order".into(),
                        ..Default::default()
                    },
                    hooks: Vec::new(),
                    instance: Some(Box::new(DropRecordingPlugin(DropLog(
                        "plugin",
                        log.clone(),
                    )))),
                    library: None,
                    state: None,
                },
            );
            let guard = DropLog("hook", log.clone());
            pm.register_hook(
                "drop_order",
                FissionEventType::AppStarted,
                HookPriority::Normal,
                move |_| {
                    let _keep = &guard;
                },
            )
            .expect("hook registers");
        }
        assert_eq!(*log.lock().unwrap(), ["hook", "plugin"]);
    }

    /// Positive control: tearing a manager down by hand with the hooks still
    /// registered must not change what a plugin with no hooks does.
    #[test]
    fn a_manager_with_a_plugin_and_no_hooks_still_drops_cleanly() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let mut pm = PluginManager::new();
            pm.plugins.insert(
                "drop_order".into(),
                LoadedPlugin {
                    info: PluginInfo {
                        id: "drop_order".into(),
                        name: "Drop Order".into(),
                        ..Default::default()
                    },
                    hooks: Vec::new(),
                    instance: Some(Box::new(DropRecordingPlugin(DropLog(
                        "plugin",
                        log.clone(),
                    )))),
                    library: None,
                    state: None,
                },
            );
        }
        assert_eq!(*log.lock().unwrap(), ["plugin"]);
    }

    /// A hook whose plugin is no longer in the map is an orphan, and it must
    /// not run. The filter skipped a hook only when its plugin existed *and* was
    /// disabled, so a missing plugin let it through.
    #[test]
    fn a_hook_whose_plugin_is_gone_does_not_run() {
        let mut pm = PluginManager::new();
        pm.plugins.insert(
            "gone".into(),
            LoadedPlugin {
                info: PluginInfo {
                    id: "gone".into(),
                    name: "Gone".into(),
                    ..Default::default()
                },
                hooks: Vec::new(),
                instance: None,
                library: None,
                state: None,
            },
        );
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        pm.register_hook(
            "gone",
            FissionEventType::AppStarted,
            HookPriority::Normal,
            move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            },
        )
        .expect("hook registers");
        pm.emit_event(&FissionEvent::AppStarted);
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "control: hook runs while its plugin is loaded"
        );

        pm.plugins.remove("gone");
        pm.emit_event(&FissionEvent::AppStarted);
        assert_eq!(counter.load(Ordering::SeqCst), 1, "orphan hook ran");
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

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        event_bus.subscribe(move |event| {
            if let FissionEvent::LogMessage { message, .. } = event
                && message == "Plugin loaded"
            {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        let plugin = EventBusPlugin {
            id: "eb_plugin".into(),
        };
        assert!(pm.register_native_plugin(Box::new(plugin)).is_ok());

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn load_plugin_does_not_register_an_unopenable_library() {
        let path = std::env::temp_dir().join(format!(
            "fission-plugin-missing-{}.dylib",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let mut pm = PluginManager::new();
        let error = pm
            .load_plugin(&path)
            .expect_err("a missing dynamic library must not load");

        assert!(error.contains("Failed to open plugin"), "{error}");
        assert_eq!(pm.plugin_count(), 0);
    }
}
