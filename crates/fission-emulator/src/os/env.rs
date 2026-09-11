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
        if answer_processor_userop(emu, userop_name, _input_vals) {
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

/// Does this `CALLOTHER` mean "enter the kernel"?
///
/// Every architecture's SLEIGH spec names its own: x86 has `syscall` and
/// `sysenter`, aarch64's `svc` lifts to `CallSupervisor`, and ARM32's to
/// `software_interrupt`. The router used to test for the name *containing*
/// "syscall", which the last two do not, so those architectures could not make
/// a syscall at all.
///
/// It lives here so a static report and the router read the same list. They
/// were separate for exactly one revision, and the report answered
/// "`software_interrupt`: nothing answers this" about a name the router had
/// been handling all along.
pub fn is_syscall_userop(name: &str) -> bool {
    name == "sysenter"
        || name == "CallSupervisor"
        || name == "software_interrupt"
        || name.eq_ignore_ascii_case("syscall")
        || name.contains("syscall")
}

/// A `CALLOTHER` that is processor semantics rather than an OS service.
///
/// One enum, so the list of names and the behaviour cannot drift apart: a
/// static report asks [`classify_processor_userop`] whether a name is answered
/// at all, and [`answer_processor_userop`] answers it. Two separate match
/// statements would eventually disagree, and the report is only useful while
/// it tells the truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessorUserop {
    /// The `ldxr`/`ldrex` side: mark the monitor.
    ExclusiveAccessMark,
    /// The `stxr`/`strex` side: did the monitor survive?
    ExclusiveMonitorPass,
    /// The status the exclusive store reports.
    ExclusiveMonitorStatus,
    /// Ordering against observers that do not exist here.
    Barrier,
    /// Advisory.
    Hint,
    /// Commit `ISAModeSwitch` to the decode context: ARM/Thumb interworking.
    SetIsaMode,

    // ── ARMv7-M system registers ────────────────────────────────────────────
    GetMainStackPointer,
    SetMainStackPointer,
    GetProcessStackPointer,
    SetProcessStackPointer,
    GetMainStackLimit,
    SetMainStackLimit,
    GetProcessStackLimit,
    SetProcessStackLimit,
    GetBasePriority,
    SetBasePriority,
    EnableIrq,
    DisableIrq,
    IsIrqEnabled,
    EnableFault,
    DisableFault,
    IsFaultEnabled,
    IsCurrentModePrivileged,
    IsThreadMode,
    IsThreadModePrivileged,
    SetThreadModePrivileged,
    IsUsingMainStack,
    SetStackMode,
    CurrentExceptionNumber,
}

/// Which processor userop a name is, if any.
pub fn classify_processor_userop(name: &str) -> Option<ProcessorUserop> {
    use ProcessorUserop::*;
    Some(match name {
        "ExclusiveAccess" => ExclusiveAccessMark,
        "ExclusiveMonitorPass" | "hasExclusiveAccess" => ExclusiveMonitorPass,
        "ExclusiveMonitorsStatus" => ExclusiveMonitorStatus,

        "DataMemoryBarrier"
        | "DataSynchronizationBarrier"
        | "InstructionSynchronizationBarrier"
        | "SpeculationBarrier"
        | "LOAcquire"
        | "LORelease"
        | "LOCK"
        | "UNLOCK"
        | "XACQUIRE"
        | "XRELEASE" => Barrier,

        "HintPreloadData"
        | "HintPreloadDataForWrite"
        | "HintPreloadInstruction"
        | "HintDebug"
        | "HintYield" => Hint,

        "setISAMode" => SetIsaMode,

        "getMainStackPointer" => GetMainStackPointer,
        "setMainStackPointer" => SetMainStackPointer,
        "getProcessStackPointer" => GetProcessStackPointer,
        "setProcessStackPointer" => SetProcessStackPointer,
        "getMainStackPointerLimit" => GetMainStackLimit,
        "setMainStackPointerLimit" => SetMainStackLimit,
        "getProcessStackPointerLimit" => GetProcessStackLimit,
        "setProcessStackPointerLimit" => SetProcessStackLimit,
        "getBasePriority" => GetBasePriority,
        "setBasePriority" => SetBasePriority,
        "enableIRQinterrupts" => EnableIrq,
        "disableIRQinterrupts" => DisableIrq,
        "isIRQinterruptsEnabled" => IsIrqEnabled,
        "enableFIQinterrupts" => EnableFault,
        "disableFIQinterrupts" => DisableFault,
        "isFIQinterruptsEnabled" => IsFaultEnabled,
        "isCurrentModePrivileged" => IsCurrentModePrivileged,
        "isThreadMode" => IsThreadMode,
        "isThreadModePrivileged" => IsThreadModePrivileged,
        "setThreadModePrivileged" => SetThreadModePrivileged,
        "isUsingMainStack" => IsUsingMainStack,
        "setStackMode" => SetStackMode,
        "getCurrentExceptionNumber" => CurrentExceptionNumber,

        _ => return None,
    })
}

