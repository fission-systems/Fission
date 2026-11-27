//! Synchronous event bus with deterministic snapshot dispatch and panic isolation.
//!
//! # Dispatch & Snapshot Semantics
//!
//! - **Per-publish snapshot**: Each call to [`EventBus::publish_with_policy`] snapshots the active
//!   subscribers while holding a brief read lock, releases the lock immediately, sorts the snapshot
//!   by `(priority, registration_id)`, and only then invokes callbacks. Subscribers are never called
//!   while holding registry locks, so handlers may freely `subscribe`, `unsubscribe`, or
//!   `unsubscribe_owner` without deadlocking.
//! - **In-flight mutations**: Subscribers registered *during* an ongoing `publish` are not part of
//!   that publish's snapshot (they see subsequent events). Subscribers unregistered *during*
//!   delivery (via `unsubscribe` or `unsubscribe_owner`) have their `active` flag cleared before
//!   their turn is reached and are recorded as [`SubscriberOutcome::SkippedUnsubscribed`].
//! - **Concurrency scope**: Ordering `(priority asc, registration_id asc)` is deterministic within
//!   each individual publish snapshot. Concurrent publishers on different threads execute their own
//!   snapshots without imposing a spurious global total order across threads.
//! - **Failure & Unwind Isolation**: Under [`FailurePolicy::ContinueOnUnwind`] (default) or
//!   [`FailurePolicy::StopOnFirstFailure`], recoverable Rust panics unwinding across safe Rust
//!   subscriber closures are caught with `std::panic::catch_unwind` and reported in
//!   [`EventDeliveryReport`]. Note that `catch_unwind` cannot intercept `panic = "abort"`, OS
//!   signals, or undefined behavior in unsafe/FFI native plugin code (handled separately by
//!   external plugin worker isolation). Critical transactional subscribers marked with
//!   [`SubscriptionCriticality::Critical`] stop subsequent delivery and propagate their panic when
//!   `propagate_critical_panics` is enabled.
//! - **Bounded reentrant publish**: Nested `publish` calls from within a subscriber on the same
//!   thread targeting the same bus are either queued breadth-first into the active outer wave
//!   ([`ReentrantPolicy::QueueBounded`], default max 64) or bounded in recursion depth
//!   ([`ReentrantPolicy::DirectBounded`]), using an unwind-safe RAII frame guard.

use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

/// Dispatch priority on [`EventBus`]; lower numeric values execute first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum EventPriority {
    High = 0,
    #[default]
    Normal = 50,
    Low = 100,
}

/// Failure sensitivity of a subscriber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubscriptionCriticality {
    /// Failure does not trigger critical unwind propagation.
    #[default]
    Independent,
    /// Failure stops remaining delivery and unwinds when `propagate_critical_panics` is true.
    Critical,
}

/// Policy governing recoverable Rust unwinds at safe subscriber boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailurePolicy {
    /// Catch recoverable panics, record [`SubscriberOutcome::Panicked`], and continue to remaining subscribers.
    #[default]
    ContinueOnUnwind,
    /// Catch recoverable panics, record failure, and mark remaining subscribers [`SubscriberOutcome::SkippedAfterFailure`].
    StopOnFirstFailure,
    /// Do not catch panics; allow them to unwind directly to the caller.
    PropagatePanic,
}

/// Policy governing same-thread, same-bus reentrant `publish` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReentrantPolicy {
    /// Queue nested events up to `max_queued` for the active outer dispatch wave to drain FIFO.
    QueueBounded { max_queued: usize },
    /// Execute nested events inline up to `max_depth` nested calls before rejecting.
    DirectBounded { max_depth: usize },
}

impl Default for ReentrantPolicy {
    fn default() -> Self {
        Self::QueueBounded { max_queued: 64 }
    }
}

/// Combined dispatch policy for [`EventBus::publish_with_policy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchPolicy {
    pub failure_policy: FailurePolicy,
    pub reentrant_policy: ReentrantPolicy,
    pub propagate_critical_panics: bool,
}

impl Default for DispatchPolicy {
    fn default() -> Self {
        Self {
            failure_policy: FailurePolicy::ContinueOnUnwind,
            reentrant_policy: ReentrantPolicy::default(),
            propagate_critical_panics: true,
        }
    }
}

