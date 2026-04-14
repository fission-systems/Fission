//! Plugin Event Hooks

pub use crate::events::{FissionEvent, FissionEventType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HookPriority {
    High = 0,
    Normal = 50,
    Low = 100,
}

impl Default for HookPriority {
    fn default() -> Self {
        HookPriority::Normal
    }
}

#[derive(Debug, Clone)]
pub struct PluginHook {
    pub id: u64,
    pub plugin_id: String,
    pub event_type: FissionEventType,
    pub priority: HookPriority,
}
