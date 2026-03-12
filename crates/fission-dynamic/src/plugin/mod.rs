//! Plugin System - Extensible plugin architecture for Fission.
//!
//! Provides a plugin API and event hooks for extending Fission functionality
//! with native Rust plugins (dynamic libraries).
//!
//! Plugins can subscribe to `FissionEvent` for system-wide events.

pub mod api;
pub mod hooks;
pub mod manager;
pub mod module;

// Re-export core plugin traits directly (no wrapper module needed)
pub use fission_core::plugin::traits::{
    self, FissionPlugin, PluginAPI as CorePluginAPI, PluginContext,
};

pub use api::PluginAPI;
pub use hooks::{FissionEvent, FissionEventType, HookPriority, PluginHook};
pub use manager::PluginManager;
