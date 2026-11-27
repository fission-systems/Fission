//! Barrier-driven concurrency, unwind-safety, eviction, cancellation, and
//! nested-Rayon saturation tests for issue #1289.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::time::Duration;

use fission_decompiler::facts::{ExecutionMode, SingleFlightCache, SingleFlightError};
use rayon::prelude::*;

fn sequential_oracle(key: u64) -> u64 {
    key.wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

#[test]
fn same_key_storm_runs_owner_once_and_matches_sequential_oracle() {
    const WORKERS: usize = 8;
    let cache = Arc::new(SingleFlightCache::new(NonZeroUsize::new(64).unwrap()));
    let start_barrier = Arc::new(Barrier::new(WORKERS));
    let compute_calls = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..WORKERS)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let barrier = Arc::clone(&start_barrier);
            let calls = Arc::clone(&compute_calls);
            std::thread::spawn(move || {
                barrier.wait();
                cache
                    .get_or_try_compute(0x401000u64, ExecutionMode::SingleFlight, || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(15));
                        Ok(sequential_oracle(0x401000))
                    })
                    .expect("storm caller result")
            })
        })
        .collect();

    let results: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(results.iter().all(|&v| v == sequential_oracle(0x401000)));
    assert_eq!(compute_calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.in_flight_len(), 0);

    let snap = cache.metrics_snapshot();
    assert_eq!(snap.single_flight_owner_computes, 1);
    assert_eq!(snap.duplicate_computes_during_in_flight, 0);
    assert!(snap.single_flight_waits + snap.cache_hits == (WORKERS - 1) as u64);
}

#[test]
fn unrelated_keys_make_progress_while_another_key_blocks() {
    let cache = Arc::new(SingleFlightCache::new(NonZeroUsize::new(64).unwrap()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();

    let cache_a = Arc::clone(&cache);
    let owner_a = std::thread::spawn(move || {
        cache_a
            .get_or_try_compute("blocked_key", ExecutionMode::SingleFlight, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(101u64)
            })
            .unwrap()
    });

    entered_rx.recv().unwrap();

    // Completely unblocked while `blocked_key` is stalled inside compute().
    let val_b = cache
        .get_or_try_compute("fast_key", ExecutionMode::SingleFlight, || Ok(202u64))
        .unwrap();
    assert_eq!(val_b, 202);
    assert_eq!(cache.in_flight_len(), 1);

    release_tx.send(()).unwrap();
    assert_eq!(owner_a.join().unwrap(), 101);
    assert_eq!(cache.in_flight_len(), 0);
}

#[test]
fn owner_panic_abandons_flight_wakes_waiters_and_allows_clean_recovery() {
    let cache = Arc::new(SingleFlightCache::new(NonZeroUsize::new(32).unwrap()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (panic_now_tx, panic_now_rx) = mpsc::channel();

    let cache_owner = Arc::clone(&cache);
    let owner = std::thread::spawn(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ =
                cache_owner.get_or_try_compute("panic_key", ExecutionMode::SingleFlight, || {
                    entered_tx.send(()).unwrap();
                    panic_now_rx.recv().unwrap();
                    panic!("simulated decoder panic inside single-flight owner");
                });
        }))
    });

    entered_rx.recv().unwrap();

    let mut waiters = Vec::new();
    for _ in 0..3 {
        let cache_w = Arc::clone(&cache);
        waiters.push(std::thread::spawn(move || {
            cache_w.get_or_try_compute("panic_key", ExecutionMode::SingleFlight, || Ok(999u64))
        }));
    }

    // Give waiters a brief moment to enter Condvar wait, then trigger owner unwind.
    std::thread::sleep(Duration::from_millis(20));
    panic_now_tx.send(()).unwrap();

    let unwind_result = owner.join().unwrap();
    assert!(unwind_result.is_err(), "owner must observe panic");

    for w in waiters {
        let outcome: Result<u64, SingleFlightError> = w.join().unwrap();
        assert_eq!(outcome, Err(SingleFlightError::OwnerAbandoned));
    }
    assert_eq!(cache.in_flight_len(), 0);
    assert_eq!(cache.cache_len(), 0);

    // Retry immediately after panic: must compute cleanly and cache result.
    let recovered = cache
        .get_or_try_compute("panic_key", ExecutionMode::SingleFlight, || Ok(777u64))
        .expect("fresh attempt after abandoned panic must succeed");
    assert_eq!(recovered, 777);
    assert_eq!(cache.cache_len(), 1);
    assert_eq!(cache.metrics_snapshot().abandoned_flights, 1);
}

