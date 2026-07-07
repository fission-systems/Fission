use anyhow::Result;
use crate::core::Emulator;
use crate::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;

/// Result of a single HLE dispatch.
pub enum HleResult {
    /// Execution should continue normally (return address has been restored).
    Continue,
    /// The emulated program has requested termination with the given exit code.
    Halt(u32),
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
    fn patch_imports(
        &self,
        state: &mut MachineState,
        binary: &LoadedBinary,
    ) -> Result<()>;

    /// Resolve `magic_addr` to a function name, or `None` if the address is
    /// not a known stub (the emulator should treat this as a fatal error).
    fn resolve_stub(&self, binary: &LoadedBinary, magic_addr: u64) -> Option<String>;

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
        _emu: &mut Emulator,
        userop_name: &str,
        _input_vals: &[u64],
        _output_size: u32,
    ) -> Result<HleResult> {
        tracing::warn!("Unimplemented USEROP: '{}'. Treating as no-op (returns 0).", userop_name);
        Ok(HleResult::Continue)
    }
}
