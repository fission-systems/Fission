use crate::arch::ArchInfo;
use crate::os::env::{HleResult, OsEnvironment};
use crate::pcode::state::MachineState;
use crate::pcode::eval::{Evaluator, StepResult};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use anyhow::{Result, Context};
use std::sync::Arc;

/// Arch-agnostic emulator.
///
/// `pc` is the architecture-independent program counter.
/// `arch` carries all architecture-specific metadata (register names, CC, …).
/// `os`  carries all OS-specific behaviour (import patching, HLE dispatch, …).
pub struct Emulator {
    pub state: MachineState,
    pub binary: LoadedBinary,
    pub sleigh: Arc<RuntimeSleighFrontend>,
    /// Architecture-independent program counter (replaces the old `rip` field).
    pub pc: u64,
    pub register_map: std::collections::HashMap<String, (u64, u64, u32)>,
    /// Architecture metadata: PC/SP register names, pointer size, CC, …
    pub arch: ArchInfo,
    /// OS execution environment: import patching, HLE dispatch, …
    pub os: Box<dyn OsEnvironment>,
}

impl Emulator {
    /// Construct an emulator for the given binary.
    ///
    /// `arch` and `os` are chosen by the caller (typically the CLI / sandbox
    /// entry point) based on the binary's target platform.
    pub fn new(
        mut state: MachineState,
        binary: LoadedBinary,
        sleigh: RuntimeSleighFrontend,
        arch: ArchInfo,
        os: Box<dyn OsEnvironment>,
    ) -> Result<Self> {
        let pc = binary.inner().entry_point;

        // Patch imports (IAT/PLT/MMIO) before execution starts.
        os.patch_imports(&mut state, &binary)?;

        let register_map = if let Some(spec) = binary.load_spec() {
            fission_sleigh::runtime::register_map_for_load_spec(spec)
                .unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        let mut emu = Self {
            state,
            binary,
            sleigh: Arc::new(sleigh),
            pc,
            register_map,
            arch,
            os,
        };

        // Initialize stack pointer using arch-agnostic sp_reg name.
        let sp_init = if emu.arch.pointer_size == 8 { 0x7FFFFFFF0000u64 } else { 0x7FFF0000u64 };
        let _ = emu.write_register_u64(emu.arch.sp_reg, sp_init);

        Ok(emu)
    }

    // ── Register I/O ─────────────────────────────────────────────────────────

    pub fn read_register_u64(&mut self, name: &str) -> Result<u64> {
        let (space_id, offset, size) = self.register_map.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
            .ok_or_else(|| anyhow::anyhow!("Register {} not found in register_map", name))?;

        if size > 8 {
            anyhow::bail!("Register {} is too large to read as u64 (size={})", name, size);
        }

        let bytes = self.state.read_space(space_id, offset, size as usize)?;
        let mut val = 0u64;
        for (i, &b) in bytes.iter().enumerate() {
            val |= (b as u64) << (i * 8);
        }
        Ok(val)
    }

    pub fn write_register_u64(&mut self, name: &str, mut val: u64) -> Result<()> {
        let (space_id, offset, size) = self.register_map.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
            .ok_or_else(|| anyhow::anyhow!("Register {} not found in register_map", name))?;

        if size > 8 {
            anyhow::bail!("Register {} is too large to write as u64 (size={})", name, size);
        }

        let mut bytes = Vec::with_capacity(size as usize);
        for _ in 0..size {
            bytes.push((val & 0xFF) as u8);
            val >>= 8;
        }
        self.state.write_space(space_id, offset, &bytes)
    }

    // ── Arch-agnostic CC convenience helpers ─────────────────────────────────
    // These avoid the borrow-checker conflict that arises when calling
    // `emu.arch.cc.method(emu, ...)` — both borrows of `emu` would be live.

    /// Read the `index`-th integer argument according to `arch.cc`.
    pub fn read_arg(&mut self, index: usize) -> Result<u64> {
        let regs = self.arch.cc.arg_regs();
        if index < regs.len() {
            let reg = regs[index];
            self.read_register_u64(reg)
        } else {
            let n = index - regs.len();
            let stack_off = self.arch.cc.stack_arg_offset(n);
            let ptr_size  = self.arch.pointer_size as usize;
            let sp_reg    = self.arch.sp_reg;
            let sp        = self.read_register_u64(sp_reg)?;
            let bytes     = self.state.read_space(3, sp + stack_off, ptr_size)?;
            Ok(crate::arch::calling_convention::le_bytes_to_u64(&bytes))
        }
    }

    /// Write `value` to the return-value register according to `arch.cc`.
    pub fn write_return_val(&mut self, value: u64) -> Result<()> {
        let reg = self.arch.cc.return_reg();
        self.write_register_u64(reg, value)
    }

    /// Simulate a function return: restore PC from the return address
    /// (link register or top of stack, depending on `arch.cc`).
    pub fn simulate_return(&mut self) -> Result<()> {
        let ptr_size = self.arch.pointer_size as usize;
        if let Some(lr) = self.arch.cc.return_addr_reg() {
            let ret_addr = self.read_register_u64(lr)?;
            self.pc = ret_addr;
        } else {
            let sp_reg = self.arch.sp_reg;
            let sp     = self.read_register_u64(sp_reg)?;
            let bytes  = self.state.read_space(3, sp, ptr_size)?;
            let ret_addr = crate::arch::calling_convention::le_bytes_to_u64(&bytes);
            self.pc = ret_addr;
            self.write_register_u64(sp_reg, sp + ptr_size as u64)?;
        }
        Ok(())
    }

    // ── Execution ─────────────────────────────────────────────────────────────

    pub fn run_instruction(&mut self) -> Result<bool> {
        tracing::debug!("Executing PC=0x{:X}", self.pc);

        // Fetch up to 16 bytes from RAM (Space 3).
        let bytes_vec = match self.state.read_space(3, self.pc, 16) {
            Ok(b)  => b,
            Err(_) => {
                tracing::error!("Failed to fetch instruction bytes at 0x{:X}", self.pc);
                return Ok(false);
            }
        };

        // Decode and lift to P-Code.
        let (pcode_ops, inst_len) = self.sleigh
            .decode_and_lift_with_len(&bytes_vec, self.pc)
            .with_context(|| format!("Failed to lift instruction at 0x{:X}", self.pc))?;

        // Pre-advance the PC register inside the Sleigh state so that
        // %pc-relative addressing and `call` (which pushes the return address)
        // observe the correct next-instruction address.
        let next_pc = self.pc + inst_len;
        let _ = self.write_register_u64(self.arch.pc_reg, next_pc);

        // Evaluate P-Code ops.
        let mut branched = false;
        let mut branch_target = 0u64;
        let mut evaluator = Evaluator::new(&mut self.state);
        for op in pcode_ops {
            tracing::debug!("    P-Code: {:?}", op.opcode);
            match evaluator.step(&op)? {
                StepResult::Next         => {}
                StepResult::Branch(tgt)  => {
                    branched      = true;
                    branch_target = tgt;
                    break;
                }
            }
        }

        // Update our PC tracker.
        self.pc = if branched { branch_target } else { next_pc };

        // HLE trap check — any address in the upper magic range.
        if self.pc >= 0xFFFFFFF000000000 {
            let magic = self.pc;
            // Resolve stub name through the OS environment.
            // We have to borrow binary independently to avoid holding a borrow
            // on `self.os` (which implements OsEnvironment) while also calling
            // mutable methods on self.
            let func_name = {
                // SAFETY: os and binary are separate fields; Rust doesn't see them
                // as separate without unsafe, so we extract what we need first.
                let func_name_opt = self.os.resolve_stub(&self.binary, magic);
                func_name_opt.unwrap_or_else(|| format!("Unknown@0x{:X}", magic))
            };

            // Dispatch — OS sets the return value.
            // We must call dispatch_hle via a raw pointer to avoid double-borrow,
            // because dispatch_hle takes `&mut Emulator` but `self.os` is also on self.
            let result = {
                // SAFETY: `self.os` and the rest of `Emulator` are separate allocations.
                // We borrow `self.os` via a raw pointer for the duration of dispatch.
                let os_ptr = &*self.os as *const dyn OsEnvironment;
                let os_ref = unsafe { &*os_ptr };
                os_ref.dispatch_hle(self, &func_name)?
            };

            match result {
                HleResult::Halt(_code) => return Ok(false),
                HleResult::Continue    => {
                    // Restore PC from the return address (stack or link register).
                    self.simulate_return()?;
                }
            }
        }

        Ok(true)
    }

    pub fn run(&mut self) -> Result<()> {
        tracing::info!("Sandbox execution started at PC=0x{:X}", self.pc);
        loop {
            if !self.run_instruction()? {
                break;
            }
        }
        tracing::info!("Sandbox execution finished");
        Ok(())
    }
}
