//! Fission Application Context
//!
//! Provides shared, thread-safe contexts for different domains of the application.
//! This separates concerns and allows core functionality to be accessed independently
//! of the GUI state.

use std::sync::{Arc, RwLock};

use crate::app::events::EventBus;
use crate::plugin::PluginManager;

/// Core application context shared across all components.
///
/// This struct provides thread-safe access to core services and state
/// that is independent of the UI layer.
#[derive(Clone)]
pub struct FissionContext {
    /// System-wide event bus for pub/sub communication
    pub event_bus: Arc<EventBus>,
    /// Plugin manager for extension system
    pub plugin_manager: Arc<RwLock<PluginManager>>,
}

impl FissionContext {
    /// Create a new FissionContext with default settings
    pub fn new() -> Self {
        let event_bus = Arc::new(EventBus::new());
        let mut plugin_manager = PluginManager::default();
        plugin_manager.set_event_bus(event_bus.clone());

        Self {
            event_bus,
            plugin_manager: Arc::new(RwLock::new(plugin_manager)),
        }
    }

    /// Create a new FissionContext with an existing event bus
    pub fn with_event_bus(event_bus: Arc<EventBus>) -> Self {
        let mut plugin_manager = PluginManager::default();
        plugin_manager.set_event_bus(event_bus.clone());

        Self {
            event_bus,
            plugin_manager: Arc::new(RwLock::new(plugin_manager)),
        }
    }

    /// Publish an event to all subscribers
    pub fn publish(&self, event: crate::app::events::FissionEvent) {
        self.event_bus.publish(event.clone());

        // Also dispatch to plugins
        if let Ok(pm) = self.plugin_manager.read() {
            pm.emit_event(&event);
        }
    }

    /// Log a message through the event system
    pub fn log(&self, level: &str, message: impl Into<String>, target: impl Into<String>) {
        self.event_bus
            .publish(crate::app::events::FissionEvent::LogMessage {
                level: level.to_string(),
                message: message.into(),
                target: target.into(),
            });
    }

    /// Log an info message
    pub fn log_info(&self, message: impl Into<String>) {
        self.log("info", message, "fission");
    }

    /// Log an error message
    pub fn log_error(&self, message: impl Into<String>) {
        self.log("error", message, "fission");
    }
}

impl Default for FissionContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_context_creation() {
        let ctx = FissionContext::new();
        assert!(ctx.plugin_manager.read().is_ok());
    }

    #[test]
    fn test_context_event_publish() {
        let ctx = FissionContext::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        ctx.event_bus.subscribe(move |_| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        ctx.log_info("Test message");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
