use crate::arch::ArchInfo;
use crate::os::env::{HleResult, OsEnvironment};
use crate::pcode::state::MachineState;
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

    /// When true, JIT exits TBs without chaining so the outer loop can stop at a
    /// symbolic branch (concolic gate). Cleared when exploration resumes.
    pub sym_stop_requested: bool,

    /// The Virtual File System.
    pub vfs: crate::os::vfs::SimVFS,

    /// The pure-Rust Symbolic Solver context
    pub solver: fission_solver::Solver,
    
    /// Native JIT Compiler instance
    pub jit: Option<crate::jit::JitCompiler>,
    
    /// Native JIT Block Cache
    pub jit_cache: crate::jit::cache::JitCache,

    /// Soft TB-chaining depth (reset at outer run-loop entry).
    pub chain_depth: u32,

    /// Set by HLE/CallOther when guest requests process exit.
    pub halt_requested: bool,

    /// Linux ELF process image metadata (stack/auxv/brk) when loaded via ELF loader.
    pub image_info: Option<crate::os::linux::image_info::ImageInfo>,

    /// Windows PE process image metadata (stack/PEB/TEB/heap) when loaded via PE loader.
    pub pe_image_info: Option<crate::os::windows::image_info::PeImageInfo>,

    /// Linux signal pending/actions/blocked mask (user-mode).
    pub signals: crate::os::linux::signal::SignalState,

    /// Windows `GetLastError` / `SetLastError` thread state (deterministic HLE).
    pub win_last_error: u32,

    /// If set, the next TB exit / outer PC update uses this instead of the
    /// computed next PC (e.g. `rt_sigreturn`).
    pub pc_override: Option<u64>,

    /// Coverage / quality telemetry for the current run.
    pub metrics: crate::metrics::EmulatorMetrics,
}

/// Max guest instructions per translation block.
pub const MAX_TB_INSNS: usize = 8;

