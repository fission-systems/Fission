//! Debug Traits - Platform-Agnostic ExecutionBackend Interface
//!
//! This module defines the [`ExecutionBackend`] trait that all platform-specific
//! debugger implementations must satisfy. It provides a unified API for:
//!
//! - Process attachment and detachment
//! - Execution control (continue, single-step)
//! - Breakpoint management (software breakpoints)
//! - Memory read/write operations
//! - Register inspection
//!
//! # Platform Implementations
//!
//! - Windows: Uses Win32 Debug API (`WaitForDebugEvent`, `ContinueDebugEvent`)
//! - Linux: Uses `ptrace` system call
//! - macOS: Stub implementation (Mach API not yet implemented)
//!
//! # Example
//!
//! ```ignore
//! use crate::debug::{ExecutionBackend, PlatformExecutionBackend};
//!
//! let mut dbg = PlatformExecutionBackend::default();
//! dbg.attach(1234)?;
//! dbg.set_sw_breakpoint(0x401000)?;
//! dbg.continue_execution()?;
//! ```

use super::capabilities::DebugBackendCapabilities;
use super::timeline::Timeline;
use super::types::{ProcessInfo, RegisterState};
use fission_core::Result as FissionResult;
use std::sync::{Arc, Mutex};

/// Platform-agnostic debugger trait
///
/// This trait defines the common interface for all platform-specific debugger implementations.
/// Each platform (Windows, Linux, macOS) provides its own implementation.
pub trait ExecutionBackend: Send {
    /// Describe this backend's build availability and implemented operations.
    ///
    /// The default fails closed for external backends that have not adopted
    /// the capability contract; callers must not infer support from missing
    /// entries. Built-in backends override this with their implementation
    /// facts. A supported operation can still fail because of target state,
    /// permissions, or runtime semantics.
    fn capabilities(&self) -> DebugBackendCapabilities {
        DebugBackendCapabilities::unreported()
    }

    /// Attach the session-owned timeline used for stop snapshots.
    ///
    /// Backends that do not execute a target can keep the default no-op.  A
    /// live debugger or emulator that supports recording stores the shared
    /// handle and records at its natural stop boundary; the session remains
    /// the owner of the public timeline value.
    fn set_timeline(&mut self, _timeline: Arc<Mutex<Timeline>>) {}

    /// Set an execution deadline for bounded execution loops.
    fn set_deadline(&mut self, _deadline: Option<std::time::Instant>) {}

    /// Enumerate running processes on the system
    fn enumerate_processes() -> Vec<ProcessInfo>
    where
        Self: Sized;

    /// Attach to a process by PID
    fn attach(&mut self, pid: u32) -> FissionResult<()>;

    /// Detach from the current process
    fn detach(&mut self) -> FissionResult<()>;

    /// Check if currently attached to a process
    fn is_attached(&self) -> bool;

    /// Get the attached process ID (if any)
    fn attached_pid(&self) -> Option<u32>;

    /// Continue execution after a debug event
    fn continue_execution(&mut self) -> FissionResult<()>;

    /// Single step one instruction
    fn single_step(&mut self) -> FissionResult<()>;

    /// Why execution last stopped, in one word, or `None` if this backend
    /// does not know.
    ///
    /// A front end driving a session has to tell "stopped at your breakpoint"
    /// from "ran out of program", and `get_state`'s three-way status cannot:
    /// both are `Suspended`.
    fn stop_reason(&self) -> Option<String> {
        None
    }

    /// How many instructions the target has executed since launch, when the
    /// backend counts them. A position for [`Self::seek_step`].
    fn execution_step(&self) -> Option<u64> {
        None
    }

    /// Move the target to execution step `step`, backwards or forwards, with
    /// registers and memory as they were there. Only a backend that records
    /// its own execution can do this; the rest refuse rather than pretend.
    fn seek_step(&mut self, step: u64) -> FissionResult<()> {
        let _ = step;
        Err(fission_core::err!(
            debug,
            "seeking to an execution step needs a recording emulator session \
             (`debug --emulator session --record`)"
        ))
    }

