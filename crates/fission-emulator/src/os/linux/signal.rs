//! Linux user-mode signal state and delivery.
//!
//! Cleanroom scaffold inspired by QEMU linux-user signal queue / default
//! dispositions. Full `ucontext` / altstack frames are not yet modeled;
//! handler delivery sets PC and records a simple return path for `rt_sigreturn`.

use serde::{Deserialize, Serialize};

/// Maximum signal number (Linux allows 1..=64 for standard+RT in our model).
pub const NSIG: usize = 64;

/// Common POSIX/Linux signal numbers.
pub mod sig {
    pub const SIGHUP: i32 = 1;
    pub const SIGINT: i32 = 2;
    pub const SIGQUIT: i32 = 3;
    pub const SIGILL: i32 = 4;
    pub const SIGTRAP: i32 = 5;
    pub const SIGABRT: i32 = 6;
    pub const SIGBUS: i32 = 7;
    pub const SIGFPE: i32 = 8;
    pub const SIGKILL: i32 = 9;
    pub const SIGUSR1: i32 = 10;
    pub const SIGSEGV: i32 = 11;
    pub const SIGUSR2: i32 = 12;
    pub const SIGPIPE: i32 = 13;
    pub const SIGALRM: i32 = 14;
    pub const SIGTERM: i32 = 15;
    pub const SIGCHLD: i32 = 17;
    pub const SIGCONT: i32 = 18;
    pub const SIGSTOP: i32 = 19;
    pub const SIGTSTP: i32 = 20;
    pub const SIGTTIN: i32 = 21;
    pub const SIGTTOU: i32 = 22;
    pub const SIGURG: i32 = 23;
    pub const SIGXCPU: i32 = 24;
    pub const SIGXFSZ: i32 = 25;
    pub const SIGVTALRM: i32 = 26;
    pub const SIGPROF: i32 = 27;
    pub const SIGWINCH: i32 = 28;
    pub const SIGIO: i32 = 29;
    pub const SIGSYS: i32 = 31;
}

/// `sa_handler` / disposition for one signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SigAction {
    /// SIG_DFL
    Default,
    /// SIG_IGN
    Ignore,
    /// User handler address (guest VA).
    Handler(u64),
}

impl Default for SigAction {
    fn default() -> Self {
        Self::Default
    }
}

/// Result of attempting to deliver pending signals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliverResult {
    /// No pending deliverable signal.
    None,
    /// Delivered to a user handler; PC was rewritten.
    Handler { signo: i32, handler: u64, old_pc: u64 },
    /// Default disposition is process termination.
    Terminate { signo: i32 },
    /// Default disposition is stop (we treat as no-op resume for single-thread).
    Stop { signo: i32 },
    /// Signal was ignored / discarded.
    Ignored { signo: i32 },
}

/// Per-process signal bookkeeping.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignalState {
    /// Bit i set ⇒ signal (i+1) is pending.
    pub pending: u64,
    /// Bit i set ⇒ signal (i+1) is blocked.
    pub blocked: u64,
    /// Actions for signals 1..=NSIG (index 0 unused). Vec length = NSIG+1.
    pub actions: Vec<SigAction>,
    /// sa_flags per signal (SA_RESTART etc.) — stored for fidelity, mostly ignored.
    pub flags: Vec<u64>,
    /// If Some, we are inside a user handler and `rt_sigreturn` should restore this PC.
    pub return_pc: Option<u64>,
    /// Signal currently being handled (for debugging / nested delivery).
    pub current: Option<i32>,
}

impl Default for SignalState {
    fn default() -> Self {
        Self {
            pending: 0,
            blocked: 0,
            actions: vec![SigAction::Default; NSIG + 1],
            flags: vec![0; NSIG + 1],
            return_pc: None,
            current: None,
        }
    }
}

impl SignalState {
    #[inline]
    fn bit(sig: i32) -> Option<u64> {
        if sig < 1 || sig as usize > NSIG {
            return None;
        }
        Some(1u64 << (sig as u32 - 1))
    }

    pub fn queue(&mut self, sig: i32) -> bool {
        let Some(b) = Self::bit(sig) else {
            return false;
        };
        // SIGKILL / SIGSTOP cannot be blocked or ignored — always queue.
        self.pending |= b;
        true
    }

    pub fn is_pending(&self, sig: i32) -> bool {
        Self::bit(sig).is_some_and(|b| self.pending & b != 0)
    }

