use super::core::PluginManager;
use super::types::LoadedPlugin;
use crate::plugin::api::{PluginInfo, PluginType};
use crate::plugin::{FissionPlugin, PluginContext};
use std::path::Path;

/// Symbol exported by a Rust dynamic plugin.
///
/// A plugin must be built against the same `fission-plugin` contract and
/// expose `fission_plugin_create` with this signature:
///
/// ```ignore
/// #[unsafe(no_mangle)]
/// pub extern "C" fn fission_plugin_create() -> *mut dyn FissionPlugin {
///     Box::into_raw(Box::new(MyPlugin::default()))
/// }
/// ```
///
/// The trait object is intentionally kept behind the existing Rust contract;
/// this is a version-matched plugin ABI, not a stable C ABI.  A future stable
/// plugin ABI should replace this boundary with an explicit `repr(C)` vtable.
#[allow(improper_ctypes_definitions)]
type PluginCreate = unsafe extern "C" fn() -> *mut dyn FissionPlugin;

const PLUGIN_CREATE_SYMBOL: &[u8] = b"fission_plugin_create\0";

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
            library: None,
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

        let library = unsafe { libloading::Library::new(path) }
            .map_err(|error| format!("Failed to open plugin {:?}: {error}", path))?;

        let raw_plugin = unsafe {
            let constructor: libloading::Symbol<'_, PluginCreate> =
                library.get(PLUGIN_CREATE_SYMBOL).map_err(|error| {
                    format!(
                        "Plugin {:?} does not export {}: {error}",
                        path,
                        String::from_utf8_lossy(
                            &PLUGIN_CREATE_SYMBOL[..PLUGIN_CREATE_SYMBOL.len() - 1]
                        )
                    )
                })?;
            constructor()
        };
        if raw_plugin.is_null() {
            return Err(format!(
                "Plugin {:?} returned a null instance from {}",
                path,
                String::from_utf8_lossy(&PLUGIN_CREATE_SYMBOL[..PLUGIN_CREATE_SYMBOL.len() - 1])
            ));
        }

        // The constructor transfers ownership of the Box allocation to the
        // manager.  It is dropped before `library`, so its vtable and drop
        // glue remain mapped for the entire lifetime of the object.
        let mut plugin = unsafe { Box::from_raw(raw_plugin) };
        let plugin_id = plugin.id().to_string();
        if plugin_id.is_empty() {
            return Err(format!("Plugin {:?} returned an empty plugin id", path));
        }
        if self.plugins.contains_key(&plugin_id) {
            return Err(format!("Plugin '{}' already loaded", plugin_id));
        }

        if let Some(api) = &self.api {
            let extension = self
                .event_bus
                .clone()
                .map(|e| e as std::sync::Arc<dyn std::any::Any + Send + Sync>);
            let ctx = PluginContext::new(api.clone(), extension);
            if let Err(error) = plugin.on_load(&ctx) {
                return Err(format!("Failed to load plugin '{}': {error:?}", plugin_id));
            }
        }

        let info = PluginInfo {
            id: plugin_id.clone(),
            name: plugin.name().to_string(),
            version: plugin.version().to_string(),
            author: "Unknown".into(),
            description: plugin.description().to_string(),
            plugin_type,
            enabled: true,
        };

        let loaded = LoadedPlugin {
            info,
            hooks: Vec::new(),
            instance: Some(plugin),
            library: Some(library),
            state: None,
        };

        self.plugins.insert(plugin_id.clone(), loaded);
        Ok(plugin_id)
    }

    pub fn unload_plugin(&mut self, plugin_id: &str) -> Result<(), String> {
        if let Some(mut plugin) = self.plugins.remove(plugin_id) {
            let mut instance = plugin.instance.take();
            if let (Some(instance_ref), Some(api)) = (instance.as_mut(), &self.api) {
                let extension = self
                    .event_bus
                    .clone()
                    .map(|e| e as std::sync::Arc<dyn std::any::Any + Send + Sync>);
                let ctx = PluginContext::new(api.clone(), extension);
                let _ = instance_ref.on_unload(&ctx);
            }
            // Drop the trait object while the originating library is still
            // mapped; its vtable and drop glue live in that library.
            drop(instance);

            for hook_id in plugin.hooks {
                self.hooks.remove(&hook_id);
            }

            drop(plugin.library.take());

            Ok(())
        } else {
            Err(format!("Plugin '{}' not found", plugin_id))
        }
    }
}
