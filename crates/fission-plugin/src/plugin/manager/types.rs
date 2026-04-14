use super::super::api::PluginInfo;
use super::super::FissionPlugin;
use crate::events::FissionEvent;

pub type HookCallback = Box<dyn Fn(&FissionEvent) + Send + Sync>;

pub(super) struct LoadedPlugin {
    pub info: PluginInfo,
    pub hooks: Vec<u64>,
    pub instance: Option<Box<dyn FissionPlugin>>,
    #[allow(dead_code)]
    pub state: Option<Box<dyn std::any::Any + Send + Sync>>,
}
