use super::super::FissionPlugin;
use super::super::api::PluginInfo;
use crate::events::FissionEvent;

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
    #[allow(dead_code)]
    pub state: Option<Box<dyn std::any::Any + Send + Sync>>,
}
