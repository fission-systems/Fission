//! Common Types for Debugging Functionality
//!
//! This module defines the core data structures used throughout the debug system:
//!
//! - [`ProcessInfo`] - Basic information about a running process
//! - [`DebugEvent`] - Events received from the debugger (breakpoints, exceptions, etc.)
//! - [`DebugState`] - Current state of the debugging session
//! - [`RegisterState`] - CPU register values (x86-64)
//! - [`Breakpoint`] - Software breakpoint representation
//! - [`ThreadInfo`] - Per-thread tracking for multi-threaded targets
//! - [`ModuleInfo`] - Loaded module/DLL information
//!
//! These types are platform-agnostic and used by all debugger implementations.

pub use fission_ttd::RegisterState;
use serde::{Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};

fn serialize_hex_address<S>(address: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{address:x}"))
}

/// Information about a running process
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: u32,
    /// Process name (executable name)
    pub name: String,
    /// Full path to the executable (if available)
    pub exe_path: Option<String>,
}

/// Information about a thread in the debugged process
#[derive(Debug, Clone)]
pub struct ThreadInfo {
    /// Thread ID
    pub thread_id: u32,
    /// Thread start address (if known)
    pub start_address: u64,
    /// Whether this thread is currently suspended by the debugger
    pub suspended: bool,
    /// Whether this is the main (initial) thread
    pub is_main: bool,
}

/// Information about a loaded module (DLL / EXE) in the target process
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    /// Base address where the module is loaded
    pub base_address: u64,
    /// Size of the module in memory (if known)
    pub size: u64,
    /// Full path to the module file
    pub path: String,
    /// Short name (e.g., "ntdll.dll")
    pub name: String,
}

/// Completeness of a backend's current process-mapping observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleListCompleteness {
    /// All mapping records exposed by the backend were parsed successfully.
    Complete,
    /// Some mappings or address transforms could not be established.
    Partial,
}

/// Permissions and file-offset facts for one runtime memory mapping.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessMemoryMapping {
    /// Inclusive runtime start address.
    #[serde(serialize_with = "serialize_hex_address")]
    pub runtime_start: u64,
    /// Exclusive runtime end address.
    #[serde(serialize_with = "serialize_hex_address")]
    pub runtime_end: u64,
    /// File offset corresponding to `runtime_start`, when the backend reports it.
    #[serde(serialize_with = "serialize_optional_hex_address")]
    pub file_offset: Option<u64>,
    /// Native permission string when available (for Linux, for example `r-xp`).
    pub permissions: Option<String>,
    /// Mapping pathname or special-region name, if available.
    pub path: Option<String>,
}

fn serialize_optional_hex_address<S>(
    address: &Option<u64>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match address {
        Some(address) => serializer.serialize_some(&format!("0x{address:x}")),
        None => serializer.serialize_none(),
    }
}

/// Address conversion evidence for one mapped file.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ModuleAddressTransform {
    /// ELF file offsets consistently map to ELF VAs with this additive bias.
    Resolved {
        format: String,
        analysis_address_domain: String,
        runtime_address_domain: String,
        formula: String,
        load_bias: i64,
    },
    /// The mapped file was readable but does not expose ELF VA coordinates.
    NotApplicable { reason: String },
    /// The mapping is known, but a trustworthy address conversion is unavailable.
    Unavailable { reason: String },
}

/// File-backed module and the mappings that comprise its runtime image.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessModule {
    pub path: String,
    pub name: String,
    /// Linux device/inode identity; absent on backends that do not expose it.
    pub device: Option<String>,
    pub inode: Option<u64>,
    pub mappings: Vec<ProcessMemoryMapping>,
    pub address_transform: ModuleAddressTransform,
}

/// Structured module/mapping query result for an attached process.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleListReport {
    pub schema_version: u16,
    pub pid: u32,
    pub completeness: ModuleListCompleteness,
    pub modules: Vec<ProcessModule>,
    /// Non-file-backed or special mappings such as stacks, heaps, and vDSO.
    pub other_mappings: Vec<ProcessMemoryMapping>,
    /// Reasons this report is partial or lacks some address transforms.
    pub diagnostics: Vec<String>,
}

