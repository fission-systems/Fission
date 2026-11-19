//! Drives the real `fission-emulator::Emulator` to call one function inside
//! a real, loaded binary image with concrete argument values and read back
//! its real return value -- the ground-truth oracle [`crate::ground_truth`]
//! checks transitional PreHIR/HIR concrete evaluation against.
//!
//! There is no "call just this one function" primitive in `fission-emulator`
//! today (only whole-binary-image `run()`); this module builds it entirely
//! out of existing, exported primitives (`read_register_u64`/
//! `write_register_u64`, `ArchInfo::from_language_id`, `Emulator::
//! run_until_pc`) plus the same image-setup sequence `fission-cli`'s
//! `sandbox` command already uses (`crates/fission-cli/src/cli/oneshot/
//! mod.rs`), rather than reimplementing binary loading.

use anyhow::{Context, Result, bail};
use fission_emulator::pcode::page_map::prot;
use fission_emulator::{
    ArchInfo, Emulator, LinuxEnv, MachineState, OsEnvironment, RunOutcome, WindowsEnv,
};
use fission_loader::loader::LoadedBinary;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A sentinel return address never mapped by the ELF/PE loaders. If control
/// ever reaches it unexpectedly (a bug in the call-setup below, or the
/// callee corrupting its own return address), the next instruction fetch
/// faults loudly instead of running garbage.
const SENTINEL_RET: u64 = 0x0;

/// Outcome of calling one function through the real emulator. Distinct,
/// reported outcomes -- never coerced into a silent pass/fail.
#[derive(Debug)]
pub enum CallOutcome {
    /// The function returned normally; the value read from the
    /// architecture's return register (unmasked -- caller narrows to the
    /// function's declared return width).
    Returned(u64),
    /// Hit the instruction budget before returning (non-terminating or very
    /// deep call).
    HitBudget,
    /// The callee terminated the whole process (e.g. called `exit`/`abort`)
    /// rather than returning normally.
    ProcessExited,
    /// Execution stopped for a reason that isn't a normal return (fault,
    /// unsupported instruction, symbolic gate, interrupt, etc).
    Other(RunOutcome),
}

/// One call the function made, seen at the call instruction before it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedCall {
    /// The call instruction's address.
    pub site: u64,
    /// The calling convention's integer argument registers, in order.
    pub args: Vec<u64>,
}

/// How a stubbed run ended.
#[derive(Debug)]
pub enum StubbedEnd {
    /// The function returned: the return register's raw value.
    Returned(u64),
    /// It reached a call the decoder found no fall-through for -- a call
    /// that does not return. The run stops there.
    NoReturnCall(u64),
    /// Anything else: budget, fault, exit.
    Other(CallOutcome),
}

/// A call into one function with every call it makes stubbed out.
#[derive(Debug)]
pub struct StubbedRun {
    pub end: StubbedEnd,
    pub calls: Vec<ObservedCall>,
    /// Every byte the function stored outside its own stack frame, with the
    /// value it held when the run ended.
    pub writes: BTreeMap<u64, u8>,
}

/// Records the address range of every RAM write.
struct WriteLog(Arc<Mutex<Vec<(u64, u32)>>>);

impl fission_emulator::observe::Observer for WriteLog {
    fn interest(&self) -> fission_emulator::observe::ObserveMask {
        fission_emulator::observe::ObserveMask {
            mem: true,
            ..fission_emulator::observe::ObserveMask::NONE
        }
    }

