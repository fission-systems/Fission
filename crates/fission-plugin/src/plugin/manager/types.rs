use super::super::FissionPlugin;
use super::super::api::PluginInfo;
use crate::events::FissionEvent;

/// Optional symbol a dynamic plugin exports to free the instance
/// `fission_plugin_create` made, with the allocator that made it:
///
/// ```ignore
/// #[unsafe(no_mangle)]
/// pub extern "C" fn fission_plugin_destroy(plugin: *mut dyn FissionPlugin) {
///     drop(unsafe { Box::from_raw(plugin) });
/// }
/// ```
#[allow(improper_ctypes_definitions)]
pub(super) type PluginDestroy = unsafe extern "C" fn(*mut dyn FissionPlugin);

/// Release an instance a plugin library created.
///
/// The library allocated it with its own global allocator, which need not be
/// the host's (the CLI uses mimalloc; a Rust `cdylib` defaults to the system
/// allocator), so the host must not free it. With a `destroy` symbol the library
/// frees it. Without one the plugin's drop glue still runs, so it can release
/// what it holds, and the instance's own allocation is left behind rather than
/// handed to an allocator that did not make it.
pub(super) fn release_library_instance(
    instance: Box<dyn FissionPlugin>,
    destroy: Option<PluginDestroy>,
) {
    let raw = Box::into_raw(instance);
    match destroy {
        // SAFETY: `raw` came from the library's own constructor and is freed once.
        Some(destroy) => unsafe { destroy(raw) },
        // SAFETY: `raw` is a valid, uniquely owned instance; it is dropped in place
        // exactly once and its storage is never reused.
        None => unsafe { std::ptr::drop_in_place(raw) },
    }
}

pub type HookCallback = Box<dyn Fn(&FissionEvent) + Send + Sync>;

pub(super) struct LoadedPlugin {
    pub info: PluginInfo,
    pub hooks: Vec<u64>,
    pub instance: Option<Box<dyn FissionPlugin>>,
    /// The dynamic library must outlive its trait object.  This field is
    /// declared after `instance` so the plugin value is dropped first.
    pub library: Option<libloading::Library>,
    /// Has `on_load` run? Registration without an API skips it, and `set_api`
    /// runs it later for exactly the plugins this says it has not run for.
    pub on_load_called: bool,
    /// The library's `fission_plugin_destroy`, valid while `library` is.
    pub destroy: Option<PluginDestroy>,
    #[allow(dead_code)]
    pub state: Option<Box<dyn std::any::Any + Send + Sync>>,
}

impl LoadedPlugin {
    /// Drop the instance the way its origin requires, while `library` is still
    /// mapped (its vtable and drop glue live there).
    pub(super) fn release_instance(&mut self) {
        let Some(instance) = self.instance.take() else {
            return;
        };
        if self.library.is_some() {
            release_library_instance(instance, self.destroy);
        } else {
            drop(instance);
        }
    }
}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        self.release_instance();
    }
}