/// Debug event received from the debugger
#[derive(Debug, Clone)]
pub enum DebugEvent {
    /// Process created/attached
    ProcessCreated { pid: u32, main_thread_id: u32 },
    /// Process exited
    ProcessExited { exit_code: u32 },
    /// Thread created
    ThreadCreated { thread_id: u32 },
    /// Thread exited
    ThreadExited { thread_id: u32 },
    /// DLL loaded
    DllLoaded { base_address: u64, name: String },
    /// DLL unloaded
    DllUnloaded { base_address: u64 },
    /// Breakpoint hit
    BreakpointHit { address: u64, thread_id: u32 },
    /// Single step completed
    SingleStep { thread_id: u32 },
    /// A watched memory range was read or written.
    ///
    /// Distinct from `BreakpointHit`, which says where execution stopped and
    /// nothing about why: the two useful facts about a watchpoint are the
    /// address that was touched and the instruction that touched it, and they
    /// are different addresses.
    WatchpointHit {
        /// The memory that was accessed.
        address: u64,
        size: u32,
        write: bool,
        /// The instruction that made the access.
        pc: u64,
        thread_id: u32,
    },
    /// Exception occurred
    Exception {
        code: u32,
        address: u64,
        first_chance: bool,
    },
    /// Debug string output from the target process
    OutputString { message: String },
}

/// Hardware breakpoint kind (x86 debug register configuration).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HwBreakpointKind {
    /// Break on instruction execution (DR7 type 00)
    Execute,
    /// Break on memory write (DR7 type 01)
    Write,
    /// Break on memory read or write (DR7 type 11)
    ReadWrite,
}

/// Debug session status
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DebugStatus {
    #[default]
    Detached,
    Attaching,
    Running,
    Suspended,
    Terminated,
}

/// How to handle a first-chance exception that is not a user breakpoint
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExceptionPolicy {
    /// Pass to application (DBG_EXCEPTION_NOT_HANDLED)
    PassToApplication,
    /// Swallow and continue (DBG_CONTINUE)
    SwallowContinue,
    /// Stop and notify the user
    Break,
}

impl Default for ExceptionPolicy {
    fn default() -> Self {
        Self::PassToApplication
    }
}

/// Memory breakpoint access kind
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryBpKind {
    /// Break on read
    Read,
    /// Break on write
    Write,
    /// Break on execute
    Execute,
    /// Break on any access (read, write, or execute)
    Access,
}

/// Unified breakpoint kind
#[derive(Debug, Clone, PartialEq)]
pub enum BreakpointKind {
    /// Software breakpoint (INT3)
    Software,
    /// Hardware breakpoint (x86 debug registers)
    Hardware(HwBreakpointKind),
    /// Memory breakpoint (guard page / page protection)
    Memory { size: usize, kind: MemoryBpKind },
    /// DLL load breakpoint
    Dll { name: String },
    /// Exception breakpoint
    Exception { code: u32 },
}

impl Default for BreakpointKind {
    fn default() -> Self {
        BreakpointKind::Software
    }
}

/// Breakpoint info (software, hardware, memory, DLL, or exception)
#[derive(Debug, Clone)]
pub struct Breakpoint {
    /// Breakpoint address (meaning varies by kind)
    pub address: u64,
    /// Original byte at this address (for software breakpoints)
    pub original_byte: u8,
    /// Is this breakpoint enabled?
    pub enabled: bool,
    /// Is this a temporary breakpoint (e.g., step-over helper)?
    pub temporary: bool,
    /// Breakpoint kind
    pub kind: BreakpointKind,
    /// Hit count
    pub hits: u64,
    /// Optional condition expression
    pub condition: Option<String>,
}

impl Default for Breakpoint {
    fn default() -> Self {
        Self {
            address: 0,
            original_byte: 0,
            enabled: true,
            temporary: false,
            kind: BreakpointKind::Software,
            hits: 0,
            condition: None,
        }
    }
}

