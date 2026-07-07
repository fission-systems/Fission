use crate::arch::ArchInfo;
use crate::os::env::{HleResult, OsEnvironment};
use crate::pcode::state::MachineState;
use crate::pcode::eval::Evaluator;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use anyhow::{Result, Context};
use std::sync::Arc;
use std::collections::BTreeMap;
use crate::snapshot::EmulatorSnapshot;
use crate::trace::{ExecutionTrace, TraceEntry};
pub static IS_INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

    pub snapshots: Vec<EmulatorSnapshot>,
    pub snapshot_triggers: Vec<u64>,

    /// Execution trace (enabled when `--dump-trace` is requested).
    pub trace: ExecutionTrace,

    /// USEROP id → name table extracted from the Sleigh compiled frontend.
    pub userop_map: BTreeMap<u32, String>,

    /// Count of executed instructions.
    pub inst_count: u64,
    /// Optional limit on the maximum number of instructions to execute.
    pub max_inst: Option<u64>,
    /// Optional buffer to mock standard input (`stdin`).
    pub stdin_buffer: Option<Vec<u8>>,
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

        let sleigh_arc = Arc::new(sleigh);

        let userop_map = {
            // Extract the userop table from the compiled frontend if available.
            // We do a single cheap decode to retrieve the details.
            let probe_bytes = vec![0u8; 16];
            if let Ok((_, _, details)) = sleigh_arc.decode_and_lift_with_details(&probe_bytes, pc) {
                details.userops
            } else {
                BTreeMap::new()
            }
        };

        let mut emu = Self {
            state,
            binary,
            sleigh: sleigh_arc,
            pc,
            register_map,
            arch,
            os,
            snapshots: Vec::new(),
            snapshot_triggers: Vec::new(),
            trace: ExecutionTrace::disabled(),
            userop_map,
            inst_count: 0,
            max_inst: None,
            stdin_buffer: None,
        };

        // Initialize stack pointer using arch-agnostic sp_reg name.
        let sp_init = if emu.arch.pointer_size == 8 { 0x7FFFFFFF0000u64 } else { 0x7FFF0000u64 };
        let _ = emu.write_register_u64(emu.arch.sp_reg, sp_init);

        Ok(emu)
    }

    pub fn with_max_inst(mut self, max: Option<u64>) -> Self {
        self.max_inst = max;
        self
    }

    pub fn with_stdin_mock(mut self, mock: Option<String>) -> Self {
        self.stdin_buffer = mock.map(|s| s.into_bytes());
        self
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
        if self.snapshot_triggers.contains(&self.pc) {
            tracing::info!("Triggering snapshot at PC=0x{:X}", self.pc);
            let snapshot = EmulatorSnapshot::capture(self, self.pc);
            self.snapshots.push(snapshot);
            self.snapshot_triggers.retain(|&addr| addr != self.pc);
        }

        tracing::debug!("Executing PC=0x{:X}", self.pc);

        // Fetch up to 16 bytes from RAM (Space 3).
        let bytes_vec = match self.state.read_space(3, self.pc, 16) {
            Ok(b)  => b,
            Err(_) => {
                let reason = format!("Failed to fetch instruction bytes at 0x{:X}", self.pc);
                tracing::error!("{}", reason);
                self.trace.push(TraceEntry::DecodeError { pc: self.pc, reason });
                // Advance past unknown area by 1 byte and continue.
                self.pc += 1;
                return Ok(true);
            }
        };

        // Decode and lift to P-Code. On failure: record, skip 1 byte, continue.
        let (pcode_ops, inst_len, details) = match self.sleigh
            .decode_and_lift_with_details(&bytes_vec, self.pc)
        {
            Ok(r) => r,
            Err(e) => {
                let reason = format!("{:#}", e);
                tracing::warn!("Decode/lift failed at 0x{:X}: {}", self.pc, reason);
                self.trace.push(TraceEntry::DecodeError { pc: self.pc, reason });
                self.pc += 1;
                return Ok(true);
            }
        };

        // Merge any newly-discovered userops into our map.
        for (id, name) in &details.userops {
            self.userop_map.entry(*id).or_insert_with(|| name.clone());
        }

        // Record instruction trace entry if tracing is enabled.
        if self.trace.enabled {
            let bytes_hex = bytes_vec[..inst_len as usize]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ");
            // Get mnemonic via decode_window(limit=1).
            let mnemonic = self.sleigh
                .decode_window(&bytes_vec, self.pc, 1)
                .ok()
                .and_then(|v| v.into_iter().next())
                .map(|d| d.instruction_text())
                .unwrap_or_else(|| "?".to_string());
            let pcode_op_names = pcode_ops.iter()
                .map(|op| format!("{:?}", op.opcode))
                .collect();
            self.trace.push(TraceEntry::Instruction {
                pc: self.pc,
                bytes_hex,
                mnemonic,
                pcode_ops: pcode_op_names,
                decode_error: None,
            });
        }

        // Pre-advance the PC register inside the Sleigh state so that
        // %pc-relative addressing and `call` (which pushes the return address)
        // observe the correct next-instruction address.
        let next_pc = self.pc + inst_len;
        let _ = self.write_register_u64(self.arch.pc_reg, next_pc);

        // Evaluate P-Code ops.
        // On CallOther we break out of the evaluator scope (so `evaluator` is dropped
        // and self.state borrow is released) and then dispatch via `self`.
        let mut branched = false;
        let mut branch_target = 0u64;
        let mut pcode_idx = 0;

        'pcode: loop {
            if pcode_idx >= pcode_ops.len() {
                break;
            }
            let op = &pcode_ops[pcode_idx];
            tracing::debug!("    P-Code: {:?}", op.opcode);
            let current_pc = self.pc;

            // Evaluator only borrows self.state — we scope it tightly.
            let step_result = {
                let mut evaluator = Evaluator::new(&mut self.state);
                evaluator.step(op)?
            };

            match step_result {
                crate::pcode::eval::StepResult::Next => {
                    pcode_idx += 1;
                }
                crate::pcode::eval::StepResult::BranchRel(rel_idx) => {
                    pcode_idx = rel_idx;
                }
                crate::pcode::eval::StepResult::Branch(tgt) => {
                    branched = true;
                    branch_target = tgt;
                    break 'pcode;
                }
                crate::pcode::eval::StepResult::CallOther { userop_id, input_vals, output_size } => {
                    // evaluator is already dropped here (scoped above).
                    // We handle the dispatch below, then resume from pcode_idx + 1.
                    pcode_idx += 1;

                    // Resolve userop name (no evaluator borrow active).
                    let userop_name = self.userop_map
                        .get(&userop_id)
                        .cloned()
                        .unwrap_or_else(|| format!("userop_{userop_id}"));

                    tracing::debug!("    USEROP: {} (id={})", userop_name, userop_id);

                    self.trace.push(TraceEntry::UseropeDispatch {
                        pc: current_pc,
                        userop_name: userop_name.clone(),
                        input_vals: input_vals.clone(),
                    });

                    let result = {
                        let os_ptr = &*self.os as *const dyn OsEnvironment;
                        let os_ref = unsafe { &*os_ptr };
                        os_ref.dispatch_userop(self, &userop_name, &input_vals, output_size)?
                    };

                    match result {
                        HleResult::Halt(code) => {
                            tracing::info!("USEROP requested halt (code={})", code);
                            return Ok(false);
                        }
                        HleResult::Continue => {}
                    }
                    // Continue the pcode loop.
                }
            }
        }

        // Update our PC tracker.
        self.pc = if branched { branch_target } else { next_pc };

        // HLE trap check — any address in the upper magic range.
        if self.pc >= 0xFFFFFFF000000000 {
            let magic = self.pc;
            let func_name = {
                let func_name_opt = self.os.resolve_stub(&self.binary, magic);
                func_name_opt.unwrap_or_else(|| format!("Unknown@0x{:X}", magic))
            };

            // Record HLE dispatch in trace.
            self.trace.push(TraceEntry::HleDispatch {
                pc: magic,
                func_name: func_name.clone(),
            });

            // Dispatch — OS sets the return value.
            let result = {
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
            if IS_INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!("Execution interrupted by Ctrl+C (SIGINT). Halting safely.");
                break;
            }

            if let Some(limit) = self.max_inst {
                if self.inst_count >= limit {
                    tracing::warn!("Instruction limit ({}) reached. Halting.", limit);
                    break;
                }
            }

            if !self.run_instruction()? {
                break;
            }
            self.inst_count += 1;
        }
        tracing::info!("Sandbox execution finished");
        Ok(())
    }
}
