use crate::arch::ArchInfo;
use crate::os::env::{HleResult, OsEnvironment};
use crate::pcode::state::MachineState;
use crate::pcode::eval::Evaluator;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use fission_ttd::{TTDRecorder, RegisterState};
use anyhow::Result;
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

    /// TTD (Time-Travel Debugging) recorder.
    pub ttd: TTDRecorder,
    /// Interval at which to record full snapshots for TTD (0 = disabled).
    pub ttd_snapshot_interval: u64,
    /// Simulated tick counter used for time-related HLE APIs.
    pub tick_count: u64,

    /// Unexplored conditional branches (used for TTD-based concolic exploration).
    pub sym_events: Vec<SymBranch>,

    /// The Virtual File System.
    pub vfs: crate::os::vfs::SimVFS,

    /// The pure-Rust Symbolic Solver context
    pub solver: fission_solver::Solver,
}

#[derive(Clone, Debug)]
pub struct SymBranch {
    pub step_index: u64,
    pub pc: u64,
    pub condition_val_taken: bool,
    /// The SymNodeId of the boolean condition AST node, if the condition was tainted/symbolic.
    pub condition_node: Option<fission_solver::ast::SymNodeId>,
    /// Target if we inverted the condition (if false it would be rel_idx, if true it would be fallback rel_idx)
    pub alt_rel_idx: Option<usize>,
    pub alt_addr: Option<u64>,
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
            ttd: TTDRecorder::new(),
            ttd_snapshot_interval: 0,
            tick_count: 0,
            sym_events: Vec::new(),
            vfs: crate::os::vfs::SimVFS::new(),
            solver: fission_solver::Solver::new(),
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

    /// Enable TTD recording with a given snapshot interval (N instructions per snapshot).
    pub fn with_ttd(mut self, interval: u64) -> Self {
        self.ttd_snapshot_interval = interval;
        if interval > 0 {
            self.ttd.start_recording();
        }
        self
    }

    /// Read all current GP registers into a `RegisterState` for TTD.
    fn capture_register_state(&mut self) -> RegisterState {
        let read = |emu: &mut Self, name: &str| emu.read_register_u64(name).unwrap_or(0);
        RegisterState {
            rax: read(self, "RAX"),
            rbx: read(self, "RBX"),
            rcx: read(self, "RCX"),
            rdx: read(self, "RDX"),
            rsi: read(self, "RSI"),
            rdi: read(self, "RDI"),
            rbp: read(self, "RBP"),
            rsp: read(self, "RSP"),
            r8:  read(self, "R8"),
            r9:  read(self, "R9"),
            r10: read(self, "R10"),
            r11: read(self, "R11"),
            r12: read(self, "R12"),
            r13: read(self, "R13"),
            r14: read(self, "R14"),
            r15: read(self, "R15"),
            rip: self.pc,
            rflags: read(self, "EFLAGS"),
        }
    }

