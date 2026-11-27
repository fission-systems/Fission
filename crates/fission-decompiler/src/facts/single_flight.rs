//! Panic-safe single-flight + LRU cache coordination for callee summaries.
//!
//! Guarantees:
//! - Lookup, in-flight admission, and cache publication share a single short-held
//!   mutex so a concurrent caller never misses between in-flight removal and
//!   cache insertion.
//! - Computation, decoding, Rayon work, and waits happen outside that mutex.
//! - Each owner acquisition carries a monotonic `generation` protected by an RAII
//!   drop guard, so unwinding/panics always mark the flight `Abandoned`, wake
//!   waiters promptly, never cache partial results, and stale cleanups cannot
//!   remove a newer generation's flight entry.
//! - [`ExecutionMode::NonBlockingPool`] keeps Rayon pool workers non-blocking
//!   (never waiting on another caller's `Condvar`) while still counting
//!   duplicate work and cache contention.

use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Execution policy selecting whether a miss enters single-flight waiting or
/// computes independently to avoid starving a bounded thread pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Deduplicate misses via an in-flight owner and wait for its completion.
    SingleFlight,
    /// Never block on an in-flight computation (used inside Rayon worker pools
    /// whose threads the active computation may itself need).
    NonBlockingPool,
}

/// Failure modes surfaced to single-flight callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SingleFlightError {
    /// The computation explicitly failed with a diagnostic message.
    Failed(String),
    /// The owning thread unwound (panicked) before completing the flight.
    OwnerAbandoned,
    /// The caller or initializer requested cancellation.
    Cancelled,
    /// A per-flight state lock was poisoned while waiting for completion.
    FlightLockPoisoned,
}

impl std::fmt::Display for SingleFlightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(msg) => write!(f, "single-flight computation failed: {msg}"),
            Self::OwnerAbandoned => write!(f, "single-flight owner abandoned flight"),
            Self::Cancelled => write!(f, "single-flight operation cancelled"),
            Self::FlightLockPoisoned => write!(f, "single-flight waiter state lock poisoned"),
        }
    }
}

impl std::error::Error for SingleFlightError {}

/// Internal lifecycle state of one key's active or finished flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlightState<V> {
    Pending,
    Ready(V),
    Failed(String),
    Cancelled,
    Abandoned,
}

struct Flight<V> {
    generation: u64,
    state: Mutex<FlightState<V>>,
    cv: Condvar,
}

impl<V> Flight<V> {
    fn new(generation: u64) -> Self {
        Self {
            generation,
            state: Mutex::new(FlightState::Pending),
            cv: Condvar::new(),
        }
    }
}

struct CacheState<K, V> {
    cache: LruCache<K, V>,
    in_flight: HashMap<K, Arc<Flight<V>>>,
    next_generation: u64,
}

/// Measured contention, deduplication, failure, and lock-hold counters for a
/// [`SingleFlightCache`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SingleFlightMetricsSnapshot {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub single_flight_owner_computes: u64,
    pub single_flight_waits: u64,
    pub rayon_nonblocking_computes: u64,
    pub duplicate_computes_during_in_flight: u64,
    pub abandoned_flights: u64,
    pub failed_flights: u64,
    pub cancelled_waits: u64,
    pub poison_recoveries: u64,
    pub in_flight_current: usize,
    pub in_flight_peak: usize,
    pub total_wait_ns: u64,
    pub max_wait_ns: u64,
    pub total_lock_hold_ns: u64,
    pub max_lock_hold_ns: u64,
}

#[derive(Default)]
struct Metrics {
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    single_flight_owner_computes: AtomicU64,
    single_flight_waits: AtomicU64,
    rayon_nonblocking_computes: AtomicU64,
    duplicate_computes_during_in_flight: AtomicU64,
    abandoned_flights: AtomicU64,
    failed_flights: AtomicU64,
    cancelled_waits: AtomicU64,
    poison_recoveries: AtomicU64,
    in_flight_peak: AtomicUsize,
    total_wait_ns: AtomicU64,
    max_wait_ns: AtomicU64,
    total_lock_hold_ns: AtomicU64,
    max_lock_hold_ns: AtomicU64,
}

