use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::any::Any;

use crate::analysis::loader::LoadedBinary;

/// System-wide events for Fission
#[derive(Debug, Clone)]
pub enum FissionEvent {
    /// A binary has been successfully loaded
    BinaryLoaded(Arc<LoadedBinary>),
    
    /// Binary loading failed
    BinaryLoadFailed(String),
    
    /// Decompilation started for an address
    DecompilationStarted {
        address: u64,
    },
    
    /// Decompilation finished successfully
    DecompilationSuccess {
        address: u64,
        code: String,
    },
    
    /// Decompilation failed
    DecompilationFailed {
        address: u64,
        error: String,
    },
    
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
    
    /// User interface focus/selection change
    SelectionChanged {
        address: Option<u64>,
    },
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
        F: Fn(&FissionEvent) + Send + Sync + 'static 
    {
        let mut id_guard = self.next_id.write().unwrap();
        let id = *id_guard;
        *id_guard += 1;
        drop(id_guard);

        let mut subs = self.subscribers.write().unwrap();
        subs.insert(id, Box::new(handler));
        
        id
    }

    /// Unsubscribe a listener
    pub fn unsubscribe(&self, id: u64) {
        let mut subs = self.subscribers.write().unwrap();
        subs.remove(&id);
    }

    /// Publish an event to all subscribers
    pub fn publish(&self, event: FissionEvent) {
        let subs = self.subscribers.read().unwrap();
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
