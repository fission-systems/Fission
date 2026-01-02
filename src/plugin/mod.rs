//! Plugin System - Extensible plugin architecture for Fission.
//!
//! Provides a plugin API and event hooks for extending Fission functionality
//! with Python scripts (via PyO3) or native Rust plugins.
//!
//! Plugins can subscribe to `FissionEvent` for system-wide events.

pub mod api;
pub mod hooks;
pub mod manager;
pub mod module;
#[cfg(feature = "python")]
pub mod python;
pub mod traits;

pub use api::PluginAPI;
pub use hooks::{FissionEvent, FissionEventType, HookPriority, PluginHook};
pub use manager::PluginManager;
pub use traits::{FissionPlugin, PluginContext};
