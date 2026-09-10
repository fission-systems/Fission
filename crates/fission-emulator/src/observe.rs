//! Dynamic analysis: watch a run without paying for what nobody watches.
//!
//! # Where the instrumentation is decided
//!
//! QEMU's TCG plugins settle this at *translation* time: a plugin sees each
//! block once, when it is compiled, and asks for callbacks only on the
//! instructions it cares about. A block nobody asked about compiles to exactly
//! the code it would have compiled to with no plugin loaded. This module works
//! the same way, for the same reason -- there is no interpreter here, so an
//! "if observing" branch per instruction would live inside the hot compiled
//! code forever.
//!
//! The consequence is [`ObserveMask`]: the union of what every registered
//! observer wants, read once per block by the compiler. Changing it has to
//! flush the block cache ([`crate::jit::cache::JitCache::flush_all`]), because
//! blocks compiled earlier carry the older decision.
//!
//! # What an observer is handed
//!
//! Plain values, never the emulator. A callback that could reach back into
//! `Emulator` would alias the `&mut` the callback was reached through, and the
//! hook sites are inside the JIT's own callbacks where that borrow is live.
//! Anything an observer needs is read at the hook site and passed in --
//! syscall arguments, for instance, are pulled from the ABI's registers by the
//! syscall hook rather than by the observer.

use std::collections::BTreeMap;

/// Which callbacks any observer has asked for.
///
/// `syscall` and `hle` are not here: those hooks are host-side already (the OS
/// layer handles them outside compiled code), so they cost nothing to leave
/// on and every observer gets them.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ObserveMask {
    /// Called once on entry to each translation block.
    pub block: bool,
    /// Called once per guest instruction. Much more expensive than `block`;
    /// coverage does not need it.
    pub insn: bool,
    /// Called on each RAM read and write.
    pub mem: bool,
}

impl ObserveMask {
    pub const NONE: Self = Self {
        block: false,
        insn: false,
        mem: false,
    };

    pub fn block() -> Self {
        Self {
            block: true,
            ..Self::NONE
        }
    }

    pub fn any(&self) -> bool {
        self.block || self.insn || self.mem
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            block: self.block || other.block,
            insn: self.insn || other.insn,
            mem: self.mem || other.mem,
        }
    }
}

/// What the per-byte shadow layer carries, if anything.
///
/// The shadow callbacks (`jit_shadow_copy`/`_load`/`_store`/`_binop`/`_unop`)
/// were emitted into every compiled block unconditionally, so every arithmetic
/// op in every run paid a host call that almost always looked up two empty
/// shadow bytes and returned. Same decision as [`ObserveMask`], same place to
/// make it: at translation, so a run that wants none carries none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadowMode {
    /// No shadow at all. Compiled blocks carry no shadow callbacks.
    #[default]
    Off,
    /// Per-byte labels, unioned when values combine. No solver: a label says
    /// *that* a byte derives from a source, not what expression produced it.
    Taint,
    /// Full symbolic expressions -- what the concolic explorer needs, and far
    /// more than taint needs.
    Symbolic,
}

impl ShadowMode {
    pub fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// One watcher of a run.
///
/// Every method has a default, so an observer implements only the events it
/// wants -- but [`Observer::interest`] must still name them, or the compiler
/// never emits the calls.
pub trait Observer: Send {
    /// What this observer wants instrumented. Read when it is registered.
    fn interest(&self) -> ObserveMask {
        ObserveMask::NONE
    }

    /// A block was compiled: `insns` is `(pc, len)` per guest instruction.
    ///
    /// Fires once per block, whatever the mask says, because translation is
    /// already a slow path and the block's shape is what most observers need
    /// in order to interpret the cheap events that follow.
    fn on_translate(&mut self, _entry_pc: u64, _insns: &[(u64, u32)]) {}

    fn on_block(&mut self, _pc: u64) {}

    fn on_insn(&mut self, _pc: u64) {}

    fn on_mem(&mut self, _addr: u64, _size: u32, _write: bool, _value: u64) {}

    /// A guest syscall, with the ABI's argument registers already read.
    ///
    /// The OS layer calls [`Observer::on_syscall_detailed`] when it can name
    /// the call; that default forwards here, so an observer that only wants
    /// numbers implements this one and gets both.
    fn on_syscall(&mut self, _pc: u64, _number: u64, _args: &[u64; 6]) {}

