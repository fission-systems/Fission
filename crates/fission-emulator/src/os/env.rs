use crate::core::Emulator;
use crate::pcode::state::MachineState;
use anyhow::Result;
use fission_loader::loader::LoadedBinary;

/// Result of a single HLE dispatch.
pub enum HleResult {
    /// Execution should continue normally (return address has been restored).
    Continue,
    /// The emulated program has requested termination with the given exit code.
    Halt(u32),
    /// Jump to `pc` without popping a return address (e.g. `__libc_start_main` → main).
    JumpTo(u64),
}

/// Abstraction over an OS execution environment.
///
/// Each concrete implementation handles one OS (Windows, Linux, bare-metal…)
/// independently of the guest architecture.  The emulator holds a
/// `Box<dyn OsEnvironment>` and calls into it:
///
/// 1. Once at load time, to patch import stubs into the RAM image.
/// 2. On every HLE trap (magic address hit), to identify and emulate the
///    intercepted function.
/// 3. On every `CallOther` (USEROP) P-Code op, to emulate the user-defined
///    operation (e.g. LOCK prefix, REP string ops, CPUID, RDTSC, etc.).
pub trait OsEnvironment: Send + Sync {
    /// Patch all external-function stubs in `state` for the given `binary`.
    ///
    /// - PE: overwrites IAT entries with magic trampolines
    /// - ELF: overwrites GOT slots for PLT entries
    /// - Bare-metal: registers MMIO ranges
    fn patch_imports(&self, state: &mut MachineState, binary: &LoadedBinary) -> Result<()>;

    /// Resolve `magic_addr` to a function name, or `None` if the address is
    /// not a known stub (the emulator should treat this as a fatal error).
    fn resolve_stub(&self, binary: &LoadedBinary, magic_addr: u64) -> Option<String>;

    /// The half-open address range this environment's trampolines occupy.
    ///
    /// The run loop tests every PC against it, so it has to be somewhere the
    /// guest can never map -- and *where* that is depends on the process. In
    /// 64 bits it is above the canonical hole; a 32-bit process has no such
    /// address, so its region has to sit in the part of its own address space
    /// the loader leaves empty. Read after `patch_imports`, which is where an
    /// implementation learns which of the two it is looking at.
    fn magic_range(&self) -> (u64, u64) {
        (0xFFFF_FFF0_0000_0000, u64::MAX)
    }

    /// Dispatch an HLE call for `func_name`.
    ///
    /// Implementations should:
    /// 1. Parse arguments via `emu.arch.cc.read_arg(emu, n)`.
    /// 2. Write a return value via `emu.arch.cc.write_return(emu, val)`.
    /// 3. Return `HleResult::Continue` (the emulator will call
    ///    `emu.arch.cc.simulate_return(emu)` afterward to restore PC).
    /// 4. Return `HleResult::Halt(code)` for termination requests.
    fn dispatch_hle(&self, emu: &mut Emulator, func_name: &str) -> Result<HleResult>;

    /// Dispatch a Sleigh USEROP (`CallOther`) operation.
    ///
    /// `userop_name` is the name from the `.sla` `<userop_head>` table, e.g.
    /// `"lock_cmpxchg"`, `"rep_stosb"`, `"cpuid"`.
    /// `input_vals` are the evaluated input operand values.
    /// `output_size` is the byte-width of the output varnode (0 if no output).
    ///
    /// Default: log a warning and treat as no-op (returns 0 to any output).
    fn dispatch_userop(
        &self,
        emu: &mut Emulator,
        userop_name: &str,
        _input_vals: &[u64],
        _output_size: u32,
    ) -> Result<HleResult> {
        if answer_processor_userop(emu, userop_name) {
            return Ok(HleResult::Continue);
        }
        tracing::warn!(
            "Unimplemented USEROP: '{}'. Treating as no-op (returns 0).",
            userop_name
        );
        // A no-op behind a warning is a wrong value the run never mentions.
        // Recording it is what lets a report say which of the userops a corpus
        // reaches are actually answered.
        emu.metrics.note_unhandled_userop(userop_name);
        Ok(HleResult::Continue)
    }
}

/// `CALLOTHER`s that are processor semantics rather than OS services.
///
/// These are answered the same way under every environment, so they live here
/// rather than three times over. `None` means the name is not one of them.
///
/// Taking only the name -- and giving back the value rather than writing it --
/// is what lets a static report ask "is this answered?" without an emulator to
/// ask it of. The translation-coverage benchmark cannot run a corpus binary,
/// so without this it can only list the userops a corpus reaches and not say
/// which of them anyone answers.
///
/// # Why "do nothing" has to be said out loud
///
/// Some of these really are no-ops on this emulator, and it matters that they
/// are *listed* as such. An unanswered userop falls through to a warning and a
/// zero, and until it is named nobody can tell the two cases apart: "a barrier,
/// and one processor has nothing to order" reads exactly like "we have no idea
/// what this instruction does". The unanswered list is only a work queue if
/// the deliberate silences are taken out of it.
pub fn processor_userop_result(name: &str) -> Option<u64> {
    Some(match name {
        // ── Exclusive access: ldxr/stxr, ldrex/strex ────────────────────────
        //
        // One processor and no other observer, so the monitor cannot be
        // stolen between the load and the store. The pass succeeds, and the
        // store reports success -- which is *zero*, because that is what the
        // architecture puts in the status register.
        //
        // Getting this wrong is not a small error. Both SLEIGH specs pre-set
        // the status to "failed" and only overwrite it on the success path, so
        // an unanswered `ExclusiveMonitorPass` makes every compare-and-swap
        // retry loop spin for ever. The two aarch64 binaries in the dev corpus
        // that ran to the instruction budget without finishing were doing
        // exactly that, half a million times.
        "ExclusiveMonitorPass" | "hasExclusiveAccess" => 1,
        "ExclusiveMonitorsStatus" => 0,

        // ── Barriers ────────────────────────────────────────────────────────
        //
        // A barrier orders this processor's accesses against what another
        // observer can see. There is no other observer, and this emulator
        // executes one instruction at a time in program order, so the ordering
        // a barrier asks for already holds.
        "DataMemoryBarrier"
        | "DataSynchronizationBarrier"
        | "InstructionSynchronizationBarrier"
        | "SpeculationBarrier"
        | "LOAcquire"
        | "LORelease"
        | "LOCK"
        | "UNLOCK"
        | "XACQUIRE"
        | "XRELEASE" => 0,

        // ── Hints ───────────────────────────────────────────────────────────
        //
        // Advisory by definition: a prefetch that does not happen changes
        // timing and nothing else, and there is no timing here.
        "HintPreloadData"
        | "HintPreloadDataForWrite"
        | "HintPreloadInstruction"
        | "HintDebug"
        | "HintYield" => 0,

        _ => return None,
    })
}

/// [`processor_userop_result`], applied. Returns whether it answered.
pub fn answer_processor_userop(emu: &mut Emulator, name: &str) -> bool {
    match processor_userop_result(name) {
        Some(value) => {
            emu.callother_result = value;
            true
        }
        None => false,
    }
}
