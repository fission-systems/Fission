//! Cooperative request-wide cancellation and deadline propagation.
//!
//! `CancellationToken` separates wall-clock deadlines and explicit user/batch
//! cancellation from deterministic `ResourceLimits` work counters. Long-running
//! stages enter a scope via [`enter_cancellation_scope`] or
//! [`with_cancellation_scope`] and periodically poll
//! [`check_current_cancellation`] / [`is_current_cancelled`].

use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

const STATE_ACTIVE: u8 = 0;
const STATE_CANCELLED: u8 = 1;
const STATE_TIMED_OUT: u8 = 2;

/// Why a cooperative cancellation token tripped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancellationReason {
    /// Explicitly cancelled by the caller or coordinator.
    Cancelled,
    /// Exceeded its wall-clock deadline.
    TimedOut,
}

impl fmt::Display for CancellationReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::TimedOut => f.write_str("timed out"),
        }
    }
}

#[derive(Debug)]
struct CancellationState {
    status: AtomicU8,
    deadline: Option<Instant>,
    parent: Option<CancellationToken>,
}

/// Shareable, thread-safe cooperative cancellation and deadline token.
#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<CancellationState>);

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    /// Create a token with no deadline that can be explicitly cancelled.
    pub fn new() -> Self {
        Self::with_deadline(None)
    }

    /// Create a token that trips with [`CancellationReason::TimedOut`] after `timeout`.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self::with_deadline(Some(Instant::now() + timeout))
    }

    /// Create a token with an optional wall-clock deadline.
    pub fn with_deadline(deadline: Option<Instant>) -> Self {
        Self(Arc::new(CancellationState {
            status: AtomicU8::new(STATE_ACTIVE),
            deadline,
            parent: None,
        }))
    }

    /// Derive a child token whose deadline is capped by `timeout` while still
    /// observing any cancellation or earlier deadline on `self`.
    ///
    /// Timing out on the child token does not cancel the parent token.
    pub fn child(&self, timeout: Option<Duration>) -> Self {
        let child_deadline = timeout.map(|duration| Instant::now() + duration);
        let merged_deadline = match (self.deadline(), child_deadline) {
            (Some(parent), Some(child)) => Some(parent.min(child)),
            (Some(parent), None) => Some(parent),
            (None, Some(child)) => Some(child),
            (None, None) => None,
        };
        Self(Arc::new(CancellationState {
            status: AtomicU8::new(self.0.status.load(Ordering::Acquire)),
            deadline: merged_deadline,
            parent: Some(self.clone()),
        }))
    }

    /// Mark this token (and any derived children) as explicitly cancelled.
    pub fn cancel(&self) {
        let _ = self.0.status.compare_exchange(
            STATE_ACTIVE,
            STATE_CANCELLED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Mark this token as having reached a timeout.
    pub fn mark_timed_out(&self) {
        let _ = self.0.status.compare_exchange(
            STATE_ACTIVE,
            STATE_TIMED_OUT,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn latch_reason(&self, reason: CancellationReason) {
        let target = match reason {
            CancellationReason::Cancelled => STATE_CANCELLED,
            CancellationReason::TimedOut => STATE_TIMED_OUT,
        };
        let _ = self.0.status.compare_exchange(
            STATE_ACTIVE,
            target,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Earliest effective deadline across `self` and its parent chain.
    pub fn deadline(&self) -> Option<Instant> {
        match (
            self.0.deadline,
            self.0.parent.as_ref().and_then(Self::deadline),
        ) {
            (Some(own), Some(parent)) => Some(own.min(parent)),
            (Some(own), None) => Some(own),
            (None, Some(parent)) => Some(parent),
            (None, None) => None,
        }
    }

    /// Check whether this token has been cancelled or passed its deadline.
    pub fn check(&self) -> Result<(), CancellationReason> {
        match self.0.status.load(Ordering::Acquire) {
            STATE_CANCELLED => return Err(CancellationReason::Cancelled),
            STATE_TIMED_OUT => return Err(CancellationReason::TimedOut),
            _ => {}
        }

        if let Some(parent) = &self.0.parent
            && let Err(reason) = parent.check()
        {
            self.latch_reason(reason);
            return Err(reason);
        }

        if let Some(deadline) = self.0.deadline
            && Instant::now() >= deadline
        {
            self.mark_timed_out();
            return match self.0.status.load(Ordering::Acquire) {
                STATE_CANCELLED => Err(CancellationReason::Cancelled),
                _ => Err(CancellationReason::TimedOut),
            };
        }

        Ok(())
    }

    /// Convenience helper returning `true` when [`Self::check`] fails.
    pub fn is_cancelled(&self) -> bool {
        self.check().is_err()
    }

    /// Return the latched or expired [`CancellationReason`], if any.
    pub fn reason(&self) -> Option<CancellationReason> {
        self.check().err()
    }

    /// Remaining wall budget if active with a deadline, `Some(Duration::ZERO)`
    /// if already cancelled/expired, or `None` if unbounded and active.
    pub fn remaining(&self) -> Option<Duration> {
        if self.is_cancelled() {
            return Some(Duration::ZERO);
        }
        self.deadline()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }
}

thread_local! {
    static ACTIVE_TOKEN: RefCell<Option<CancellationToken>> = const { RefCell::new(None) };
}

/// RAII guard restoring the thread-local cancellation token on drop.
pub struct CancellationScopeGuard {
    previous: Option<CancellationToken>,
}

impl Drop for CancellationScopeGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        let _ = ACTIVE_TOKEN.try_with(|slot| {
            *slot.borrow_mut() = previous;
        });
    }
}

/// Bind `token` as the active thread-local cancellation scope until the returned
/// guard drops (including on stack unwind).
pub fn enter_cancellation_scope(token: CancellationToken) -> CancellationScopeGuard {
    let previous = ACTIVE_TOKEN
        .try_with(|slot| slot.borrow_mut().replace(token))
        .unwrap_or(None);
    CancellationScopeGuard { previous }
}

/// Run `f` with `token` bound to the active thread-local cancellation scope.
pub fn with_cancellation_scope<T>(token: CancellationToken, f: impl FnOnce() -> T) -> T {
    let _guard = enter_cancellation_scope(token);
    f()
}

/// Return a clone of the current thread-local [`CancellationToken`], if any.
pub fn current_cancellation_token() -> Option<CancellationToken> {
    ACTIVE_TOKEN
        .try_with(|slot| slot.borrow().clone())
        .ok()
        .flatten()
}

/// Check the active thread-local cancellation token, if one is bound.
pub fn check_current_cancellation() -> Result<(), CancellationReason> {
    ACTIVE_TOKEN
        .try_with(|slot| match slot.borrow().as_ref() {
            Some(token) => token.check(),
            None => Ok(()),
        })
        .unwrap_or(Ok(()))
}

/// Fast-path boolean check for hot midend/lifter loops.
pub fn is_current_cancelled() -> bool {
    check_current_cancellation().is_err()
}

/// Remaining deadline from the current thread-local token, if any.
pub fn current_remaining_timeout() -> Option<Duration> {
    current_cancellation_token().and_then(|token| token.remaining())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn explicit_cancel_trips_token_and_child() {
        let parent = CancellationToken::new();
        let child = parent.child(Some(Duration::from_secs(60)));
        assert_eq!(parent.check(), Ok(()));
        assert_eq!(child.check(), Ok(()));

        parent.cancel();
        assert_eq!(parent.check(), Err(CancellationReason::Cancelled));
        assert_eq!(child.check(), Err(CancellationReason::Cancelled));
        assert!(child.is_cancelled());
    }

    #[test]
    fn child_timeout_does_not_cancel_parent() {
        let parent = CancellationToken::new();
        let child = parent.child(Some(Duration::ZERO));

        assert_eq!(child.check(), Err(CancellationReason::TimedOut));
        assert_eq!(child.remaining(), Some(Duration::ZERO));
        assert_eq!(parent.check(), Ok(()));
        assert_eq!(parent.remaining(), None);
    }

    #[test]
    fn nested_scope_guard_restores_parent_across_panic() {
        let outer = CancellationToken::new();
        let inner = outer.child(Some(Duration::ZERO));

        let _outer_guard = enter_cancellation_scope(outer.clone());
        assert_eq!(check_current_cancellation(), Ok(()));

        let panicked = catch_unwind(AssertUnwindSafe(|| {
            with_cancellation_scope(inner.clone(), || {
                assert_eq!(
                    check_current_cancellation(),
                    Err(CancellationReason::TimedOut)
                );
                panic!("unwind restores prior cancellation scope");
            });
        }));
        assert!(panicked.is_err());
        assert_eq!(check_current_cancellation(), Ok(()));
        assert_eq!(outer.check(), Ok(()));
    }
}