    /// The same call, with its arguments read the way its ABI says to.
    ///
    /// Pointer arguments are resolved here, at call time, because the buffer
    /// they point at is the guest's to reuse the moment the call returns.
    fn on_syscall_detailed(
        &mut self,
        pc: u64,
        number: u64,
        _name: Option<&'static str>,
        args: &[u64; 6],
        _detail: Vec<SyscallArg>,
    ) {
        self.on_syscall(pc, number, args);
    }

    /// A high-level-emulation stub stood in for a real call.
    fn on_hle(&mut self, _pc: u64, _name: &str) {}

    fn as_any(&self) -> &dyn std::any::Any;
}

/// Which code actually ran.
///
/// Block granularity, so it needs no per-instruction callback: a translation
/// block is straight-line by construction, and `on_translate` already said
/// which instructions it holds. That makes coverage nearly free -- one call
/// per block entry -- while still answering per-instruction questions.
#[derive(Debug, Default)]
pub struct Coverage {
    /// Block entry PC → times entered.
    pub blocks: BTreeMap<u64, u64>,
    /// Block entry PC → the instructions it covers, from translation.
    pub block_insns: BTreeMap<u64, Vec<(u64, u32)>>,
}

impl Coverage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every guest instruction address that executed at least once.
    pub fn executed_instructions(&self) -> Vec<u64> {
        let mut out: Vec<u64> = self
            .blocks
            .keys()
            .filter_map(|pc| self.block_insns.get(pc))
            .flat_map(|insns| insns.iter().map(|(pc, _)| *pc))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Bytes of code reached, counting each instruction once.
    pub fn bytes_covered(&self) -> u64 {
        let mut seen: BTreeMap<u64, u32> = BTreeMap::new();
        for pc in self.blocks.keys() {
            if let Some(insns) = self.block_insns.get(pc) {
                for (ipc, len) in insns {
                    seen.insert(*ipc, *len);
                }
            }
        }
        seen.values().map(|l| u64::from(*l)).sum()
    }
}

impl Observer for Coverage {
    fn interest(&self) -> ObserveMask {
        ObserveMask::block()
    }

    fn on_translate(&mut self, entry_pc: u64, insns: &[(u64, u32)]) {
        self.block_insns.insert(entry_pc, insns.to_vec());
    }

    fn on_block(&mut self, pc: u64) {
        *self.blocks.entry(pc).or_insert(0) += 1;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// One syscall argument, read the way its ABI says to read it.
///
/// Captured **when the call happens**, not when a report is written: a path
/// name lives in a buffer the guest is free to reuse the moment the call
/// returns, so resolving it later reads whatever happens to be there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyscallArg {
    Int(i64),
    Hex(u64),
    Fd(i64),
    Ptr(u64),
    Str {
        ptr: u64,
        text: String,
        truncated: bool,
    },
    Buf {
        ptr: u64,
        len: u64,
        preview: Vec<u8>,
    },
    Flags {
        raw: u64,
        decoded: String,
    },
}

impl std::fmt::Display for SyscallArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Hex(v) => write!(f, "0x{v:X}"),
            Self::Fd(v) => write!(f, "{v}"),
            Self::Ptr(0) => write!(f, "NULL"),
            Self::Ptr(v) => write!(f, "0x{v:X}"),
            Self::Str {
                text, truncated, ..
            } => {
                write!(
                    f,
                    "\"{}\"{}",
                    text.escape_debug(),
                    if *truncated { "..." } else { "" }
                )
            }
            Self::Buf { len, preview, .. } => {
                let shown: String = preview
                    .iter()
                    .map(|b| {
                        if b.is_ascii_graphic() || *b == b' ' {
                            (*b as char).to_string()
                        } else {
                            format!("\\x{b:02x}")
                        }
                    })
                    .collect();
                let more = if (preview.len() as u64) < *len {
                    "..."
                } else {
                    ""
                };
                write!(f, "\"{shown}\"{more} ({len})")
            }
            Self::Flags { decoded, .. } => write!(f, "{decoded}"),
        }
    }
}