impl Metrics {
    fn observe_max(slot: &AtomicU64, value: u64) {
        let mut cur = slot.load(Ordering::Relaxed);
        while value > cur {
            match slot.compare_exchange_weak(cur, value, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }

    fn observe_peak_in_flight(&self, count: usize) {
        let mut cur = self.in_flight_peak.load(Ordering::Relaxed);
        while count > cur {
            match self.in_flight_peak.compare_exchange_weak(
                cur,
                count,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }

    fn record_lock_hold(&self, elapsed: Duration) {
        let ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        self.total_lock_hold_ns.fetch_add(ns, Ordering::Relaxed);
        Self::observe_max(&self.max_lock_hold_ns, ns);
    }

    fn record_wait(&self, elapsed: Duration) {
        let ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        self.total_wait_ns.fetch_add(ns, Ordering::Relaxed);
        Self::observe_max(&self.max_wait_ns, ns);
    }
}

enum LookupOutcome<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    Hit(V),
    Owner(OwnerGuard<K, V>),
    Waiter(Arc<Flight<V>>),
    NonBlockingCompute,
}

struct OwnerGuard<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    state: Arc<Mutex<CacheState<K, V>>>,
    metrics: Arc<Metrics>,
    key: K,
    flight: Arc<Flight<V>>,
    completed: bool,
}

impl<K, V> OwnerGuard<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn complete_ready(mut self, value: V) {
        self.completed = true;
        finish_owner_flight(
            &self.state,
            &self.metrics,
            &self.key,
            &self.flight,
            Some(value.clone()),
            FlightState::Ready(value),
        );
    }

    fn complete_failed(mut self, reason: String) {
        self.completed = true;
        self.metrics.failed_flights.fetch_add(1, Ordering::Relaxed);
        finish_owner_flight(
            &self.state,
            &self.metrics,
            &self.key,
            &self.flight,
            None,
            FlightState::Failed(reason),
        );
    }

    fn complete_cancelled(mut self) {
        self.completed = true;
        finish_owner_flight(
            &self.state,
            &self.metrics,
            &self.key,
            &self.flight,
            None,
            FlightState::Cancelled,
        );
    }
}

impl<K, V> Drop for OwnerGuard<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn drop(&mut self) {
        if !self.completed {
            self.completed = true;
            self.metrics
                .abandoned_flights
                .fetch_add(1, Ordering::Relaxed);
            finish_owner_flight(
                &self.state,
                &self.metrics,
                &self.key,
                &self.flight,
                None,
                FlightState::Abandoned,
            );
        }
    }
}

fn with_recovered_state_lock<K, V, R>(
    state_lock: &Mutex<CacheState<K, V>>,
    metrics: &Metrics,
    f: impl FnOnce(&mut CacheState<K, V>) -> R,
) -> R
where
    K: Eq + Hash,
{
    let mut orphaned_flights: Vec<Arc<Flight<V>>> = Vec::new();
    let result = {
        let hold_start = Instant::now();
        let out = match state_lock.lock() {
            Ok(mut guard) => f(&mut guard),
            Err(poisoned) => {
                // Only disposable LRU/in-flight bookkeeping lives under this lock:
                // clear cached entries cleanly, detach in-flight entries so waiters
                // are woken outside the lock, and clear poison.
                let mut guard = poisoned.into_inner();
                guard.cache.clear();
                orphaned_flights.extend(guard.in_flight.drain().map(|(_, flight)| flight));
                metrics.poison_recoveries.fetch_add(1, Ordering::Relaxed);
                state_lock.clear_poison();
                f(&mut guard)
            }
        };
        metrics.record_lock_hold(hold_start.elapsed());
        out
    };
    if !orphaned_flights.is_empty() {
        metrics
            .abandoned_flights
            .fetch_add(orphaned_flights.len() as u64, Ordering::Relaxed);
        for flight in orphaned_flights {
            let mut flight_guard = match flight.state.lock() {
                Ok(g) => g,
                Err(poisoned) => {
                    let g = poisoned.into_inner();
                    flight.state.clear_poison();
                    g
                }
            };
            if matches!(*flight_guard, FlightState::Pending) {
                *flight_guard = FlightState::Abandoned;
            }
            flight.cv.notify_all();
        }
    }
    result
}

