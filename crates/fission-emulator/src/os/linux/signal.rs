//! Linux user-mode signal state and delivery.
//!
//! Cleanroom scaffold inspired by QEMU linux-user signal queue / default
//! dispositions. Full `ucontext` / altstack frames are not yet modeled;
//! handler delivery sets PC and records a simple return path for `rt_sigreturn`.

use serde::{Deserialize, Serialize};

/// Maximum signal number (Linux allows 1..=64 for standard+RT in our model).
pub const NSIG: usize = 64;

/// Do not prevent the signal from being received from within its own handler.
pub const SA_NODEFER: u64 = 0x4000_0000;
/// Restore the signal action to the default upon entry to the signal handler.
pub const SA_RESETHAND: u64 = 0x8000_0000;

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
    Handler {
        signo: i32,
        handler: u64,
        old_pc: u64,
    },
    /// Default disposition is process termination.
    Terminate { signo: i32 },
    /// Default disposition is stop (we treat as no-op resume for single-thread).
    Stop { signo: i32 },
    /// Signal was ignored / discarded.
    Ignored { signo: i32 },
}

/// Saved execution frame for an active signal handler invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalFrame {
    pub signo: i32,
    pub return_pc: u64,
    pub saved_blocked: u64,
    #[serde(skip)]
    pub saved_registers: Option<fission_ttd::RegisterState>,
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
    /// sa_flags per signal (SA_RESTART etc.).
    pub flags: Vec<u64>,
    /// If Some, we are inside a user handler and `rt_sigreturn` should restore this PC.
    pub return_pc: Option<u64>,
    /// Signal currently being handled (for debugging / nested delivery).
    pub current: Option<i32>,
    /// Stack of active signal frames for nested handler reentrancy and register restoration.
    pub frames: Vec<SignalFrame>,
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
            frames: Vec::new(),
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
                let saved_blocked = self.blocked;
                let sa_flags = self.flags.get(sig as usize).copied().unwrap_or(0);
                if sa_flags & SA_NODEFER == 0 {
                    self.set_blocked_mask(self.blocked | b);
                }
                if sa_flags & SA_RESETHAND != 0 {
                    self.actions[sig as usize] = SigAction::Default;
                }
                self.frames.push(SignalFrame {
                    signo: sig,
                    return_pc: current_pc,
                    saved_blocked,
                    saved_registers: None,
                });
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

    /// Attach a full register state snapshot to the most recently delivered signal frame.
    pub fn set_current_frame_registers(&mut self, registers: fission_ttd::RegisterState) {
        if let Some(frame) = self.frames.last_mut() {
            frame.saved_registers = Some(registers);
        }
    }

    /// Pop the innermost active signal frame and restore the pre-handler blocked mask.
    pub fn pop_frame(&mut self) -> Option<SignalFrame> {
        if let Some(frame) = self.frames.pop() {
            self.set_blocked_mask(frame.saved_blocked);
            self.return_pc = self.frames.last().map(|f| f.return_pc);
            self.current = self.frames.last().map(|f| f.signo);
            Some(frame)
        } else if let Some(pc) = self.return_pc.take() {
            let signo = self.current.take().unwrap_or(0);
            Some(SignalFrame {
                signo,
                return_pc: pc,
                saved_blocked: self.blocked,
                saved_registers: None,
            })
        } else {
            self.current = None;
            None
        }
    }

    /// `rt_sigreturn`: restore PC after user handler.
    pub fn sigreturn(&mut self) -> Option<u64> {
        self.pop_frame().map(|frame| frame.return_pc)
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

    #[test]
    fn nested_signal_delivery_preserves_frame_stack_and_masks_until_sigreturn() {
        let mut s = SignalState::default();
        s.set_action(sig::SIGUSR1, SigAction::Handler(0x401000), 0);
        s.set_action(sig::SIGUSR2, SigAction::Handler(0x402000), SA_RESETHAND);

        // Deliver SIGUSR1 at 0x1000
        s.queue(sig::SIGUSR1);
        assert_eq!(
            s.take_delivery(0x1000),
            DeliverResult::Handler {
                signo: sig::SIGUSR1,
                handler: 0x401000,
                old_pc: 0x1000,
            }
        );
        // Same signal queued without SA_NODEFER is masked while handler is active
        s.queue(sig::SIGUSR1);
        assert_eq!(s.take_delivery(0x401010), DeliverResult::None);

        // Nested SIGUSR2 delivered inside SIGUSR1 handler at 0x401020
        s.queue(sig::SIGUSR2);
        assert_eq!(
            s.take_delivery(0x401020),
            DeliverResult::Handler {
                signo: sig::SIGUSR2,
                handler: 0x402000,
                old_pc: 0x401020,
            }
        );
        // SA_RESETHAND resets SIGUSR2 disposition to Default
        assert_eq!(s.action(sig::SIGUSR2), SigAction::Default);
        assert_eq!(s.return_pc, Some(0x401020));
        assert_eq!(s.current, Some(sig::SIGUSR2));

        // Inner handler sigreturn restores outer handler frame and unblocks SIGUSR2 while keeping SIGUSR1 blocked
        assert_eq!(s.sigreturn(), Some(0x401020));
        assert_eq!(s.return_pc, Some(0x1000));
        assert_eq!(s.current, Some(sig::SIGUSR1));
        assert_ne!(
            s.blocked_mask() & SignalState::bit(sig::SIGUSR1).unwrap(),
            0
        );

        // Outer handler sigreturn restores original PC 0x1000 and unblocks SIGUSR1
        assert_eq!(s.sigreturn(), Some(0x1000));
        assert_eq!(s.return_pc, None);
        assert_eq!(s.current, None);
        assert_eq!(s.blocked_mask(), 0);

        // Now the queued SIGUSR1 is deliverable again
        assert!(matches!(
            s.take_delivery(0x1000),
            DeliverResult::Handler {
                signo: sig::SIGUSR1,
                old_pc: 0x1000,
                ..
            }
        ));
    }

    #[test]
    fn emulator_signal_delivery_honors_red_zone_alignment_and_restores_registers_on_sigreturn() {
        use crate::arch::ArchInfo;
        use crate::core::Emulator;
        use crate::os::linux::LinuxEnv;
        use crate::os::linux::syscall::SysRtSigreturn;
        use crate::os::procedure::SimProcedure;
        use crate::pcode::state::MachineState;
        use fission_loader::loader::LoadedBinary;
        use fission_sleigh::runtime::RuntimeSleighFrontend;
        use std::path::PathBuf;

        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_x64_hello_sys.elf");
        let binary = LoadedBinary::from_file(path).expect("load x86-64 ELF fixture");
        let mut state = MachineState::new();
        let image =
            crate::os::linux::loader::load_elf(&mut state, &binary).expect("load fixture image");
        let load_spec = binary.load_spec().expect("fixture load spec").clone();
        let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
            .expect("candidate Sleigh frontends")
            .into_iter()
            .next()
            .expect("Sleigh frontend");
        let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
            .expect("fixture architecture");
        let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
            .expect("construct emulator");
        emu.apply_linux_image(image).expect("apply Linux image");

        let base_sp = emu.read_register_u64("RSP").unwrap();
        let initial_sp = (base_sp & !0xF) + 4; // intentionally misaligned by 4 bytes
        let red_zone_slot = initial_sp - 64;
        let ram = emu.state.ram_space();
        emu.state
            .write_space(ram, red_zone_slot, &0xdead_beef_cafe_babeu64.to_le_bytes())
            .unwrap();

        emu.pc = 0x400500;
        emu.write_register_u64("RSP", initial_sp).unwrap();
        emu.write_register_u64("RAX", 0x1122_3344_5566_7788)
            .unwrap();
        emu.write_register_u64("RDI", 0x99aa_bbcc_ddee_ff00)
            .unwrap();

        emu.signals
            .set_action(sig::SIGUSR1, SigAction::Handler(0x401000), 0);
        emu.raise_signal(sig::SIGUSR1);
        assert!(emu.process_pending_signals().unwrap());
        assert_eq!(emu.pc, 0x401000);

        let handler_sp = emu.read_register_u64("RSP").unwrap();
        // Must skip 128-byte red zone and leave (RSP + 8) % 16 == 0 at handler entry
        assert!(handler_sp <= initial_sp - 128 - 8);
        assert_eq!((handler_sp + 8) % 16, 0);

        // Red zone data must remain intact
        let rz_check = emu.state.read_space(ram, red_zone_slot, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(rz_check.try_into().unwrap()),
            0xdead_beef_cafe_babe
        );

        // Handler clobbers caller-saved registers
        emu.write_register_u64("RAX", 0x42).unwrap();
        emu.write_register_u64("RCX", 0x9999).unwrap();

        // rt_sigreturn must restore original PC, RSP, RAX, and RDI
        SysRtSigreturn.run(&mut emu).unwrap();
        assert_eq!(emu.pc, 0x400500);
        assert_eq!(emu.read_register_u64("RSP").unwrap(), initial_sp);
        assert_eq!(emu.read_register_u64("RAX").unwrap(), 0x1122_3344_5566_7788);
        assert_eq!(emu.read_register_u64("RDI").unwrap(), 0x99aa_bbcc_ddee_ff00);
    }
}