    pub fn set_blocked_mask(&mut self, mask: u64) {
        // Cannot block KILL/STOP.
        let immutable = Self::bit(sig::SIGKILL).unwrap() | Self::bit(sig::SIGSTOP).unwrap();
        self.blocked = mask & !immutable;
    }

    pub fn blocked_mask(&self) -> u64 {
        self.blocked
    }

    pub fn set_action(&mut self, sig: i32, action: SigAction, flags: u64) -> bool {
        if sig < 1 || sig as usize > NSIG {
            return false;
        }
        if sig == sig::SIGKILL || sig == sig::SIGSTOP {
            return false;
        }
        self.actions[sig as usize] = action;
        self.flags[sig as usize] = flags;
        true
    }

    pub fn action(&self, sig: i32) -> SigAction {
        if sig < 1 || sig as usize > NSIG {
            return SigAction::Default;
        }
        self.actions[sig as usize]
    }

    /// Default disposition for a signal (Linux-like).
    pub fn default_disposition(sig: i32) -> DeliverResult {
        match sig {
            sig::SIGCHLD | sig::SIGURG | sig::SIGWINCH => DeliverResult::Ignored { signo: sig },
            sig::SIGSTOP | sig::SIGTSTP | sig::SIGTTIN | sig::SIGTTOU => {
                DeliverResult::Stop { signo: sig }
            }
            sig::SIGCONT => DeliverResult::Ignored { signo: sig },
            _ => DeliverResult::Terminate { signo: sig },
        }
    }

    /// Pick one pending unblocked signal and compute delivery (does not clear yet).
    pub fn peek_deliverable(&self) -> Option<i32> {
        let ready = self.pending & !self.blocked;
        if ready == 0 {
            return None;
        }
        // Prefer lower signal numbers (kernel-ish).
        for sig in 1..=NSIG as i32 {
            let b = Self::bit(sig).unwrap();
            if ready & b != 0 {
                // KILL/STOP always deliverable even if "blocked" (we force unblock).
                return Some(sig);
            }
        }
        None
    }

    /// Deliver one pending signal. `current_pc` is rewritten by the caller on Handler.
    pub fn take_delivery(&mut self, current_pc: u64) -> DeliverResult {
        let Some(sig) = self.peek_deliverable() else {
            return DeliverResult::None;
        };
        let b = Self::bit(sig).unwrap();
        self.pending &= !b;

        // SIGKILL always terminates.
        if sig == sig::SIGKILL {
            return DeliverResult::Terminate { signo: sig };
        }

        let action = if sig == sig::SIGSTOP {
            SigAction::Default
        } else {
            self.actions[sig as usize]
        };

        match action {
            SigAction::Ignore => DeliverResult::Ignored { signo: sig },
            SigAction::Default => Self::default_disposition(sig),
            SigAction::Handler(handler) => {
                self.return_pc = Some(current_pc);
                self.current = Some(sig);
                DeliverResult::Handler {
                    signo: sig,
                    handler,
                    old_pc: current_pc,
                }
            }
        }
    }

    /// `rt_sigreturn`: restore PC after user handler.
    pub fn sigreturn(&mut self) -> Option<u64> {
        self.current = None;
        self.return_pc.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_and_default_terminate() {
        let mut s = SignalState::default();
        assert!(s.queue(sig::SIGTERM));
        match s.take_delivery(0x1000) {
            DeliverResult::Terminate { signo } => assert_eq!(signo, sig::SIGTERM),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn ignore_and_handler() {
        let mut s = SignalState::default();
        s.set_action(sig::SIGINT, SigAction::Ignore, 0);
        s.queue(sig::SIGINT);
        assert!(matches!(
            s.take_delivery(0x1000),
            DeliverResult::Ignored { .. }
        ));

        s.set_action(sig::SIGUSR1, SigAction::Handler(0x401000), 0);
        s.queue(sig::SIGUSR1);
        match s.take_delivery(0x2000) {
            DeliverResult::Handler {
                handler, old_pc, ..
            } => {
                assert_eq!(handler, 0x401000);
                assert_eq!(old_pc, 0x2000);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(s.sigreturn(), Some(0x2000));
    }

    #[test]
    fn blocked_masks_delivery() {
        let mut s = SignalState::default();
        s.queue(sig::SIGTERM);
        s.set_blocked_mask(SignalState::bit(sig::SIGTERM).unwrap());
        assert!(matches!(s.take_delivery(0), DeliverResult::None));
        s.set_blocked_mask(0);
        assert!(matches!(
            s.take_delivery(0),
            DeliverResult::Terminate { .. }
        ));
    }
}