fn finish_owner_flight<K, V>(
    state_lock: &Mutex<CacheState<K, V>>,
    metrics: &Metrics,
    key: &K,
    flight: &Arc<Flight<V>>,
    cache_value: Option<V>,
    terminal_state: FlightState<V>,
) where
    K: Eq + Hash + Clone,
{
    with_recovered_state_lock(state_lock, metrics, |state| {
        if let Some(value) = cache_value {
            state.cache.put(key.clone(), value);
        }
        // Only remove the entry if it still belongs to this exact generation,
        // preventing delayed/stale owner cleanup from deleting a newer flight.
        let same_generation = state
            .in_flight
            .get(key)
            .is_some_and(|cur| cur.generation == flight.generation);
        if same_generation {
            state.in_flight.remove(key);
        }
    });

    // Notify waiters strictly outside `state_lock` to preserve lock ordering.
    let mut flight_guard = match flight.state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            let guard = poisoned.into_inner();
            flight.state.clear_poison();
            guard
        }
    };
    *flight_guard = terminal_state;
    flight.cv.notify_all();
}

/// Bounded LRU cache paired with panic-safe per-key single-flight coordination
/// and contention telemetry.
pub struct SingleFlightCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    state: Arc<Mutex<CacheState<K, V>>>,
    metrics: Arc<Metrics>,
}

