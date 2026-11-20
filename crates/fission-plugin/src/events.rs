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

type EventHandler = Arc<dyn Fn(&FissionEvent) + Send + Sync>;

struct EventBusInner {
    subscribers: RwLock<HashMap<u64, (Option<String>, EventHandler)>>,
    next_id: RwLock<u64>,
}

pub struct EventBus {
    inner: Arc<EventBusInner>,
    owner: Option<String>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(EventBusInner {
                subscribers: RwLock::new(HashMap::new()),
                next_id: RwLock::new(0),
            }),
            owner: None,
        }
    }

    /// Return a handle sharing the same underlying bus that tags any new
    /// subscriptions with `owner`, allowing bulk revocation on plugin unload.
    pub fn scoped(&self, owner: impl Into<String>) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            owner: Some(owner.into()),
        }
    }

    pub fn subscribe<F>(&self, handler: F) -> u64
    where
        F: Fn(&FissionEvent) + Send + Sync + 'static,
    {
        let mut id_guard = self
            .inner
            .next_id
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let id = *id_guard;
        *id_guard += 1;
        drop(id_guard);

        let mut subs = self
            .inner
            .subscribers
            .write()
            .unwrap_or_else(|e| e.into_inner());
        subs.insert(id, (self.owner.clone(), Arc::new(handler)));

        id
    }

    pub fn unsubscribe(&self, id: u64) {
        let mut subs = self
            .inner
            .subscribers
            .write()
            .unwrap_or_else(|e| e.into_inner());
        subs.remove(&id);
    }

    /// Remove every subscription registered through a scoped handle for `owner`.
    pub fn unsubscribe_owner(&self, owner: &str) -> usize {
        let mut subs = self
            .inner
            .subscribers
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let before = subs.len();
        subs.retain(|_, (sub_owner, _)| sub_owner.as_deref() != Some(owner));
        before - subs.len()
    }

    pub fn publish(&self, event: FissionEvent) {
        // Call the handlers with the lock released. A handler that subscribes
        // or unsubscribes takes the write lock, and waiting for it while this
        // thread still held the read lock deadlocked.
        let handlers: Vec<EventHandler> = {
            let subs = self
                .inner
                .subscribers
                .read()
                .unwrap_or_else(|e| e.into_inner());
            subs.values().map(|(_, handler)| handler.clone()).collect()
        };
        for handler in handlers {
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

    #[test]
    fn a_handler_may_subscribe_while_an_event_is_being_published() {
        // `publish` held the subscriber read lock across the handler calls, so
        // a handler that subscribes (a write) on the same bus waited for a lock
        // its own caller held. Run it on a thread so a regression fails the
        // test instead of hanging it.
        let bus = Arc::new(EventBus::new());
        let seen = Arc::new(AtomicUsize::new(0));
        {
            let (bus_inner, seen_inner) = (bus.clone(), seen.clone());
            bus.subscribe(move |_| {
                let seen_late = seen_inner.clone();
                bus_inner.subscribe(move |_| {
                    seen_late.fetch_add(1, Ordering::SeqCst);
                });
            });
        }
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let publisher = bus.clone();
        std::thread::spawn(move || {
            publisher.publish(FissionEvent::AppStarted);
            let _ = done_tx.send(());
        });
        assert!(
            done_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .is_ok(),
            "publish deadlocked on a handler that subscribes"
        );
        // The late subscriber is there for the next event.
        bus.publish(FissionEvent::AppStarted);
        assert!(seen.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn every_subscriber_still_receives_each_event() {
        // Positive control: the snapshot taken for the fix still reaches all of
        // them, in one publish.
        let bus = EventBus::new();
        let hits = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            let hits = hits.clone();
            bus.subscribe(move |_| {
                hits.fetch_add(1, Ordering::SeqCst);
            });
        }
        bus.publish(FissionEvent::AppStarted);
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn scoped_unsubscribe_owner_removes_only_that_owners_subscriptions() {
        let bus = EventBus::new();
        let host_hits = Arc::new(AtomicUsize::new(0));
        let alpha_hits = Arc::new(AtomicUsize::new(0));
        let beta_hits = Arc::new(AtomicUsize::new(0));

        let host_c = host_hits.clone();
        bus.subscribe(move |_| {
            host_c.fetch_add(1, Ordering::SeqCst);
        });
        let alpha_c = alpha_hits.clone();
        bus.scoped("alpha").subscribe(move |_| {
            alpha_c.fetch_add(1, Ordering::SeqCst);
        });
        let beta_c = beta_hits.clone();
        bus.scoped("beta").subscribe(move |_| {
            beta_c.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(bus.unsubscribe_owner("alpha"), 1);
        assert_eq!(bus.unsubscribe_owner("alpha"), 0);
        bus.publish(FissionEvent::AppStarted);

        assert_eq!(host_hits.load(Ordering::SeqCst), 1);
        assert_eq!(alpha_hits.load(Ordering::SeqCst), 0);
        assert_eq!(beta_hits.load(Ordering::SeqCst), 1);
    }
}