/// Information about a module export
#[derive(Debug, Clone)]
pub struct ExportInfo {
    /// Export name (if available)
    pub name: String,
    /// Export address
    pub address: u64,
    /// Export ordinal
    pub ordinal: Option<u32>,
}

/// Information about a module import
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Importing module name
    pub module: String,
    /// Import name (if by name)
    pub name: Option<String>,
    /// Import ordinal (if by ordinal)
    pub ordinal: Option<u16>,
    /// IAT slot address
    pub address: u64,
}

/// Debug state for GUI and session tracking
#[derive(Debug, Clone, Default)]
pub struct DebugState {
    /// Attached process ID
    pub attached_pid: Option<u32>,
    /// Main thread ID
    pub main_thread_id: Option<u32>,
    /// The thread that raised the most recent debug event, reported or not.
    /// A Win32 debugger must continue the event on this thread.
    pub last_thread_id: Option<u32>,
    /// The selected thread: the one register reads and writes, steps and
    /// stack operations act on. See [`DebugState::selected_thread_id`] for
    /// how it moves.
    pub current_thread_id: Option<u32>,
    /// Current debug status
    pub status: DebugStatus,
    /// Active breakpoints
    pub breakpoints: HashMap<u64, Breakpoint>,
    /// Active threads (ordered by thread ID)
    pub threads: BTreeMap<u32, ThreadInfo>,
    /// Loaded modules (keyed by base address)
    pub modules: BTreeMap<u64, ModuleInfo>,
    /// Current register state (for the selected thread)
    pub registers: Option<RegisterState>,
    /// Last event description
    pub last_event: Option<String>,
    /// Whether the initial system breakpoint has been consumed
    pub system_breakpoint_consumed: bool,
    /// Total debug events processed
    pub event_count: u64,
}

/// Which thread a stopped native target is "on".
///
/// The contract every native backend follows, and the one the CLI session
/// reports as `state.thread_id`:
///
/// 1. A *reported* debug event selects the thread that raised it. After a
///    breakpoint on a worker thread, `regs` reads that worker, `step` steps
///    it, and `state.thread_id`, `state.pc` and `state.stop` describe the
///    same thread and the same stop. An explicit selection made before the
///    event does not survive it: the newest stop is authoritative, as in gdb
///    and WinDbg.
/// 2. An explicit selection (`switch-thread`) holds until the next reported
///    event. Polling a frozen target reports nothing and changes nothing.
/// 3. An event the backend absorbs without reporting it (for instance the
///    trap that ends a step off a breakpoint) moves only
///    [`DebugState::last_thread_id`], never the selection.
/// 4. A thread that exits is never left selected: the selection falls back
///    to the main thread while it lives, else the lowest-numbered live thread.
/// 5. Process exit leaves the selection as it was; there is no live thread to
///    move it to.
impl DebugState {
    /// The selected thread, falling back to the last event thread and then
    /// the main thread while nothing is selected.
    pub fn selected_thread_id(&self) -> Option<u32> {
        self.current_thread_id
            .or(self.last_thread_id)
            .or(self.main_thread_id)
    }

    /// Apply rules 1, 3, 4 and 5 for a debug event raised by `thread_id`.
    /// `reported` is the event handed to the caller, `None` when the backend
    /// absorbed it.
    pub fn observe_event_thread(&mut self, thread_id: u32, reported: Option<&DebugEvent>) {
        self.last_thread_id = Some(thread_id);
        match reported {
            None => {}
            Some(DebugEvent::ProcessCreated { .. }) => {
                self.main_thread_id = Some(thread_id);
                self.current_thread_id = Some(thread_id);
            }
            Some(DebugEvent::ThreadExited { .. }) => self.forget_thread(thread_id),
            Some(DebugEvent::ProcessExited { .. }) => {}
            Some(_) => self.current_thread_id = Some(thread_id),
        }
    }

    /// Rule 2: select a live thread explicitly.
    pub fn select_thread(&mut self, thread_id: u32) -> Result<(), String> {
        if self.threads.contains_key(&thread_id) {
            self.current_thread_id = Some(thread_id);
            Ok(())
        } else {
            Err(format!("Thread {thread_id} not found in tracked threads"))
        }
    }