impl<K, V> SingleFlightCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            state: Arc::new(Mutex::new(CacheState {
                cache: LruCache::new(capacity),
                in_flight: HashMap::new(),
                next_generation: 1,
            })),
            metrics: Arc::new(Metrics::default()),
        }
    }

    pub fn get_or_try_compute<F>(
        &self,
        key: K,
        mode: ExecutionMode,
        compute: F,
    ) -> Result<V, SingleFlightError>
    where
        F: FnOnce() -> Result<V, SingleFlightError>,
    {
        self.get_or_try_compute_cancellable(key, mode, || false, compute)
    }

    pub fn get_or_try_compute_cancellable<F, C>(
        &self,
        key: K,
        mode: ExecutionMode,
        is_cancelled: C,
        compute: F,
    ) -> Result<V, SingleFlightError>
    where
        F: FnOnce() -> Result<V, SingleFlightError>,
        C: Fn() -> bool,
    {
        if is_cancelled() {
            self.metrics.cancelled_waits.fetch_add(1, Ordering::Relaxed);
            return Err(SingleFlightError::Cancelled);
        }

        let lookup = with_recovered_state_lock(&self.state, &self.metrics, |state| {
            if let Some(hit) = state.cache.get(&key).cloned() {
                self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
                return LookupOutcome::Hit(hit);
            }
            self.metrics.cache_misses.fetch_add(1, Ordering::Relaxed);

            match mode {
                ExecutionMode::NonBlockingPool => {
                    self.metrics
                        .rayon_nonblocking_computes
                        .fetch_add(1, Ordering::Relaxed);
                    if state.in_flight.contains_key(&key) {
                        self.metrics
                            .duplicate_computes_during_in_flight
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    LookupOutcome::NonBlockingCompute
                }
                ExecutionMode::SingleFlight => {
                    if let Some(existing) = state.in_flight.get(&key) {
                        self.metrics
                            .single_flight_waits
                            .fetch_add(1, Ordering::Relaxed);
                        LookupOutcome::Waiter(Arc::clone(existing))
                    } else {
                        let generation = state.next_generation.max(1);
                        state.next_generation = generation.wrapping_add(1).max(1);
                        let flight = Arc::new(Flight::new(generation));
                        state.in_flight.insert(key.clone(), Arc::clone(&flight));
                        self.metrics
                            .single_flight_owner_computes
                            .fetch_add(1, Ordering::Relaxed);
                        self.metrics.observe_peak_in_flight(state.in_flight.len());
                        LookupOutcome::Owner(OwnerGuard {
                            state: Arc::clone(&self.state),
                            metrics: Arc::clone(&self.metrics),
                            key: key.clone(),
                            flight,
                            completed: false,
                        })
                    }
                }
            }
        });

        match lookup {
            LookupOutcome::Hit(hit) => Ok(hit),
            LookupOutcome::NonBlockingCompute => {
                if is_cancelled() {
                    self.metrics.cancelled_waits.fetch_add(1, Ordering::Relaxed);
                    return Err(SingleFlightError::Cancelled);
                }
                let value = compute()?;
                with_recovered_state_lock(&self.state, &self.metrics, |state| {
                    state.cache.put(key, value.clone());
                });
                Ok(value)
            }
            LookupOutcome::Owner(guard) => {
                if is_cancelled() {
                    self.metrics.cancelled_waits.fetch_add(1, Ordering::Relaxed);
                    guard.complete_cancelled();
                    return Err(SingleFlightError::Cancelled);
                }
                match compute() {
                    Ok(value) => {
                        guard.complete_ready(value.clone());
                        Ok(value)
                    }
                    Err(SingleFlightError::Cancelled) => {
                        guard.complete_cancelled();
                        Err(SingleFlightError::Cancelled)
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        guard.complete_failed(msg);
                        Err(err)
                    }
                }
            }
            LookupOutcome::Waiter(flight) => self.wait_for_flight(&flight, &is_cancelled),
        }
    }

    fn wait_for_flight<C>(
        &self,
        flight: &Arc<Flight<V>>,
        is_cancelled: &C,
    ) -> Result<V, SingleFlightError>
    where
        C: Fn() -> bool,
    {
        let wait_start = Instant::now();
        let mut guard = match flight.state.lock() {
            Ok(guard) => guard,
            Err(_) => {
                self.metrics.record_wait(wait_start.elapsed());
                return Err(SingleFlightError::FlightLockPoisoned);
            }
        };

        loop {
            match &*guard {
                FlightState::Ready(val) => {
                    self.metrics.record_wait(wait_start.elapsed());
                    return Ok(val.clone());
                }
                FlightState::Failed(msg) => {
                    self.metrics.record_wait(wait_start.elapsed());
                    return Err(SingleFlightError::Failed(msg.clone()));
                }
                FlightState::Cancelled => {
                    self.metrics.record_wait(wait_start.elapsed());
                    return Err(SingleFlightError::Cancelled);
                }
                FlightState::Abandoned => {
                    self.metrics.record_wait(wait_start.elapsed());
                    return Err(SingleFlightError::OwnerAbandoned);
                }
                FlightState::Pending => {
                    if is_cancelled() {
                        self.metrics.cancelled_waits.fetch_add(1, Ordering::Relaxed);
                        self.metrics.record_wait(wait_start.elapsed());
                        return Err(SingleFlightError::Cancelled);
                    }
                    match flight.cv.wait_timeout(guard, WAIT_POLL_INTERVAL) {
                        Ok((next_guard, _)) => guard = next_guard,
                        Err(_) => {
                            self.metrics.record_wait(wait_start.elapsed());
                            return Err(SingleFlightError::FlightLockPoisoned);
                        }
                    }
                }
            }
        }
    }

    pub fn in_flight_len(&self) -> usize {
        with_recovered_state_lock(&self.state, &self.metrics, |state| state.in_flight.len())
    }

    pub fn cache_len(&self) -> usize {
        with_recovered_state_lock(&self.state, &self.metrics, |state| state.cache.len())
    }

    pub fn metrics_snapshot(&self) -> SingleFlightMetricsSnapshot {
        let in_flight_current =
            with_recovered_state_lock(&self.state, &self.metrics, |state| state.in_flight.len());
        SingleFlightMetricsSnapshot {
            cache_hits: self.metrics.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.metrics.cache_misses.load(Ordering::Relaxed),
            single_flight_owner_computes: self
                .metrics
                .single_flight_owner_computes
                .load(Ordering::Relaxed),
            single_flight_waits: self.metrics.single_flight_waits.load(Ordering::Relaxed),
            rayon_nonblocking_computes: self
                .metrics
                .rayon_nonblocking_computes
                .load(Ordering::Relaxed),
            duplicate_computes_during_in_flight: self
                .metrics
                .duplicate_computes_during_in_flight
                .load(Ordering::Relaxed),
            abandoned_flights: self.metrics.abandoned_flights.load(Ordering::Relaxed),
            failed_flights: self.metrics.failed_flights.load(Ordering::Relaxed),
            cancelled_waits: self.metrics.cancelled_waits.load(Ordering::Relaxed),
            poison_recoveries: self.metrics.poison_recoveries.load(Ordering::Relaxed),
            in_flight_current,
            in_flight_peak: self.metrics.in_flight_peak.load(Ordering::Relaxed),
            total_wait_ns: self.metrics.total_wait_ns.load(Ordering::Relaxed),
            max_wait_ns: self.metrics.max_wait_ns.load(Ordering::Relaxed),
            total_lock_hold_ns: self.metrics.total_lock_hold_ns.load(Ordering::Relaxed),
            max_lock_hold_ns: self.metrics.max_lock_hold_ns.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poisoned_state_lock_clears_disposable_cache_and_abandons_in_flight_waiters() {
        let cache: Arc<SingleFlightCache<&'static str, u64>> =
            Arc::new(SingleFlightCache::new(NonZeroUsize::new(8).unwrap()));

        // Seed a cached entry and an artificial pending flight under an older generation.
        assert_eq!(
            cache
                .get_or_try_compute("cached", ExecutionMode::SingleFlight, || Ok(11))
                .unwrap(),
            11
        );
        let orphan_flight = Arc::new(Flight::new(999));
        {
            let mut guard = cache.state.lock().unwrap();
            guard.in_flight.insert("orphan", Arc::clone(&orphan_flight));
        }

        // Poison `cache.state`.
        let poison_target = Arc::clone(&cache);
        let _ = std::thread::spawn(move || {
            let _guard = poison_target.state.lock().unwrap();
            panic!("simulated mutex poison");
        })
        .join();
        assert!(cache.state.is_poisoned());

        // Subsequent operations recover by resetting disposable cache state,
        // marking orphaned flights Abandoned, and never leaving poisoned state.
        let value = cache
            .get_or_try_compute("fresh", ExecutionMode::SingleFlight, || Ok(42))
            .expect("poisoned disposable cache recovers cleanly");
        assert_eq!(value, 42);
        assert!(!cache.state.is_poisoned());
        assert_eq!(cache.in_flight_len(), 0);
        assert!(matches!(
            *orphan_flight.state.lock().unwrap(),
            FlightState::Abandoned
        ));
        let snap = cache.metrics_snapshot();
        assert_eq!(snap.poison_recoveries, 1);
        assert_eq!(snap.abandoned_flights, 1);
    }

    #[test]
    fn stale_generation_cleanup_never_removes_a_newer_flight() {
        let cache: SingleFlightCache<&'static str, u64> =
            SingleFlightCache::new(NonZeroUsize::new(8).unwrap());
        let stale_flight = Arc::new(Flight::new(1));
        let newer_flight = Arc::new(Flight::new(2));

        {
            let mut guard = cache.state.lock().unwrap();
            guard.in_flight.insert("k", Arc::clone(&newer_flight));
        }

        finish_owner_flight(
            &cache.state,
            &cache.metrics,
            &"k",
            &stale_flight,
            None,
            FlightState::Abandoned,
        );

        let guard = cache.state.lock().unwrap();
        let active = guard
            .in_flight
            .get("k")
            .expect("newer generation must stay");
        assert_eq!(active.generation, 2);
    }
}
