use crate::arch::ArchInfo;
use crate::os::env::{HleResult, OsEnvironment};
use crate::pcode::state::MachineState;
use crate::snapshot::EmulatorSnapshot;
use crate::trace::{ExecutionTrace, TraceEntry};
use anyhow::Result;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use fission_ttd::{RegisterState, TTDRecorder};
use std::collections::BTreeMap;
use std::sync::Arc;
pub static IS_INTERRUPTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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
    /// The registers a TTD snapshot records, derived once from
    /// [`Self::register_map`]. See [`Self::snapshot_register_names`].
    snapshot_registers: Vec<String>,
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
    /// P-Code ops retired (intra-insn loops); used with `max_inst` as a soft fuse.
    pub pcode_ops: u64,
    /// What the per-byte shadow layer carries. See [`crate::observe::ShadowMode`].
    ///
    /// Not `pub`: changing it has to flush the block cache, so it goes through
    /// [`Emulator::set_shadow_mode`].
    pub(crate) shadow_mode: crate::observe::ShadowMode,
    /// Taint sources, interned label sets, and what reached a sink. Only
    /// meaningful under [`crate::observe::ShadowMode::Taint`].
    pub taint: crate::taint::TaintState,
    /// Force every block through the interpreter, never the JIT.
    ///
    /// Exists so the two engines can be run against each other on the same
    /// binary: a fallback that is only ever reached by accident is a fallback
    /// nobody has tested.
    pub force_interpreter: bool,
    /// Blocks the JIT declined, and the interpreter ran instead.
    pub interpreted_blocks: u64,
    /// Dynamic-analysis watchers. See [`crate::observe`].
    ///
    /// Not `pub`: registering one has to be able to flush the block cache, so
    /// it goes through [`Emulator::add_observer`].
    pub(crate) observers: Vec<Box<dyn crate::observe::Observer>>,
    /// Union of what `observers` asked for, read by the compiler once per
    /// block. Kept beside the list rather than recomputed, because the
    /// compiler reads it on every translation.
    pub(crate) observe: crate::observe::ObserveMask,
    /// Entry PC of the translation block that exhausted the p-code fuse.
    ///
    /// Recorded where it trips rather than where the run notices, because by
    /// then the PC has already moved on to the next block.
    pub pcode_budget_pc: Option<u64>,
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

    /// When true, a tainted CBranch records `sym_events` **and** requests a run
    /// stop (`sym_stop_requested`). Default false so concrete sandbox runs with
    /// tainted stdin continue on the concrete path; enable for concolic explore.
    pub concolic_stop_on_branch: bool,

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

    /// When set, the run loop executes exactly one guest instruction (or one
    /// HLE dispatch) and returns. See [`Self::step_instruction`].
    single_step: bool,

    /// The guest's exit code, once it has asked to terminate.
    pub exit_code: Option<u32>,

    /// Memory ranges the run loop stops on. See [`Self::set_watchpoint`].
    watchpoints: Vec<Watchpoint>,
    /// The access that tripped a watchpoint, waiting for the run loop to see
    /// it. Set inside a compiled block, which cannot stop by itself.
    watch_hit: Option<WatchHit>,
    /// The guest instruction currently executing, recorded only while
    /// something asked for per-instruction callbacks. A watchpoint asks,
    /// because "what wrote this" is the question watchpoints exist to answer.
    current_insn_pc: u64,

    /// Addresses the run loop stops at, for a debugger front end.
    ///
    /// Private because setting one has to flush the JIT cache: a block
    /// compiled before the breakpoint existed runs straight through it.
    /// Use [`Self::set_breakpoint`] / [`Self::clear_breakpoint`].
    breakpoints: std::collections::BTreeSet<u64>,

    /// Set by HLE/CallOther when guest requests process exit.
    pub halt_requested: bool,
    /// The trampoline region, cached from the OS environment at construction.
    /// The run loop compares every PC against it, so it is a field and not a
    /// virtual call.
    magic_range: (u64, u64),
    /// The processor mode this image's code is in, when the addresses cannot
    /// say so themselves.
    ///
    /// A stripped Cortex-M image is entirely Thumb and entirely even-addressed,
    /// because the ABI's bit-0 marker lives on function symbols that are gone.
    /// Decoding it in the language's default ARM mode does not fail loudly --
    /// ARM has an encoding for nearly every 32-bit word -- so the emulator
    /// executed plausible nonsense instead.
    decode_context: Option<fission_sleigh::runtime::PackedContextOverride>,
    /// ARMv7-M system registers. See [`crate::arch::cortex_m`]; inert on every
    /// other architecture, because nothing reaches for them.
    pub cortex_m: crate::arch::cortex_m::CortexMState,
    /// P-code ops the JIT compiled a call-out for rather than lowering.
    ///
    /// Append-only, and never touched while a block is running: a compiled
    /// block names an entry by index, and `jit_wide_op` reads it back. The
    /// ops are the 128-bit integer ones, which go through the evaluator so
    /// that the two engines cannot lower them differently.
    pub(crate) wide_ops: Vec<fission_pcode::ir::PcodeOp>,

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

    /// Bump-pointer heap cursor for libc `malloc`/`calloc` HLE (0 = init from brk).
    pub heap_cursor: u64,

    /// Linux FS base (ARCH_SET_FS / TLS). Used by `segment` / `segment_fs` userops.
    pub fs_base: u64,
    /// Linux GS base (ARCH_SET_GS).
    pub gs_base: u64,
    /// `set_tid_address` clear_child_tid pointer (0 = none).
    pub clear_child_tid: u64,
    /// Last CallOther/userop data result (consumed by JIT after `jit_call_other`).
    pub callother_result: u64,
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

        // Patch imports (IAT/PLT/MMIO) before execution starts. Only after
        // that does the environment know how wide this process is, and so
        // where it put the trampolines.
        os.patch_imports(&mut state, &binary)?;
        let magic_range = os.magic_range();

        let register_map = if let Some(spec) = binary.load_spec() {
            fission_sleigh::runtime::register_map_for_load_spec(spec).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        // Before the frontend is shared: the mode is a fact about this image
        // and this language, and it does not change during a run.
        let decode_context =
            fission_static::analysis::function_discovery::decode_context_for_address(
                &binary, &sleigh, None,
            );

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
            for (id, name) in cf.userops.iter() {
                userop_map.entry(*id).or_insert_with(|| name.clone());
            }
        }
        {
            let probe_bytes = vec![0u8; 16];
            if let Ok((_, _, details)) = sleigh_arc.decode_and_lift_with_details(&probe_bytes, pc) {
                for (id, name) in details.userops.iter() {
                    userop_map.entry(*id).or_insert_with(|| name.clone());
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

        let snapshot_registers = Self::snapshot_register_names(&register_map);

        let mut emu = Self {
            state,
            binary,
            sleigh: sleigh_arc,
            pc,
            register_map,
            snapshot_registers,
            arch,
            os,
            snapshots: Vec::new(),
            snapshot_triggers: Vec::new(),
            trace: ExecutionTrace::disabled(),
            userop_map,
            inst_count: 0,
            max_inst: None,
            pcode_ops: 0,
            shadow_mode: crate::observe::ShadowMode::Off,
            taint: crate::taint::TaintState::new(),
            force_interpreter: std::env::var_os("FISSION_EMU_INTERP").is_some(),
            interpreted_blocks: 0,
            observers: Vec::new(),
            observe: crate::observe::ObserveMask::NONE,
            pcode_budget_pc: None,
            stdin_buffer: None,
            ttd: TTDRecorder::new(),
            ttd_snapshot_interval: 0,
            tick_count: 0,
            sym_events: Vec::new(),
            sym_stop_requested: false,
            concolic_stop_on_branch: false,
            vfs: crate::os::vfs::SimVFS::new(),
            solver: fission_solver::Solver::new(),
            jit: crate::jit::JitCompiler::new().ok(),
            jit_cache: crate::jit::cache::JitCache::new(),
            chain_depth: 0,
            single_step: false,
            exit_code: None,
            watchpoints: Vec::new(),
            watch_hit: None,
            current_insn_pc: 0,
            breakpoints: std::collections::BTreeSet::new(),
            halt_requested: false,
            magic_range,
            decode_context,
            cortex_m: crate::arch::cortex_m::CortexMState::at_reset(),
            wide_ops: Vec::new(),
            image_info: None,
            pe_image_info: None,
            signals: crate::os::linux::signal::SignalState::default(),
            win_last_error: 0,
            pc_override: None,
            metrics: crate::metrics::EmulatorMetrics::default(),
            heap_cursor: 0,
            fs_base: 0,
            gs_base: 0,
            clear_child_tid: 0,
            callother_result: 0,
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

    /// Bump-allocate `size` bytes from the guest heap (extends `brk` as needed).
    ///
    /// Aligns to 16 bytes. `free` is a no-op for this allocator (no reuse).
    pub fn heap_alloc(&mut self, size: u64) -> Result<u64> {
        let size = size.max(1u64).saturating_add(15) & !15u64;
        if self.heap_cursor == 0 {
            let base = self.state.page_map.brk.max(self.state.page_map.brk_base);
            if base == 0 {
                self.state.page_map.set_brk_base(0x0000_0000_5000_0000);
            }
            self.heap_cursor = self.state.page_map.brk.max(self.state.page_map.brk_base);
        }
        let ptr = self.heap_cursor;
        let end = ptr.saturating_add(size);
        if end > self.state.page_map.brk {
            self.state.page_map.set_brk(end);
        }
        // Ensure bytes are resident under page-fault enforcement.
        let zeros = vec![0u8; size as usize];
        self.state
            .write_space(self.state.ram_space(), ptr, &zeros)
            .map_err(|e| anyhow::anyhow!("heap_alloc write 0x{ptr:X}: {e}"))?;
        self.heap_cursor = end;
        Ok(ptr)
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
        // Heap bump starts at program break.
        self.heap_cursor = info.brk;

        // Seed VFS so openat/read/mmap (ld.so path) can see the main binary.
        let guest_name = info.execfn.clone();
        let host_path = self.binary.inner().path.clone();
        let bytes = self.binary.inner().data.as_slice().to_vec();
        self.vfs.seed_path(&guest_name, bytes.clone());
        self.vfs.seed_path(&host_path, bytes);
        if !host_path.is_empty() {
            self.vfs
                .alias_host(&guest_name, std::path::PathBuf::from(&host_path));
            self.vfs
                .alias_host(&host_path, std::path::PathBuf::from(&host_path));
        }
        // Also seed argv[0]-style basenames.
        if let Some(base) = std::path::Path::new(&guest_name)
            .file_name()
            .and_then(|s| s.to_str())
        {
            self.vfs
                .seed_path(base, self.binary.inner().data.as_slice().to_vec());
        }

        self.image_info = Some(info);
        Ok(())
    }

    /// Attach PE image metadata and apply stack pointer / PC from it.
    /// The active stack pointer, whatever this architecture calls it.
    pub fn read_stack_pointer(&mut self) -> u64 {
        let sp = self.arch.sp_reg;
        self.read_register_u64(sp).unwrap_or(0)
    }

    /// Write the active stack pointer.
    pub fn write_stack_pointer(&mut self, value: u64) -> Result<()> {
        let sp = self.arch.sp_reg;
        self.write_register_u64(sp, value)
    }

    /// Commit `ISAModeSwitch` to the decode context: ARM's `setISAMode`.
    ///
    /// SLEIGH models interworking in two halves. `SetISAModeSwitch(value)`
    /// writes the `ISAModeSwitch` register, and `setISAMode()` -- a userop --
    /// copies it into the `TMode` *context*, which is what the decoder reads.
    /// Only the first half is p-code; without the second the decode context
    /// never moves, so a `bx` into the other instruction set kept decoding in
    /// the mode the image started in.
    ///
    /// A mode change invalidates every compiled block, because the same
    /// address decodes to different instructions in the two modes and the
    /// translation cache is keyed on the address alone. That is why the flush
    /// is conditional: `bx lr` is how every Thumb function returns, and it
    /// commits Thumb again each time. Flushing on those would be flushing
    /// on every return.
    pub fn commit_isa_mode(&mut self) {
        // Not `unwrap_or(0)`. A failed read would read as "ARM", and switching
        // a Thumb-only image into ARM mode makes every instruction after it
        // nonsense -- ARM has an encoding for nearly every word, so it would
        // not even fail. Leaving the mode alone when the answer is unknown is
        // the only safe direction.
        let Ok(raw) = self.read_register_u64("ISAModeSwitch") else {
            return;
        };
        let Some(wanted) = self.sleigh.isa_mode_override(raw != 0) else {
            return;
        };
        if self.decode_context == Some(wanted) {
            return;
        }
        tracing::debug!(
            "ISA mode -> {} at 0x{:X}",
            if raw != 0 { "Thumb" } else { "ARM" },
            self.pc
        );
        self.decode_context = Some(wanted);
        self.jit_cache.flush_all();
    }

    /// The syscall number the guest asked for, in the registry's numbering.
    ///
    /// `None` means the architecture's number has no counterpart -- an honest
    /// "unknown syscall", and better than translating it into an unrelated one.
    pub fn syscall_number(&mut self) -> Option<u64> {
        let abi = crate::os::linux::syscall_conv::SyscallAbi::for_arch(&self.arch);
        let raw = self.read_register_u64(abi.number).unwrap_or(0);
        abi.canonical_number(raw)
    }

    /// The raw number the guest put in the number register, for reporting.
    pub fn raw_syscall_number(&mut self) -> u64 {
        let abi = crate::os::linux::syscall_conv::SyscallAbi::for_arch(&self.arch);
        self.read_register_u64(abi.number).unwrap_or(0)
    }

    /// Argument `n` of the syscall in progress.
    ///
    /// Not the same as [`Self::read_arg`]: a syscall's registers are not its
    /// architecture's *call* registers. x86-64 passes the fourth argument in
    /// `R10` here and `RCX` there, because `syscall` clobbers `RCX`.
    pub fn syscall_arg(&mut self, n: usize) -> u64 {
        let abi = crate::os::linux::syscall_conv::SyscallAbi::for_arch(&self.arch);
        abi.args
            .get(n)
            .and_then(|reg| self.read_register_u64(reg).ok())
            .unwrap_or(0)
    }

    /// All six argument registers, for a report.
    pub fn syscall_args(&mut self) -> [u64; 6] {
        let mut out = [0u64; 6];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.syscall_arg(i);
        }
        out
    }

    /// Where a syscall's result goes.
    pub fn set_syscall_return(&mut self, value: u64) -> Result<()> {
        let abi = crate::os::linux::syscall_conv::SyscallAbi::for_arch(&self.arch);
        self.write_register_u64(abi.result, value)
    }

    /// Set the FS segment base, in both places that can be asked for it.
    ///
    /// Ghidra's x86 spec resolves `fs:[x]` in 32/64-bit mode by adding the
    /// `FS_OFFSET` *register*, not by calling the `segment` userop -- the
    /// userop is the 16-bit path. So a base kept only in `self.fs_base` is
    /// invisible to lifted code, which is how every 32-bit PE ended up
    /// reading `fs:[0x18]` as address 0x18.
    pub fn set_fs_base(&mut self, base: u64) {
        self.fs_base = base;
        let _ = self.write_register_u64("FS_OFFSET", base);
    }

    /// The same for GS, which is where 64-bit Windows keeps the TEB and
    /// 64-bit Linux keeps the thread pointer.
    pub fn set_gs_base(&mut self, base: u64) {
        self.gs_base = base;
        let _ = self.write_register_u64("GS_OFFSET", base);
    }

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
            tracing::info!(
                "Signal {} queued (pending=0x{:X})",
                signo,
                self.signals.pending
            );
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

    /// Register a dynamic-analysis watcher.
    ///
    /// Flushes the block cache whenever this widens what needs instrumenting:
    /// blocks compiled before now were compiled against the older mask and
    /// carry none of the newly-wanted callbacks. QEMU does the same on plugin
    /// load, and for the same reason.
    pub fn add_observer(&mut self, observer: Box<dyn crate::observe::Observer>) {
        self.observers.push(observer);
        self.recompute_observe_mask();
    }

    /// What needs instrumenting: the union of every observer's interest and
    /// whatever the debugger surface needs.
    ///
    /// One place, because there are now two sources. Setting the mask from
    /// `add_observer` alone meant `take_observers` reset it to `NONE` and
    /// silently disarmed any watchpoint.
    fn recompute_observe_mask(&mut self) {
        let mut mask = crate::observe::ObserveMask::NONE;
        for observer in &self.observers {
            mask = mask.union(observer.interest());
        }
        if !self.watchpoints.is_empty() {
            mask.mem = true;
            // For the instruction address in the hit: without it a watchpoint
            // can say what was written and not by what.
            mask.insn = true;
        }
        if mask != self.observe {
            self.observe = mask;
            self.jit_cache.flush_all();
        }
    }

    /// Hand back everything registered, in registration order.
    ///
    /// Observers are owned by the emulator while it runs -- the JIT reaches
    /// them through a raw pointer -- so a caller reads its results out
    /// afterwards rather than keeping a handle across the run.
    pub fn take_observers(&mut self) -> Vec<Box<dyn crate::observe::Observer>> {
        let taken = std::mem::take(&mut self.observers);
        self.recompute_observe_mask();
        taken
    }

    pub fn observe_mask(&self) -> crate::observe::ObserveMask {
        self.observe
    }

    /// Turn the per-byte shadow layer on or off.
    ///
    /// Flushes the block cache: whether a block carries shadow callbacks is
    /// decided when it is compiled, so blocks compiled under the old mode
    /// carry the old decision.
    pub fn set_shadow_mode(&mut self, mode: crate::observe::ShadowMode) {
        if mode != self.shadow_mode {
            self.shadow_mode = mode;
            self.jit_cache.flush_all();
        }
    }

    pub fn shadow_mode(&self) -> crate::observe::ShadowMode {
        self.shadow_mode
    }

    /// Declare a range of guest memory as untrusted input.
    ///
    /// Turns the shadow layer on if it is off: marking a source and then not
    /// propagating it would report nothing and look like a clean run, which is
    /// the worst answer available.
    pub fn taint_range(&mut self, addr: u64, len: u64, label: impl Into<String>) {
        if !matches!(self.shadow_mode, crate::observe::ShadowMode::Taint) {
            self.set_shadow_mode(crate::observe::ShadowMode::Taint);
        }
        let set = self.taint.add_source(crate::taint::TaintSource {
            label: label.into(),
            pc: self.pc,
            addr,
            len,
        });
        let ram = self.state.ram_space();
        for i in 0..len {
            self.state.set_shadow_memory(ram, addr.wrapping_add(i), set);
        }
    }

    /// The taint set on a register's first byte, if any.
    ///
    /// First byte, not all of them: a tainted value reaching a sink is the
    /// finding, and a register whose low byte is clean while a higher one is
    /// tainted is still a tainted register.
    pub fn register_taint(&mut self, name: &str) -> Option<u32> {
        let (space_id, offset, size) = self
            .register_map
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)?;
        (0..u64::from(size).min(8)).find_map(|i| self.state.get_shadow_memory(space_id, offset + i))
    }

    pub(crate) fn notify_translate(&mut self, entry_pc: u64, insns: &[(u64, u32)]) {
        for o in &mut self.observers {
            o.on_translate(entry_pc, insns);
        }
    }

    pub(crate) fn notify_block(&mut self, pc: u64) {
        for o in &mut self.observers {
            o.on_block(pc);
        }
    }

    pub(crate) fn notify_insn(&mut self, pc: u64) {
        self.current_insn_pc = pc;
        for o in &mut self.observers {
            o.on_insn(pc);
        }
    }

    /// A guest read or write of RAM.
    ///
    /// Only RAM: a register access is not what an observer asking about
    /// "memory" means, and every p-code op touches registers.
    pub(crate) fn notify_mem(&mut self, addr: u64, size: u32, write: bool, value: u64) {
        if !self.watchpoints.is_empty() && self.watch_hit.is_none() {
            let end = addr.saturating_add(u64::from(size));
            if let Some(watch) = self.watchpoints.iter().find(|w| {
                (if write { w.on_write } else { w.on_read })
                    && addr < w.start.saturating_add(w.len)
                    && w.start < end
            }) {
                self.watch_hit = Some(WatchHit {
                    address: addr,
                    size,
                    write,
                    value,
                    pc: self.current_insn_pc,
                    step: self.inst_count,
                    watch: *watch,
                });
            }
        }
        for o in &mut self.observers {
            o.on_mem(addr, size, write, value);
        }
    }

    pub(crate) fn notify_syscall_detailed(
        &mut self,
        pc: u64,
        number: u64,
        name: Option<&'static str>,
        args: &[u64; 6],
        detail: Vec<crate::observe::SyscallArg>,
    ) {
        for o in &mut self.observers {
            o.on_syscall_detailed(pc, number, name, args, detail.clone());
        }
    }

    pub(crate) fn notify_hle(&mut self, pc: u64, name: &str) {
        for o in &mut self.observers {
            o.on_hle(pc, name);
        }
    }

    pub fn with_max_inst(mut self, max: Option<u64>) -> Self {
        self.max_inst = max;
        self
    }

    pub fn with_stdin_mock(mut self, mock: Option<String>) -> Self {
        self.stdin_buffer = mock.map(|s| s.into_bytes());
        // Keep VFS fd 0 in sync so `sys_read` (not only libc read) sees the mock.
        if let Some(ref bytes) = self.stdin_buffer {
            if let Some(f) = self.vfs.files.get_mut(&0) {
                f.content = bytes.clone();
                f.cursor = 0;
            }
        }
        self
    }

    /// Seed guest stdin (fd 0) with concrete bytes and optional taint sources.
    ///
    /// When `taint` is true, each byte is registered as a solver var and tagged
    /// in shadow memory only after a later read fills guest RAM — the read HLE
    /// path already taints the destination buffer. This helper mainly seeds VFS.
    pub fn seed_stdin(&mut self, data: &[u8]) {
        self.stdin_buffer = Some(data.to_vec());
        if let Some(f) = self.vfs.files.get_mut(&0) {
            f.content = data.to_vec();
            f.cursor = 0;
        }
    }

    /// Enable run-loop stops on tainted CBranch (for `SimulationManager` / explore).
    /// Stop the run at the first tainted `CBRANCH`.
    ///
    /// Turns the shadow layer on, because there is no such thing as a tainted
    /// branch without it. Shadow is off by default now -- asking for a
    /// behaviour that depends on it is what asks for it.
    pub fn with_concolic_stop(mut self, enabled: bool) -> Self {
        self.concolic_stop_on_branch = enabled;
        if enabled {
            // Builder, so nothing is compiled yet and the cache flush that
            // `set_shadow_mode` exists for has nothing to flush.
            self.shadow_mode = crate::observe::ShadowMode::Symbolic;
        }
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

    /// The registers a TTD snapshot should record, from the language's own
    /// register map rather than a per-architecture table.
    ///
    /// One filter: **maximal only**. `EAX`, `AX`, `AL` and `AH` are views of
    /// `RAX`'s storage, so recording them all would store the same bytes five
    /// times and -- worse -- restoring them in map order would write `AL` over
    /// the low byte `RAX` had just restored. Keeping only registers no other
    /// register contains makes the set both smaller and safe to replay in any
    /// order.
    ///
    /// Vector registers (`ZMM0`, `Q0`) are included: `RegisterState` carries
    /// anything wider than a `u64` as bytes, and they are read and written
    /// through the register space directly rather than through
    /// `read_register_u64`, which refuses them.
    ///
    /// Computed once per emulator: the containment filter is quadratic in
    /// the map size (x86-64 names around five hundred registers) and a
    /// snapshot is taken every `ttd_snapshot_interval` instructions.
    fn snapshot_register_names(
        register_map: &std::collections::HashMap<String, (u64, u64, u32)>,
    ) -> Vec<String> {
        let entries: Vec<(&str, u64, u64, u32)> = register_map
            .iter()
            .map(|(name, &(space, offset, size))| (name.as_str(), space, offset, size))
            .filter(|&(_, _, _, size)| size > 0)
            .collect();

        let mut names: Vec<String> = entries
            .iter()
            .filter(|&&(name, space, offset, size)| {
                !entries
                    .iter()
                    .any(|&(other, other_space, other_offset, other_size)| {
                        other_space == space
                        && other_offset <= offset
                        && offset + u64::from(size) <= other_offset + u64::from(other_size)
                        // A strictly larger register, or an equal-sized alias
                        // whose name sorts first, so exactly one of a pair of
                        // aliases survives.
                        && (other_size > size || (other_size == size && other < name))
                    })
            })
            .map(|&(name, _, _, _)| name.to_string())
            .collect();
        // The map is a `HashMap`; a snapshot's contents must not depend on
        // its iteration order.
        names.sort_unstable();
        names
    }

    /// Drop any compiled code covering `[address, address + len)`.
    ///
    /// Writing to memory that has been translated -- self-modifying code, or a
    /// debugger patching an instruction -- leaves the block cache holding a
    /// compilation of bytes that are no longer there. The syscall and HLE
    /// paths that map or protect memory already do this; a front end writing
    /// through the debug backend needs the same.
    pub fn invalidate_translations(&mut self, address: u64, len: usize) {
        let first = address & !0xFFF;
        let last = address.saturating_add(len.saturating_sub(1) as u64) & !0xFFF;
        let mut page = first;
        loop {
            self.jit_cache.invalidate_page(page);
            if page >= last {
                break;
            }
            page += 0x1000;
        }
    }

    // ── Stepping ────────────────────────────────────────────────────────────

    /// Execute exactly one guest instruction, or dispatch one HLE stub.
    ///
    /// `run_instruction` is named for what it once did; both of its paths run
    /// a whole translation block, so the debug backend's "single step" was
    /// advancing between one and eight instructions at a time depending on
    /// where the block boundaries fell.
    pub fn step_instruction(&mut self) -> Result<RunOutcome> {
        let saved = std::mem::replace(&mut self.single_step, true);
        let outcome = self.run_inner(None);
        self.single_step = saved;
        outcome
    }

    /// The instruction at the program counter: how long it is, and whether it
    /// is a call.
    ///
    /// "Is a call" is read off the lifted p-code rather than a mnemonic, so it
    /// is the same answer on every architecture -- and it is what "step over"
    /// needs in order to know whether there is anything to step over.
    pub fn instruction_at_pc(&mut self) -> Result<InstructionShape> {
        let insns = self.collect_translation_block()?;
        let first = insns
            .first()
            .ok_or_else(|| anyhow::anyhow!("no instruction at 0x{:X}", self.pc))?;
        let is_call = first.ops.iter().any(|op| {
            matches!(
                op.opcode,
                fission_pcode::ir::PcodeOpcode::Call | fission_pcode::ir::PcodeOpcode::CallInd
            )
        });
        let is_return = first
            .ops
            .iter()
            .any(|op| op.opcode == fission_pcode::ir::PcodeOpcode::Return);
        Ok(InstructionShape {
            address: first.pc,
            length: first.len,
            is_call,
            is_return,
        })
    }

    /// Record bytes the guest wrote to its standard output, and echo them.
    ///
    /// Recording matters more than echoing: for a program under examination
    /// the output *is* the evidence, and printing it straight to the host's
    /// terminal left nothing any caller could read back. The Linux syscall
    /// layer has always written through the simulated filesystem; the Windows
    /// stubs printed and forgot.
    pub fn guest_stdout(&mut self, bytes: &[u8]) {
        const GUEST_STDOUT: u64 = 1;
        let _ = self.vfs.write(GUEST_STDOUT, bytes);
        print!("{}", String::from_utf8_lossy(bytes));
    }

    // ── Watchpoints ─────────────────────────────────────────────────────────

    /// Stop when the guest reads or writes `[start, start + len)`.
    ///
    /// Costs nothing when none are set: whether compiled code carries memory
    /// callbacks at all is decided when a block is compiled, so setting the
    /// first one flushes the block cache the same way registering an observer
    /// does.
    ///
    /// The stop is *after* the access, at the end of the translation block
    /// containing it -- a compiled block cannot stop in its own middle. The
    /// reported [`WatchHit`] carries the exact address, size, value and the
    /// program counter of the instruction that did it, which is the part that
    /// answers "who wrote this".
    pub fn set_watchpoint(&mut self, start: u64, len: u64, on_read: bool, on_write: bool) {
        self.watchpoints.push(Watchpoint {
            start,
            len: len.max(1),
            on_read,
            on_write,
        });
        self.recompute_observe_mask();
    }

    /// Remove every watchpoint starting at `start`. Returns how many went.
    pub fn clear_watchpoint(&mut self, start: u64) -> usize {
        let before = self.watchpoints.len();
        self.watchpoints.retain(|w| w.start != start);
        let removed = before - self.watchpoints.len();
        if removed > 0 {
            self.recompute_observe_mask();
        }
        removed
    }

    pub fn clear_all_watchpoints(&mut self) {
        if !self.watchpoints.is_empty() {
            self.watchpoints.clear();
            self.recompute_observe_mask();
        }
    }

    pub fn watchpoints(&self) -> &[Watchpoint] {
        &self.watchpoints
    }

    /// The access that stopped the last run, if a watchpoint stopped it.
    pub fn last_watch_hit(&self) -> Option<&WatchHit> {
        self.watch_hit.as_ref()
    }

    /// Whether a watchpoint has tripped and the run loop has yet to stop.
    ///
    /// Read from the JIT's chaining gate: a block that tripped one must return
    /// to the run loop rather than chain on for up to another thirty-two.
    #[inline]
    pub fn watch_pending(&self) -> bool {
        self.watch_hit.is_some()
    }

    // ── Breakpoints ─────────────────────────────────────────────────────────

    /// Stop the run loop whenever the program counter reaches `address`.
    ///
    /// Flushes the JIT cache, because a block compiled before the breakpoint
    /// existed contains the instruction at `address` in its middle and runs
    /// straight through it. QEMU does the same thing for the same reason.
    pub fn set_breakpoint(&mut self, address: u64) {
        if self.breakpoints.insert(address) {
            self.jit_cache.flush_all();
        }
    }

    /// Stop stopping at `address`. Returns whether there was a breakpoint
    /// there, so a front end can say "no breakpoint at ..." rather than
    /// silently succeeding.
    pub fn clear_breakpoint(&mut self, address: u64) -> bool {
        let had = self.breakpoints.remove(&address);
        if had {
            self.jit_cache.flush_all();
        }
        had
    }

    pub fn clear_all_breakpoints(&mut self) {
        if !self.breakpoints.is_empty() {
            self.breakpoints.clear();
            self.jit_cache.flush_all();
        }
    }

    /// Every breakpoint, in address order.
    pub fn breakpoints(&self) -> impl Iterator<Item = u64> + '_ {
        self.breakpoints.iter().copied()
    }

    /// Whether the run loop would stop at `address`.
    ///
    /// Also read from the JIT's chaining gate, which is why it is `pub` and
    /// takes `&self`.
    #[inline]
    pub fn is_breakpoint(&self, address: u64) -> bool {
        !self.breakpoints.is_empty() && self.breakpoints.contains(&address)
    }

    /// Read the current registers into a [`RegisterState`].
    ///
    /// The one place registers are turned into a snapshot, so the TTD
    /// recorder and a debugger front end cannot disagree about which
    /// registers a machine has.
    pub fn register_state(&mut self) -> RegisterState {
        let names = std::mem::take(&mut self.snapshot_registers);
        let mut state = RegisterState::at(self.pc);
        for name in &names {
            let Some(&(space, offset, size)) = self
                .register_map
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, v)| v)
            else {
                continue;
            };
            let Ok(bytes) = self.state.read_space(space, offset, size as usize) else {
                continue;
            };
            if size <= 8 {
                let mut value = 0u64;
                for (i, &b) in bytes.iter().enumerate() {
                    value |= u64::from(b) << (i * 8);
                }
                state.set(name, value);
            } else {
                state.set_bytes(name, &bytes);
            }
        }
        self.snapshot_registers = names;
        state
    }

    /// Write one register from a snapshot, whatever its width.
    ///
    /// `write_register_u64` refuses anything wider than eight bytes, which is
    /// why a seek used to leave vector registers holding whatever the run had
    /// left in them.
    fn restore_register(&mut self, name: &str, value: &fission_ttd::RegisterValue) -> Result<()> {
        let (space, offset, size) = self
            .register_map
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
            .ok_or_else(|| anyhow::anyhow!("Register {} not found in register_map", name))?;
        let mut bytes = value.to_bytes();
        bytes.resize(size as usize, 0);
        self.state.write_space(space, offset, &bytes)
    }

    /// Seek the TTD timeline to a given instruction step index.
    ///
    /// 1. Restore the nearest snapshot at or before `target_step` (registers +
    ///    forward memory/shadow deltas recorded with that snapshot).
    /// 2. If still short of `target_step`, **recompute** by running JIT with
    ///    chaining disabled and `max_inst` set to the remaining steps.
    pub fn ttd_seek(&mut self, target_step: u64) -> Result<()> {
        let snapshot = self
            .ttd
            .get_snapshot(target_step)
            .or_else(|| {
                // Find the closest snapshot at or before target_step
                self.ttd
                    .snapshots()
                    .into_iter()
                    .filter(|s| s.step_index <= target_step)
                    .next_back()
            })
            .cloned();

        let Some(snap) = snapshot else {
            anyhow::bail!(
                "No TTD snapshot available at or before step {}",
                target_step
            );
        };

        // Bulk restore: drop register cache so restored values are authoritative.
        self.state.invalidate_reg_cache();

        // Restore registers
        // Whatever the snapshot recorded, and only that. This used to name
        // the sixteen x86-64 general-purpose registers, so on any other
        // architecture the first write failed and the seek returned an error
        // -- the recorder had stored snapshots that could never be restored.
        let restored: Vec<(String, fission_ttd::RegisterValue)> = snap
            .registers
            .iter_all()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect();
        for (name, value) in restored {
            self.restore_register(&name, &value)?;
        }
        self.pc = snap.registers.pc;
        self.inst_count = snap.step_index;

        // Restore memory via stored deltas (forward apply new_value at keyframe).
        for delta in &snap.memory_deltas {
            let _ = self
                .state
                .write_space(self.state.ram_space(), delta.address, &delta.new_value);
        }

        // Restore shadow state via stored deltas
        for delta in &snap.shadow_deltas {
            if let Some(new_node) = delta.new_node {
                self.state
                    .set_shadow_memory(delta.space_id, delta.address, new_node);
            } else {
                self.state
                    .clear_shadow_memory(delta.space_id, delta.address);
            }
        }

        tracing::info!(
            "TTD: Restored to step {} (PC=0x{:X})",
            snap.step_index,
            self.pc
        );

        // Recompute remaining guest instructions to reach target_step.
        if self.inst_count < target_step {
            let remaining = target_step - self.inst_count;
            let saved_max = self.max_inst;
            let saved_ttd_interval = self.ttd_snapshot_interval;
            let saved_halt = self.halt_requested;
            // Pause TTD recording during recompute to avoid nested snapshots.
            self.ttd_snapshot_interval = 0;
            self.max_inst = Some(self.inst_count.saturating_add(remaining));
            self.halt_requested = false;
            self.sym_stop_requested = false;
            let _ = self.run();
            self.max_inst = saved_max;
            self.ttd_snapshot_interval = saved_ttd_interval;
            // Don't force-clear halt if recompute hit a real halt.
            if !self.halt_requested {
                self.halt_requested = saved_halt;
            }
            tracing::info!(
                "TTD: recomputed to step {} (target {}, PC=0x{:X})",
                self.inst_count,
                target_step,
                self.pc
            );
        }
        Ok(())
    }

    // ── Register I/O ─────────────────────────────────────────────────────────

    pub fn read_register_u64(&mut self, name: &str) -> Result<u64> {
        let (space_id, offset, size) = self
            .register_map
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
            .ok_or_else(|| anyhow::anyhow!("Register {} not found in register_map", name))?;

        if size > 8 {
            anyhow::bail!(
                "Register {} is too large to read as u64 (size={})",
                name,
                size
            );
        }

        let bytes = self.state.read_space(space_id, offset, size as usize)?;
        let mut val = 0u64;
        for (i, &b) in bytes.iter().enumerate() {
            val |= (b as u64) << (i * 8);
        }
        Ok(val)
    }

    pub fn write_register_u64(&mut self, name: &str, mut val: u64) -> Result<()> {
        let (space_id, offset, size) = self
            .register_map
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
            .ok_or_else(|| anyhow::anyhow!("Register {} not found in register_map", name))?;

        if size > 8 {
            anyhow::bail!(
                "Register {} is too large to write as u64 (size={})",
                name,
                size
            );
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
            let ptr_size = self.arch.pointer_size as usize;
            let sp_reg = self.arch.sp_reg;
            let sp = self.read_register_u64(sp_reg)?;
            let ram = self.state.ram_space();
            let bytes = self.state.read_space(ram, sp + stack_off, ptr_size)?;
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
            let sp = self.read_register_u64(sp_reg)?;
            let ram = self.state.ram_space();
            let bytes = self.state.read_space(ram, sp, ptr_size)?;
            let ret_addr = crate::arch::calling_convention::le_bytes_to_u64(&bytes);
            self.pc = ret_addr;
            self.write_register_u64(sp_reg, sp + ptr_size as u64)?;
        }
        Ok(())
    }

    // ── Execution ─────────────────────────────────────────────────────────────

    /// Collect a multi-instruction TB starting at `self.pc`.
    ///
    /// `pub(crate)`, not private: the interpreter path takes the same
    /// `GuestInsn` sequence, so both engines run what one decode produced
    /// rather than each deciding for itself where a block ends.
    pub(crate) fn collect_translation_block(
        &mut self,
    ) -> Result<Vec<crate::jit::compiler::GuestInsn>> {
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
            // And before a breakpoint, so the program counter lands exactly on
            // it at a block boundary where the run loop can see it. Not for the
            // first instruction: resuming from a breakpoint has to be able to
            // execute the instruction it stopped at.
            if !out.is_empty() && self.is_breakpoint(cur) {
                break;
            }
            if cur >= 0xFFFFFFF0_00000000 {
                break;
            }

            let bytes_vec = self
                .state
                .read_space(ram, cur, 16)
                .map_err(|_| anyhow::anyhow!("Failed to fetch instruction bytes at 0x{:X}", cur))?;

            let (pcode_ops, inst_len, details) = self
                .sleigh
                .decode_and_lift_with_context_override(&bytes_vec, cur, self.decode_context)
                .map_err(|e| anyhow::anyhow!("Decode/lift failed at 0x{:X}: {:#}", cur, e))?;

            for (id, name) in details.userops.iter() {
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

            // With a watchpoint armed, end the block after any instruction
            // that touches memory. A compiled block cannot stop in its own
            // middle, so the stop would otherwise land wherever the block
            // happened to end: measured over 300 hits on the fixture, 209 of
            // them stopped between one and seven instructions past the
            // access. Ending here makes the block boundary *be* the
            // instruction boundary, so the machine stops exactly after the
            // access that tripped the watch.
            //
            // Only the memory instructions end a block, not every one, and
            // only while something is watching -- arming and disarming both
            // flush the block cache, so no block outlives the decision.
            let touches_memory = !self.watchpoints.is_empty()
                && pcode_ops
                    .iter()
                    .any(|op| matches!(op.opcode, PcodeOpcode::Load | PcodeOpcode::Store));

            let len = inst_len as u32;
            out.push(GuestInsn {
                pc: cur,
                len,
                ops: pcode_ops,
            });
            cur = cur.wrapping_add(len as u64);
            if terminates || touches_memory {
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

        // A debugger's step is one instruction. Both paths below run a whole
        // translation block -- that is the point of them -- so a step takes
        // the collected block and interprets only its first instruction.
        // Decoding a block to run one of it is wasteful and is the right
        // trade: stepping happens at human speed.
        if self.single_step {
            let insns = self.collect_translation_block()?;
            return self.run_block_interpreted(&insns[..1]);
        }

        // ─── Multi-instruction TB path ────────────────────────────────────────
        //
        //   1. Cache hit  → run host TB (counts insns + soft-chains internally).
        //   2. Cache miss → collect TB → compile → insert → run.
        //   3. Compile fail, or no JIT at all → interpret the block.

        if !self.force_interpreter {
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

        if !self.observers.is_empty() {
            let shape: Vec<(u64, u32)> = insns.iter().map(|i| (i.pc, i.len)).collect();
            self.notify_translate(start_pc, &shape);
        }

        // Interpret when asked to, when there is no JIT for this host, or when
        // the JIT declines this particular block. Compiling is the fast path,
        // not the only one.
        if self.force_interpreter || self.jit.is_none() {
            return self.run_block_interpreted(&insns);
        }

        let observe = self.observe;
        let shadow = self.shadow_mode;
        let jit = self.jit.as_mut().expect("checked above");
        jit.observe = observe;
        jit.shadow = shadow;

        let reg_sp = self.state.register_space();
        let uniq_sp = self.state.unique_space();
        let func_ptr = match jit.compile_translation_block(
            &insns,
            reg_sp,
            uniq_sp,
            &mut self.wide_ops,
        ) {
            Ok(ptr) => ptr,
            Err(e) => {
                // Not fatal any more. An opcode Cranelift cannot lower is a
                // slower block, not a dead run.
                tracing::debug!(
                    "JIT declined TB@0x{start_pc:X} ({guest_insns} insns): {e:#} -- interpreting"
                );
                return self.run_block_interpreted(&insns);
            }
        };
        // An opcode the compiler lowered to nothing is a wrong answer waiting
        // to happen, and until now the only trace of it was a `tracing::warn`
        // nobody reads. Drain it where the run can report it.
        if !jit.unimplemented_ops.is_empty() {
            let found = std::mem::take(&mut jit.unimplemented_ops);
            for (op, n) in found {
                *self.metrics.unimplemented_opcodes.entry(op).or_insert(0) += n;
            }
        }
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

    /// Interpret one block and settle `pc`, matching `run_instruction`'s
    /// contract: `Ok(false)` means the run should stop.
    fn run_block_interpreted(&mut self, insns: &[crate::jit::compiler::GuestInsn]) -> Result<bool> {
        use crate::interp::InterpExit;
        self.interpreted_blocks = self.interpreted_blocks.saturating_add(1);
        match self.interpret_translation_block(insns)? {
            InterpExit::Branch(pc) | InterpExit::FallThrough(pc) => {
                self.pc = pc;
                Ok(!self.halt_requested)
            }
            InterpExit::Halt => {
                // Leave `pc` where a compiled block would have left it: past
                // the whole block, not at the instruction that halted. Which
                // of the two is more *useful* is arguable -- the halting
                // instruction says more -- but the two engines have to answer
                // the same, and this is the answer the JIT has always given.
                if let Some(last) = insns.last() {
                    self.pc = last.pc.wrapping_add(u64::from(last.len));
                }
                Ok(false)
            }
        }
    }

    /// Run until something stops the machine, and say what did.
    ///
    /// [`Self::run`] discards the outcome, which is fine for a sandbox run
    /// that only wants the final state and not fine for a debugger front
    /// end, which has to tell a breakpoint from a finished program.
    pub fn resume(&mut self) -> Result<RunOutcome> {
        self.run_inner(None)
    }

    pub fn run(&mut self) -> Result<()> {
        self.resume()?;
        Ok(())
    }

    /// Like [`Self::run`], but also stops the moment `self.pc == stop_pc` --
    /// e.g. the sentinel return address a caller wrote to the link
    /// register/stack before jumping into a function, to drive "call just
    /// this one function" without reimplementing `run`'s HLE-trap/signal/TTD
    /// handling in a parallel loop (a called function may legitimately
    /// trigger the HLE-trap path itself, e.g. an internal `memcpy`/`malloc`
    /// call).
    pub fn run_until_pc(&mut self, stop_pc: u64) -> Result<RunOutcome> {
        self.run_inner(Some(stop_pc))
    }

    fn run_inner(&mut self, stop_pc: Option<u64>) -> Result<RunOutcome> {
        tracing::info!("Sandbox execution started at PC=0x{:X}", self.pc);
        self.halt_requested = false;
        self.chain_depth = 0;
        self.pcode_budget_pc = None;
        self.watch_hit = None;
        let mut started = false;
        let outcome = loop {
            if IS_INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed) {
                tracing::warn!("Execution interrupted by Ctrl+C (SIGINT). Halting safely.");
                break RunOutcome::Interrupted;
            }
            if let Some(target) = stop_pc {
                if self.pc == target {
                    break RunOutcome::Returned;
                }
            }
            if self.halt_requested {
                break RunOutcome::Halted;
            }
            // Not on the way in: `continue` from a breakpoint has to be able
            // to leave the address it stopped at. Armed after the first check
            // rather than after the first executed instruction, so an
            // iteration that only rewrites the PC (an HLE `JumpTo`) still
            // arms it.
            if started && self.is_breakpoint(self.pc) {
                break RunOutcome::HitBreakpoint(self.pc);
            }
            started = true;
            if self.sym_stop_requested {
                tracing::debug!(
                    "Symbolic gate stop at PC=0x{:X} ({} events)",
                    self.pc,
                    self.sym_events.len()
                );
                break RunOutcome::SymGate;
            }

            if let Some(limit) = self.max_inst {
                if self.inst_count >= limit {
                    tracing::warn!("Instruction limit ({}) reached. Halting.", limit);
                    break RunOutcome::HitBudget;
                }
                // The p-code fuse (`jit_count_pcode`) only ends the *block* it
                // fires in, and every later block then exits at its own first
                // op and returns its fall-through -- so the run walks forward
                // through memory until it decodes something that is not code
                // and blames that address. A real one put 21 KB between the
                // symptom and the cause.
                //
                // Ending the run is not enough on its own: ending it *quietly*
                // turns a loud wrong answer into no answer, and this fuse only
                // trips on a defect. `max_inst` is a bound the caller asked
                // for; spending 2048 p-code ops per instruction of it is a
                // lifter or emulator bug -- so it is an error, and it names the
                // block, which is the thing worth knowing.
                if let Some(at) = self.pcode_budget_pc {
                    if self.metrics.exit_reason.is_none() {
                        self.metrics.exit_reason = Some("pcode_budget".into());
                    }
                    self.metrics.stop_pc = at;
                    anyhow::bail!(
                        "p-code budget ({}) exhausted in the block at 0x{:X} after \
                         {} guest instructions -- one instruction is looping",
                        crate::jit::callbacks::pcode_budget(limit),
                        at,
                        self.inst_count
                    );
                }
            }

            // ── HLE Trap Check (before fetch/compile — magic is not code) ────
            if self.pc >= self.magic_range.0 && self.pc < self.magic_range.1 {
                let magic = self.pc;
                let func_name = {
                    let opt = self.os.resolve_stub(&self.binary, magic);
                    opt.unwrap_or_else(|| format!("Unknown@0x{:X}", magic))
                };

                self.trace.push(crate::trace::TraceEntry::HleDispatch {
                    pc: magic,
                    func_name: func_name.clone(),
                });
                if !self.observers.is_empty() {
                    self.notify_hle(magic, &func_name);
                }

                let result = {
                    let os_ptr = &*self.os as *const dyn OsEnvironment;
                    let os_ref = unsafe { &*os_ptr };
                    os_ref.dispatch_hle(self, &func_name)?
                };

                match result {
                    HleResult::Halt(code) => {
                        // The OS layer has always reported the guest's exit
                        // code here and this dropped it on the floor, so
                        // "the program exited" could not be followed by
                        // "with what".
                        self.exit_code = Some(code);
                        self.halt_requested = true;
                        break RunOutcome::ProcessExited;
                    }
                    HleResult::Continue => {
                        self.simulate_return()?;
                    }
                    HleResult::JumpTo(pc) => {
                        self.pc = pc;
                    }
                }
                if self.single_step {
                    break RunOutcome::Stepped;
                }
                continue;
            }

            if !self.run_instruction()? {
                break RunOutcome::LoopExit;
            }

            // ── Pending Linux signals (between TBs) ───────────────────────────
            if !self.process_pending_signals()? {
                break RunOutcome::LoopExit;
            }
            if self.halt_requested {
                break RunOutcome::Halted;
            }
            if let Some(hit) = self.watch_hit {
                break RunOutcome::HitWatchpoint(hit);
            }
            if self.single_step {
                break RunOutcome::Stepped;
            }

            // TTD: record a snapshot every N instructions.
            if self.ttd_snapshot_interval > 0
                && self.ttd.is_recording()
                && self.inst_count > 0
                && self.inst_count % self.ttd_snapshot_interval == 0
            {
                let regs = self.register_state();
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
                    .map(
                        |(space_id, addr, old_node, new_node)| fission_ttd::ShadowDelta {
                            space_id: *space_id,
                            address: *addr,
                            old_node: *old_node,
                            new_node: *new_node,
                        },
                    )
                    .collect();
                self.ttd
                    .record_step_with_memory(self.inst_count, regs, 0, deltas, shadow_deltas);
                self.state.trace_mem_writes.clear();
                self.state.trace_mem_reads.clear();
                self.state.trace_shadow_writes.clear();
                tracing::trace!(
                    "TTD: recorded step {} at PC=0x{:X}",
                    self.inst_count,
                    self.pc
                );
            }
        };
        if self.ttd.is_recording() {
            let stats = self.ttd.stats();
            tracing::info!(
                "TTD recording stopped: {} snapshots, ~{} bytes",
                stats.count,
                stats.memory_bytes
            );
        }
        self.metrics.instructions = self.inst_count;
        self.metrics.stop_pc = self.pc;
        self.metrics.reg_cache_hits = self.state.reg_cache_hits;
        self.metrics.reg_cache_misses = self.state.reg_cache_misses;
        if self.metrics.exit_reason.is_none() {
            self.metrics.exit_reason = Some(match outcome {
                RunOutcome::Halted | RunOutcome::ProcessExited => "halt".into(),
                RunOutcome::SymGate => "sym_gate".into(),
                RunOutcome::HitBudget => "max_inst".into(),
                RunOutcome::Returned => "returned".into(),
                RunOutcome::Interrupted => "interrupted".into(),
                RunOutcome::LoopExit => "loop_exit".into(),
                RunOutcome::HitBreakpoint(pc) => format!("breakpoint:0x{pc:x}"),
                RunOutcome::Stepped => "stepped".into(),
                RunOutcome::HitWatchpoint(hit) => format!("watchpoint:0x{:x}", hit.address),
            });
        }
        tracing::info!(
            "Sandbox execution finished at PC=0x{:X} ({} instructions)",
            self.pc,
            self.inst_count
        );
        tracing::info!("Emulator metrics: {}", self.metrics.summary_line());
        Ok(outcome)
    }
}

/// A memory range the run loop stops on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watchpoint {
    pub start: u64,
    pub len: u64,
    pub on_read: bool,
    pub on_write: bool,
}

/// The access that tripped a [`Watchpoint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchHit {
    /// Address of the access, which is not necessarily the watched address:
    /// an eight-byte store can straddle a one-byte watch.
    pub address: u64,
    pub size: u32,
    pub write: bool,
    pub value: u64,
    /// The instruction that made the access.
    pub pc: u64,
    /// The retired-instruction count at the access -- the same number a TTD
    /// snapshot is indexed by, so "seek back to just before this write" is a
    /// `ttd_seek` away.
    pub step: u64,
    pub watch: Watchpoint,
}