    /// Rule 4: drop an exited thread, and move the selection off it.
    pub fn forget_thread(&mut self, thread_id: u32) {
        self.threads.remove(&thread_id);
        if self.current_thread_id == Some(thread_id) {
            self.current_thread_id = self
                .main_thread_id
                .filter(|main| self.threads.contains_key(main))
                .or_else(|| self.threads.keys().next().copied());
        }
    }
}

/// A short, stable label for why a native target stopped on `event`, in the
/// vocabulary the emulator backend uses (`breakpoint:0x..`, `stepped`,
/// `exited`).
pub fn stop_reason_for_event(event: &DebugEvent) -> String {
    match event {
        DebugEvent::ProcessCreated { .. } => "process-created".into(),
        DebugEvent::ProcessExited { .. } => "exited".into(),
        DebugEvent::ThreadCreated { .. } => "thread-created".into(),
        DebugEvent::ThreadExited { .. } => "thread-exited".into(),
        DebugEvent::DllLoaded { .. } => "dll-loaded".into(),
        DebugEvent::DllUnloaded { .. } => "dll-unloaded".into(),
        DebugEvent::BreakpointHit { address, .. } => format!("breakpoint:0x{address:x}"),
        DebugEvent::WatchpointHit { address, .. } => format!("watchpoint:0x{address:x}"),
        DebugEvent::SingleStep { .. } => "stepped".into(),
        DebugEvent::Exception { code, .. } => format!("exception:0x{code:08x}"),
        DebugEvent::OutputString { .. } => "output".into(),
    }
}

#[cfg(test)]
mod thread_selection_tests {
    use super::*;

    const MAIN: u32 = 100;
    const WORKER: u32 = 200;
    const LOADER: u32 = 300;

    fn thread(thread_id: u32, is_main: bool) -> ThreadInfo {
        ThreadInfo {
            thread_id,
            start_address: 0,
            suspended: false,
            is_main,
        }
    }

    /// A process whose main thread and one worker are alive, stopped at the
    /// loader breakpoint on the main thread.
    fn two_threads_at_loader_breakpoint() -> DebugState {
        let mut state = DebugState::default();
        state.threads.insert(MAIN, thread(MAIN, true));
        state.observe_event_thread(
            MAIN,
            Some(&DebugEvent::ProcessCreated {
                pid: 1,
                main_thread_id: MAIN,
            }),
        );
        state.threads.insert(WORKER, thread(WORKER, false));
        state.observe_event_thread(
            WORKER,
            Some(&DebugEvent::ThreadCreated { thread_id: WORKER }),
        );
        state.observe_event_thread(
            MAIN,
            Some(&DebugEvent::BreakpointHit {
                address: 0x1000,
                thread_id: MAIN,
            }),
        );
        state
    }

    #[test]
    fn the_loader_breakpoint_selects_the_main_thread() {
        let state = two_threads_at_loader_breakpoint();
        assert_eq!(state.selected_thread_id(), Some(MAIN));
        assert_eq!(state.last_thread_id, Some(MAIN));
    }

    #[test]
    fn a_breakpoint_on_a_worker_selects_the_worker() {
        let mut state = two_threads_at_loader_breakpoint();
        state.observe_event_thread(
            WORKER,
            Some(&DebugEvent::BreakpointHit {
                address: 0x2000,
                thread_id: WORKER,
            }),
        );
        assert_eq!(
            state.selected_thread_id(),
            Some(WORKER),
            "registers must be read from the thread the stop names"
        );
    }

    #[test]
    fn a_new_stop_overrides_an_explicit_selection() {
        let mut state = two_threads_at_loader_breakpoint();
        state.select_thread(WORKER).expect("worker is live");
        assert_eq!(state.selected_thread_id(), Some(WORKER));
        // Selected the worker, then the main thread stops again.
        state.observe_event_thread(MAIN, Some(&DebugEvent::SingleStep { thread_id: MAIN }));
        assert_eq!(state.selected_thread_id(), Some(MAIN));
        // And the other way round: main selected, the worker stops.
        state.select_thread(MAIN).expect("main is live");
        state.observe_event_thread(
            WORKER,
            Some(&DebugEvent::Exception {
                code: 0xC000_0005,
                address: 0x2000,
                first_chance: true,
            }),
        );
        assert_eq!(state.selected_thread_id(), Some(WORKER));
    }