    /// Seek the TTD timeline to a given instruction step index.
    /// Replays forward from the nearest stored snapshot to reach the target.
    pub fn ttd_seek(&mut self, target_step: u64) -> Result<()> {
        let snapshot = self.ttd.get_snapshot(target_step)
            .or_else(|| {
                // Find the closest snapshot at or before target_step
                self.ttd.snapshots().into_iter()
                    .filter(|s| s.step_index <= target_step)
                    .last()
            })
            .cloned();

        let Some(snap) = snapshot else {
            anyhow::bail!("No TTD snapshot available at or before step {}", target_step);
        };

        // Restore registers
        self.write_register_u64("RAX", snap.registers.rax)?;
        self.write_register_u64("RBX", snap.registers.rbx)?;
        self.write_register_u64("RCX", snap.registers.rcx)?;
        self.write_register_u64("RDX", snap.registers.rdx)?;
        self.write_register_u64("RSI", snap.registers.rsi)?;
        self.write_register_u64("RDI", snap.registers.rdi)?;
        self.write_register_u64("RBP", snap.registers.rbp)?;
        self.write_register_u64("RSP", snap.registers.rsp)?;
        self.write_register_u64("R8",  snap.registers.r8)?;
        self.write_register_u64("R9",  snap.registers.r9)?;
        self.write_register_u64("R10", snap.registers.r10)?;
        self.write_register_u64("R11", snap.registers.r11)?;
        self.write_register_u64("R12", snap.registers.r12)?;
        self.write_register_u64("R13", snap.registers.r13)?;
        self.write_register_u64("R14", snap.registers.r14)?;
        self.write_register_u64("R15", snap.registers.r15)?;
        self.write_register_u64("EFLAGS", snap.registers.rflags)?;
        self.pc = snap.registers.rip;
        self.inst_count = snap.step_index;

        // Restore memory via stored deltas (reverse the old_value → new_value)
        for delta in &snap.memory_deltas {
            let _ = self.state.write_space(3, delta.address, &delta.new_value);
        }

        // Restore shadow state via stored deltas
        for delta in &snap.shadow_deltas {
            if let Some(new_node) = delta.new_node {
                self.state.set_shadow_memory(delta.space_id, delta.address, new_node);
            } else {
                self.state.clear_shadow_memory(delta.space_id, delta.address);
            }
        }

        tracing::info!("TTD: Restored to step {} (PC=0x{:X})", snap.step_index, self.pc);
        Ok(())
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

        // Pre-advance the PC register inside the Sleigh state so that
        // %pc-relative addressing and `call` (which pushes the return address)
        // observe the correct next-instruction address.
        let original_pc = self.pc;
        let next_pc = self.pc + inst_len;
        let _ = self.write_register_u64(self.arch.pc_reg, next_pc);

        let mut bytes_hex = String::new();
        let mut mnemonic = "?".to_string();
        let mut pcode_op_names = Vec::new();
        
        if self.trace.enabled {
            self.state.tracing_memory = true;
            self.state.trace_mem_reads.clear();
            self.state.trace_mem_writes.clear();
            self.state.trace_shadow_writes.clear();

            bytes_hex = bytes_vec[..inst_len as usize]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ");
            mnemonic = self.sleigh
                .decode_window(&bytes_vec, self.pc, 1)
                .ok()
                .and_then(|v| v.into_iter().next())
                .map(|d| d.instruction_text())
                .unwrap_or_else(|| "?".to_string());
            pcode_op_names = pcode_ops.iter()
                .map(|op| format!("{:?}", op.opcode))
                .collect();
        } else {
            self.state.tracing_memory = false;
        }

        // Evaluate P-Code ops.
        // On CallOther we break out of the evaluator scope (so `evaluator` is dropped
        // and self.state borrow is released) and then dispatch via `self`.
        let mut branched = false;
        let mut branch_target = 0u64;
        let mut pcode_idx = 0;
        let mut pcode_step_limit = 0;

        'pcode: loop {
            pcode_step_limit += 1;
            if pcode_step_limit > 500_000 {
                tracing::warn!("Infinite P-Code loop timeout at 0x{:X}. Breaking.", self.pc);
                self.trace.push(TraceEntry::DecodeError { 
                    pc: self.pc, 
                    reason: "Infinite P-Code loop timeout".to_string() 
                });
                break 'pcode;
            }

            if pcode_idx >= pcode_ops.len() {
                break;
            }
            let op = &pcode_ops[pcode_idx];
            tracing::debug!("    P-Code: {:?}", op.opcode);
            let current_pc = self.pc;

            // Evaluator only borrows self.state and self.solver — we scope it tightly.
            let step_result = {
                let mut evaluator = Evaluator::new(&mut self.state, &mut self.solver);
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
                crate::pcode::eval::StepResult::CBranch { condition_val, condition_node, true_rel_idx, true_addr } => {
                    // Record unexplored path if TTD is recording (sym_explore active)
                    if self.ttd.is_recording() {
                        let (alt_rel, alt_addr) = if condition_val {
                            // Taken true path, alternate is false path (fallthrough)
                            (Some(pcode_idx + 1), None)
                        } else {
                            // Taken false path, alternate is true path
                            (true_rel_idx, true_addr)
                        };
                        self.sym_events.push(SymBranch {
                            step_index: self.inst_count,
                            pc: current_pc,
                            condition_val_taken: condition_val,
                            condition_node,
                            alt_rel_idx: alt_rel,
                            alt_addr: alt_addr,
                        });
                    }

                    // CBranch taken path determines next step
                    if condition_val {
                        if let Some(rel_idx) = true_rel_idx {
                            pcode_idx = rel_idx;
                        } else if let Some(addr) = true_addr {
                            branched = true;
                            branch_target = addr;
                            break 'pcode;
                        }
                    } else {
                        // Condition false, fall through to next pcode op
                        pcode_idx += 1;
                    }
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
        if self.trace.enabled {
            let mut registers = std::collections::HashMap::new();
            let reg_names: Vec<String> = self.register_map.iter()
                .filter(|(name, info)| {
                    let size = info.2;
                    size <= 8 && !name.starts_with("tmp") && !name.contains("UNIQUE") && !name.starts_with("#")
                })
                .map(|(name, _)| name.clone())
                .collect();

            for name in reg_names {
                if let Ok(val) = self.read_register_u64(&name) {
                    registers.insert(name, val);
                }
            }

            let mem_reads = std::mem::take(&mut self.state.trace_mem_reads)
                .into_iter()
                .map(|(addr, data)| (addr, hex::encode(data)))
                .collect();
            let mem_writes = std::mem::take(&mut self.state.trace_mem_writes)
                .into_iter()
                .map(|(addr, _old, data)| (addr, hex::encode(data)))
                .collect();

            self.trace.push(TraceEntry::Instruction {
                pc: original_pc,
                bytes_hex,
                mnemonic,
                pcode_ops: pcode_op_names,
                registers,
                mem_reads,
                mem_writes,
                decode_error: None,
            });
            self.state.tracing_memory = false;
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

            // TTD: record a snapshot every N instructions.
            if self.ttd_snapshot_interval > 0
                && self.ttd.is_recording()
                && self.inst_count % self.ttd_snapshot_interval == 0
            {
                let regs = self.capture_register_state();
                // Collect memory writes from this instruction as deltas.
                let deltas: Vec<fission_ttd::MemoryDelta> = self.state.trace_mem_writes
                    .iter()
                    .map(|(addr, old, new)| fission_ttd::MemoryDelta::new(*addr, old.clone(), new.clone()))
                    .collect();
                // Collect shadow taint changes from this instruction as deltas.
                let shadow_deltas: Vec<fission_ttd::ShadowDelta> = self.state.trace_shadow_writes
                    .iter()
                    .map(|(space_id, addr, old_node, new_node)| fission_ttd::ShadowDelta {
                        space_id: *space_id,
                        address: *addr,
                        old_node: *old_node,
                        new_node: *new_node,
                    })
                    .collect();
                self.ttd.record_step_with_memory(regs, 0, deltas, shadow_deltas);
                tracing::trace!("TTD: recorded step {} at PC=0x{:X}", self.inst_count, self.pc);
            }

            self.inst_count += 1;
        }
        if self.ttd.is_recording() {
            let stats = self.ttd.stats();
            tracing::info!(
                "TTD recording stopped: {} snapshots, ~{} bytes",
                stats.count, stats.memory_bytes
            );
        }
        tracing::info!("Sandbox execution finished");
        Ok(())
    }
}