    /// Poll for the next debug event
    fn poll_event(&mut self, timeout_ms: u32) -> FissionResult<Option<super::types::DebugEvent>> {
        let _ = timeout_ms;
        Err(fission_core::err!(
            debug,
            "poll_event is not supported on this platform"
        ))
    }

    /// Set a software breakpoint at the given address
    fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()>;

    /// Remove a software breakpoint at the given address
    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()>;

    /// Read memory from the target process as an exact, unpadded slice.
    ///
    /// Fails if any requested byte cannot be read; never pads unread tail bytes
    /// with zeros.
    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>>;

    /// Read memory from the target process up to `size` bytes.
    ///
    /// If an unmapped or unreadable boundary is encountered before `size` bytes,
    /// returns the contiguous prefix bytes available up to that boundary without
    /// zero padding. Returns an error only if zero bytes could be read.
    fn read_memory_partial(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        if size == 0 {
            return Ok(Vec::new());
        }
        match self.read_memory(address, size) {
            Ok(bytes) => {
                if bytes.len() != size {
                    return Err(fission_core::err!(
                        debug,
                        "short memory read at 0x{:x}: expected {}, got {}",
                        address,
                        size,
                        bytes.len()
                    ));
                }
                Ok(bytes)
            }
            Err(e) => {
                let page_size = 4096usize;
                let page_offset = (address as usize) % page_size;
                let in_first_page = page_size - page_offset;
                if size > in_first_page {
                    self.read_memory(address, in_first_page).map_err(|_| e)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Perform a bounded sparse acquisition over `[address, address + size)`.
    ///
    /// Preserves genuinely observed `0x00` bytes inside
    /// [`crate::debug::memory::ObservedMemoryRun`] while recording unreadable
    /// intervals as explicit [`crate::debug::memory::MemoryGap`] segments rather
    /// than padding unread bytes with zeros.
    fn read_memory_sparse(
        &self,
        address: u64,
        size: usize,
        options: super::memory::SparseReadOptions,
    ) -> FissionResult<super::memory::SparseMemoryCapture> {
        use super::memory::{
            MemoryGap, MemoryGapReason, MemorySegment, ObservedMemoryRun, SparseMemoryCapture,
        };

        let size_u64 = u64::try_from(size)
            .map_err(|_| fission_core::err!(debug, "sparse read size out of range"))?;
        let end = address.checked_add(size_u64).ok_or_else(|| {
            fission_core::err!(
                debug,
                "sparse read range overflows address space at 0x{address:x}"
            )
        })?;
        if options.page_size == 0 {
            return Err(fission_core::err!(
                debug,
                "sparse read page_size must be non-zero"
            ));
        }
        if size == 0 {
            return SparseMemoryCapture::from_segments(address, 0, Vec::new())
                .map_err(|e| fission_core::err!(debug, "{e}"));
        }

        let page_size_u64 = options.page_size as u64;
        let mut cursor = address;
        let mut observed_total = 0usize;
        let mut probed_span = 0usize;
        let mut segments = Vec::new();

        while cursor < end {
            if observed_total >= options.max_bytes || probed_span >= options.max_span_bytes {
                segments.push(MemorySegment::Gap(MemoryGap {
                    start: cursor,
                    len: end - cursor,
                    reason: MemoryGapReason::Budget,
                }));
                break;
            }
            let offset_in_page = (cursor % page_size_u64) as usize;
            let to_page_end = options.page_size - offset_in_page;
            let remaining_req = usize::try_from(end - cursor).unwrap_or(usize::MAX);
            let remaining_obs_budget = options.max_bytes.saturating_sub(observed_total);
            let remaining_span_budget = options.max_span_bytes.saturating_sub(probed_span);
            let chunk_len = to_page_end
                .min(remaining_req)
                .min(remaining_obs_budget)
                .min(remaining_span_budget);
            if chunk_len == 0 {
                segments.push(MemorySegment::Gap(MemoryGap {
                    start: cursor,
                    len: end - cursor,
                    reason: MemoryGapReason::Budget,
                }));
                break;
            }

            match self.read_memory(cursor, chunk_len) {
                Ok(bytes) => {
                    if bytes.len() > chunk_len {
                        return Err(fission_core::err!(
                            debug,
                            "backend returned oversized transfer at 0x{cursor:x}: requested {chunk_len}, got {}",
                            bytes.len()
                        ));
                    }
                    let got = bytes.len();
                    observed_total = observed_total.saturating_add(got);
                    if got > 0 {
                        segments.push(MemorySegment::Observed(ObservedMemoryRun {
                            start: cursor,
                            bytes,
                        }));
                    }
                    if got < chunk_len {
                        segments.push(MemorySegment::Gap(MemoryGap {
                            start: cursor.saturating_add(got as u64),
                            len: (chunk_len - got) as u64,
                            reason: MemoryGapReason::Partial,
                        }));
                    }
                }
                Err(err) => {
                    if let Ok(prefix) = self.read_memory_partial(cursor, chunk_len) {
                        if !prefix.is_empty() && prefix.len() <= chunk_len {
                            let got = prefix.len();
                            observed_total = observed_total.saturating_add(got);
                            segments.push(MemorySegment::Observed(ObservedMemoryRun {
                                start: cursor,
                                bytes: prefix,
                            }));
                            if got < chunk_len {
                                segments.push(MemorySegment::Gap(MemoryGap {
                                    start: cursor.saturating_add(got as u64),
                                    len: (chunk_len - got) as u64,
                                    reason: MemoryGapReason::Partial,
                                }));
                            }
                            cursor = cursor.saturating_add(chunk_len as u64);
                            probed_span = probed_span.saturating_add(chunk_len);
                            continue;
                        }
                    }

                    let msg = err.to_string();
                    let lower = msg.to_ascii_lowercase();
                    if lower.contains("exited") || lower.contains("not attached") {
                        segments.push(MemorySegment::Gap(MemoryGap {
                            start: cursor,
                            len: end - cursor,
                            reason: MemoryGapReason::ProcessExited,
                        }));
                        break;
                    }
                    let reason = if lower.contains("denied") || lower.contains("permission") {
                        MemoryGapReason::Denied
                    } else if lower.contains("unmapped")
                        || lower.contains("cannot access memory")
                        || lower.contains("no memory is mapped")
                    {
                        MemoryGapReason::Unmapped
                    } else {
                        MemoryGapReason::ReadError { detail: msg }
                    };
                    segments.push(MemorySegment::Gap(MemoryGap {
                        start: cursor,
                        len: chunk_len as u64,
                        reason,
                    }));
                }
            }
            cursor = cursor.saturating_add(chunk_len as u64);
            probed_span = probed_span.saturating_add(chunk_len);
        }

        // A producer bug must surface as an error, never as a capture whose
        // coverage metadata overstates what was observed.
        SparseMemoryCapture::from_segments(address, size_u64, segments)
            .map_err(|e| fission_core::err!(debug, "sparse read at 0x{address:x}: {e}"))
    }

    /// Write memory to the target process
    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()>;

    /// Fetch CPU registers for a thread
    fn fetch_registers(&mut self, thread_id: u32) -> FissionResult<RegisterState>;

    /// Write CPU registers for a thread
    fn set_registers(&mut self, thread_id: u32, regs: &RegisterState) -> FissionResult<()> {
        let _ = (thread_id, regs);
        Err(fission_core::err!(
            debug,
            "set_registers is not supported on this platform"
        ))
    }

    /// Launch a new process under the debugger
    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        let _ = (path, args);
        Err(fission_core::err!(
            debug,
            "launch is not supported on this platform"
        ))
    }

    /// Single step over a CALL (step over)
    fn step_over(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "step_over is not supported on this platform"
        ))
    }

    /// Step out of the current function (run to return)
    fn step_out(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "step_out is not supported on this platform"
        ))
    }

    /// Pause the target process
    fn pause(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "pause is not supported on this platform"
        ))
    }

    /// Terminate the target process
    fn terminate(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "terminate is not supported on this platform"
        ))
    }

    /// Switch the active thread for subsequent operations
    fn set_current_thread(&mut self, thread_id: u32) -> FissionResult<()> {
        let _ = thread_id;
        Err(fission_core::err!(
            debug,
            "set_current_thread is not supported on this platform"
        ))
    }

    /// Suspend a thread by ID
    fn suspend_thread(&mut self, thread_id: u32) -> FissionResult<u32> {
        let _ = thread_id;
        Err(fission_core::err!(
            debug,
            "suspend_thread is not supported on this platform"
        ))
    }

    /// Resume a thread by ID
    fn resume_thread(&mut self, thread_id: u32) -> FissionResult<u32> {
        let _ = thread_id;
        Err(fission_core::err!(
            debug,
            "resume_thread is not supported on this platform"
        ))
    }

    /// Skip the current instruction by advancing the instruction pointer.
    fn skip_instruction(&mut self) -> FissionResult<()> {
        Err(fission_core::err!(
            debug,
            "skip_instruction is not supported on this platform"
        ))
    }

    /// Set a memory breakpoint on a range
    fn set_memory_breakpoint(
        &mut self,
        address: u64,
        size: usize,
        kind: super::types::MemoryBpKind,
    ) -> FissionResult<()> {
        let _ = (address, size, kind);
        Err(fission_core::err!(
            debug,
            "set_memory_breakpoint is not supported on this platform"
        ))
    }

    /// Remove a memory breakpoint
    fn remove_memory_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "remove_memory_breakpoint is not supported on this platform"
        ))
    }

    /// Set a DLL load breakpoint
    fn set_dll_breakpoint(&mut self, dll_name: &str) -> FissionResult<()> {
        let _ = dll_name;
        Err(fission_core::err!(
            debug,
            "set_dll_breakpoint is not supported on this platform"
        ))
    }

    /// Remove a DLL load breakpoint
    fn remove_dll_breakpoint(&mut self, dll_name: &str) -> FissionResult<()> {
        let _ = dll_name;
        Err(fission_core::err!(
            debug,
            "remove_dll_breakpoint is not supported on this platform"
        ))
    }

    /// Set an exception breakpoint
    fn set_exception_breakpoint(&mut self, code: u32) -> FissionResult<()> {
        let _ = code;
        Err(fission_core::err!(
            debug,
            "set_exception_breakpoint is not supported on this platform"
        ))
    }

    /// Remove an exception breakpoint
    fn remove_exception_breakpoint(&mut self, code: u32) -> FissionResult<()> {
        let _ = code;
        Err(fission_core::err!(
            debug,
            "remove_exception_breakpoint is not supported on this platform"
        ))
    }

    /// Enable a breakpoint by address
    fn enable_breakpoint(&mut self, address: u64) -> FissionResult<bool> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "enable_breakpoint is not supported on this platform"
        ))
    }

    /// Disable a breakpoint by address
    fn disable_breakpoint(&mut self, address: u64) -> FissionResult<bool> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "disable_breakpoint is not supported on this platform"
        ))
    }

    /// List all active breakpoints
    fn list_breakpoints(&self) -> Vec<super::types::Breakpoint> {
        Vec::new()
    }

    /// Allocate memory in the target process
    fn remote_alloc(&mut self, address: u64, size: usize) -> FissionResult<u64> {
        let _ = (address, size);
        Err(fission_core::err!(
            debug,
            "remote_alloc is not supported on this platform"
        ))
    }

    /// Free memory in the target process
    fn remote_free(&mut self, address: u64) -> FissionResult<()> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "remote_free is not supported on this platform"
        ))
    }

    /// Get page protection rights at an address
    fn get_page_rights(&self, address: u64) -> FissionResult<u32> {
        let _ = address;
        Err(fission_core::err!(
            debug,
            "get_page_rights is not supported on this platform"
        ))
    }

    /// Set page protection rights for a region
    fn set_page_rights(&mut self, address: u64, size: usize, protect: u32) -> FissionResult<()> {
        let _ = (address, size, protect);
        Err(fission_core::err!(
            debug,
            "set_page_rights is not supported on this platform"
        ))
    }

    /// Peek a value from the stack at an offset from RSP
    fn stack_peek(&self, offset: isize) -> FissionResult<u64> {
        let _ = offset;
        Err(fission_core::err!(
            debug,
            "stack_peek is not supported on this platform"
        ))
    }

    /// Pop a value from the stack (adjusts RSP and returns the value)
    fn stack_pop(&mut self) -> FissionResult<u64> {
        Err(fission_core::err!(
            debug,
            "stack_pop is not supported on this platform"
        ))
    }

    /// Push a value onto the stack (adjusts RSP)
    fn stack_push(&mut self, value: u64) -> FissionResult<()> {
        let _ = value;
        Err(fission_core::err!(
            debug,
            "stack_push is not supported on this platform"
        ))
    }

    /// Search for a byte pattern in target memory
    fn find_pattern(&self, start: u64, size: usize, pattern: &[u8]) -> FissionResult<Vec<u64>> {
        let _ = (start, size, pattern);
        Err(fission_core::err!(
            debug,
            "find_pattern is not supported on this platform"
        ))
    }

    /// Enumerate exports from a module by base address
    fn get_module_exports(&self, base: u64) -> FissionResult<Vec<super::types::ExportInfo>> {
        let _ = base;
        Err(fission_core::err!(
            debug,
            "get_module_exports is not supported on this platform"
        ))
    }

    /// Enumerate imports from a module by base address
    fn get_module_imports(&self, base: u64) -> FissionResult<Vec<super::types::ImportInfo>> {
        let _ = base;
        Err(fission_core::err!(
            debug,
            "get_module_imports is not supported on this platform"
        ))
    }

    /// Set a single register by name (e.g. "rax", "rip")
    fn set_register(&mut self, thread_id: u32, name: &str, value: u64) -> FissionResult<()> {
        let _ = (thread_id, name, value);
        Err(fission_core::err!(
            debug,
            "set_register is not supported on this platform"
        ))
    }

    /// Get a CPU flag by name (e.g. "zf", "cf")
    fn get_flag(&self, flag: &str) -> FissionResult<bool> {
        let _ = flag;
        Err(fission_core::err!(
            debug,
            "get_flag is not supported on this platform"
        ))
    }

    /// Set a CPU flag by name (e.g. "zf", "cf")
    fn set_flag(&mut self, flag: &str, value: bool) -> FissionResult<()> {
        let _ = (flag, value);
        Err(fission_core::err!(
            debug,
            "set_flag is not supported on this platform"
        ))
    }

    /// Get current debug state
    fn get_state(&self) -> super::types::DebugState {
        super::types::DebugState::default()
    }

    /// Query the currently attached process's modules and runtime mappings.
    ///
    /// The default preserves event-driven backends: it returns their known
    /// modules but labels the result partial because those events do not prove
    /// that every mapping or static/runtime address transform is available.
    /// Backends with an authoritative live mapping source should override it.
    fn list_modules(&mut self) -> FissionResult<super::types::ModuleListReport> {
        use super::capabilities::{DebugOperation, OperationSupport};
        use super::types::{
            ModuleAddressTransform, ModuleListCompleteness, ModuleListReport, ProcessMemoryMapping,
            ProcessModule,
        };

        let supported = self
            .capabilities()
            .operation(DebugOperation::ModuleList)
            .is_some_and(|capability| {
                !matches!(&capability.support, OperationSupport::Unsupported { .. })
            });
        if !supported {
            return Err(fission_core::FissionError::debug(
                "modules listing is not supported by this backend",
            ));
        }

        let pid = self
            .attached_pid()
            .ok_or_else(|| fission_core::FissionError::debug("Not attached to a process"))?;
        let state = self.get_state();
        let modules = state
            .modules
            .into_values()
            .map(|module| ProcessModule {
                path: module.path.clone(),
                name: module.name,
                device: None,
                inode: None,
                mappings: vec![ProcessMemoryMapping {
                    runtime_start: module.base_address,
                    runtime_end: module.base_address.saturating_add(module.size),
                    file_offset: None,
                    permissions: None,
                    path: Some(module.path),
                }],
                address_transform: ModuleAddressTransform::Unavailable {
                    reason: "backend reports module events but not file-offset address mapping"
                        .to_string(),
                },
            })
            .collect();

        Ok(ModuleListReport {
            schema_version: 1,
            pid,
            completeness: ModuleListCompleteness::Partial,
            modules,
            other_mappings: Vec::new(),
            diagnostics: vec![
                "module records reflect backend-observed events; mapping completeness is not guaranteed"
                    .to_string(),
            ],
        })
    }

    /// Capture stable process identity (including any backend lifetime token) for live acquisition.
    fn capture_identity(&self, session_id: &str) -> Option<super::types::ProcessCaptureIdentity> {
        let pid = self.attached_pid()?;
        Some(super::types::ProcessCaptureIdentity {
            host_id: None,
            session_id: session_id.to_string(),
            pid,
            lifetime_token: None,
            architecture: None,
        })
    }

    /// Describe current execution consistency and thread-enumeration completeness.
    fn capture_consistency_state(
        &self,
    ) -> (
        super::types::CaptureConsistency,
        super::types::EnumerationCompleteness,
    ) {
        use super::types::{CaptureConsistency, DebugStatus, EnumerationCompleteness};

        let state = self.get_state();
        let thread_enum = if state.threads.is_empty() {
            EnumerationCompleteness::Unavailable
        } else {
            // Default DebugState tracks observed threads rather than guaranteeing host-wide completeness.
            EnumerationCompleteness::Partial
        };

        let consistency = match state.status {
            DebugStatus::Running => CaptureConsistency::LiveBestEffort,
            DebugStatus::Suspended
                if !state.threads.is_empty() && state.threads.values().all(|t| t.suspended) =>
            {
                CaptureConsistency::StoppedAllObservedThreads
            }
            _ => CaptureConsistency::Unknown,
        };

        (consistency, thread_enum)
    }

    /// Report active debugger interventions overlapping `[start, end)` without hiding patched bytes.
    fn recorded_interventions(
        &self,
        start: u64,
        end: u64,
    ) -> Vec<super::types::DebuggerIntervention> {
        use super::types::{BreakpointKind, DebuggerIntervention, DebuggerInterventionKind};

        self.list_breakpoints()
            .into_iter()
            .filter(|bp| {
                bp.kind == BreakpointKind::Software && bp.address >= start && bp.address < end
            })
            .map(|bp| DebuggerIntervention {
                kind: DebuggerInterventionKind::SoftwareBreakpointPatched {
                    address: bp.address,
                    original_byte: bp.original_byte,
                    enabled: bp.enabled,
                },
                detail: format!(
                    "software breakpoint patched at 0x{:x} (enabled={})",
                    bp.address, bp.enabled
                ),
            })
            .collect()
    }

    /// Perform an epoch-bound bounded sparse acquisition, verifying capability authorization,
    /// pre/post process identity stability, thread/mapping churn, and debugger interventions.
    ///
    /// Never implies that stopped threads make shared mappings immutable, never authorizes live
    /// unstopped reads unless `options.allow_live_unstopped` is true (without auto-suspending),
    /// and never silently overwrites observed breakpoint bytes with `original_byte`.
    fn acquire_epoch_bound_memory(
        &mut self,
        address: u64,
        size: usize,
        options: super::types::LiveAcquisitionOptions,
    ) -> FissionResult<super::types::EpochBoundMemoryCapture> {
        use super::capabilities::{DebugOperation, OperationSupport};
        use super::types::{
            AcquisitionEpoch, CaptureCoherenceStatus, CaptureConsistency, EnumerationCompleteness,
            EpochBoundMemoryCapture, LIVE_ACQUISITION_SCHEMA_VERSION, MemoryViewKind,
            ModuleListCompleteness, ModuleListReport,
        };

        fn fnv1a_update(hash: &mut u64, bytes: &[u8]) {
            for &b in bytes {
                *hash ^= u64::from(b);
                *hash = hash.wrapping_mul(0x1000_0000_01b3);
            }
        }

        fn fingerprint_modules(report: &ModuleListReport) -> u64 {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            fnv1a_update(&mut h, &report.pid.to_le_bytes());
            for m in &report.modules {
                fnv1a_update(&mut h, m.path.as_bytes());
                for seg in &m.mappings {
                    fnv1a_update(&mut h, &seg.runtime_start.to_le_bytes());
                    fnv1a_update(&mut h, &seg.runtime_end.to_le_bytes());
                }
            }
            for seg in &report.other_mappings {
                fnv1a_update(&mut h, &seg.runtime_start.to_le_bytes());
                fnv1a_update(&mut h, &seg.runtime_end.to_le_bytes());
            }
            h
        }

        fn unix_now_ms() -> u64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
                .unwrap_or(0)
        }

        if options.require_memory_read_capability {
            let readable = self
                .capabilities()
                .operation(DebugOperation::MemoryRead)
                .is_some_and(|cap| !matches!(&cap.support, OperationSupport::Unsupported { .. }));
            if !readable {
                return Err(fission_core::err!(
                    debug,
                    "live memory acquisition refused: backend does not report MemoryRead capability"
                ));
            }
        }

        let identity_before = self
            .capture_identity(&options.session_id)
            .ok_or_else(|| fission_core::err!(debug, "not attached to a target process"))?;

        let (consistency_before, thread_enum) = self.capture_consistency_state();
        if consistency_before == CaptureConsistency::LiveBestEffort && !options.allow_live_unstopped
        {
            return Err(fission_core::err!(
                debug,
                "live memory acquisition on an unstopped target requires allow_live_unstopped authorization"
            ));
        }

        let size_u64 = u64::try_from(size)
            .map_err(|_| fission_core::err!(debug, "acquisition size out of range"))?;
        let end = address.checked_add(size_u64).ok_or_else(|| {
            fission_core::err!(
                debug,
                "acquisition range overflows address space at 0x{address:x}"
            )
        })?;

        let started_unix_ms = unix_now_ms();
        let observed_thread_ids_before: Vec<u32> =
            self.get_state().threads.keys().copied().collect();

        let modules_before = self.list_modules().ok();
        let map_enum_before = modules_before.as_ref().map(|r| match r.completeness {
            ModuleListCompleteness::Complete => EnumerationCompleteness::Complete,
            ModuleListCompleteness::Partial => EnumerationCompleteness::Partial,
        });

        let sparse_capture = self.read_memory_sparse(address, size, options.sparse)?;
        let ended_unix_ms = unix_now_ms();

        let identity_after = self.capture_identity(&options.session_id);
        let observed_thread_ids_after: Vec<u32> =
            self.get_state().threads.keys().copied().collect();
        let modules_after = self.list_modules().ok();

        let identity_stable = identity_after.as_ref() == Some(&identity_before);
        let threads_changed = observed_thread_ids_before != observed_thread_ids_after;

        let (mapping_enumeration, mappings_changed) = match (&modules_before, &modules_after) {
            (Some(before), Some(after)) => {
                let enum_state = map_enum_before.unwrap_or(EnumerationCompleteness::Partial);
                let changed = fingerprint_modules(before) != fingerprint_modules(after)
                    || before.modules.len() != after.modules.len();
                (enum_state, changed)
            }
            (None, None) => (EnumerationCompleteness::Unavailable, false),
            _ => (EnumerationCompleteness::Partial, true),
        };

        let mut diagnostics = Vec::new();
        if !identity_stable {
            diagnostics.push(
                "target process identity changed, reused PID, or exited during acquisition; refusing to merge observations"
                    .to_string(),
            );
        }
        if identity_before.lifetime_token.is_none() {
            diagnostics.push(
                "backend does not provide a process lifetime token beyond numeric PID; PID reuse risk remains"
                    .to_string(),
            );
        }
        if threads_changed {
            diagnostics.push(format!(
                "observed thread set changed during acquisition ({} -> {} threads)",
                observed_thread_ids_before.len(),
                observed_thread_ids_after.len()
            ));
        }
        if mappings_changed {
            diagnostics.push(
                "module/memory mapping snapshot changed between pre- and post-acquisition checks"
                    .to_string(),
            );
        }
        if mapping_enumeration != EnumerationCompleteness::Complete {
            diagnostics.push(format!(
                "mapping enumeration completeness is {:?}",
                mapping_enumeration
            ));
        }
        if thread_enum != EnumerationCompleteness::Complete {
            diagnostics.push(format!(
                "thread enumeration completeness is {:?}",
                thread_enum
            ));
        }
        if sparse_capture.coverage().missing_bytes > 0 {
            diagnostics.push(format!(
                "capture recorded {} unread gap byte(s) across {} gap(s)",
                sparse_capture.coverage().missing_bytes,
                sparse_capture.coverage().gap_count
            ));
        }

        let interventions = self.recorded_interventions(address, end);
        if !interventions.is_empty() {
            diagnostics.push(format!(
                "{} debugger intervention(s) overlap raw observed range; raw bytes preserved without silent unpatching",
                interventions.len()
            ));
        }

        let (coherence, payload) =
            if !identity_stable || (mappings_changed && options.refuse_on_mapping_churn) {
                (CaptureCoherenceStatus::InconsistentRefused, None)
            } else if sparse_capture.coverage().missing_bytes > 0
                || mappings_changed
                || threads_changed
                || thread_enum != EnumerationCompleteness::Complete
                || mapping_enumeration != EnumerationCompleteness::Complete
                || identity_before.lifetime_token.is_none()
                || matches!(
                    consistency_before,
                    CaptureConsistency::LiveBestEffort | CaptureConsistency::Unknown
                )
            {
                (CaptureCoherenceStatus::Partial, Some(sparse_capture))
            } else {
                (CaptureCoherenceStatus::Coherent, Some(sparse_capture))
            };

        let shared_mappings_mutable = consistency_before != CaptureConsistency::OsSnapshot;

        Ok(EpochBoundMemoryCapture {
            schema_version: LIVE_ACQUISITION_SCHEMA_VERSION,
            identity: identity_before,
            epoch: AcquisitionEpoch {
                epoch_id: options.epoch_id,
                started_unix_ms,
                ended_unix_ms,
                consistency: consistency_before,
                shared_mappings_mutable,
                thread_enumeration: thread_enum,
                mapping_enumeration,
                observed_thread_ids_before,
                observed_thread_ids_after,
                threads_changed,
                mappings_changed,
                identity_stable,
            },
            requested_start: address,
            requested_size: size,
            view_kind: MemoryViewKind::RawObserved,
            interventions,
            coherence,
            diagnostics,
            capture: payload,
        })
    }
}