    #[test]
    fn an_explicit_selection_holds_until_the_next_reported_event() {
        let mut state = two_threads_at_loader_breakpoint();
        state.select_thread(WORKER).expect("worker is live");
        // An absorbed event (a step-off trap on the main thread) moves only
        // the continue thread.
        state.observe_event_thread(MAIN, None);
        assert_eq!(state.last_thread_id, Some(MAIN));
        assert_eq!(state.selected_thread_id(), Some(WORKER));
    }

    #[test]
    fn selecting_an_unknown_thread_is_refused_and_changes_nothing() {
        let mut state = two_threads_at_loader_breakpoint();
        assert!(state.select_thread(999).is_err());
        assert_eq!(state.selected_thread_id(), Some(MAIN));
    }

    #[test]
    fn every_reported_event_selects_its_thread() {
        let events = [
            DebugEvent::ThreadCreated { thread_id: WORKER },
            DebugEvent::DllLoaded {
                base_address: 0x7ff0_0000,
                name: "x.dll".into(),
            },
            DebugEvent::DllUnloaded {
                base_address: 0x7ff0_0000,
            },
            DebugEvent::OutputString {
                message: "hi".into(),
            },
            DebugEvent::WatchpointHit {
                address: 0x3000,
                size: 4,
                write: true,
                pc: 0x2000,
                thread_id: WORKER,
            },
        ];
        for event in events {
            let mut state = two_threads_at_loader_breakpoint();
            state.select_thread(MAIN).expect("main is live");
            state.observe_event_thread(WORKER, Some(&event));
            assert_eq!(state.selected_thread_id(), Some(WORKER), "{event:?}");
        }
    }

    #[test]
    fn an_exiting_selected_thread_hands_the_selection_to_main() {
        let mut state = two_threads_at_loader_breakpoint();
        state.select_thread(WORKER).expect("worker is live");
        state.observe_event_thread(
            WORKER,
            Some(&DebugEvent::ThreadExited { thread_id: WORKER }),
        );
        assert!(!state.threads.contains_key(&WORKER));
        assert_eq!(state.selected_thread_id(), Some(MAIN));
        // Continuing the exit event still needs the exiting thread.
        assert_eq!(state.last_thread_id, Some(WORKER));
    }

    #[test]
    fn an_exiting_unselected_thread_leaves_the_selection_alone() {
        let mut state = two_threads_at_loader_breakpoint();
        state.threads.insert(LOADER, thread(LOADER, false));
        state.select_thread(WORKER).expect("worker is live");
        state.observe_event_thread(
            LOADER,
            Some(&DebugEvent::ThreadExited { thread_id: LOADER }),
        );
        assert_eq!(state.selected_thread_id(), Some(WORKER));
    }

    #[test]
    fn when_main_is_gone_the_lowest_live_thread_is_selected() {
        let mut state = two_threads_at_loader_breakpoint();
        state.threads.insert(LOADER, thread(LOADER, false));
        state.observe_event_thread(MAIN, Some(&DebugEvent::ThreadExited { thread_id: MAIN }));
        assert_eq!(state.selected_thread_id(), Some(WORKER));
        state.observe_event_thread(
            WORKER,
            Some(&DebugEvent::ThreadExited { thread_id: WORKER }),
        );
        assert_eq!(state.selected_thread_id(), Some(LOADER));
    }

    #[test]
    fn process_exit_keeps_the_last_selection() {
        let mut state = two_threads_at_loader_breakpoint();
        state.select_thread(WORKER).expect("worker is live");
        state.observe_event_thread(MAIN, Some(&DebugEvent::ProcessExited { exit_code: 0 }));
        assert_eq!(state.selected_thread_id(), Some(WORKER));
    }