/// Ghidra x86 `define pcodeop` order (ia.sinc) — indices match CallOther const ids.
const X86_FALLBACK_USEROPS: &[(u32, &str)] = &[
    (0, "segment"),
    (1, "in"),
    (2, "out"),
    (3, "sysenter"),
    (4, "sysexit"),
    (5, "syscall"),
    (6, "sysret"),
    (7, "swapgs"),
    (8, "invlpg"),
    (9, "invlpga"),
    (10, "invpcid"),
    (11, "rdtscp"),
    (12, "mwait"),
    (13, "mwaitx"),
    (14, "monitor"),
    (15, "monitorx"),
    (16, "swi"),
    (17, "LOCK"),
    (18, "UNLOCK"),
    (19, "XACQUIRE"),
    (20, "XRELEASE"),
];

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

        // Resolve SLA-native space indices. Guest image may already have been
        // loaded under the fallback ram id (3); migrate pages if SLA differs.
        let layout = sleigh_arc
            .compiled_frontend()
            .map(crate::pcode::spaces::SpaceLayout::from_compiled)
            .unwrap_or_default();
        let old_ram = state.spaces_layout.ram;
        let old_reg = state.spaces_layout.register;
        let old_unique = state.spaces_layout.unique;
        if old_ram != layout.ram {
            if let Some(space) = state.spaces.remove(&old_ram) {
                state.spaces.insert(layout.ram, space);
            }
        }
        if old_reg != layout.register {
            if let Some(space) = state.spaces.remove(&old_reg) {
                state.spaces.insert(layout.register, space);
            }
        }
        if old_unique != layout.unique {
            if let Some(space) = state.spaces.remove(&old_unique) {
                state.spaces.insert(layout.unique, space);
            }
        }
        state.spaces_layout = layout.clone();
        for (name, &idx) in &layout.by_name {
            if !state.spaces.contains_key(&idx) {
                state
                    .spaces
                    .insert(idx, crate::pcode::state::AddressSpace::new(name.clone()));
            }
        }
        tracing::info!(
            "SpaceLayout: ram={}, register={}, unique={} (from SLA)",
            layout.ram,
            layout.register,
            layout.unique
        );

        // Prefer SLA `<userop_head>` (via packaged .sla) so CallOther names resolve
        // ("syscall", "cpuid", …). CompiledFrontend may ship without the table.
        let mut userop_map = if let Some(spec) = binary.load_spec() {
            fission_sleigh::runtime::userop_map_for_load_spec(spec).unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        if let Some(cf) = sleigh_arc.compiled_frontend() {
            for (id, name) in &cf.userops {
                userop_map.entry(*id).or_insert_with(|| name.clone());
            }
        }
        {
            let probe_bytes = vec![0u8; 16];
            if let Ok((_, _, details)) = sleigh_arc.decode_and_lift_with_details(&probe_bytes, pc) {
                for (id, name) in details.userops {
                    userop_map.entry(id).or_insert(name);
                }
            }
        }
        if userop_map.is_empty() {
            // Last-resort x86 ia.sinc order if SLA USEROP_HEAD still missing.
            let lang = binary
                .load_spec()
                .map(|s| s.pair.language_id.as_str())
                .unwrap_or("");
            if lang.starts_with("x86:") {
                for (id, name) in X86_FALLBACK_USEROPS {
                    userop_map.insert(*id, (*name).to_string());
                }
                tracing::warn!(
                    "SLA userops empty; using x86 fallback table ({} entries)",
                    userop_map.len()
                );
            } else {
                tracing::warn!("No Sleigh userop table loaded; CallOther names may be userop_N");
            }
        } else {
            tracing::info!(
                "Loaded {} Sleigh userops (sample: {:?})",
                userop_map.len(),
                userop_map.iter().take(8).collect::<Vec<_>>()
            );
        }

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
            sym_stop_requested: false,
            vfs: crate::os::vfs::SimVFS::new(),
            solver: fission_solver::Solver::new(),
            jit: crate::jit::JitCompiler::new().ok(),
            jit_cache: crate::jit::cache::JitCache::new(),
            chain_depth: 0,
            halt_requested: false,
            image_info: None,
            pe_image_info: None,
            signals: crate::os::linux::signal::SignalState::default(),
            win_last_error: 0,
            pc_override: None,
            metrics: crate::metrics::EmulatorMetrics::default(),
        };

        // Default SP if no ELF image_info applied yet (Windows / bare-metal).
        let sp_init = if emu.arch.pointer_size == 8 {
            0x0000_7FFF_FFFF_F000u64
        } else {
            0x7FFF_E000u64
        };
        let _ = emu.write_register_u64(emu.arch.sp_reg, sp_init);

        // Enable PageFault checks for user-mode RAM after layout is ready.
        emu.state.enforce_page_faults = true;

        Ok(emu)
    }

    /// Attach ELF image metadata and apply stack pointer / PC / brk from it.
    pub fn apply_linux_image(
        &mut self,
        info: crate::os::linux::image_info::ImageInfo,
    ) -> Result<()> {
        self.pc = info.entry;
        crate::os::linux::image_info::apply_stack_pointer(self, &info)?;
        self.state.page_map.brk = info.brk;
        self.state.page_map.brk_base = info.brk;
        self.image_info = Some(info);
        Ok(())
    }

    /// Attach PE image metadata and apply stack pointer / PC from it.
    pub fn apply_windows_image(
        &mut self,
        info: crate::os::windows::image_info::PeImageInfo,
    ) -> Result<()> {
        crate::os::windows::image_info::apply_stack_and_entry(self, &info)?;
        self.pe_image_info = Some(info);
        Ok(())
    }

    /// Queue a Linux signal for later delivery between TBs.
    pub fn raise_signal(&mut self, signo: i32) {
        if self.signals.queue(signo) {
            tracing::info!("Signal {} queued (pending=0x{:X})", signo, self.signals.pending);
        }
    }

    /// Deliver at most one pending unblocked signal. May rewrite PC or halt.
    pub fn process_pending_signals(&mut self) -> Result<bool> {
        use crate::os::linux::signal::DeliverResult;
        match self.signals.take_delivery(self.pc) {
            DeliverResult::None => Ok(true),
            DeliverResult::Ignored { signo } => {
                tracing::debug!("Signal {} ignored", signo);
                Ok(true)
            }
            DeliverResult::Stop { signo } => {
                tracing::info!("Signal {} stop (single-thread: resume)", signo);
                Ok(true)
            }
            DeliverResult::Terminate { signo } => {
                tracing::warn!("Signal {} → process terminate", signo);
                self.halt_requested = true;
                Ok(false)
            }
            DeliverResult::Handler {
                signo,
                handler,
                old_pc,
            } => {
                tracing::info!(
                    "Deliver signal {} to handler 0x{:X} (return PC 0x{:X})",
                    signo,
                    handler,
                    old_pc
                );
                // Minimal frame: push old PC so a cooperative handler can return via stack,
                // and set PC to the handler. Full ucontext is future work.
                let sp_reg = self.arch.sp_reg;
                let ptr_size = self.arch.pointer_size as u64;
                if let Ok(sp) = self.read_register_u64(sp_reg) {
                    let new_sp = sp.saturating_sub(ptr_size);
                    let _ = self.write_register_u64(sp_reg, new_sp);
                    let ram = self.state.ram_space();
                    if ptr_size == 8 {
                        let _ = self.state.write_space(ram, new_sp, &old_pc.to_le_bytes());
                    } else {
                        let _ = self
                            .state
                            .write_space(ram, new_sp, &(old_pc as u32).to_le_bytes());
                    }
                }
                // First argument: signo in the first integer arg register when possible.
                let _ = self.write_arg0_signo(signo as u64);
                self.pc = handler;
                Ok(true)
            }
        }
    }

    fn write_arg0_signo(&mut self, signo: u64) -> Result<()> {
        // Best-effort: use arch calling convention first integer arg register.
        let regs = self.arch.cc.arg_regs();
        if let Some(reg) = regs.first() {
            self.write_register_u64(reg, signo)?;
        }
        Ok(())
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
    ///
    /// Enables memory/shadow delta tracing and disables hard TB chaining while
    /// recording so snapshots land at outer-loop (segment) boundaries.
    pub fn with_ttd(mut self, interval: u64) -> Self {
        self.ttd_snapshot_interval = interval;
        if interval > 0 {
            self.ttd.start_recording();
            self.state.tracing_memory = true;
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
            let _ = self.state.write_space(self.state.ram_space(), delta.address, &delta.new_value);
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
            let ram = self.state.ram_space();
            let bytes     = self.state.read_space(ram, sp + stack_off, ptr_size)?;
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
            let ram = self.state.ram_space();
            let bytes  = self.state.read_space(ram, sp, ptr_size)?;
            let ret_addr = crate::arch::calling_convention::le_bytes_to_u64(&bytes);
            self.pc = ret_addr;
            self.write_register_u64(sp_reg, sp + ptr_size as u64)?;
        }
        Ok(())
    }

    // ── Execution ─────────────────────────────────────────────────────────────

    /// Collect a multi-instruction TB starting at `self.pc`.
    fn collect_translation_block(&mut self) -> Result<Vec<crate::jit::compiler::GuestInsn>> {
        use crate::jit::compiler::GuestInsn;
        use fission_pcode::ir::PcodeOpcode;

        let mut out = Vec::new();
        let mut cur = self.pc;
        let page = cur & !0xFFF;
        let ram = self.state.ram_space();

        for _ in 0..MAX_TB_INSNS {
            if (cur & !0xFFF) != page {
                break;
            }
            // Stop before an already-compiled TB so soft chaining can re-enter it.
            if !out.is_empty() && self.jit_cache.lookup(cur).is_some() {
                break;
            }
            if cur >= 0xFFFFFFF0_00000000 {
                break;
            }

            let bytes_vec = self.state.read_space(ram, cur, 16).map_err(|_| {
                anyhow::anyhow!("Failed to fetch instruction bytes at 0x{:X}", cur)
            })?;

            let (pcode_ops, inst_len, details) = self
                .sleigh
                .decode_and_lift_with_details(&bytes_vec, cur)
                .map_err(|e| anyhow::anyhow!("Decode/lift failed at 0x{:X}: {:#}", cur, e))?;

            for (id, name) in &details.userops {
                self.userop_map.entry(*id).or_insert_with(|| name.clone());
            }

            let terminates = pcode_ops.iter().any(|op| {
                matches!(
                    op.opcode,
                    PcodeOpcode::Call
                        | PcodeOpcode::CallInd
                        | PcodeOpcode::Return
                        | PcodeOpcode::BranchInd
                        | PcodeOpcode::Branch
                        | PcodeOpcode::CBranch
                ) && {
                    // Relative branches stay inside the insn; absolute exit the TB.
                    match op.opcode {
                        PcodeOpcode::Branch | PcodeOpcode::CBranch => {
                            let dest = &op.inputs[0];
                            !(dest.space_id == 0 || dest.is_constant)
                        }
                        _ => true,
                    }
                }
            });

            let len = inst_len as u32;
            out.push(GuestInsn {
                pc: cur,
                len,
                ops: pcode_ops,
            });
            cur = cur.wrapping_add(len as u64);
            if terminates {
                break;
            }
        }

        if out.is_empty() {
            anyhow::bail!("TB collection produced no instructions at 0x{:X}", self.pc);
        }
        Ok(out)
    }

    pub fn run_instruction(&mut self) -> Result<bool> {
        if self.halt_requested {
            return Ok(false);
        }

        if self.snapshot_triggers.contains(&self.pc) {
            tracing::info!("Triggering snapshot at PC=0x{:X}", self.pc);
            let snapshot = EmulatorSnapshot::capture(self, self.pc);
            self.snapshots.push(snapshot);
            self.snapshot_triggers.retain(|&addr| addr != self.pc);
        }

        tracing::debug!("Executing PC=0x{:X}", self.pc);

        // ─── JIT-only multi-instruction TB path ───────────────────────────────
        //
        //   1. Cache hit  → run host TB (counts insns + soft-chains internally).
        //   2. Cache miss → collect TB → compile → insert → run.
        //   3. Compile fail → hard error (no interpreter).

        if let Some(block) = self.jit_cache.lookup(self.pc) {
            tracing::debug!(
                "JIT: cache hit TB@0x{:X} ({} guest insns)",
                self.pc,
                block.guest_insns
            );
            self.metrics.tbs_cache_hits += 1;
            let func: extern "C" fn(*mut Emulator) -> u64 =
                unsafe { std::mem::transmute(block.host_func_ptr) };
            // inst_count is advanced inside the TB via jit_count_insn.
            let next_pc = func(self as *mut _);
            self.pc = next_pc;
            return Ok(!self.halt_requested);
        }

        let insns = self.collect_translation_block().map_err(|e| {
            self.metrics.decode_errors += 1;
            self.trace.push(TraceEntry::DecodeError {
                pc: self.pc,
                reason: e.to_string(),
            });
            e
        })?;

        // Telemetry: count opcodes the JIT will no-op.
        for insn in &insns {
            for op in &insn.ops {
                if !crate::metrics::is_jit_supported(op.opcode) {
                    self.metrics.note_unimplemented(op.opcode);
                }
            }
        }

        let start_pc = insns[0].pc;
        let total_bytes: usize = insns.iter().map(|i| i.len as usize).sum();
        let guest_insns = insns.len() as u32;
        let mut pages = Vec::new();
        for insn in &insns {
            let p = insn.pc & !0xFFF;
            if !pages.contains(&p) {
                pages.push(p);
            }
        }
        let fallthrough = {
            let last = insns.last().unwrap();
            last.pc.wrapping_add(last.len as u64)
        };

        let jit = self
            .jit
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("JIT compiler unavailable (host ISA unsupported)"))?;

        let func_ptr = jit.compile_translation_block(&insns).map_err(|e| {
            anyhow::anyhow!(
                "JIT compile failed at TB@0x{:X} ({} insns): {:#}",
                start_pc,
                guest_insns,
                e
            )
        })?;
        self.metrics.tbs_compiled += 1;

        let block = std::sync::Arc::new(crate::jit::cache::JitBlock {
            guest_pc: start_pc,
            host_func_ptr: func_ptr,
            block_size: total_bytes,
            guest_insns,
            next_pc: Some(fallthrough),
            pages,
            abs_exit_targets: Vec::new(),
        });
        self.jit_cache.insert(start_pc, block.clone());

        tracing::debug!(
            "JIT: compiled TB@0x{:X} ({} insns, {} bytes)",
            start_pc,
            guest_insns,
            total_bytes
        );
        let func: extern "C" fn(*mut Emulator) -> u64 =
            unsafe { std::mem::transmute(block.host_func_ptr) };
        let next_pc = func(self as *mut _);
        self.pc = next_pc;
        Ok(!self.halt_requested)
    }

    pub fn run(&mut self) -> Result<()> {
        tracing::info!("Sandbox execution started at PC=0x{:X}", self.pc);
        self.halt_requested = false;
        self.chain_depth = 0;
        loop {
            if IS_INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!("Execution interrupted by Ctrl+C (SIGINT). Halting safely.");
                break;
            }
            if self.halt_requested {
                break;
            }
            if self.sym_stop_requested {
                tracing::debug!(
                    "Symbolic gate stop at PC=0x{:X} ({} events)",
                    self.pc,
                    self.sym_events.len()
                );
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

            // ── Pending Linux signals (between TBs) ───────────────────────────
            if !self.process_pending_signals()? {
                break;
            }
            if self.halt_requested {
                break;
            }

            // ── HLE Trap Check ────────────────────────────────────────────────
            if self.pc >= 0xFFFFFFF000000000 {
                let magic = self.pc;
                let func_name = {
                    let opt = self.os.resolve_stub(&self.binary, magic);
                    opt.unwrap_or_else(|| format!("Unknown@0x{:X}", magic))
                };

                self.trace.push(crate::trace::TraceEntry::HleDispatch {
                    pc: magic,
                    func_name: func_name.clone(),
                });

                let result = {
                    let os_ptr = &*self.os as *const dyn OsEnvironment;
                    let os_ref = unsafe { &*os_ptr };
                    os_ref.dispatch_hle(self, &func_name)?
                };

                match result {
                    HleResult::Halt(_) => {
                        self.halt_requested = true;
                        break;
                    }
                    HleResult::Continue => {
                        self.simulate_return()?;
                    }
                }
            }

            // TTD: record a snapshot every N instructions.
            if self.ttd_snapshot_interval > 0
                && self.ttd.is_recording()
                && self.inst_count > 0
                && self.inst_count % self.ttd_snapshot_interval == 0
            {
                let regs = self.capture_register_state();
                let deltas: Vec<fission_ttd::MemoryDelta> = self
                    .state
                    .trace_mem_writes
                    .iter()
                    .map(|(addr, old, new)| {
                        fission_ttd::MemoryDelta::new(*addr, old.clone(), new.clone())
                    })
                    .collect();
                let shadow_deltas: Vec<fission_ttd::ShadowDelta> = self
                    .state
                    .trace_shadow_writes
                    .iter()
                    .map(|(space_id, addr, old_node, new_node)| fission_ttd::ShadowDelta {
                        space_id: *space_id,
                        address: *addr,
                        old_node: *old_node,
                        new_node: *new_node,
                    })
                    .collect();
                self.ttd.record_step_with_memory(regs, 0, deltas, shadow_deltas);
                self.state.trace_mem_writes.clear();
                self.state.trace_mem_reads.clear();
                self.state.trace_shadow_writes.clear();
                tracing::trace!("TTD: recorded step {} at PC=0x{:X}", self.inst_count, self.pc);
            }
        }
        if self.ttd.is_recording() {
            let stats = self.ttd.stats();
            tracing::info!(
                "TTD recording stopped: {} snapshots, ~{} bytes",
                stats.count,
                stats.memory_bytes
            );
        }
        self.metrics.instructions = self.inst_count;
        if self.metrics.exit_reason.is_none() {
            self.metrics.exit_reason = Some(if self.halt_requested {
                "halt".into()
            } else if self.sym_stop_requested {
                "sym_gate".into()
            } else if self.max_inst.is_some_and(|m| self.inst_count >= m) {
                "max_inst".into()
            } else {
                "loop_exit".into()
            });
        }
        tracing::info!(
            "Sandbox execution finished at PC=0x{:X} ({} instructions)",
            self.pc,
            self.inst_count
        );
        tracing::info!("Emulator metrics: {}", self.metrics.summary_line());
        Ok(())
    }
}