#[test]
fn tight_cache_eviction_and_cancellation_preserve_sound_outcomes() {
    let cache = Arc::new(SingleFlightCache::new(NonZeroUsize::new(2).unwrap()));

    // Concurrent tight LRU capacity test compared against sequential oracle.
    std::thread::scope(|scope| {
        for tid in 0..6u64 {
            let cache = Arc::clone(&cache);
            scope.spawn(move || {
                for key in 0..24u64 {
                    let k = (key + tid) % 8;
                    let got = cache
                        .get_or_try_compute(k, ExecutionMode::SingleFlight, || {
                            Ok(sequential_oracle(k))
                        })
                        .expect("eviction-heavy compute succeeds");
                    assert_eq!(got, sequential_oracle(k));
                }
            });
        }
    });
    assert!(cache.cache_len() <= 2);
    assert_eq!(cache.in_flight_len(), 0);

    // Waiter cancellation while owner continues; non-cancelled waiter still succeeds.
    let (owner_in_tx, owner_in_rx) = mpsc::channel();
    let (owner_go_tx, owner_go_rx) = mpsc::channel();
    let cache_o = Arc::clone(&cache);
    let owner = std::thread::spawn(move || {
        cache_o.get_or_try_compute(99u64, ExecutionMode::SingleFlight, || {
            owner_in_tx.send(()).unwrap();
            owner_go_rx.recv().unwrap();
            Ok(sequential_oracle(99))
        })
    });
    owner_in_rx.recv().unwrap();

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel_flag);
    let cache_w = Arc::clone(&cache);
    let cancelled_waiter = std::thread::spawn(move || {
        cache_w.get_or_try_compute_cancellable(
            99u64,
            ExecutionMode::SingleFlight,
            || cancel_clone.load(Ordering::Acquire),
            || unreachable!("waiter must not execute owner compute"),
        )
    });

    std::thread::sleep(Duration::from_millis(10));
    cancel_flag.store(true, Ordering::Release);
    assert_eq!(
        cancelled_waiter.join().unwrap(),
        Err(SingleFlightError::Cancelled)
    );

    owner_go_tx.send(()).unwrap();
    assert_eq!(owner.join().unwrap(), Ok(sequential_oracle(99)));
    assert_eq!(cache.in_flight_len(), 0);

    // Initializer returning Cancelled must fail cleanly, cache nothing, and recover.
    let init_err = cache.get_or_try_compute(100u64, ExecutionMode::SingleFlight, || {
        Err(SingleFlightError::Cancelled)
    });
    assert_eq!(init_err, Err(SingleFlightError::Cancelled));
    assert_eq!(cache.in_flight_len(), 0);
    let ok = cache
        .get_or_try_compute(100u64, ExecutionMode::SingleFlight, || {
            Ok(sequential_oracle(100))
        })
        .unwrap();
    assert_eq!(ok, sequential_oracle(100));
}

#[test]
fn nested_one_and_two_thread_rayon_pools_do_not_deadlock_and_measure_contention() {
    for threads in [1usize, 2] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("build small rayon pool");
        let cache = Arc::new(SingleFlightCache::new(NonZeroUsize::new(16).unwrap()));

        let (done_tx, done_rx) = mpsc::channel();
        let cache_outer = Arc::clone(&cache);
        std::thread::spawn(move || {
            let results: Vec<u64> = pool.install(|| {
                (0..12u64)
                    .into_par_iter()
                    .map(|idx| {
                        let key = idx % 3;
                        let mode = if rayon::current_thread_index().is_some() {
                            ExecutionMode::NonBlockingPool
                        } else {
                            ExecutionMode::SingleFlight
                        };
                        cache_outer
                            .get_or_try_compute(key, mode, || {
                                // Nested Rayon join/sub-tasks inside the same pool.
                                let (left, right) = rayon::join(
                                    || sequential_oracle(key) & 0xFFFF_FFFF,
                                    || sequential_oracle(key) & !0xFFFF_FFFF,
                                );
                                Ok(left | right)
                            })
                            .expect("nested rayon summary compute succeeds")
                    })
                    .collect()
            });
            done_tx.send(results).unwrap();
        });

        let results = done_rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|e| {
                panic!("nested {threads}-thread Rayon pool stalled/deadlocked: {e}")
            });
        for (idx, val) in results.into_iter().enumerate() {
            assert_eq!(val, sequential_oracle((idx as u64) % 3));
        }

        assert_eq!(cache.in_flight_len(), 0);
        let snap = cache.metrics_snapshot();
        eprintln!(
            "[nested rayon threads={threads}] hits={} misses={} rayon_computes={} dup_in_flight={} total_hold_ns={} max_hold_ns={}",
            snap.cache_hits,
            snap.cache_misses,
            snap.rayon_nonblocking_computes,
            snap.duplicate_computes_during_in_flight,
            snap.total_lock_hold_ns,
            snap.max_lock_hold_ns
        );
        assert!(snap.rayon_nonblocking_computes >= 1);
        assert_eq!(snap.single_flight_waits, 0);
        assert!(snap.total_lock_hold_ns > 0);
    }

    // Verify NonBlockingPool computes independently during an active outer
    // SingleFlight owner (measuring `duplicate_computes_during_in_flight`),
    // without blocking the Rayon worker or removing the owner's flight.
    let cache = Arc::new(SingleFlightCache::new(NonZeroUsize::new(8).unwrap()));
    let (owner_in_tx, owner_in_rx) = mpsc::channel();
    let (owner_release_tx, owner_release_rx) = mpsc::channel();
    let cache_o = Arc::clone(&cache);
    let owner = std::thread::spawn(move || {
        cache_o
            .get_or_try_compute(555u64, ExecutionMode::SingleFlight, || {
                owner_in_tx.send(()).unwrap();
                owner_release_rx.recv().unwrap();
                Ok(sequential_oracle(555))
            })
            .unwrap()
    });
    owner_in_rx.recv().unwrap();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let dup_val = pool.install(|| {
        cache
            .get_or_try_compute(555u64, ExecutionMode::NonBlockingPool, || {
                Ok(sequential_oracle(555))
            })
            .unwrap()
    });
    assert_eq!(dup_val, sequential_oracle(555));
    owner_release_tx.send(()).unwrap();
    assert_eq!(owner.join().unwrap(), sequential_oracle(555));
    assert_eq!(cache.in_flight_len(), 0);
    let snap = cache.metrics_snapshot();
    assert_eq!(snap.duplicate_computes_during_in_flight, 1);
    assert_eq!(snap.single_flight_owner_computes, 1);
    assert_eq!(snap.rayon_nonblocking_computes, 1);
}