/// Outcome of delivering one event to a single subscriber/hook/plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriberOutcome {
    Delivered,
    Panicked { message: String },
    SkippedUnsubscribed,
    SkippedAfterFailure,
}

/// Per-subscriber outcome entry in an [`EventDeliveryReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriberDeliveryRecord {
    pub subscription_id: u64,
    pub priority: EventPriority,
    pub owner: Option<String>,
    pub criticality: SubscriptionCriticality,
    pub outcome: SubscriberOutcome,
}

/// How a publish call interacted with same-thread reentrancy controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReentrantStatus {
    ExecutedInline { depth: usize },
    QueuedForOuterWave,
    RejectedQueueFull,
    RejectedDepthExceeded,
}

/// Typed delivery diagnostics returned by [`EventBus::publish_checked`] and [`EventBus::publish_with_policy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDeliveryReport {
    pub event_type: FissionEventType,
    pub reentrant_status: ReentrantStatus,
    pub records: Vec<SubscriberDeliveryRecord>,
    pub reentrant_drained: usize,
    pub reentrant_rejected: usize,
    pub drained_failures: usize,
    pub complete: bool,
}

pub(crate) fn format_panic_payload(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

type EventHandler = Arc<dyn Fn(&FissionEvent) + Send + Sync>;

#[derive(Clone)]
struct SubscriptionEntry {
    id: u64,
    priority: EventPriority,
    criticality: SubscriptionCriticality,
    owner: Option<String>,
    active: Arc<AtomicBool>,
    handler: EventHandler,
}

static NEXT_BUS_ID: AtomicU64 = AtomicU64::new(1);

struct EventBusInner {
    bus_id: u64,
    subscribers: RwLock<BTreeMap<u64, SubscriptionEntry>>,
    next_id: RwLock<u64>,
}

#[derive(Default)]
struct ThreadBusDispatch {
    depth: usize,
    wave_enqueued: usize,
    wave_rejected: usize,
    queue: VecDeque<(FissionEvent, DispatchPolicy)>,
}

thread_local! {
    static DISPATCH_STATE: RefCell<HashMap<u64, ThreadBusDispatch>> = RefCell::new(HashMap::new());
}

struct DispatchFrameGuard {
    bus_id: u64,
}

impl Drop for DispatchFrameGuard {
    fn drop(&mut self) {
        DISPATCH_STATE.with(|state| {
            let mut map = state.borrow_mut();
            let remove_entry = if let Some(entry) = map.get_mut(&self.bus_id) {
                entry.depth = entry.depth.saturating_sub(1);
                entry.depth == 0
            } else {
                false
            };
            if remove_entry {
                map.remove(&self.bus_id);
            }
        });
    }
}

pub struct EventBus {
    inner: Arc<EventBusInner>,
    owner: Option<String>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(EventBusInner {
                bus_id: NEXT_BUS_ID.fetch_add(1, Ordering::Relaxed),
                subscribers: RwLock::new(BTreeMap::new()),
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
        self.subscribe_with_options(
            EventPriority::Normal,
            SubscriptionCriticality::Independent,
            handler,
        )
    }

    pub fn subscribe_with_priority<F>(&self, priority: EventPriority, handler: F) -> u64
    where
        F: Fn(&FissionEvent) + Send + Sync + 'static,
    {
        self.subscribe_with_options(priority, SubscriptionCriticality::Independent, handler)
    }

    pub fn subscribe_with_options<F>(
        &self,
        priority: EventPriority,
        criticality: SubscriptionCriticality,
        handler: F,
    ) -> u64
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

        let entry = SubscriptionEntry {
            id,
            priority,
            criticality,
            owner: self.owner.clone(),
            active: Arc::new(AtomicBool::new(true)),
            handler: Arc::new(handler),
        };

        let mut subs = self
            .inner
            .subscribers
            .write()
            .unwrap_or_else(|e| e.into_inner());
        subs.insert(id, entry);

        id
    }

    pub fn unsubscribe(&self, id: u64) {
        let mut subs = self
            .inner
            .subscribers
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = subs.remove(&id) {
            entry.active.store(false, Ordering::Release);
        }
    }

    /// Remove every subscription registered through a scoped handle for `owner`.
    pub fn unsubscribe_owner(&self, owner: &str) -> usize {
        let mut subs = self
            .inner
            .subscribers
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let before = subs.len();
        subs.retain(|_, entry| {
            if entry.owner.as_deref() == Some(owner) {
                entry.active.store(false, Ordering::Release);
                false
            } else {
                true
            }
        });
        before - subs.len()
    }

    /// Publish an event with the default policy (deterministic order, recoverable panic isolation,
    /// bounded reentrant queue), discarding the detailed delivery report.
    pub fn publish(&self, event: FissionEvent) {
        let _ = self.publish_with_policy(event, DispatchPolicy::default());
    }

    /// Publish an event with the default policy and return its typed delivery report.
    pub fn publish_checked(&self, event: FissionEvent) -> EventDeliveryReport {
        self.publish_with_policy(event, DispatchPolicy::default())
    }

    /// Publish an event under an explicit [`DispatchPolicy`].
    pub fn publish_with_policy(
        &self,
        event: FissionEvent,
        policy: DispatchPolicy,
    ) -> EventDeliveryReport {
        let bus_id = self.inner.bus_id;
        let event_type = event.event_type();

        // Check whether this thread already has an active dispatch frame for `bus_id`.
        enum EntryDecision {
            StartOuter,
            RunInline { depth: usize },
            ReturnEarly(EventDeliveryReport),
        }

        let decision = DISPATCH_STATE.with(|state| {
            let mut map = state.borrow_mut();
            let entry = map.entry(bus_id).or_default();
            if entry.depth == 0 {
                entry.depth = 1;
                entry.wave_enqueued = 0;
                entry.wave_rejected = 0;
                entry.queue.clear();
                EntryDecision::StartOuter
            } else {
                match policy.reentrant_policy {
                    ReentrantPolicy::QueueBounded { max_queued } => {
                        if entry.wave_enqueued < max_queued {
                            entry.wave_enqueued += 1;
                            entry.queue.push_back((event.clone(), policy));
                            EntryDecision::ReturnEarly(EventDeliveryReport {
                                event_type,
                                reentrant_status: ReentrantStatus::QueuedForOuterWave,
                                records: Vec::new(),
                                reentrant_drained: 0,
                                reentrant_rejected: 0,
                                drained_failures: 0,
                                complete: true,
                            })
                        } else {
                            entry.wave_rejected += 1;
                            EntryDecision::ReturnEarly(EventDeliveryReport {
                                event_type,
                                reentrant_status: ReentrantStatus::RejectedQueueFull,
                                records: Vec::new(),
                                reentrant_drained: 0,
                                reentrant_rejected: 1,
                                drained_failures: 0,
                                complete: false,
                            })
                        }
                    }
                    ReentrantPolicy::DirectBounded { max_depth } => {
                        if entry.depth >= max_depth {
                            entry.wave_rejected += 1;
                            EntryDecision::ReturnEarly(EventDeliveryReport {
                                event_type,
                                reentrant_status: ReentrantStatus::RejectedDepthExceeded,
                                records: Vec::new(),
                                reentrant_drained: 0,
                                reentrant_rejected: 1,
                                drained_failures: 0,
                                complete: false,
                            })
                        } else {
                            let inline_depth = entry.depth;
                            entry.depth += 1;
                            EntryDecision::RunInline {
                                depth: inline_depth,
                            }
                        }
                    }
                }
            }
        });

        match decision {
            EntryDecision::ReturnEarly(report) => report,
            EntryDecision::RunInline { depth } => {
                let _guard = DispatchFrameGuard { bus_id };
                let (records, direct_ok) = self.deliver_snapshot(&event, policy);
                EventDeliveryReport {
                    event_type,
                    reentrant_status: ReentrantStatus::ExecutedInline { depth },
                    records,
                    reentrant_drained: 0,
                    reentrant_rejected: 0,
                    drained_failures: 0,
                    complete: direct_ok,
                }
            }
            EntryDecision::StartOuter => {
                let _guard = DispatchFrameGuard { bus_id };
                let (records, mut wave_ok) = self.deliver_snapshot(&event, policy);

                let mut reentrant_drained = 0usize;
                let mut drained_failures = 0usize;
                loop {
                    let next = DISPATCH_STATE.with(|state| {
                        state
                            .borrow_mut()
                            .get_mut(&bus_id)
                            .and_then(|d| d.queue.pop_front())
                    });
                    let Some((queued_event, queued_policy)) = next else {
                        break;
                    };
                    reentrant_drained += 1;
                    let (_, drained_ok) = self.deliver_snapshot(&queued_event, queued_policy);
                    if !drained_ok {
                        drained_failures += 1;
                        wave_ok = false;
                    }
                }

                let reentrant_rejected = DISPATCH_STATE.with(|state| {
                    state
                        .borrow()
                        .get(&bus_id)
                        .map(|d| d.wave_rejected)
                        .unwrap_or(0)
                });
                if reentrant_rejected > 0 {
                    wave_ok = false;
                }

                EventDeliveryReport {
                    event_type,
                    reentrant_status: ReentrantStatus::ExecutedInline { depth: 0 },
                    records,
                    reentrant_drained,
                    reentrant_rejected,
                    drained_failures,
                    complete: wave_ok,
                }
            }
        }
    }

    fn deliver_snapshot(
        &self,
        event: &FissionEvent,
        policy: DispatchPolicy,
    ) -> (Vec<SubscriberDeliveryRecord>, bool) {
        // Snapshot handlers and release the lock before calling any callback.
        let mut snapshot: Vec<SubscriptionEntry> = {
            let subs = self
                .inner
                .subscribers
                .read()
                .unwrap_or_else(|e| e.into_inner());
            subs.values().cloned().collect()
        };
        snapshot.sort_by_key(|entry| (entry.priority, entry.id));

        let mut records = Vec::with_capacity(snapshot.len());
        let mut complete = true;
        let mut stop_remaining = false;

        for entry in snapshot {
            if !entry.active.load(Ordering::Acquire) {
                records.push(SubscriberDeliveryRecord {
                    subscription_id: entry.id,
                    priority: entry.priority,
                    owner: entry.owner,
                    criticality: entry.criticality,
                    outcome: SubscriberOutcome::SkippedUnsubscribed,
                });
                continue;
            }
            if stop_remaining {
                records.push(SubscriberDeliveryRecord {
                    subscription_id: entry.id,
                    priority: entry.priority,
                    owner: entry.owner,
                    criticality: entry.criticality,
                    outcome: SubscriberOutcome::SkippedAfterFailure,
                });
                continue;
            }

            if policy.failure_policy == FailurePolicy::PropagatePanic {
                (entry.handler)(event);
                records.push(SubscriberDeliveryRecord {
                    subscription_id: entry.id,
                    priority: entry.priority,
                    owner: entry.owner,
                    criticality: entry.criticality,
                    outcome: SubscriberOutcome::Delivered,
                });
                continue;
            }

            match catch_unwind(AssertUnwindSafe(|| (entry.handler)(event))) {
                Ok(()) => {
                    records.push(SubscriberDeliveryRecord {
                        subscription_id: entry.id,
                        priority: entry.priority,
                        owner: entry.owner,
                        criticality: entry.criticality,
                        outcome: SubscriberOutcome::Delivered,
                    });
                }
                Err(payload) => {
                    if entry.criticality == SubscriptionCriticality::Critical
                        && policy.propagate_critical_panics
                    {
                        resume_unwind(payload);
                    }
                    complete = false;
                    let message = format_panic_payload(&*payload);
                    records.push(SubscriberDeliveryRecord {
                        subscription_id: entry.id,
                        priority: entry.priority,
                        owner: entry.owner,
                        criticality: entry.criticality,
                        outcome: SubscriberOutcome::Panicked { message },
                    });
                    if policy.failure_policy == FailurePolicy::StopOnFirstFailure
                        || entry.criticality == SubscriptionCriticality::Critical
                    {
                        stop_remaining = true;
                    }
                }
            }
        }

        (records, complete)
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
