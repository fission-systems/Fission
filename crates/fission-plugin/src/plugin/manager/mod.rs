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
                on_load_called: false,
                info,
                hooks: Vec::new(),
                instance: None,
                library: None,
                destroy: None,
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
                    on_load_called: false,
                    destroy: None,
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
                    on_load_called: false,
                    destroy: None,
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
                on_load_called: false,
                destroy: None,
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

    struct NoopApi;
    impl PluginAPI for NoopApi {
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

    struct FailingLoadPlugin;
    impl FissionPlugin for FailingLoadPlugin {
        fn id(&self) -> &str {
            "fails_to_load"
        }
        fn name(&self) -> &str {
            "Fails To Load"
        }
        fn on_load(&mut self, _ctx: &PluginContext) -> Result<()> {
            anyhow::bail!("refusing to load")
        }
    }

    /// A plugin registered before the API exists used to be skipped by
    /// `on_load` for good: registration gates the call on the API, and
    /// `set_api` never went back to the plugins already in the map.
    #[test]
    fn set_api_calls_on_load_for_plugins_registered_before_it() {
        let mut pm = PluginManager::new();
        let count = Arc::new(AtomicU32::new(0));
        pm.register_native_plugin(Box::new(MockPlugin {
            id: "early".into(),
            load_count: count.clone(),
        }))
        .expect("registers without an API");
        assert_eq!(count.load(Ordering::SeqCst), 0, "no API yet, no on_load");

        pm.set_api(Arc::new(NoopApi));
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // Positive control: a second set_api does not load it a second time.
        pm.set_api(Arc::new(NoopApi));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_plugin_registered_with_the_api_present_is_loaded_exactly_once() {
        let mut pm = PluginManager::new();
        pm.set_api(Arc::new(NoopApi));
        let count = Arc::new(AtomicU32::new(0));
        pm.register_native_plugin(Box::new(MockPlugin {
            id: "late".into(),
            load_count: count.clone(),
        }))
        .expect("registers");
        assert_eq!(count.load(Ordering::SeqCst), 1);
        pm.set_api(Arc::new(NoopApi));
        assert_eq!(count.load(Ordering::SeqCst), 1, "set_api reloaded it");
    }

    #[test]
    fn a_plugin_whose_on_load_fails_at_set_api_is_disabled() {
        // Registration refuses a plugin whose on_load fails; the late call has
        // to end the same way rather than leave it running half-loaded.
        let mut pm = PluginManager::new();
        pm.register_native_plugin(Box::new(FailingLoadPlugin))
            .expect("registers without an API");
        pm.set_api(Arc::new(NoopApi));
        let info = pm.get_plugin("fails_to_load").expect("still listed");
        assert!(!info.enabled);
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

/// An instance a dynamic plugin made was allocated by that library's own
/// allocator, which need not be the host's (`fission_cli` uses mimalloc, a Rust
/// `cdylib` defaults to the system allocator). The host must not free it.
#[cfg(all(test, unix))]
mod foreign_allocation_tests {
    use super::*;
    use crate::plugin::FissionPlugin;
    use crate::plugin::api::PluginInfo;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use types::LoadedPlugin;

    /// Distinctive size, so only this test's allocation is counted.
    const PROBE_SIZE: usize = 3001;
    static FREES_OF_PROBE: AtomicUsize = AtomicUsize::new(0);
    static PROBE_DROPPED: AtomicBool = AtomicBool::new(false);

    struct CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if layout.size() == PROBE_SIZE {
                FREES_OF_PROBE.fetch_add(1, Ordering::SeqCst);
            }
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    struct Probe {
        _padding: [u8; PROBE_SIZE],
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            PROBE_DROPPED.store(true, Ordering::SeqCst);
        }
    }

    impl FissionPlugin for Probe {
        fn id(&self) -> &str {
            "probe"
        }
        fn name(&self) -> &str {
            "Probe"
        }
    }

    #[test]
    fn the_host_does_not_free_an_instance_a_library_allocated() {
        let mut manager = PluginManager::new();
        // Any library handle will do: the plugin counts as library-made.
        let library = libloading::Library::from(libloading::os::unix::Library::this());
        manager.plugins.insert(
            "probe".to_string(),
            LoadedPlugin {
                info: PluginInfo {
                    id: "probe".into(),
                    name: "Probe".into(),
                    ..Default::default()
                },
                hooks: Vec::new(),
                instance: Some(Box::new(Probe {
                    _padding: [0; PROBE_SIZE],
                })),
                library: Some(library),
                on_load_called: true,
                destroy: None,
                state: None,
            },
        );

        manager.unload_plugin("probe").expect("unload");

        assert!(
            PROBE_DROPPED.load(Ordering::SeqCst),
            "the plugin's own drop glue still runs"
        );
        assert_eq!(
            FREES_OF_PROBE.load(Ordering::SeqCst),
            0,
            "the host must not free memory the plugin library allocated"
        );
    }

    static DESTROY_CALLS: AtomicUsize = AtomicUsize::new(0);

    #[allow(improper_ctypes_definitions)]
    unsafe extern "C" fn test_destroy(plugin: *mut dyn FissionPlugin) {
        DESTROY_CALLS.fetch_add(1, Ordering::SeqCst);
        drop(unsafe { Box::from_raw(plugin) });
    }

    #[test]
    fn a_library_that_exports_destroy_frees_its_own_instance() {
        let before_frees = FREES_OF_PROBE.load(Ordering::SeqCst);
        let before_calls = DESTROY_CALLS.load(Ordering::SeqCst);
        let mut manager = PluginManager::new();
        let library = libloading::Library::from(libloading::os::unix::Library::this());
        manager.plugins.insert(
            "probe2".to_string(),
            LoadedPlugin {
                info: PluginInfo {
                    id: "probe2".into(),
                    name: "Probe".into(),
                    ..Default::default()
                },
                hooks: Vec::new(),
                instance: Some(Box::new(Probe {
                    _padding: [0; PROBE_SIZE],
                })),
                library: Some(library),
                on_load_called: true,
                destroy: Some(test_destroy),
                state: None,
            },
        );

        manager.unload_plugin("probe2").expect("unload");

        assert_eq!(DESTROY_CALLS.load(Ordering::SeqCst), before_calls + 1);
        assert_eq!(
            FREES_OF_PROBE.load(Ordering::SeqCst),
            before_frees + 1,
            "freed once, by the library's destroy"
        );
    }
}
