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
    #[allow(dead_code)]
    pub state: Option<Box<dyn std::any::Any + Send + Sync>>,
}
