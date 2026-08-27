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
use std::path::Path;

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
        // Restore baseline -- a called function can mutate globals/heap.
        self.emu.state = self.baseline_state.clone();
        self.emu.pc = self.baseline_pc;

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
            let new_sp = sp - ptr_size - shadow_space;
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

        let outcome = self.emu.run_until_pc(SENTINEL_RET)?;
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
}
