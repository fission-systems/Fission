//! A bounded local queue whose waits cannot execute unrelated Rayon jobs.
use std::sync::atomic::{AtomicUsize, Ordering};

struct HelperBudget(AtomicUsize);
struct HelperPermit<'a>(&'a HelperBudget);

impl HelperBudget {
    fn try_acquire(&self, limit: usize) -> Option<HelperPermit<'_>> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < limit).then(|| active + 1)
            })
            .ok()
            .map(|_| HelperPermit(self))
    }
}
impl Drop for HelperPermit<'_> {
    fn drop(&mut self) {
        self.0.0.fetch_sub(1, Ordering::AcqRel);
    }
}
static HELPERS: HelperBudget = HelperBudget(AtomicUsize::new(0));

pub(super) fn collect_scoped<T: Send>(
    len: usize,
    width: usize,
    decode: impl Fn(usize) -> Option<T> + Sync,
) -> Vec<T> {
    collect_with(len, width, decode, &HELPERS, || {
        std::thread::Builder::new().name("fission-noreturn".into())
    })
}

fn collect_with<T: Send>(
    len: usize,
    width: usize,
    decode: impl Fn(usize) -> Option<T> + Sync,
    budget: &HelperBudget,
    builder: impl Fn() -> std::thread::Builder,
) -> Vec<T> {
    let width = width.max(1).min(len.max(1));
    let next = AtomicUsize::new(0);
    let work = || {
        let mut rows = Vec::new();
        loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            if index >= len {
                break;
            }
            if let Some(row) = decode(index) {
                rows.push((index, row));
            }
        }
        rows
    };
    let mut rows = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 1..width {
            let Some(permit) = budget.try_acquire(width - 1) else {
                break;
            };
            let work = &work;
            // Failure drops the captured permit. The remaining workers/caller
            // still drain every unassigned item from the same local queue.
            if let Ok(handle) = builder().spawn_scoped(scope, move || {
                let _permit = permit;
                work()
            }) {
                handles.push(handle);
            }
        }
        let mut rows = work();
        for handle in handles {
            match handle.join() {
                Ok(batch) => rows.extend(batch),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        rows
    });
    rows.sort_unstable_by_key(|(index, _)| *index);
    rows.into_iter().map(|(_, row)| row).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, OnceLock};

    #[test]
    fn scoped_no_return_queue_preserves_order_skips_and_every_input_once() {
        let budget = HelperBudget(AtomicUsize::new(0));
        for width in [0, 1, 2, 4, 16] {
            for len in [0, 1, 3, 137] {
                let seen: Vec<_> = (0..len).map(|_| AtomicUsize::new(0)).collect();
                let actual = collect_with(
                    len,
                    width,
                    |i| {
                        seen[i].fetch_add(1, Ordering::Relaxed);
                        std::thread::yield_now();
                        (i % 3 != 1).then_some(i % 7)
                    },
                    &budget,
                    std::thread::Builder::new,
                );
                let expected: Vec<_> = (0..len).filter(|i| i % 3 != 1).map(|i| i % 7).collect();
                assert_eq!(actual, expected);
                assert!(seen.iter().all(|n| n.load(Ordering::Relaxed) == 1));
                assert_eq!(budget.0.load(Ordering::Acquire), 0);
            }
        }
    }

    #[test]
    fn scoped_no_return_queue_spawn_failure_and_unavailable_permits_keep_all_work() {
        let budget = HelperBudget(AtomicUsize::new(0));
        let actual = collect_with(17, 4, Some, &budget, || {
            std::thread::Builder::new().stack_size(usize::MAX / 4)
        });
        assert_eq!(actual, (0..17).collect::<Vec<_>>());
        assert_eq!(budget.0.load(Ordering::Acquire), 0);
        let held = budget.try_acquire(1).unwrap();
        assert!(budget.try_acquire(1).is_none());
        let actual = collect_with(17, 2, Some, &budget, || {
            panic!("permit unavailable: must not spawn")
        });
        assert_eq!(actual, (0..17).collect::<Vec<_>>());
        drop(held);
        assert_eq!(budget.0.load(Ordering::Acquire), 0);
    }

    #[test]
    fn scoped_no_return_queue_propagates_panics_and_returns_permits() {
        let budget = HelperBudget(AtomicUsize::new(0));
        let result = std::panic::catch_unwind(|| {
            collect_with(
                64,
                4,
                |i| {
                    assert_ne!(i, 7, "decode failure");
                    Some(i)
                },
                &budget,
                std::thread::Builder::new,
            )
        });
        assert!(result.is_err());
        assert_eq!(budget.0.load(Ordering::Acquire), 0);
    }

    #[test]
    fn scoped_no_return_queue_bounds_helpers_across_simultaneous_callers() {
        let budget = HelperBudget(AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let budget = &budget;
                scope.spawn(move || {
                    let actual = collect_with(
                        64,
                        4,
                        |i| {
                            assert!(budget.0.load(Ordering::Acquire) <= 3);
                            std::thread::yield_now();
                            Some(i)
                        },
                        budget,
                        std::thread::Builder::new,
                    );
                    assert_eq!(actual, (0..64).collect::<Vec<_>>());
                });
            }
        });
        assert_eq!(budget.0.load(Ordering::Acquire), 0);
    }

    #[test]
    fn scoped_no_return_queue_finishes_once_initialization_inside_saturated_rayon() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use rayon::prelude::*;
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(4)
                .build()
                .unwrap();
            for _ in 0..24 {
                let slot = Arc::new(OnceLock::new());
                let initialized = AtomicUsize::new(0);
                pool.install(|| {
                    (0..24).into_par_iter().for_each(|_| {
                        let result = slot.get_or_init(|| {
                            initialized.fetch_add(1, Ordering::Relaxed);
                            collect_scoped(96, 4, |i| {
                                std::thread::yield_now();
                                Some(i)
                            })
                        });
                        assert_eq!(*result, (0..96).collect::<Vec<_>>());
                    })
                });
                assert_eq!(initialized.load(Ordering::Relaxed), 1);
            }
            tx.send(()).unwrap();
        });
        rx.recv_timeout(std::time::Duration::from_secs(15))
            .expect("OnceLock holder must never steal an outer waiting caller");
    }
}