    fn on_mem(&mut self, addr: u64, size: u32, write: bool, _value: u64) {
        if write {
            self.0.lock().expect("write log").push((addr, size));
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Stubbed calls one run may make before it is stopped as over budget.
///
/// A stub returns the same values whatever it is passed, so a loop that
/// waits on a callee to make progress (`while (n) n -= read(..)`) never ends.
/// Each call is a round trip out of compiled code; the instruction budget
/// alone let such a loop run for minutes.
pub const MAX_STUBBED_CALLS: usize = 4096;

/// How far below the entry stack pointer a function's own frame may reach.
/// Writes there are its locals, which the decompiled body keeps in variables.
const FRAME_BELOW: u64 = 1 << 20;

/// How far above the entry stack pointer still belongs to the call: the
/// return address and the Win64 home slots a prologue spills arguments to.
const FRAME_ABOVE: u64 = 64;

/// A real emulator with a real, loaded process image, reusable across many
/// concrete calls into the same binary. Restores to the post-image-setup
/// baseline before every call (see [`Self::call`]) so one call's global/heap
/// side effects can't leak into the next.
pub struct EmulatorHarness {
    emu: Emulator,
    baseline_state: MachineState,
    baseline_pc: u64,
    /// Address of the scratch buffer, once installed.
    scratch: Option<u64>,
}

/// Stack pointer at the callee's entry for a call with `stack_args` stack
/// passed arguments, given the caller's baseline `sp`.
///
/// The return address sits at the new `sp`, the calling convention's own gap
/// (`shadow_space`: Win64's 32 bytes, zero elsewhere) above it, and the
/// stack-passed arguments above that. All of them have to live *below* the
/// baseline `sp`: that is where a real caller's outgoing area is, whereas the
/// memory at and above `sp` belongs to the entry frame (the auxv/argv tables
/// on Linux, the top of the mapped stack on Windows).
fn call_frame_sp(sp: u64, ptr_size: u64, shadow_space: u64, stack_args: u64) -> u64 {
    // Round the argument area up to 16 so the entry alignment survives.
    let arg_area = (stack_args * ptr_size).div_ceil(16) * 16;
    sp - arg_area - ptr_size - shadow_space
}

impl EmulatorHarness {
    /// Build a fully-loaded emulator for `binary_path`, mirroring
    /// `fission-cli`'s `sandbox` setup sequence exactly (format-aware image
    /// load, Sleigh frontend selection, `ArchInfo` derived from the
    /// binary's own language ID -- independent of any decompiler-side
    /// `CallingConvention` enum).
    pub fn build(binary_path: &Path, max_inst: Option<u64>) -> Result<Self> {
        let binary = LoadedBinary::from_file(binary_path)
            .with_context(|| format!("failed to read binary at {}", binary_path.display()))?;

        let mut state = MachineState::new();
        let mut linux_image = None;
        let mut pe_image = None;
        match binary.format.as_str() {
            "PE" => {
                pe_image = Some(fission_emulator::os::windows::loader::load_pe(
                    &mut state, &binary,
                )?);
            }
            "ELF" | "ELF64" => {
                linux_image = Some(fission_emulator::os::linux::loader::load_elf(
                    &mut state, &binary,
                )?);
            }
            fmt => bail!("unsupported binary format for emulator ground truth: {fmt}"),
        }

        let load_spec = binary
            .load_spec()
            .context("binary lacks a load_spec (can't select a Sleigh frontend)")?;
        let frontends =
            fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(
                load_spec,
            )
            .context("failed to create Sleigh frontend candidates")?;
        let sleigh = frontends
            .into_iter()
            .next()
            .context("no suitable Sleigh frontend found")?;

        let lang_id = load_spec.pair.language_id.as_str();
        let arch = ArchInfo::from_language_id(lang_id, Some(&binary))
            .with_context(|| format!("unsupported architecture: {lang_id}"))?;

        let os: Box<dyn OsEnvironment> = match binary.format.as_str() {
            "PE" => Box::new(WindowsEnv::new()),
            "ELF" | "ELF64" => Box::new(LinuxEnv::new()),
            fmt => bail!("unsupported binary format for emulator ground truth: {fmt}"),
        };

        let mut emu = Emulator::new(state, binary, sleigh, arch, os)?.with_max_inst(max_inst);

        if let Some(info) = linux_image {
            emu.apply_linux_image(info)?;
        }
        if let Some(info) = pe_image {
            emu.apply_windows_image(info)?;
        }

        let baseline_state = emu.state.clone();
        let baseline_pc = emu.pc;
        Ok(Self {
            emu,
            baseline_state,
            baseline_pc,
            scratch: None,
        })
    }

    /// Map a readable/writable scratch buffer and fill it, returning its
    /// address.
    ///
    /// Exists so a pointer parameter can be given something real to point at.
    /// Without it, a boundary-value sweep hands a function like `list_sum` the
    /// integers `0`, `1`, `-1`: every one but null faults the call outright,
    /// and null returns before the loop body runs. The function grounds on one
    /// sample of seven and none of its control flow is ever exercised, which
    /// is why this tier could not see a structuring bug.
    ///
    /// The buffer becomes part of the restored baseline, so its contents are
    /// identical for every call rather than carrying one sample's writes into
    /// the next. The caller is responsible for placing the same bytes at the
    /// same address in whatever evaluates the decompiled side -- the two
    /// memories only agree because both are told the same thing.
    pub fn install_scratch(&mut self, contents: &[u8]) -> Result<u64> {
        if let Some(addr) = self.scratch {
            return Ok(addr);
        }
        let len = contents.len().max(1) as u64;
        let addr = self
            .emu
            .state
            .page_map
            .mmap_anon(len, prot::READ | prot::WRITE);
        let ram = self.emu.state.ram_space();
        self.emu
            .state
            .write_space(ram, addr, contents)
            .context("failed to fill the scratch buffer")?;
        // Re-baseline so every later call sees the buffer, and sees it
        // unmodified.
        self.baseline_state = self.emu.state.clone();
        self.scratch = Some(addr);
        Ok(addr)
    }

    /// Read the machine registers a decompiled body might name, as they
    /// stand at the start of a call.
    ///
    /// A decompiled body routinely reads a register the function never wrote
    /// -- `rax`, `cf`, `rbp` -- because the value came from the caller. The
    /// interpreter has no such state and refuses, which is right in isolation
    /// but leaves those functions permanently uncheckable: measured on the
    /// corpus's own functions, undeclared reads are the second largest reason
    /// this tier says nothing, and roughly five in six of them are registers.
    ///
    /// So take the values from the machine and hand the interpreter the same
    /// ones -- the same trick as the scratch buffer, and sound for the same
    /// reason: the two sides agree because both were told what the *emulator*
    /// is about to do, not because either guessed.
    ///
    /// Call after [`Self::install_scratch`] and before [`Self::call`]; the
    /// values come from the restored baseline, which is what every call
    /// starts from.
    pub fn entry_registers(&mut self, names: &[String]) -> Vec<(String, i64)> {
        self.emu.state = self.baseline_state.clone();
        names
            .iter()
            .filter_map(|n| {
                self.emu
                    .read_register_u64(n)
                    .ok()
                    .map(|v| (n.clone(), v as i64))
            })
            .collect()
    }

    /// Call `address` with `args` (in declared-parameter order, restricted
    /// by the caller to `Bool`/`Int`-typed parameters -- see this crate's
    /// scope notes), and read back the real return value. Restores the
    /// post-image-setup baseline first, so this is safe to call repeatedly
    /// for a boundary-value sweep without one sample's side effects leaking
    /// into the next.
    pub fn call(&mut self, address: u64, args: &[u64]) -> Result<CallOutcome> {
        self.prepare_call(address, args)?;
        let outcome = self.emu.run_until_pc(SENTINEL_RET)?;
        self.finish_outcome(outcome)
    }

    /// Read one byte of the restored baseline: the program image as every
    /// call starts from it. `None` where nothing is mapped.
    pub fn baseline_byte(&self, addr: u64) -> Option<u8> {
        let ram = self.baseline_state.ram_space();
        self.baseline_state
            .read_space_readonly(ram, addr, 1)
            .ok()
            .and_then(|bytes| bytes.first().copied())
    }

    /// Call `address` as [`Self::call`] does, but stop at every call
    /// instruction in `sites` instead of running the callee: record the
    /// argument registers, put `call_return(n)` in the return register and
    /// continue at the site's fall-through. A site mapped to `None` has no
    /// fall-through and ends the run.
    ///
    /// Stubbing is what lets the decompiled body be compared at all: both
    /// sides see the same return values without either having to model the
    /// callee. What it costs is the callee's own effects -- an out-parameter
    /// it would have written stays as it was, on both sides alike.
    pub fn call_stubbed(
        &mut self,
        address: u64,
        args: &[u64],
        sites: &BTreeMap<u64, Option<u64>>,
        call_return: &dyn Fn(usize) -> u64,
    ) -> Result<StubbedRun> {
        self.prepare_call(address, args)?;
        let sp_reg = self.emu.arch.sp_reg;
        let entry_sp = self.emu.read_register_u64(sp_reg)?;
        let log = Arc::new(Mutex::new(Vec::new()));
        self.emu.add_observer(Box::new(WriteLog(Arc::clone(&log))));
        for site in sites.keys() {
            self.emu.set_breakpoint(*site);
        }
        let arg_regs: Vec<&'static str> = self.emu.arch.cc.arg_regs().to_vec();
        let return_reg = self.emu.arch.cc.return_reg();
        let mut calls = Vec::new();
        let end = loop {
            let outcome = match self.emu.run_until_pc(SENTINEL_RET) {
                Ok(outcome) => outcome,
                Err(err) => {
                    self.release_run();
                    return Err(err);
                }
            };
            match outcome {
                RunOutcome::HitBreakpoint(pc) if sites.contains_key(&pc) => {
                    if calls.len() >= MAX_STUBBED_CALLS {
                        break StubbedEnd::Other(CallOutcome::HitBudget);
                    }
                    let args = arg_regs
                        .iter()
                        .map(|reg| self.emu.read_register_u64(reg))
                        .collect::<Result<Vec<_>>>()?;
                    let n = calls.len();
                    calls.push(ObservedCall { site: pc, args });
                    let Some(next) = sites[&pc] else {
                        break StubbedEnd::NoReturnCall(pc);
                    };
                    self.emu.write_register_u64(return_reg, call_return(n))?;
                    self.emu.pc = next;
                }
                RunOutcome::Returned => {
                    break StubbedEnd::Returned(self.emu.read_register_u64(return_reg)?);
                }
                other => break StubbedEnd::Other(self.finish_outcome(other)?),
            }
        };
        self.release_run();
        let ram = self.emu.state.ram_space();
        let frame = entry_sp.saturating_sub(FRAME_BELOW)..entry_sp.saturating_add(FRAME_ABOVE);
        let mut written = BTreeSet::new();
        for (addr, size) in log.lock().expect("write log").iter() {
            for i in 0..u64::from(*size) {
                let at = addr.wrapping_add(i);
                if !frame.contains(&at) {
                    written.insert(at);
                }
            }
        }
        let mut writes = BTreeMap::new();
        for at in written {
            if let Ok(bytes) = self.emu.state.read_space(ram, at, 1) {
                writes.insert(at, bytes[0]);
            }
        }
        Ok(StubbedRun { end, calls, writes })
    }

    /// Undo what [`Self::call_stubbed`] armed for one run.
    fn release_run(&mut self) {
        self.emu.clear_all_breakpoints();
        drop(self.emu.take_observers());
    }

    fn finish_outcome(&mut self, outcome: RunOutcome) -> Result<CallOutcome> {
        match outcome {
            RunOutcome::Returned => {
                let reg = self.emu.arch.cc.return_reg();
                Ok(CallOutcome::Returned(self.emu.read_register_u64(reg)?))
            }
            RunOutcome::HitBudget => Ok(CallOutcome::HitBudget),
            RunOutcome::ProcessExited => Ok(CallOutcome::ProcessExited),
            other => Ok(CallOutcome::Other(other)),
        }
    }

    /// Restore the baseline and set up a call of `address` with `args`,
    /// stopped at its first instruction.
    fn prepare_call(&mut self, address: u64, args: &[u64]) -> Result<()> {
        // Restore baseline -- a called function can mutate globals/heap.
        self.emu.state = self.baseline_state.clone();
        self.emu.pc = self.baseline_pc;
        // The instruction budget is per call. The count lives on the
        // emulator, not in the restored state, so without this every call
        // spent what the earlier ones left and a batch ran out partway.
        self.emu.inst_count = 0;

        // Set up the call like a real `call`/`bl`: write the sentinel
        // return address to the link register (ARM/MIPS/PPC) or push it on
        // the stack (x86) -- exactly what `simulate_return()` already
        // expects to read back when the callee's own `ret` executes.
        self.emu.pc = address;
        if let Some(lr) = self.emu.arch.cc.return_addr_reg() {
            self.emu.write_register_u64(lr, SENTINEL_RET)?;
        } else {
            let sp_reg = self.emu.arch.sp_reg;
            let ptr_size = self.emu.arch.pointer_size as u64;
            let sp = self.emu.read_register_u64(sp_reg)?;
            // `stack_arg_offset(0) - ptr_size` is the CC's own required gap
            // between the return address and the first stack-passed arg --
            // on Win64 that's the mandatory 32-byte "shadow space" the
            // callee's prologue spills its register args into (`stack_
            // arg_offset(0) == 0x28`: 8-byte return address + 0x20 shadow
            // space); SysV64/cdecl have no such gap (`stack_arg_offset(0)
            // == ptr_size`, so this is 0). Without reserving it, a callee
            // that spills its register args to their stack home slots (as
            // `clamp`'s real prologue does) corrupts whatever memory
            // happens to sit above our minimal stack frame.
            let shadow_space = self
                .emu
                .arch
                .cc
                .stack_arg_offset(0)
                .saturating_sub(ptr_size);
            let stack_args = (args.len().saturating_sub(self.emu.arch.cc.arg_regs().len())) as u64;
            let new_sp = call_frame_sp(sp, ptr_size, shadow_space, stack_args);
            let ram = self.emu.state.ram_space();
            let bytes = SENTINEL_RET.to_le_bytes()[..ptr_size as usize].to_vec();
            self.emu.state.write_space(ram, new_sp, &bytes)?;
            self.emu.write_register_u64(sp_reg, new_sp)?;
        }

        // Inject args via the calling convention's arg registers/stack
        // slots -- mirrors `read_arg`'s logic in reverse.
        let arg_regs: Vec<&'static str> = self.emu.arch.cc.arg_regs().to_vec();
        let ptr_size = self.emu.arch.pointer_size as u64;
        let sp_reg = self.emu.arch.sp_reg;
        for (i, &value) in args.iter().enumerate() {
            if i < arg_regs.len() {
                self.emu.write_register_u64(arg_regs[i], value)?;
            } else {
                let n = i - arg_regs.len();
                let stack_off = self.emu.arch.cc.stack_arg_offset(n);
                let sp = self.emu.read_register_u64(sp_reg)?;
                let ram = self.emu.state.ram_space();
                let bytes = value.to_le_bytes()[..ptr_size as usize].to_vec();
                self.emu.state.write_space(ram, sp + stack_off, &bytes)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::call_frame_sp;

    /// Where the callee finds stack argument `n`, as `call` writes it:
    /// `new_sp + stack_arg_offset(n)` with `stack_arg_offset(n) = ptr + shadow
    /// + n * ptr` (SysV/cdecl have `shadow == 0`, Win64 has 32).
    fn arg_slot(new_sp: u64, ptr: u64, shadow: u64, n: u64) -> u64 {
        new_sp + ptr + shadow + n * ptr
    }

    #[test]
    fn stack_arguments_live_below_the_baseline_sp() {
        for (ptr, shadow) in [(8u64, 0u64), (8, 32), (4, 0)] {
            for stack_args in [0u64, 1, 2, 5, 9] {
                let sp = 0x7fff_ffff_e000u64;
                let new_sp = call_frame_sp(sp, ptr, shadow, stack_args);
                for n in 0..stack_args {
                    let slot = arg_slot(new_sp, ptr, shadow, n);
                    assert!(
                        slot + ptr <= sp,
                        "ptr {ptr} shadow {shadow}: argument {n} of {stack_args} at {slot:#x} reaches the baseline sp {sp:#x}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_entry_stack_pointer_keeps_its_abi_alignment() {
        // At function entry (return address just pushed) `sp + ptr` is 16-byte
        // aligned on SysV and Win64, whatever the number of stack arguments.
        for (ptr, shadow) in [(8u64, 0u64), (8, 32)] {
            for stack_args in 0..12u64 {
                let new_sp = call_frame_sp(0x7fff_ffff_e000, ptr, shadow, stack_args);
                assert_eq!(
                    (new_sp + ptr) % 16,
                    0,
                    "ptr {ptr} shadow {shadow} stack_args {stack_args}"
                );
            }
        }
    }

    #[test]
    fn a_call_without_stack_arguments_is_unchanged() {
        assert_eq!(call_frame_sp(0x1000, 8, 0, 0), 0x1000 - 8);
        assert_eq!(call_frame_sp(0x1000, 8, 32, 0), 0x1000 - 8 - 32);
    }
}