/// Answer a processor userop. Returns whether it was one.
///
/// The result, if the op has an output, is left in `emu.callother_result`.
///
/// # Why "do nothing" has to be said out loud
///
/// Several of these really are no-ops here, and it matters that they are
/// *listed* as such. An unanswered userop falls through to a warning and a
/// zero, and until it is named nobody can tell the two cases apart: "a
/// barrier, and one processor has nothing to order" reads exactly like "we
/// have no idea what this instruction does". The unanswered list is only a
/// work queue if the deliberate silences are taken out of it.
pub fn answer_processor_userop(emu: &mut Emulator, name: &str, inputs: &[u64]) -> bool {
    use ProcessorUserop::*;
    let Some(op) = classify_processor_userop(name) else {
        return false;
    };
    let arg = |n: usize| inputs.get(n).copied().unwrap_or(0);
    emu.callother_result = 0;

    match op {
        // ── Exclusive access: ldxr/stxr, ldrex/strex ────────────────────────
        //
        // One processor and no other observer, so the monitor cannot be stolen
        // between the load and the store. The pass succeeds, and the store
        // reports success -- which is *zero*, because that is what the
        // architecture puts in the status register.
        //
        // Getting this wrong is not a small error. Both ARM specs pre-set the
        // status to "failed" and only overwrite it on the success path, so an
        // unanswered `ExclusiveMonitorPass` makes every compare-and-swap retry
        // loop spin for ever. The two aarch64 binaries in the dev corpus that
        // ran to the instruction budget without finishing were doing exactly
        // that, half a million times.
        ExclusiveAccessMark => {}
        ExclusiveMonitorPass => emu.callother_result = 1,
        ExclusiveMonitorStatus => emu.callother_result = 0,

        // A barrier orders this processor's accesses against what another
        // observer can see. There is no other observer, and this emulator
        // executes one instruction at a time in program order.
        Barrier => {}
        // Advisory by definition: a prefetch that does not happen changes
        // timing, and there is no timing here.
        Hint => {}

        // ── ARM/Thumb interworking ──────────────────────────────────────────
        SetIsaMode => emu.commit_isa_mode(),

        // ── ARMv7-M system registers ────────────────────────────────────────
        //
        // `sp` is the *active* stack pointer, banked by `CONTROL.SPSEL`, so
        // only the inactive one is stored. Reading the active one has to go
        // through `sp` or it reports a stale bank.
        GetMainStackPointer => {
            emu.callother_result = if emu.cortex_m.process_stack_active {
                emu.cortex_m.banked_sp
            } else {
                emu.read_stack_pointer()
            };
        }
        SetMainStackPointer => {
            if emu.cortex_m.process_stack_active {
                emu.cortex_m.banked_sp = arg(0);
            } else {
                let _ = emu.write_stack_pointer(arg(0));
            }
        }
        GetProcessStackPointer => {
            emu.callother_result = if emu.cortex_m.process_stack_active {
                emu.read_stack_pointer()
            } else {
                emu.cortex_m.banked_sp
            };
        }
        SetProcessStackPointer => {
            if emu.cortex_m.process_stack_active {
                let _ = emu.write_stack_pointer(arg(0));
            } else {
                emu.cortex_m.banked_sp = arg(0);
            }
        }
        GetMainStackLimit => emu.callother_result = emu.cortex_m.main_stack_limit,
        SetMainStackLimit => emu.cortex_m.main_stack_limit = arg(0),
        GetProcessStackLimit => emu.callother_result = emu.cortex_m.process_stack_limit,
        SetProcessStackLimit => emu.cortex_m.process_stack_limit = arg(0),

        GetBasePriority => emu.callother_result = emu.cortex_m.base_priority,
        SetBasePriority => emu.cortex_m.base_priority = arg(0),

        // No interrupt controller is wired to this emulator, so the masks are
        // recorded and nothing is ever delivered either way. Firmware that
        // reads one back sees what it wrote, which is the part it can tell.
        EnableIrq => emu.cortex_m.irq_enabled = true,
        DisableIrq => emu.cortex_m.irq_enabled = false,
        IsIrqEnabled => emu.callother_result = u64::from(emu.cortex_m.irq_enabled),
        EnableFault => emu.cortex_m.fault_enabled = true,
        DisableFault => emu.cortex_m.fault_enabled = false,
        IsFaultEnabled => emu.callother_result = u64::from(emu.cortex_m.fault_enabled),

        IsCurrentModePrivileged => {
            emu.callother_result = u64::from(emu.cortex_m.current_mode_privileged());
        }
        IsThreadMode => emu.callother_result = u64::from(emu.cortex_m.in_thread_mode()),
        IsThreadModePrivileged => {
            emu.callother_result = u64::from(emu.cortex_m.thread_mode_privileged);
        }
        SetThreadModePrivileged => emu.cortex_m.thread_mode_privileged = arg(0) != 0,
        IsUsingMainStack => {
            emu.callother_result = u64::from(!emu.cortex_m.process_stack_active);
        }
        // The argument is "main stack selected", so selecting the other one
        // swaps which pointer lives in `sp`.
        SetStackMode => {
            let want_process = arg(0) == 0;
            if want_process != emu.cortex_m.process_stack_active {
                let active = emu.read_stack_pointer();
                let _ = emu.write_stack_pointer(emu.cortex_m.banked_sp);
                emu.cortex_m.banked_sp = active;
                emu.cortex_m.process_stack_active = want_process;
            }
        }
        CurrentExceptionNumber => {
            emu.callother_result = u64::from(emu.cortex_m.exception_number);
        }
    }
    true
}
