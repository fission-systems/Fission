use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use fission_loader::loader::LoadedBinary;

/// System-wide events for Fission plugin/runtime boundaries.
#[derive(Debug, Clone)]
pub enum FissionEvent {
    AppStarted,
    AppShutdown,
    BinaryLoaded(Arc<LoadedBinary>),
    BinaryLoadFailed(String),
    DecompilationStarted {
        address: u64,
    },
    DecompilationSuccess {
        address: u64,
        function_name: Option<String>,
        code: String,
    },
    DecompilationFailed {
        address: u64,
        error: String,
    },
    BreakpointHit {
        address: u64,
        thread_id: u32,
    },
    // Keep debug-step event without depending on debugger register types.
    DebugStep {
        thread_id: u32,
    },
    CommandExecuted {
        command: String,
    },
    SelectionChanged {
        address: Option<u64>,
    },
    LogMessage {
        level: String,
        message: String,
        target: String,
    },
    Progress {
        task_id: String,
        current: usize,
        total: usize,
        message: String,
    },
}

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
    All,
}

impl FissionEvent {
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

    pub fn unsubscribe(&self, id: u64) {
        let mut subs = self.subscribers.write().unwrap_or_else(|e| e.into_inner());
        subs.remove(&id);
    }

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
