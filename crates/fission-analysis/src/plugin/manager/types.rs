//! Plugin Manager Types

use super::super::api::PluginInfo;
use super::super::traits::FissionPlugin;
use crate::app::events::FissionEvent;

/// Callback function type for plugin hooks
pub type HookCallback = Box<dyn Fn(&FissionEvent) + Send + Sync>;

/// A loaded plugin
pub(super) struct LoadedPlugin {
    /// Plugin metadata
    pub info: PluginInfo,
    /// Registered hooks
    pub hooks: Vec<u64>,
    /// Native plugin instance
    pub instance: Option<Box<dyn FissionPlugin>>,
    /// Plugin state (opaque, for legacy/script plugins)
    #[allow(dead_code)]
    pub state: Option<Box<dyn std::any::Any + Send + Sync>>,
}
