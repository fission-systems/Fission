use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::debug::types::RegisterState;
use fission_loader::loader::LoadedBinary;

/// System-wide events for Fission
///
/// This is the unified event system for the entire application.
/// Both core components and plugins subscribe to these events.
#[derive(Debug, Clone)]
pub enum FissionEvent {
    // ===== Application Lifecycle =====
    /// Application has started
    AppStarted,

    /// Application is shutting down
    AppShutdown,

    // ===== Binary Analysis =====
    /// A binary has been successfully loaded
    BinaryLoaded(Arc<LoadedBinary>),

    /// Binary loading failed
    BinaryLoadFailed(String),

    // ===== Decompilation =====
    /// Decompilation started for an address
    DecompilationStarted { address: u64 },

    /// Decompilation finished successfully
    DecompilationSuccess {
        address: u64,
        /// Function name (if known)
        function_name: Option<String>,
        code: String,
    },

    /// Decompilation failed
    DecompilationFailed { address: u64, error: String },

    // ===== Debugging =====
    /// A breakpoint was hit
    BreakpointHit { address: u64, thread_id: u32 },

    /// A debug step was executed
    DebugStep {
        registers: RegisterState,
        thread_id: u32,
    },

    // ===== User Interaction =====
    /// User executed a command (CLI or UI)
    CommandExecuted { command: String },

    /// User interface focus/selection change
    SelectionChanged { address: Option<u64> },

    // ===== System Messages =====
    /// Log message to be displayed/stored
    LogMessage {
        level: String, // "info", "warn", "error", etc.
        message: String,
        target: String, // Component name
    },

    /// Generic progress update
    Progress {
        task_id: String,
        current: usize,
        total: usize,
        message: String,
    },
}

/// Event type identifiers for filtering/subscribing to specific events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FissionEventType {
    AppStarted,
    AppShutdown,
    BinaryLoaded,
    BinaryLoadFailed,
    DecompilationStarted,
    DecompilationSuccess,
    DecompilationFailed,
    BreakpointHit,
    DebugStep,
    CommandExecuted,
    SelectionChanged,
    LogMessage,
    Progress,
    /// Subscribe to all events
    All,
}

impl FissionEvent {
    /// Get the event type for filtering
    pub fn event_type(&self) -> FissionEventType {
        match self {
            FissionEvent::AppStarted => FissionEventType::AppStarted,
            FissionEvent::AppShutdown => FissionEventType::AppShutdown,
            FissionEvent::BinaryLoaded(_) => FissionEventType::BinaryLoaded,
            FissionEvent::BinaryLoadFailed(_) => FissionEventType::BinaryLoadFailed,
            FissionEvent::DecompilationStarted { .. } => FissionEventType::DecompilationStarted,
            FissionEvent::DecompilationSuccess { .. } => FissionEventType::DecompilationSuccess,
            FissionEvent::DecompilationFailed { .. } => FissionEventType::DecompilationFailed,
            FissionEvent::BreakpointHit { .. } => FissionEventType::BreakpointHit,
            FissionEvent::DebugStep { .. } => FissionEventType::DebugStep,
            FissionEvent::CommandExecuted { .. } => FissionEventType::CommandExecuted,
            FissionEvent::SelectionChanged { .. } => FissionEventType::SelectionChanged,
            FissionEvent::LogMessage { .. } => FissionEventType::LogMessage,
            FissionEvent::Progress { .. } => FissionEventType::Progress,
        }
    }
}

type EventHandler = Box<dyn Fn(&FissionEvent) + Send + Sync>;

/// Simple Pub/Sub Event Bus
pub struct EventBus {
    subscribers: RwLock<HashMap<u64, EventHandler>>,
    next_id: RwLock<u64>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: RwLock::new(HashMap::new()),
            next_id: RwLock::new(0),
        }
    }

    /// Subscribe to all events
    pub fn subscribe<F>(&self, handler: F) -> u64
    where
        F: Fn(&FissionEvent) + Send + Sync + 'static,
    {
        let mut id_guard = self.next_id.write().unwrap_or_else(|e| e.into_inner());
        let id = *id_guard;
        *id_guard += 1;
        drop(id_guard);

        let mut subs = self.subscribers.write().unwrap_or_else(|e| e.into_inner());
        subs.insert(id, Box::new(handler));

        id
    }

    /// Unsubscribe a listener
    pub fn unsubscribe(&self, id: u64) {
        let mut subs = self.subscribers.write().unwrap_or_else(|e| e.into_inner());
        subs.remove(&id);
    }

    /// Publish an event to all subscribers
    pub fn publish(&self, event: FissionEvent) {
        let subs = self.subscribers.read().unwrap_or_else(|e| e.into_inner());
        for handler in subs.values() {
            handler(&event);
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_pub_sub() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let _id = bus.subscribe(move |event| {
            if let FissionEvent::LogMessage { .. } = event {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        bus.publish(FissionEvent::LogMessage {
            level: "info".into(),
            message: "test".into(),
            target: "test".into(),
        });

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
