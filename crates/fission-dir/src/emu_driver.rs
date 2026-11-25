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
use fission_emulator::arch::Endianness;
use fission_emulator::pcode::page_map::{AccessKind, PageFault, prot};
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
    /// Execution reached code the decode did not cover (a switch case it
    /// did not resolve, an indirect tail jump). Calls there are not stubbed,
    /// so the run says nothing about the tree.
    LeftDecodedCode(u64),
    /// Anything else: budget, fault, exit.
    Other(CallOutcome),
}

/// How far an aliased input pointer pointing into the stack frame is
/// conservatively treated as observable caller/out-param storage (in bytes).
pub const ALIAS_OBSERVATION_WINDOW: u64 = 256;

/// How far above `entry_sp` is inspected when auditing writes into caller
/// stack space outside ABI home/shadow slots.
const CALLER_STACK_AUDIT_WINDOW: u64 = 4096;

/// Maximum representative addresses retained in per-run audit summaries.
const MAX_AUDIT_SAMPLES: usize = 16;

/// Exact ABI-derived specification of which addresses belong to a called
/// function's private stack frame versus observable caller-owned / aliased
/// storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameExclusionSpec {
    pub entry_sp: u64,
    pub cfa: u64,
    pub pointer_size: u64,
    /// `[entry_sp - FRAME_BELOW .. entry_sp)`: function's private stack locals.
    pub private_below: std::ops::Range<u64>,
    /// Return-address push slot `[entry_sp .. entry_sp + pointer_size)` for
    /// stack-return ABIs (`None` for link-register ABIs). Never excluded.
    pub return_addr_slot: Option<std::ops::Range<u64>>,
    /// Mandatory callee-owned spill/home region above `cfa` (32B on Win64;
    /// empty on SysV64/x86-32). For link-register ABIs this covers
    /// `[entry_sp .. entry_sp + base_offset)`.
    pub abi_home_space: std::ops::Range<u64>,
    /// Outgoing caller-prepared stack argument slots above `abi_home_space`.
    /// Never excluded.
    pub caller_stack_args: std::ops::Range<u64>,
    /// Subranges inside the frame that an incoming parameter pointer can alias;
    /// writes there are observable rather than private.
    pub aliased_input_ranges: Vec<std::ops::Range<u64>>,
}

impl FrameExclusionSpec {
    /// Construct an ABI-justified exclusion specification for a call entered at
    /// `entry_sp` with `args`.
    #[must_use]
    pub fn from_abi(
        entry_sp: u64,
        pointer_size: u64,
        has_link_reg: bool,
        stack_arg_offset_0: u64,
        reg_arg_count: usize,
        args: &[u64],
    ) -> Self {
        let ptr_size = pointer_size.max(1);
        let (cfa, return_addr_slot, abi_home_space) = if has_link_reg {
            (
                entry_sp,
                None,
                entry_sp..entry_sp.saturating_add(stack_arg_offset_0),
            )
        } else {
            let cfa = entry_sp.wrapping_add(ptr_size);
            let ret_slot = entry_sp..cfa;
            let home_end = entry_sp.saturating_add(stack_arg_offset_0.max(ptr_size));
            (cfa, Some(ret_slot), cfa..home_end)
        };
        let stack_arg_count = args.len().saturating_sub(reg_arg_count) as u64;
        let args_start = entry_sp.saturating_add(stack_arg_offset_0);
        let args_end = args_start.saturating_add(stack_arg_count.saturating_mul(ptr_size));
        let caller_stack_args = args_start..args_end;
        let private_below = entry_sp.saturating_sub(FRAME_BELOW)..entry_sp;

        let mut spec = Self {
            entry_sp,
            cfa,
            pointer_size: ptr_size,
            private_below,
            return_addr_slot,
            abi_home_space,
            caller_stack_args,
            aliased_input_ranges: Vec::new(),
        };
        for &arg in args {
            if spec.in_private_frame(arg) {
                spec.aliased_input_ranges
                    .push(arg..arg.saturating_add(ALIAS_OBSERVATION_WINDOW));
            }
        }
        spec
    }

    /// True if `addr` lies in the function's private stack area (`private_below`
    /// or `abi_home_space`) and is NOT the return address slot or caller stack
    /// arguments.
    #[must_use]
    pub fn in_private_frame(&self, addr: u64) -> bool {
        let in_candidate =
            self.private_below.contains(&addr) || self.abi_home_space.contains(&addr);
        let in_ret = self
            .return_addr_slot
            .as_ref()
            .is_some_and(|r| r.contains(&addr));
        let in_caller_args = self.caller_stack_args.contains(&addr);
        in_candidate && !in_ret && !in_caller_args
    }

    /// True if a write at `addr` may be excluded from external-memory diffing.
    #[must_use]
    pub fn is_excluded(&self, addr: u64) -> bool {
        self.in_private_frame(addr)
            && !self
                .aliased_input_ranges
                .iter()
                .any(|range| range.contains(&addr))
    }

    /// True if `addr` lies in caller-owned stack space above/at `entry_sp`
    /// outside ABI callee home slots.
    #[must_use]
    pub fn is_caller_stack(&self, addr: u64) -> bool {
        let above_entry =
            addr >= self.entry_sp && addr < self.entry_sp.saturating_add(CALLER_STACK_AUDIT_WINDOW);
        above_entry && !self.abi_home_space.contains(&addr)
    }

    /// Retain only writes outside the audited private frame exclusion.
    #[must_use]
    pub fn filter_observable(&self, writes: &BTreeMap<u64, u8>) -> BTreeMap<u64, u8> {
        writes
            .iter()
            .filter(|(addr, _)| !self.is_excluded(**addr))
            .map(|(addr, value)| (*addr, *value))
            .collect()
    }
}

/// How an address within the private frame escaped the function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscapedFramePointer {
    Returned(u64),
    StoredExternally {
        at: u64,
        target: u64,
    },
    PassedToCall {
        call_index: usize,
        site: u64,
        ptr: u64,
    },
}