    #[test]
    fn stop_reasons_name_the_stop() {
        assert_eq!(
            stop_reason_for_event(&DebugEvent::BreakpointHit {
                address: 0x401000,
                thread_id: 1
            }),
            "breakpoint:0x401000"
        );
        assert_eq!(
            stop_reason_for_event(&DebugEvent::SingleStep { thread_id: 1 }),
            "stepped"
        );
        assert_eq!(
            stop_reason_for_event(&DebugEvent::ProcessExited { exit_code: 3 }),
            "exited"
        );
        assert_eq!(
            stop_reason_for_event(&DebugEvent::Exception {
                code: 0xC000_0005,
                address: 0,
                first_chance: false
            }),
            "exception:0xc0000005"
        );
    }
}

/// Schema version for epoch-bound live memory acquisition contracts.
pub const LIVE_ACQUISITION_SCHEMA_VERSION: u16 = 1;

/// Stable target process identity binding a capture against PID reuse or exit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessCaptureIdentity {
    /// Host identifier (when known to the backend/session).
    pub host_id: Option<String>,
    /// Analysis/debugger session identifier.
    pub session_id: String,
    /// Numeric OS/guest process identifier (never sufficient alone when `lifetime_token` is absent).
    pub pid: u32,
    /// Backend-supplied process start time or lifetime token (e.g. Linux `/proc/<pid>/stat` starttime).
    pub lifetime_token: Option<String>,
    /// Target architecture label when known (e.g. `"x86_64"`, `"aarch64"`).
    pub architecture: Option<String>,
}

/// Execution-consistency classification under which memory was sampled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureConsistency {
    /// All observed target threads were suspended; shared mappings may still mutate externally.
    StoppedAllObservedThreads,
    /// Atomic OS/hypervisor point-in-time memory snapshot.
    OsSnapshot,
    /// Live target sampled without suspending all observed threads.
    LiveBestEffort,
    /// Backend cannot establish whether all threads were stopped during sampling.
    Unknown,
}

/// Completeness of thread or module-mapping enumeration for an acquisition epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnumerationCompleteness {
    Complete,
    Partial,
    Unavailable,
}

/// Explicit bounded epoch metadata recorded around a live memory capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcquisitionEpoch {
    pub epoch_id: u64,
    pub started_unix_ms: u64,
    pub ended_unix_ms: u64,
    pub consistency: CaptureConsistency,
    /// Explicitly true unless taken from an atomic [`CaptureConsistency::OsSnapshot`].
    pub shared_mappings_mutable: bool,
    pub thread_enumeration: EnumerationCompleteness,
    pub mapping_enumeration: EnumerationCompleteness,
    pub observed_thread_ids_before: Vec<u32>,
    pub observed_thread_ids_after: Vec<u32>,
    pub threads_changed: bool,
    pub mappings_changed: bool,
    pub identity_stable: bool,
}

/// Distinguishes raw debugger-observed bytes from logical unpatched reconstructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryViewKind {
    RawObserved,
    LogicalUnpatched,
}

/// Debugger or acquisition-induced observer intervention intersecting a range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DebuggerInterventionKind {
    SoftwareBreakpointPatched {
        #[serde(serialize_with = "serialize_hex_address")]
        address: u64,
        original_byte: u8,
        enabled: bool,
    },
    MemoryWrite {
        #[serde(serialize_with = "serialize_hex_address")]
        address: u64,
        size: usize,
    },
    ThreadSuspended {
        thread_id: u32,
    },
    ThreadResumed {
        thread_id: u32,
    },
}

/// One recorded observer intervention overlapping an acquired memory interval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebuggerIntervention {
    pub kind: DebuggerInterventionKind,
    pub detail: String,
}

/// Overall coherence verdict for an epoch-bound live memory acquisition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureCoherenceStatus {
    Coherent,
    Partial,
    InconsistentRefused,
}

/// Policy and bounds governing [`super::traits::ExecutionBackend::acquire_epoch_bound_memory`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAcquisitionOptions {
    pub session_id: String,
    pub epoch_id: u64,
    pub sparse: super::memory::SparseReadOptions,
    /// Fail closed when the backend does not report [`super::capabilities::DebugOperation::MemoryRead`].
    pub require_memory_read_capability: bool,
    /// Authorize best-effort capture on a running process; false never forces suspension.
    pub allow_live_unstopped: bool,
    /// Refuse capture (dropping raw observations) if module mappings churn mid-acquisition.
    pub refuse_on_mapping_churn: bool,
}