/// What the instruction at some address is, as far as stepping cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstructionShape {
    pub address: u64,
    pub length: u32,
    pub is_call: bool,
    pub is_return: bool,
}

impl InstructionShape {
    /// Where execution continues if the instruction does not branch -- the
    /// address a "step over" of a call stops at.
    pub fn fall_through(&self) -> u64 {
        self.address.wrapping_add(u64::from(self.length))
    }
}

/// Why [`Emulator::run_inner`] (and therefore [`Emulator::run`]/
/// [`Emulator::run_until_pc`]) stopped -- distinct, reported outcomes so a
/// caller (e.g. `fission-dir`'s "call one function" driver) never has to
/// guess or silently coerce a non-`Returned` stop into a pass/fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// `stop_pc` (from [`Emulator::run_until_pc`]) was reached.
    Returned,
    /// `halt_requested` was set (explicit halt request, e.g. `exit`/`abort`
    /// HLE reaching a terminal state that isn't itself `ProcessExited`).
    Halted,
    /// The concolic/symbolic exploration gate requested a stop.
    SymGate,
    /// `max_inst` instruction budget was reached.
    HitBudget,
    /// An HLE call resulted in `HleResult::Halt` (e.g. the process called
    /// `exit`/`_exit`/`abort`) -- the callee terminated the whole process
    /// rather than returning normally.
    ProcessExited,
    /// `run_instruction`/signal processing returned `false` (no more code,
    /// or a fatal signal) -- the pre-existing `run()` "loop_exit" case.
    LoopExit,
    /// `Ctrl+C` (SIGINT) was observed.
    Interrupted,
    /// The program counter reached an address set with
    /// [`Emulator::set_breakpoint`]. The machine is stopped *before* the
    /// instruction there, so resuming executes it.
    HitBreakpoint(u64),
    /// [`Emulator::step_instruction`] executed its one instruction.
    Stepped,
    /// A watched memory range was accessed. Details in
    /// [`Emulator::last_watch_hit`].
    HitWatchpoint(WatchHit),
}
