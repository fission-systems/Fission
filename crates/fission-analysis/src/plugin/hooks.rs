//! Plugin Event Hooks - Define hooks and priorities for plugin event subscriptions.
//!
//! Note: The unified event system is now in `app::events::FissionEvent`.
//! This module provides hook management types and backwards compatibility aliases.

// Re-export the unified event types from core
pub use crate::app::events::{FissionEvent, FissionEventType};

/// Hook priority (lower = earlier execution)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HookPriority {
    /// Run first
    High = 0,
    /// Default priority
    Normal = 50,
    /// Run last
    Low = 100,
}

impl Default for HookPriority {
    fn default() -> Self {
        HookPriority::Normal
    }
}

/// A plugin hook registration
#[derive(Debug, Clone)]
pub struct PluginHook {
    /// Unique hook ID
    pub id: u64,
    /// Plugin ID that registered this hook
    pub plugin_id: String,
    /// Event type to hook
    pub event_type: FissionEventType,
    /// Execution priority
    pub priority: HookPriority,
}