/// What the run did to the outside world, in order.
///
/// Needs no compiled-code instrumentation at all: syscalls and HLE stubs are
/// dispatched by the OS layer, outside the JIT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BehaviorEvent {
    Syscall {
        pc: u64,
        number: u64,
        /// The ABI name, when the table knows this number. `None` is not a
        /// failure -- a report shows `syscall_<n>` rather than guessing.
        name: Option<&'static str>,
        /// The raw argument registers, always. The faithful record: `detail`
        /// is an interpretation of these, and an interpretation can be wrong.
        args: [u64; 6],
        detail: Vec<SyscallArg>,
    },
    Hle {
        pc: u64,
        name: String,
    },
}

impl BehaviorEvent {
    /// One line, the way an analyst reads it.
    pub fn render(&self) -> String {
        match self {
            Self::Syscall {
                number,
                name,
                args,
                detail,
                ..
            } => {
                let name = name
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| format!("syscall_{number}"));
                if detail.is_empty() {
                    // No spec: show the registers rather than an empty call,
                    // since an unknown syscall is exactly what wants looking at.
                    let raw: Vec<String> = args.iter().map(|a| format!("0x{a:X}")).collect();
                    format!("{name}({})", raw.join(", "))
                } else {
                    let rendered: Vec<String> = detail.iter().map(|a| a.to_string()).collect();
                    format!("{name}({})", rendered.join(", "))
                }
            }
            Self::Hle { name, .. } => format!("{name}()"),
        }
    }
}

/// Ordered log of [`BehaviorEvent`]s, bounded so a long run cannot exhaust
/// memory on its own.
#[derive(Debug, Default)]
pub struct BehaviorLog {
    pub events: Vec<BehaviorEvent>,
    /// Events dropped after `limit` was reached.
    pub dropped: u64,
    limit: usize,
}

impl BehaviorLog {
    pub const DEFAULT_LIMIT: usize = 100_000;

    pub fn new() -> Self {
        Self::with_limit(Self::DEFAULT_LIMIT)
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            events: Vec::new(),
            dropped: 0,
            limit,
        }
    }

    fn push(&mut self, event: BehaviorEvent) {
        if self.events.len() >= self.limit {
            self.dropped += 1;
            return;
        }
        self.events.push(event);
    }
}

impl Observer for BehaviorLog {
    fn interest(&self) -> ObserveMask {
        ObserveMask::NONE
    }

    fn on_syscall(&mut self, pc: u64, number: u64, args: &[u64; 6]) {
        self.push(BehaviorEvent::Syscall {
            pc,
            number,
            name: None,
            args: *args,
            detail: Vec::new(),
        });
    }

    fn on_syscall_detailed(
        &mut self,
        pc: u64,
        number: u64,
        name: Option<&'static str>,
        args: &[u64; 6],
        detail: Vec<SyscallArg>,
    ) {
        self.push(BehaviorEvent::Syscall {
            pc,
            number,
            name,
            args: *args,
            detail,
        });
    }

    fn on_hle(&mut self, pc: u64, name: &str) {
        self.push(BehaviorEvent::Hle {
            pc,
            name: name.to_string(),
        });
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_is_the_union_of_what_observers_asked_for() {
        let cov = Coverage::new();
        assert_eq!(cov.interest(), ObserveMask::block());
        let log = BehaviorLog::new();
        assert_eq!(log.interest(), ObserveMask::NONE);
        let union = cov.interest().union(log.interest());
        assert!(union.block);
        assert!(!union.insn, "coverage must not force per-instruction calls");
        assert!(union.any());
    }

    #[test]
    fn coverage_reports_instructions_from_the_blocks_that_ran() {
        let mut cov = Coverage::new();
        cov.on_translate(0x1000, &[(0x1000, 2), (0x1002, 3)]);
        cov.on_translate(0x2000, &[(0x2000, 4)]);
        // Only the first block ever executes.
        cov.on_block(0x1000);
        cov.on_block(0x1000);

        assert_eq!(cov.blocks.get(&0x1000), Some(&2));
        assert_eq!(cov.executed_instructions(), vec![0x1000, 0x1002]);
        assert_eq!(cov.bytes_covered(), 5, "the untaken block must not count");
    }

    #[test]
    fn a_behavior_log_stops_growing_and_says_how_much_it_dropped() {
        let mut log = BehaviorLog::with_limit(2);
        for i in 0..5 {
            log.on_syscall(0x400000 + i, i, &[0; 6]);
        }
        assert_eq!(log.events.len(), 2);
        assert_eq!(log.dropped, 3);
    }
}