// ============================================================================
// Time Travel Debugging Trait
// ============================================================================

use super::timeline::ExecutionSnapshot;

/// Time-travel / reversible execution backend trait
///
/// Backends include:
/// - **Internal recorder** (`fission-ttd`): snapshot-based timeline (used heavily on Windows flows)
/// - **RR (Record and Replay)**: Linux-only GDB/MI integration (`crate::debug::rr`)
///
/// # Example
///
/// ```ignore
/// use crate::debug::TimeTravelDebugger;
///
/// // Record execution
/// debugger.start_recording()?;
/// // ... run program ...
/// debugger.stop_recording()?;
///
/// // Navigate timeline
/// debugger.seek_to(100)?;          // Go to step 100
/// debugger.reverse_step()?;        // Step backwards
/// debugger.reverse_continue()?;    // Run backwards to breakpoint
/// ```
pub trait TimeTravelDebugger: Send {
    /// Start recording execution
    fn start_recording(&mut self) -> FissionResult<()>;

    /// Stop recording execution
    fn stop_recording(&mut self) -> FissionResult<()>;

    /// Check if currently recording
    fn is_recording(&self) -> bool;

    /// Check if in replay/navigation mode
    fn is_replay_mode(&self) -> bool;

    /// Seek to a specific step/position in the timeline
    fn seek_to(&mut self, position: u64) -> FissionResult<ExecutionSnapshot>;

    /// Step backwards one instruction
    fn reverse_step(&mut self) -> FissionResult<ExecutionSnapshot>;

    /// Continue backwards until next breakpoint
    fn reverse_continue(&mut self) -> FissionResult<ExecutionSnapshot>;

    /// Step forwards one instruction (in replay mode)
    fn forward_step(&mut self) -> FissionResult<ExecutionSnapshot>;

    /// Continue forwards until next breakpoint (in replay mode)
    fn forward_continue(&mut self) -> FissionResult<ExecutionSnapshot>;

    /// Get current position in timeline
    fn current_position(&self) -> Option<u64>;

    /// Get current execution snapshot
    fn current_snapshot(&self) -> Option<&ExecutionSnapshot>;

    /// Get timeline range (min_step, max_step)
    fn timeline_range(&self) -> Option<(u64, u64)>;

    /// Get total number of recorded steps
    fn step_count(&self) -> usize;

    /// Clear all recorded data
    fn clear_timeline(&mut self);
}
