//! Plugin System - Extensible plugin architecture for Fission.

pub mod api;
pub mod hooks;
pub mod manager;

pub use fission_core::plugin::traits::{
    self, FissionPlugin, PluginAPI as CorePluginAPI, PluginContext,
};

pub use api::PluginAPI;
pub use hooks::{FissionEvent, FissionEventType, HookPriority, PluginHook};
pub use manager::PluginManager;
