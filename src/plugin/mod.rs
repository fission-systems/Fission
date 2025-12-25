//! Plugin System - Extensible plugin architecture for Fission.
//!
//! Provides a plugin API and event hooks for extending Fission functionality
//! with Python scripts (via PyO3) or native Rust plugins.
//!
//! Plugins can subscribe to `FissionEvent` for system-wide events.

pub mod hooks;
pub mod manager;
pub mod api;
pub mod traits;
#[cfg(feature = "python")]
pub mod python;
pub mod module;

pub use hooks::{FissionEvent, FissionEventType, PluginHook, HookPriority};
pub use manager::PluginManager;
pub use api::PluginAPI;
pub use traits::{FissionPlugin, PluginContext};
