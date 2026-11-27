//! Plugin Event Hooks & Dispatch Outcomes

pub use crate::events::{FailurePolicy, FissionEvent, FissionEventType, SubscriberOutcome};

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

/// Delivery record for a loaded plugin instance's direct lifecycle callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDeliveryRecord {
    pub plugin_id: String,
    pub outcome: SubscriberOutcome,
}

/// Delivery record for an individual registered plugin hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookDeliveryRecord {
    pub hook_id: u64,
    pub plugin_id: String,
    pub priority: HookPriority,
    pub outcome: SubscriberOutcome,
}

/// Typed diagnostic report returned by [`super::PluginManager::emit_event_checked`] and
/// [`super::PluginManager::emit_event_with_policy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookDispatchReport {
    pub event_type: FissionEventType,
    pub plugin_outcomes: Vec<PluginDeliveryRecord>,
    pub hook_outcomes: Vec<HookDeliveryRecord>,
    pub complete: bool,
}
