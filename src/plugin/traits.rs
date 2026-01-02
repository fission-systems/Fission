//! Plugin Traits
//!
//! Defines the standard interface that all Fission plugins must implement.

use crate::core::prelude::*;
use crate::plugin::api::{BinaryInfo, PluginAPI};
use std::any::Any;
use std::sync::Arc;

use crate::core::events::EventBus;

/// Context provided to plugins during callbacks
pub struct PluginContext {
    /// Access to Fission API
    pub api: Arc<dyn PluginAPI>,
    /// System-wide Event Bus
    pub event_bus: Option<Arc<EventBus>>,
}

impl PluginContext {
    pub fn new(api: Arc<dyn PluginAPI>, event_bus: Option<Arc<EventBus>>) -> Self {
        Self { api, event_bus }
    }
}

/// The main trait for Fission plugins.
/// All plugins (native or script adapters) must implement this.
pub trait FissionPlugin: Send + Sync + Any {
    /// Get unique plugin ID
    fn id(&self) -> &str;

    /// Get human-readable name
    fn name(&self) -> &str;

    /// Get plugin version
    fn version(&self) -> &str {
        "0.1.0"
    }

    /// Get plugin description
    fn description(&self) -> &str {
        ""
    }

    /// Called when the plugin is loaded
    fn on_load(&mut self, _ctx: &PluginContext) -> Result<()> {
        Ok(())
    }

    /// Called when the plugin is unloaded
    fn on_unload(&mut self, _ctx: &PluginContext) -> Result<()> {
        Ok(())
    }

    /// Called when a binary is loaded
    fn on_binary_loaded(&self, _ctx: &PluginContext, _info: &BinaryInfo) {}

    /// Called when a function is decompiled
    fn on_function_decompiled(&self, _ctx: &PluginContext, _addr: u64, _code: &str) {}
}

// Allow downcasting for native plugins
impl dyn FissionPlugin {
    pub fn downcast_ref<T: FissionPlugin + 'static>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }

    pub fn downcast_mut<T: FissionPlugin + 'static>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut::<T>()
    }
}