/// Whether excluding private frame writes in this run is justified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameExclusionJustification {
    /// No private-frame pointer escaped, SP balanced, no overlapping accesses.
    JustifiedPrivateFrame,
    /// One or more private-frame pointers were passed directly to stubbed callees
    /// (modeled separately by argument comparison and callee-write stubs).
    EscapedToCallee { count: usize },
    /// Frame exclusion could not be fully justified as unobservable private
    /// storage (e.g. frame pointer returned/stored externally, SP mismatch, or
    /// write straddling the frame boundary).
    Unjustified { reasons: Vec<String> },
}

/// Full frame-exclusion and ABI-preservation audit for one emulated run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameExclusionAudit {
    pub spec: FrameExclusionSpec,
    pub excluded_write_bytes: usize,
    pub caller_stack_write_addrs: Vec<u64>,
    pub aliased_input_addrs: Vec<u64>,
    pub escaped_frame_pointers: Vec<EscapedFramePointer>,
    pub boundary_overlaps: Vec<(u64, u32)>,
    pub sp_delta: Option<i64>,
    pub clobbered_callee_saved: Vec<String>,
    pub justification: FrameExclusionJustification,
}

/// Audit of memory mapping/permission compliance versus `lazy_memory` fallback.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryAccessAudit {
    pub lazy_memory_active: bool,
    pub synthetic_unmapped_reads: Vec<u64>,
    pub synthetic_unmapped_writes: Vec<u64>,
    pub permission_violations: Vec<u64>,
    pub total_synthetic_pages: usize,
}

impl MemoryAccessAudit {
    #[must_use]
    pub fn used_synthetic_memory(&self) -> bool {
        !self.synthetic_unmapped_reads.is_empty()
            || !self.synthetic_unmapped_writes.is_empty()
            || !self.permission_violations.is_empty()
    }
}

/// A call into one function with every call it makes stubbed out.
#[derive(Debug)]
pub struct StubbedRun {
    pub end: StubbedEnd,
    pub calls: Vec<ObservedCall>,
    /// Every byte the function stored outside its audited private frame, with
    /// the value it held when the run ended.
    pub writes: BTreeMap<u64, u8>,
    /// All bytes written by the function before frame exclusion, used to audit
    /// escaped frame storage.
    pub all_writes: BTreeMap<u64, u8>,
    /// The first byte of the function's own frame, below its entry stack
    /// pointer, that it read before anything in this run wrote it.
    pub undefined_frame_read: Option<u64>,
    pub exit_code: Option<u32>,
    pub exit_signal: Option<i32>,
    pub frame_audit: FrameExclusionAudit,
    pub memory_audit: MemoryAccessAudit,
}

/// Which bytes of the frame below the entry stack pointer this run has
/// written, and the first one it read before writing.
#[derive(Default)]
struct FrameState {
    written: BTreeSet<u64>,
    first_undefined_read: Option<u64>,
}

/// Watches the frame below the entry stack pointer for reads of bytes
/// nothing in the run wrote: the stack garbage C leaves undefined.
struct FrameReads {
    region: std::ops::Range<u64>,
    state: Arc<Mutex<FrameState>>,
}

impl fission_emulator::observe::Observer for FrameReads {
    fn interest(&self) -> fission_emulator::observe::ObserveMask {
        fission_emulator::observe::ObserveMask {
            mem: true,
            ..fission_emulator::observe::ObserveMask::NONE
        }
    }