impl Default for LiveAcquisitionOptions {
    fn default() -> Self {
        Self {
            session_id: "default-session".to_string(),
            epoch_id: 0,
            sparse: super::memory::SparseReadOptions::default(),
            require_memory_read_capability: true,
            allow_live_unstopped: false,
            refuse_on_mapping_churn: false,
        }
    }
}

/// Opt-in derived logical view with software-breakpoint patches restored from original bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DerivedLogicalMemoryView {
    #[serde(serialize_with = "serialize_hex_address")]
    pub base_address: u64,
    pub view_kind: MemoryViewKind,
    pub bytes: Vec<(u64, Vec<u8>)>,
    pub applied_interventions: Vec<DebuggerIntervention>,
}

/// Sparse live memory capture explicitly bound to process identity, epoch, and observer interventions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EpochBoundMemoryCapture {
    pub schema_version: u16,
    pub identity: ProcessCaptureIdentity,
    pub epoch: AcquisitionEpoch,
    #[serde(serialize_with = "serialize_hex_address")]
    pub requested_start: u64,
    pub requested_size: usize,
    pub view_kind: MemoryViewKind,
    pub interventions: Vec<DebuggerIntervention>,
    pub coherence: CaptureCoherenceStatus,
    pub diagnostics: Vec<String>,
    pub capture: Option<super::memory::SparseMemoryCapture>,
}

impl EpochBoundMemoryCapture {
    /// Enforce strict identity and epoch binding before merging two memory captures.
    pub fn can_merge_with(&self, other: &Self) -> Result<(), String> {
        if self.coherence == CaptureCoherenceStatus::InconsistentRefused
            || other.coherence == CaptureCoherenceStatus::InconsistentRefused
        {
            return Err("cannot merge an inconsistent/refused acquisition".to_string());
        }
        if !self.epoch.identity_stable || !other.epoch.identity_stable {
            return Err(
                "cannot merge acquisitions whose process identity did not remain stable"
                    .to_string(),
            );
        }
        if self.identity != other.identity {
            return Err(
                "cannot merge acquisitions from different process identities or lifetime tokens"
                    .to_string(),
            );
        }
        if self.epoch.epoch_id != other.epoch.epoch_id {
            return Err(format!(
                "cannot merge captures across different acquisition epochs ({} vs {})",
                self.epoch.epoch_id, other.epoch.epoch_id
            ));
        }
        if self.view_kind != other.view_kind {
            return Err("cannot merge raw observed bytes with a derived logical view".to_string());
        }
        Ok(())
    }

    /// Derive a separate [`MemoryViewKind::LogicalUnpatched`] copy without altering raw observed bytes.
    pub fn derive_logical_unpatched_view(&self) -> Result<DerivedLogicalMemoryView, String> {
        let Some(ref raw_capture) = self.capture else {
            return Err("no raw capture payload available for logical view derivation".to_string());
        };

        let mut runs: Vec<(u64, Vec<u8>)> = raw_capture
            .observed_runs()
            .map(|run| (run.start, run.bytes.clone()))
            .collect();
        let mut applied = Vec::new();

        for iv in &self.interventions {
            if let DebuggerInterventionKind::SoftwareBreakpointPatched {
                address,
                original_byte,
                enabled: true,
            } = iv.kind
            {
                for (run_start, bytes) in &mut runs {
                    let run_end = run_start.saturating_add(bytes.len() as u64);
                    if address >= *run_start && address < run_end {
                        let offset = (address - *run_start) as usize;
                        bytes[offset] = original_byte;
                        applied.push(iv.clone());
                    }
                }
            }
        }

        Ok(DerivedLogicalMemoryView {
            base_address: self.requested_start,
            view_kind: MemoryViewKind::LogicalUnpatched,
            bytes: runs,
            applied_interventions: applied,
        })
    }
}