    fn on_mem(&mut self, addr: u64, size: u32, write: bool, _value: u64) {
        let mut state = self.state.lock().expect("frame state");
        for at in (0..u64::from(size)).map(|i| addr.wrapping_add(i)) {
            if !self.region.contains(&at) {
                continue;
            }
            if write {
                state.written.insert(at);
            } else if !state.written.contains(&at) && state.first_undefined_read.is_none() {
                state.first_undefined_read = Some(at);
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
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

/// Records the first byte read per 4 KiB page (`page_base -> first_addr`)
/// so post-run auditing can check mapped permissions without storing a per-access
/// trace.
struct ReadPageLog(Arc<Mutex<BTreeMap<u64, u64>>>);

impl fission_emulator::observe::Observer for ReadPageLog {
    fn interest(&self) -> fission_emulator::observe::ObserveMask {
        fission_emulator::observe::ObserveMask {
            mem: true,
            ..fission_emulator::observe::ObserveMask::NONE
        }
    }

    fn on_mem(&mut self, addr: u64, size: u32, write: bool, _value: u64) {
        if write || size == 0 {
            return;
        }
        let first_page = addr & !0xfff;
        let last_page = addr.saturating_add(u64::from(size) - 1) & !0xfff;
        let mut map = self.0.lock().expect("read page log");
        let mut page = first_page;
        loop {
            let sample = if page == first_page { addr } else { page };
            map.entry(page).or_insert(sample);
            if page >= last_page {
                break;
            }
            page = page.saturating_add(4096);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Records the first translation block that starts outside the function's
/// decoded instructions.
struct CoverageLog {
    instructions: Arc<BTreeSet<u64>>,
    stray: Arc<Mutex<Option<u64>>>,
}

impl fission_emulator::observe::Observer for CoverageLog {
    fn interest(&self) -> fission_emulator::observe::ObserveMask {
        fission_emulator::observe::ObserveMask {
            block: true,
            ..fission_emulator::observe::ObserveMask::NONE
        }
    }

    fn on_block(&mut self, pc: u64) {
        if pc != SENTINEL_RET && !self.instructions.contains(&pc) {
            self.stray.lock().expect("coverage log").get_or_insert(pc);
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
pub const MAX_STUBBED_CALLS: usize = 256;

/// What a stand-in for each callee does: the same on both sides of a check,
/// so a call has one effect whichever side makes it.
pub struct CallStub<'a> {
    /// The value the `n`th call returns.
    pub result: &'a dyn Fn(usize) -> u64,
    /// What the `n`th call leaves in the `index`th register the convention
    /// lets a callee overwrite (`killed_by_call`). `None` leaves them alone.
    pub clobber: Option<&'a dyn Fn(usize, usize) -> u64>,
    /// The bytes the `n`th call, given these argument registers, writes.
    pub writes: &'a dyn Fn(usize, &[u64]) -> Vec<(u64, u8)>,
}

impl<'a> CallStub<'a> {
    /// A stub that returns `result(n)` and does nothing else.
    pub fn returning(result: &'a dyn Fn(usize) -> u64) -> Self {
        Self {
            result,
            clobber: None,
            writes: &no_writes,
        }
    }
}

fn no_writes(_: usize, _: &[u64]) -> Vec<(u64, u8)> {
    Vec::new()
}

/// How far below the entry stack pointer a function's own frame may reach.
/// Writes there are its locals, which the decompiled body keeps in variables.
const FRAME_BELOW: u64 = 1 << 20;

/// A real emulator with a real, loaded process image, reusable across many
/// concrete calls into the same binary. Restores to the post-image-setup
/// baseline before every call (see [`Self::call`]) so one call's global/heap
/// side effects can't leak into the next.
pub struct EmulatorHarness {
    emu: Emulator,
    baseline_state: MachineState,
    baseline_pc: u64,
    /// Address and byte length of the scratch buffer, once installed.
    scratch: Option<(u64, usize)>,
    lazy_memory_active: bool,
    last_call_args: Vec<u64>,
}

fn collect_memory_access_audit(
    lazy_memory_active: bool,
    baseline_state: &MachineState,
    read_pages: &BTreeMap<u64, u64>,
    write_pages: &BTreeMap<u64, u64>,
) -> MemoryAccessAudit {
    let mut synthetic_unmapped_reads = Vec::new();
    let mut synthetic_unmapped_writes = Vec::new();
    let mut permission_violations = Vec::new();
    let mut synthetic_pages = BTreeSet::new();

    if baseline_state.enforce_page_faults {
        for (&page, &addr) in read_pages {
            match baseline_state.page_map.check_access(addr, AccessKind::Read) {
                Ok(()) => {}
                Err(PageFault::NotMapped { .. }) => {
                    synthetic_pages.insert(page);
                    if synthetic_unmapped_reads.len() < MAX_AUDIT_SAMPLES {
                        synthetic_unmapped_reads.push(addr);
                    }
                }
                Err(PageFault::Prot { .. }) => {
                    synthetic_pages.insert(page);
                    if permission_violations.len() < MAX_AUDIT_SAMPLES {
                        permission_violations.push(addr);
                    }
                }
            }
        }
        for (&page, &addr) in write_pages {
            match baseline_state
                .page_map
                .check_access(addr, AccessKind::Write)
            {
                Ok(()) => {}
                Err(PageFault::NotMapped { .. }) => {
                    synthetic_pages.insert(page);
                    if synthetic_unmapped_writes.len() < MAX_AUDIT_SAMPLES {
                        synthetic_unmapped_writes.push(addr);
                    }
                }
                Err(PageFault::Prot { .. }) => {
                    synthetic_pages.insert(page);
                    if permission_violations.len() < MAX_AUDIT_SAMPLES {
                        permission_violations.push(addr);
                    }
                }
            }
        }
    }

    MemoryAccessAudit {
        lazy_memory_active,
        synthetic_unmapped_reads,
        synthetic_unmapped_writes,
        permission_violations,
        total_synthetic_pages: synthetic_pages.len(),
    }
}

fn scan_stored_frame_pointers(
    observable_writes: &BTreeMap<u64, u8>,
    spec: &FrameExclusionSpec,
    ptr_size: u8,
    endian: Endianness,
    escaped: &mut Vec<EscapedFramePointer>,
) {
    let size = usize::from(ptr_size);
    if size != 4 && size != 8 {
        return;
    }
    for &addr in observable_writes.keys() {
        if escaped.len() >= MAX_AUDIT_SAMPLES {
            break;
        }
        let Some(bytes) = (0..size as u64)
            .map(|off| observable_writes.get(&addr.wrapping_add(off)).copied())
            .collect::<Option<Vec<u8>>>()
        else {
            continue;
        };
        let target = match (size, endian) {
            (8, Endianness::Little) => u64::from_le_bytes(bytes.try_into().expect("8 bytes")),
            (8, Endianness::Big) => u64::from_be_bytes(bytes.try_into().expect("8 bytes")),
            (4, Endianness::Little) => {
                u64::from(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
            }
            (4, Endianness::Big) => {
                u64::from(u32::from_be_bytes(bytes.try_into().expect("4 bytes")))
            }
            _ => continue,
        };
        if spec.in_private_frame(target)
            && !escaped.iter().any(|existing| {
                matches!(
                    existing,
                    EscapedFramePointer::StoredExternally { at, .. } if *at == addr
                )
            })
        {
            escaped.push(EscapedFramePointer::StoredExternally { at: addr, target });
        }
    }
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

/// Stack pointer at the callee's entry for a link-register ABI (ARM/AArch64/
/// MIPS/PowerPC) where no return address is pushed on the stack, reserving
/// room below baseline `sp` for outgoing stack arguments (and any ABI base
/// offset such as MIPS o32's 16-byte home area).
fn link_reg_call_frame_sp(sp: u64, ptr_size: u64, base_offset: u64, stack_args: u64) -> u64 {
    let outgoing = base_offset.saturating_add(stack_args.saturating_mul(ptr_size));
    let frame = outgoing.div_ceil(16) * 16;
    sp.saturating_sub(frame)
}

fn encode_word(value: u64, ptr_size: u8, endian: Endianness) -> Vec<u8> {
    let size = usize::from(ptr_size).min(8);
    match endian {
        Endianness::Little => value.to_le_bytes()[..size].to_vec(),
        Endianness::Big => value.to_be_bytes()[8 - size..].to_vec(),
    }
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
            lazy_memory_active: false,
            last_call_args: Vec::new(),
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
        let ram = self.emu.state.ram_space();
        if let Some((addr, mapped_len)) = self.scratch
            && contents.len() <= mapped_len
        {
            if self
                .baseline_state
                .read_space_readonly(ram, addr, contents.len())
                .ok()
                .as_deref()
                == Some(contents)
            {
                return Ok(addr);
            }
            self.emu.state = self.baseline_state.clone();
            self.emu
                .state
                .write_space(ram, addr, contents)
                .context("failed to update the scratch buffer")?;
            self.baseline_state = self.emu.state.clone();
            return Ok(addr);
        }
        self.emu.state = self.baseline_state.clone();
        let len = contents.len().max(1) as u64;
        let addr = self
            .emu
            .state
            .page_map
            .mmap_anon(len, prot::READ | prot::WRITE);
        self.emu
            .state
            .write_space(ram, addr, contents)
            .context("failed to fill the scratch buffer")?;
        // Re-baseline so every later call sees the buffer, and sees it
        // unmodified.
        self.baseline_state = self.emu.state.clone();
        self.scratch = Some((addr, len as usize));
        Ok(addr)
    }

    /// Write `bytes` at `addr` into the restored baseline, so every later
    /// call starts with them there -- a global the caller wants to carry
    /// from one call to the next, say. The address must already be mapped
    /// (the image, or a buffer installed here).
    pub fn write_baseline(&mut self, addr: u64, bytes: &[u8]) -> Result<()> {
        if self.baseline_state.enforce_page_faults
            && (0..bytes.len() as u64).any(|offset| {
                self.baseline_state
                    .page_map
                    .check_access(
                        addr + offset,
                        fission_emulator::pcode::page_map::AccessKind::Read,
                    )
                    .is_err()
            })
        {
            bail!("{addr:#x}: not mapped in the baseline");
        }
        let ram = self.baseline_state.ram_space();
        // Bytes already there need no write, so read-only memory (a constant
        // table) can be passed back unchanged.
        if self
            .baseline_state
            .read_space_readonly(ram, addr, bytes.len())
            .ok()
            .as_deref()
            == Some(bytes)
        {
            return Ok(());
        }
        self.baseline_state
            .write_space(ram, addr, bytes)
            .context("failed to write the baseline")
    }

    /// Map a readable/writable block holding `contents`, point `register`
    /// at it, and make both part of the restored baseline. Returns the
    /// block's address.
    ///
    /// For a register the caller sets up and a function only reads memory
    /// through -- the thread pointer, whose block holds thread-local storage
    /// such as the stack protector's canary. Without it that register is 0
    /// at every call and the first such read faults.
    pub fn install_register_block(&mut self, register: &str, contents: &[u8]) -> Result<u64> {
        self.emu.state = self.baseline_state.clone();
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
            .context("failed to fill the register's block")?;
        self.emu
            .write_register_u64(register, addr)
            .with_context(|| format!("failed to point {register} at its block"))?;
        self.baseline_state = self.emu.state.clone();
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

    /// Read `len` bytes of RAM as the last [`Self::call`] left them -- the
    /// state a caller sees after the function returns, before the next call
    /// restores the baseline.
    pub fn read_memory(&mut self, addr: u64, len: usize) -> Result<Vec<u8>> {
        let ram = self.emu.state.ram_space();
        self.emu.state.read_space(ram, addr, len)
    }

    /// Read one byte of the restored baseline: the program image as every
    /// call starts from it. `None` where nothing is mapped.
    /// Whether the loaded image maps `addr`.
    pub fn in_image(&self, addr: u64) -> bool {
        self.baseline_byte(addr).is_some()
    }

    pub fn baseline_byte(&self, addr: u64) -> Option<u8> {
        // The raw space read zero-fills a page nothing mapped, which would
        // let a tree's stray load agree with machine code that never made it.
        if self.baseline_state.enforce_page_faults
            && self
                .baseline_state
                .page_map
                .check_access(addr, fission_emulator::pcode::page_map::AccessKind::Read)
                .is_err()
        {
            return None;
        }
        let ram = self.baseline_state.ram_space();
        self.baseline_state
            .read_space_readonly(ram, addr, 1)
            .ok()
            .and_then(|bytes| bytes.first().copied())
    }

    /// Call `address` as [`Self::call`] does, but stop at every call
    /// instruction in `flow` instead of running the callee: record the
    /// argument registers, put `call_return(n)` in the return register and
    /// continue at the site's fall-through. A call mapped to `None` does not
    /// return and ends the run. A tail jump is a call whose return is this
    /// function's: it is recorded and stubbed the same way, then returns.
    ///
    /// Stubbing is what lets the decompiled body be compared at all: both
    /// sides see the same return values without either having to model the
    /// callee. This entry point stubs only the return value; see
    /// [`CallStub`] for one that also overwrites what a callee may.
    pub fn call_stubbed(
        &mut self,
        address: u64,
        args: &[u64],
        flow: &crate::call_sites::MachineFlow,
        call_return: &dyn Fn(usize) -> u64,
    ) -> Result<StubbedRun> {
        self.prepare_call(address, args)?;
        self.run_prepared(flow, &CallStub::returning(call_return))
    }

    /// Set up one run of `address` from `state`: the baseline image, with
    /// every unmapped byte, every general-purpose register and every argument
    /// slot taken from `state`. `args` are the raw slot words
    /// ([`crate::initial_state::InitialState::argument_word`]).
    ///
    /// The machine is left stopped at the function's first instruction, so
    /// the caller can read registers ([`Self::registers`]) before
    /// [`Self::run_prepared`] runs it.
    pub fn begin_run(
        &mut self,
        address: u64,
        args: &[u64],
        state: &crate::initial_state::InitialState,
    ) -> Result<()> {
        self.restore_baseline();
        let lazy = *state;
        self.emu.state.lazy_memory = Some(Arc::new(move |addr| lazy.memory_byte(addr)));
        self.lazy_memory_active = true;
        let sp = self.emu.arch.sp_reg;
        let pc = self.emu.arch.pc_reg;
        let link = self.emu.arch.cc.return_addr_reg();
        let gp: Vec<&'static str> = self.emu.arch.gp_regs.to_vec();
        for (index, name) in gp.iter().enumerate() {
            let fixed = [Some(sp), Some(pc), link]
                .iter()
                .flatten()
                .any(|reg| reg.eq_ignore_ascii_case(name));
            if !fixed {
                self.emu
                    .write_register_u64(name, state.register_word(index as u64))?;
            }
        }
        self.setup_call(address, args)
    }

    /// Set up a strict native-admissible run of `address` with `args`: NO
    /// `lazy_memory`, strict `PageMap` page-fault enforcement, and baseline
    /// registers unmodified outside the ABI argument slots.
    pub fn begin_native_run(&mut self, address: u64, args: &[u64]) -> Result<()> {
        self.restore_baseline();
        self.emu.state.lazy_memory = None;
        self.emu.state.enforce_page_faults = true;
        self.lazy_memory_active = false;
        self.setup_call(address, args)
    }

    /// Width of an argument slot: the target's pointer size, in bits.
    pub fn pointer_bits(&self) -> u32 {
        u32::from(self.emu.arch.pointer_size) * 8
    }

    /// The canonical frame address of the call [`Self::begin_run`] set up:
    /// the stack pointer before the call. A target that pushes its return
    /// address (x86) entered one word below it; one that keeps it in a link
    /// register entered at it.
    pub fn cfa(&mut self) -> Result<u64> {
        let sp = self.emu.read_register_u64(self.emu.arch.sp_reg)?;
        Ok(if self.emu.arch.cc.return_addr_reg().is_some() {
            sp
        } else {
            sp.wrapping_add(u64::from(self.emu.arch.pointer_size))
        })
    }

    /// The part of the frame below the entry stack pointer in the run
    /// [`Self::begin_run`] set up: what the function's own locals occupy, and
    /// what C leaves undefined until the function writes it.
    pub fn frame_below_entry(&mut self) -> Result<std::ops::Range<u64>> {
        let entry_sp = self.emu.read_register_u64(self.emu.arch.sp_reg)?;
        Ok(entry_sp.saturating_sub(FRAME_BELOW)..entry_sp)
    }

    /// ABI-derived private frame range for [`Self::begin_run`]. Covers locals
    /// below `entry_sp` and mandatory ABI home/shadow slots above `cfa` (e.g.
    /// 32B Win64 shadow space), excluding caller stack arguments. See
    /// [`Self::frame_exclusion_spec`] for full return-slot/alias exclusions.
    pub fn frame_range(&mut self) -> Result<std::ops::Range<u64>> {
        let entry_sp = self.emu.read_register_u64(self.emu.arch.sp_reg)?;
        let above = self.emu.arch.cc.stack_arg_offset(0);
        Ok(entry_sp.saturating_sub(FRAME_BELOW)..entry_sp.saturating_add(above))
    }

    /// Audited ABI exclusion specification for the call currently prepared on
    /// this harness.
    pub fn frame_exclusion_spec(&mut self) -> Result<FrameExclusionSpec> {
        let entry_sp = self.emu.read_register_u64(self.emu.arch.sp_reg)?;
        Ok(FrameExclusionSpec::from_abi(
            entry_sp,
            u64::from(self.emu.arch.pointer_size),
            self.emu.arch.cc.return_addr_reg().is_some(),
            self.emu.arch.cc.stack_arg_offset(0),
            self.emu.arch.cc.arg_regs().len(),
            &self.last_call_args,
        ))
    }

    /// Registers as they stand now -- after [`Self::begin_run`], what the
    /// function finds at entry. Names the machine does not know are left out.
    pub fn registers(&mut self, names: &[String]) -> Vec<(String, i64)> {
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

    /// What a run from `state` finds at `addr` before anything writes it: the
    /// image where the image maps it, the state's own byte elsewhere -- what
    /// the machine's lazy memory fills an unmapped page with.
    pub fn initial_byte(&self, state: &crate::initial_state::InitialState, addr: u64) -> u8 {
        self.baseline_byte(addr)
            .unwrap_or_else(|| state.memory_byte(addr))
    }

    /// Run the call [`Self::prepare_call`] or [`Self::begin_run`] set up, as
    /// [`Self::call_stubbed`] describes.
    pub fn run_prepared(
        &mut self,
        flow: &crate::call_sites::MachineFlow,
        stub: &CallStub<'_>,
    ) -> Result<StubbedRun> {
        let sp_reg = self.emu.arch.sp_reg;
        let pc_reg = self.emu.arch.pc_reg;
        let return_reg = self.emu.arch.cc.return_reg();
        let link_reg = self.emu.arch.cc.return_addr_reg();
        let arg_regs: Vec<&'static str> = self.emu.arch.cc.arg_regs().to_vec();
        let killed: Vec<&'static str> = self.emu.arch.cc.killed_by_call().to_vec();

        let entry_sp = self.emu.read_register_u64(sp_reg)?;
        let ptr_size_u8 = self.emu.arch.pointer_size;
        let endian = self.emu.arch.endian;
        let frame_spec = FrameExclusionSpec::from_abi(
            entry_sp,
            u64::from(ptr_size_u8),
            link_reg.is_some(),
            self.emu.arch.cc.stack_arg_offset(0),
            arg_regs.len(),
            &self.last_call_args,
        );

        let callee_saved_names: Vec<&'static str> = self
            .emu
            .arch
            .gp_regs
            .iter()
            .copied()
            .filter(|name| {
                let is_special = [Some(sp_reg), Some(pc_reg), Some(return_reg), link_reg]
                    .into_iter()
                    .flatten()
                    .any(|r| r.eq_ignore_ascii_case(name));
                let is_arg = arg_regs.iter().any(|r| r.eq_ignore_ascii_case(name));
                let is_killed = killed.iter().any(|r| r.eq_ignore_ascii_case(name));
                !is_special && !is_arg && !is_killed
            })
            .collect();
        let entry_callee_saved: Vec<(&'static str, u64)> = callee_saved_names
            .iter()
            .filter_map(|&name| self.emu.read_register_u64(name).ok().map(|val| (name, val)))
            .collect();

        let log = Arc::new(Mutex::new(Vec::new()));
        self.emu.add_observer(Box::new(WriteLog(Arc::clone(&log))));
        let read_pages = Arc::new(Mutex::new(BTreeMap::new()));
        self.emu
            .add_observer(Box::new(ReadPageLog(Arc::clone(&read_pages))));
        let below_entry = frame_spec.private_below.clone();
        let frame_state = Arc::new(Mutex::new(FrameState::default()));
        self.emu.add_observer(Box::new(FrameReads {
            region: below_entry.clone(),
            state: Arc::clone(&frame_state),
        }));
        let stray = Arc::new(Mutex::new(None));
        self.emu.add_observer(Box::new(CoverageLog {
            instructions: Arc::new(flow.instructions.clone()),
            stray: Arc::clone(&stray),
        }));
        for site in flow.calls.keys().chain(&flow.tail_jumps) {
            self.emu.set_breakpoint(*site);
        }
        let mut calls = Vec::new();
        let end = loop {
            // The emulator does not stop at a breakpoint on the PC it starts
            // from -- a debugger has to be able to continue past one -- so a
            // site at the entry, or one right after the previous stub's
            // fall-through, is handled before resuming rather than run.
            let pc = self.emu.pc;
            let is_call = flow.calls.contains_key(&pc);
            if is_call || flow.tail_jumps.contains(&pc) {
                if calls.len() >= MAX_STUBBED_CALLS {
                    break StubbedEnd::Other(CallOutcome::HitBudget);
                }
                let args = arg_regs
                    .iter()
                    .map(|reg| self.emu.read_register_u64(reg))
                    .collect::<Result<Vec<_>>>()?;
                let n = calls.len();
                let writes = (stub.writes)(n, &args);
                calls.push(ObservedCall { site: pc, args });
                if is_call && flow.calls[&pc].is_none() {
                    break StubbedEnd::NoReturnCall(pc);
                }
                // What the callee may do: write the memory the stub says,
                // leave anything in the registers a call kills, and return.
                // Written straight into the state, these are not stores the
                // function made.
                let ram = self.emu.state.ram_space();
                for (addr, byte) in writes {
                    if self.emu.state.write_space(ram, addr, &[byte]).is_ok()
                        && below_entry.contains(&addr)
                    {
                        // The callee wrote it: defined from here on.
                        frame_state
                            .lock()
                            .expect("frame state")
                            .written
                            .insert(addr);
                    }
                }
                if let Some(clobber) = stub.clobber {
                    for (index, reg) in killed.iter().enumerate() {
                        let _ = self.emu.write_register_u64(reg, clobber(n, index));
                    }
                }
                self.emu.write_register_u64(return_reg, (stub.result)(n))?;
                if is_call {
                    self.emu.pc = flow.calls[&pc].expect("checked above");
                } else {
                    // Return to this function's caller, as the callee would.
                    self.emu.simulate_return_for("")?;
                }
                continue;
            }
            let outcome = match self.emu.run_until_pc(SENTINEL_RET) {
                Ok(outcome) => outcome,
                Err(err) => {
                    self.release_run();
                    return Err(err);
                }
            };
            if let Some(pc) = *stray.lock().expect("coverage log") {
                break StubbedEnd::LeftDecodedCode(pc);
            }
            match outcome {
                RunOutcome::HitBreakpoint(_) => continue,
                RunOutcome::Returned => {
                    break StubbedEnd::Returned(self.emu.read_register_u64(return_reg)?);
                }
                other => break StubbedEnd::Other(self.finish_outcome(other)?),
            }
        };
        self.release_run();

        let (sp_delta, clobbered_callee_saved) = if matches!(end, StubbedEnd::Returned(_)) {
            let expected_sp = frame_spec.cfa;
            let delta = self
                .emu
                .read_register_u64(sp_reg)
                .ok()
                .map(|exit_sp| exit_sp.wrapping_sub(expected_sp) as i64);
            let mut clobbered = Vec::new();
            for (name, before) in &entry_callee_saved {
                if let Ok(after) = self.emu.read_register_u64(name)
                    && after != *before
                {
                    clobbered.push((*name).to_string());
                }
            }
            (delta, clobbered)
        } else {
            (None, Vec::new())
        };

        let ram = self.emu.state.ram_space();
        let write_log_entries = log.lock().expect("write log").clone();
        let mut all_written_addrs = BTreeSet::new();
        let mut written = BTreeSet::new();
        let mut boundary_overlaps = Vec::new();
        let mut caller_stack_addrs = BTreeSet::new();
        let mut excluded_addrs = BTreeSet::new();
        let mut write_pages: BTreeMap<u64, u64> = BTreeMap::new();

        for &(addr, size) in &write_log_entries {
            if size == 0 {
                continue;
            }
            let first_page = addr & !0xfff;
            let last_page = addr.saturating_add(u64::from(size) - 1) & !0xfff;
            let mut page = first_page;
            loop {
                let sample = if page == first_page { addr } else { page };
                write_pages.entry(page).or_insert(sample);
                if page >= last_page {
                    break;
                }
                page = page.saturating_add(4096);
            }

            let mut any_excluded = false;
            let mut any_observable = false;
            for i in 0..u64::from(size) {
                let at = addr.wrapping_add(i);
                all_written_addrs.insert(at);
                if frame_spec.is_excluded(at) {
                    any_excluded = true;
                } else {
                    any_observable = true;
                }
            }
            let overlaps = any_excluded && any_observable;
            if overlaps && boundary_overlaps.len() < MAX_AUDIT_SAMPLES {
                boundary_overlaps.push((addr, size));
            }
            for i in 0..u64::from(size) {
                let at = addr.wrapping_add(i);
                if !overlaps && frame_spec.is_excluded(at) {
                    excluded_addrs.insert(at);
                } else {
                    written.insert(at);
                    if frame_spec.is_caller_stack(at)
                        && caller_stack_addrs.len() < MAX_AUDIT_SAMPLES
                    {
                        caller_stack_addrs.insert(at);
                    }
                }
            }
        }

        let mut all_writes = BTreeMap::new();
        let mut writes = BTreeMap::new();
        for at in all_written_addrs {
            if let Ok(bytes) = self.emu.state.read_space(ram, at, 1) {
                let b = bytes[0];
                all_writes.insert(at, b);
                if written.contains(&at) {
                    writes.insert(at, b);
                }
            }
        }

        let undefined_frame_read = frame_state
            .lock()
            .expect("frame state")
            .first_undefined_read;

        let mut escaped_frame_pointers = Vec::new();
        if let StubbedEnd::Returned(ret) = end
            && frame_spec.in_private_frame(ret)
        {
            escaped_frame_pointers.push(EscapedFramePointer::Returned(ret));
        }
        for (call_index, call) in calls.iter().enumerate() {
            for &arg in &call.args {
                if frame_spec.in_private_frame(arg)
                    && escaped_frame_pointers.len() < MAX_AUDIT_SAMPLES
                {
                    escaped_frame_pointers.push(EscapedFramePointer::PassedToCall {
                        call_index,
                        site: call.site,
                        ptr: arg,
                    });
                }
            }
        }
        scan_stored_frame_pointers(
            &writes,
            &frame_spec,
            ptr_size_u8,
            endian,
            &mut escaped_frame_pointers,
        );

        let mut unjustified_reasons = Vec::new();
        let mut passed_to_callee_count = 0usize;
        for esc in &escaped_frame_pointers {
            match esc {
                EscapedFramePointer::Returned(ptr) => unjustified_reasons.push(format!(
                    "function returned pointer {ptr:#x} inside its private frame"
                )),
                EscapedFramePointer::StoredExternally { at, target } => {
                    unjustified_reasons.push(format!(
                        "function stored frame pointer {target:#x} to external address {at:#x}"
                    ));
                }
                EscapedFramePointer::PassedToCall { .. } => passed_to_callee_count += 1,
            }
        }
        if let Some(delta) = sp_delta
            && delta != 0
        {
            unjustified_reasons.push(format!(
                "stack pointer shifted by {delta} bytes across return"
            ));
        }
        if !boundary_overlaps.is_empty() {
            let (at, size) = boundary_overlaps[0];
            unjustified_reasons.push(format!(
                "write at {at:#x} ({size} bytes) straddles the excluded frame boundary"
            ));
        }
        let justification = if !unjustified_reasons.is_empty() {
            FrameExclusionJustification::Unjustified {
                reasons: unjustified_reasons,
            }
        } else if passed_to_callee_count > 0 {
            FrameExclusionJustification::EscapedToCallee {
                count: passed_to_callee_count,
            }
        } else {
            FrameExclusionJustification::JustifiedPrivateFrame
        };

        let aliased_input_addrs: Vec<u64> = frame_spec
            .aliased_input_ranges
            .iter()
            .map(|r| r.start)
            .collect();
        let frame_audit = FrameExclusionAudit {
            spec: frame_spec,
            excluded_write_bytes: excluded_addrs.len(),
            caller_stack_write_addrs: caller_stack_addrs.into_iter().collect(),
            aliased_input_addrs,
            escaped_frame_pointers,
            boundary_overlaps,
            sp_delta,
            clobbered_callee_saved,
            justification,
        };

        let read_pages_guard = read_pages.lock().expect("read page log");
        let memory_audit = collect_memory_access_audit(
            self.lazy_memory_active,
            &self.baseline_state,
            &read_pages_guard,
            &write_pages,
        );

        Ok(StubbedRun {
            end,
            calls,
            writes,
            all_writes,
            undefined_frame_read,
            exit_code: self.emu.exit_code,
            exit_signal: self.emu.exit_signal,
            frame_audit,
            memory_audit,
        })
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
        self.restore_baseline();
        self.setup_call(address, args)
    }

    /// Restore the baseline -- a called function can mutate globals/heap.
    fn restore_baseline(&mut self) {
        self.emu.state = self.baseline_state.clone();
        self.emu.pc = self.baseline_pc;
        // The instruction budget is per call. The count lives on the
        // emulator, not in the restored state, so without this every call
        // spent what the earlier ones left and a batch ran out partway.
        self.emu.inst_count = 0;
        self.emu.exit_code = None;
        self.emu.exit_signal = None;
        self.emu.halt_requested = false;
        self.lazy_memory_active = false;
        self.last_call_args.clear();
    }

    /// Set up a call of `address` with `args` on the current state, stopped
    /// at its first instruction.
    fn setup_call(&mut self, address: u64, args: &[u64]) -> Result<()> {
        self.last_call_args = args.to_vec();
        // Set up the call like a real `call`/`bl`: write the sentinel
        // return address to the link register (ARM/MIPS/PPC) or push it on
        // the stack (x86) -- exactly what `simulate_return()` already
        // expects to read back when the callee's own `ret` executes.
        let sp_reg = self.emu.arch.sp_reg;
        let ptr_size_u8 = self.emu.arch.pointer_size;
        let ptr_size = u64::from(ptr_size_u8);
        let endian = self.emu.arch.endian;
        let sp = self.emu.read_register_u64(sp_reg)?;
        let arg_regs: Vec<&'static str> = self.emu.arch.cc.arg_regs().to_vec();
        let stack_args = (args.len().saturating_sub(arg_regs.len())) as u64;

        self.emu.pc = address;
        if let Some(lr) = self.emu.arch.cc.return_addr_reg() {
            self.emu.write_register_u64(lr, SENTINEL_RET)?;
            let base_offset = self.emu.arch.cc.stack_arg_offset(0);
            let new_sp = link_reg_call_frame_sp(sp, ptr_size, base_offset, stack_args);
            if new_sp != sp {
                self.emu.write_register_u64(sp_reg, new_sp)?;
            }
        } else {
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
            let new_sp = call_frame_sp(sp, ptr_size, shadow_space, stack_args);
            let ram = self.emu.state.ram_space();
            let bytes = encode_word(SENTINEL_RET, ptr_size_u8, endian);
            self.emu.state.write_space(ram, new_sp, &bytes)?;
            self.emu.write_register_u64(sp_reg, new_sp)?;
        }

        // Inject args via the calling convention's arg registers/stack
        // slots -- mirrors `read_arg`'s logic in reverse.
        for (i, &value) in args.iter().enumerate() {
            if i < arg_regs.len() {
                self.emu.write_register_u64(arg_regs[i], value)?;
            } else {
                let n = i - arg_regs.len();
                let stack_off = self.emu.arch.cc.stack_arg_offset(n);
                let sp = self.emu.read_register_u64(sp_reg)?;
                let ram = self.emu.state.ram_space();
                let bytes = encode_word(value, ptr_size_u8, endian);
                self.emu.state.write_space(ram, sp + stack_off, &bytes)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EmulatorHarness, FrameExclusionSpec, call_frame_sp, encode_word, link_reg_call_frame_sp,
    };
    use fission_emulator::arch::Endianness;

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

    #[test]
    fn link_register_stack_args_reserve_aligned_frame_below_baseline_sp() {
        let sp = 0x7fff_ffff_e000u64;
        for (ptr, base) in [(8u64, 0u64), (4, 0), (4, 0x10), (4, 0x08)] {
            for stack_args in 1..=6u64 {
                let new_sp = link_reg_call_frame_sp(sp, ptr, base, stack_args);
                assert_eq!(new_sp % 16, 0);
                for n in 0..stack_args {
                    let slot = new_sp + base + n * ptr;
                    assert!(
                        slot + ptr <= sp,
                        "ptr={ptr} base={base:#x} arg {n}: {slot:#x} reaches {sp:#x}"
                    );
                }
            }
        }
    }

    #[test]
    fn encode_word_respects_endianness_and_pointer_width() {
        assert_eq!(
            encode_word(0x1234_5678, 4, Endianness::Little),
            vec![0x78, 0x56, 0x34, 0x12]
        );
        assert_eq!(
            encode_word(0x1234_5678, 4, Endianness::Big),
            vec![0x12, 0x34, 0x56, 0x78]
        );
        assert_eq!(
            encode_word(0x0102_0304_0506_0708, 8, Endianness::Big),
            vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        );
    }

    #[test]
    fn install_scratch_expands_or_rewrites_when_buffer_grows() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fission-emulator/testdata/win_x64_write.exe"
        );
        let mut harness = EmulatorHarness::build(std::path::Path::new(path), Some(1000))
            .expect("build harness from fixture");
        let small = vec![0xAAu8; 128];
        let addr_small = harness.install_scratch(&small).expect("install 128B");
        assert_eq!(harness.baseline_byte(addr_small + 127), Some(0xAA));

        let large = vec![0x55u8; 512];
        let addr_large = harness.install_scratch(&large).expect("install 512B");
        assert_eq!(harness.baseline_byte(addr_large + 511), Some(0x55));
    }

    #[test]
    fn frame_exclusion_spec_respects_abi_boundaries_and_aliases() {
        let entry_sp = 0x7fff_0000_u64;
        // SysV64: 8B return address at [entry_sp..entry_sp+8), 0 shadow space,
        // arg6 at [entry_sp+8..entry_sp+16).
        let sysv = FrameExclusionSpec::from_abi(entry_sp, 8, false, 8, 6, &[1, 2, 3, 4, 5, 6, 7]);
        assert!(sysv.is_excluded(entry_sp - 4));
        assert!(!sysv.is_excluded(entry_sp));
        assert!(!sysv.is_excluded(entry_sp + 4));
        assert!(!sysv.is_excluded(entry_sp + 8));
        assert!(!sysv.is_excluded(entry_sp + 16));
        assert!(sysv.is_caller_stack(entry_sp + 16));
        assert!(sysv.is_caller_stack(entry_sp));

        // Win64: 8B return address at [entry_sp..entry_sp+8), 32B shadow space at
        // [entry_sp+8..entry_sp+0x28), stack arg at [entry_sp+0x28..entry_sp+0x30).
        let win64 = FrameExclusionSpec::from_abi(entry_sp, 8, false, 0x28, 4, &[1, 2, 3, 4, 5]);
        assert!(win64.is_excluded(entry_sp - 16));
        assert!(!win64.is_excluded(entry_sp));
        assert!(win64.is_excluded(entry_sp + 8));
        assert!(win64.is_excluded(entry_sp + 0x20));
        assert!(!win64.is_excluded(entry_sp + 0x28));
        assert!(!win64.is_caller_stack(entry_sp + 8));
        assert!(win64.is_caller_stack(entry_sp + 0x28));

        // Out-param pointer aliasing into private frame disables exclusion around alias.
        let alias_ptr = entry_sp - 0x200;
        let aliased = FrameExclusionSpec::from_abi(entry_sp, 8, false, 8, 6, &[alias_ptr]);
        assert!(aliased.in_private_frame(alias_ptr));
        assert!(!aliased.is_excluded(alias_ptr));
        assert!(!aliased.is_excluded(alias_ptr + 8));
        assert!(aliased.is_excluded(entry_sp - 4));
    }
}
