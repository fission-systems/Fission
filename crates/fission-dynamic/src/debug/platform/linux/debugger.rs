//! Linux-specific debugger implementation using ptrace.
//!
//! This module provides debugging capabilities on Linux using the ptrace system call.

use crate::debug::timeline::Timeline;
use crate::debug::traits::ExecutionBackend;
use crate::debug::types::{
    Breakpoint, DebugEvent, DebugState, DebugStatus, ModuleAddressTransform,
    ModuleListCompleteness, ModuleListReport, ProcessInfo, ProcessMemoryMapping, ProcessModule,
    RegisterState, ThreadInfo,
};
use fission_core::{FissionError, Result as FissionResult};
use fission_loader::loader::elf::{ElfLoadSegment, ElfLoader};
use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Linux debugger implementation using ptrace
pub struct LinuxDebugger {
    /// Current debug state
    state: DebugState,
    /// Target process ID
    target_pid: Option<u32>,
    /// Events produced synchronously while establishing the initial launch stop.
    pending_events: VecDeque<DebugEvent>,
    /// Session-owned timeline for snapshots at ptrace stop boundaries.
    ttd_timeline: Option<Arc<Mutex<Timeline>>>,
}

impl LinuxDebugger {
    /// Create a new Linux debugger instance
    pub fn new() -> Self {
        Self {
            state: DebugState::default(),
            target_pid: None,
            pending_events: VecDeque::new(),
            ttd_timeline: None,
        }
    }

    /// Get current state
    pub fn state(&self) -> &DebugState {
        &self.state
    }

    fn record_ttd_snapshot(&mut self, thread_id: u32, registers: RegisterState) {
        let Some(timeline) = &self.ttd_timeline else {
            return;
        };
        if let Ok(mut timeline) = timeline.lock() {
            if timeline.is_recording() {
                timeline.record_event(registers, thread_id);
            }
        }
    }

    fn stop_ttd_recording(&self) {
        if let Some(timeline) = &self.ttd_timeline {
            if let Ok(mut timeline) = timeline.lock() {
                timeline.stop_recording();
            }
        }
    }
}

fn output_mapping(mapping: &super::memory::LinuxProcessMapping) -> ProcessMemoryMapping {
    ProcessMemoryMapping {
        runtime_start: mapping.start,
        runtime_end: mapping.end,
        file_offset: Some(mapping.file_offset),
        permissions: Some(mapping.permissions.clone()),
        path: mapping.path.clone(),
    }
}

fn derive_elf_load_bias(
    mappings: &[super::memory::LinuxProcessMapping],
    segments: &[ElfLoadSegment],
) -> Result<i64, String> {
    let mut candidates = std::collections::BTreeSet::new();
    for mapping in mappings {
        let mapping_file_end = mapping
            .file_offset
            .checked_add(mapping.end.saturating_sub(mapping.start))
            .ok_or_else(|| "mapping file range overflows".to_string())?;
        for segment in segments.iter().filter(|segment| segment.file_size > 0) {
            let segment_file_end = segment
                .file_offset
                .checked_add(segment.file_size)
                .ok_or_else(|| "ELF segment file range overflows".to_string())?;
            let overlap_start = mapping.file_offset.max(segment.file_offset);
            let overlap_end = mapping_file_end.min(segment_file_end);
            if overlap_start >= overlap_end {
                continue;
            }

            let runtime_address = mapping
                .start
                .checked_add(overlap_start - mapping.file_offset)
                .ok_or_else(|| "runtime mapping address overflows".to_string())?;
            let analysis_address = segment
                .virtual_address
                .checked_add(overlap_start - segment.file_offset)
                .ok_or_else(|| "ELF virtual address overflows".to_string())?;
            let bias = i64::try_from(i128::from(runtime_address) - i128::from(analysis_address))
                .map_err(|_| "ELF load bias is outside the supported signed range".to_string())?;
            candidates.insert(bias);
        }
    }

    match candidates.len() {
        0 => Err("no process mapping overlaps a file-backed PT_LOAD range".to_string()),
        1 => Ok(*candidates.first().expect("one load-bias candidate")),
        _ => Err("file-backed PT_LOAD mappings disagree on the ELF load bias".to_string()),
    }
}

fn linux_device_number(device: &str) -> Option<u64> {
    let (major, minor) = device.split_once(':')?;
    let major = u64::from_str_radix(major, 16).ok()?;
    let minor = u64::from_str_radix(minor, 16).ok()?;
    Some(
        ((major & 0x0000_0fff) << 8)
            | (minor & 0x0000_00ff)
            | ((minor & 0xffff_ff00) << 12)
            | ((major & 0xffff_f000) << 32),
    )
}

fn module_address_transform(
    path: &str,
    device: &str,
    inode: u64,
    mappings: &[super::memory::LinuxProcessMapping],
) -> ModuleAddressTransform {
    let file_path = path.strip_suffix(" (deleted)").unwrap_or(path);
    let unavailable = |reason: String| ModuleAddressTransform::Unavailable { reason };
    let mut file = match File::open(file_path) {
        Ok(file) => file,
        Err(error) => {
            return unavailable(format!("mapped file is unavailable: {error}"));
        }
    };

    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(error) => return unavailable(format!("could not inspect mapped file: {error}")),
    };
    if metadata.ino() != inode || linux_device_number(device) != Some(metadata.dev()) {
        return unavailable(
            "mapped path no longer identifies the device/inode observed in procfs".to_string(),
        );
    }

    let mut magic = [0; 4];
    if let Err(error) = file.read_exact(&mut magic) {
        return unavailable(format!("could not read mapped-file signature: {error}"));
    }
    if magic != *b"\x7fELF" {
        return ModuleAddressTransform::NotApplicable {
            reason: "mapped file is not ELF; ELF VA conversion does not apply".to_string(),
        };
    }

    let segments = match ElfLoader::load_segments_from_reader(&mut file) {
        Ok(segments) => segments,
        Err(error) => return unavailable(format!("could not parse ELF load segments: {error}")),
    };
    match derive_elf_load_bias(mappings, &segments) {
        Ok(load_bias) => ModuleAddressTransform::Resolved {
            format: "elf".to_string(),
            analysis_address_domain: "elf_virtual_address".to_string(),
            runtime_address_domain: "process_virtual_address".to_string(),
            formula: "runtime_va = analysis_va + load_bias".to_string(),
            load_bias,
        },
        Err(reason) => unavailable(reason),
    }
}

fn module_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

impl Default for LinuxDebugger {
    fn default() -> Self {
        Self::new()
    }
}

/// Enumerate running processes on Linux by reading /proc
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Ok(pid) = name.parse::<u32>() {
                    // Read process name from /proc/[pid]/comm
                    let comm_path = path.join("comm");
                    let process_name = std::fs::read_to_string(&comm_path)
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|_| "<unknown>".to_string());

                    // Read exe path from /proc/[pid]/exe
                    let exe_path = path.join("exe");
                    let exe = std::fs::read_link(&exe_path)
                        .ok()
                        .and_then(|p| p.to_str().map(String::from));

                    processes.push(ProcessInfo {
                        pid,
                        name: process_name,
                        exe_path: exe,
                    });
                }
            }
        }
    }

    // Sort by PID
    processes.sort_by_key(|p| p.pid);
    processes
}

impl ExecutionBackend for LinuxDebugger {
    fn capabilities(&self) -> crate::debug::capabilities::DebugBackendCapabilities {
        use crate::debug::capabilities::{
            BackendAvailability, DebugBackendCapabilities, DebugBackendKind, DebugOperation,
            RuntimeRequirement,
        };

        DebugBackendCapabilities::new(
            DebugBackendKind::LinuxPtrace,
            BackendAvailability::Available,
            &[
                DebugOperation::ProcessEnumeration,
                DebugOperation::Launch,
                DebugOperation::Attach,
                DebugOperation::Detach,
                DebugOperation::ContinueExecution,
                DebugOperation::PollEvent,
                DebugOperation::SingleStep,
                DebugOperation::SoftwareBreakpoints,
                DebugOperation::RegisterRead,
                DebugOperation::MemoryRead,
                DebugOperation::MemoryWrite,
                DebugOperation::ModuleList,
            ],
        )
        .conditionally_supporting(
            DebugOperation::Launch,
            &[RuntimeRequirement::PtracePolicyAllowsLaunch],
        )
        .conditionally_supporting(
            DebugOperation::Attach,
            &[RuntimeRequirement::PtracePolicyAllowsAttach],
        )
        .conditionally_supporting(
            DebugOperation::ModuleList,
            &[RuntimeRequirement::TargetProcessAllowsDebugging],
        )
    }

    fn list_modules(&mut self) -> FissionResult<ModuleListReport> {
        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached to a process"))?;
        let (process_mappings, mut diagnostics) = super::memory::read_process_mappings(pid)
            .map_err(|error| {
                FissionError::debug(format!("Could not read /proc/{pid}/maps: {error}"))
            })?;

        let mut grouped: BTreeMap<(String, u64), Vec<super::memory::LinuxProcessMapping>> =
            BTreeMap::new();
        let mut other_mappings = Vec::new();
        for mapping in process_mappings {
            let file_backed = mapping.inode != 0
                && mapping
                    .path
                    .as_deref()
                    .is_some_and(|path| !path.starts_with('['));
            if file_backed {
                grouped
                    .entry((mapping.device.clone(), mapping.inode))
                    .or_default()
                    .push(mapping);
            } else {
                other_mappings.push(output_mapping(&mapping));
            }
        }

        let mut modules = Vec::with_capacity(grouped.len());
        for ((device, inode), mappings) in grouped {
            let path = mappings
                .iter()
                .find_map(|mapping| mapping.path.clone())
                .expect("file-backed mapping has a path");
            let address_transform = module_address_transform(&path, &device, inode, &mappings);
            if let ModuleAddressTransform::Unavailable { reason } = &address_transform {
                diagnostics.push(format!("{}: {reason}", path));
            }
            modules.push(ProcessModule {
                name: module_name(&path),
                path,
                device: Some(device),
                inode: Some(inode),
                mappings: mappings.iter().map(output_mapping).collect(),
                address_transform,
            });
        }
        modules.sort_by_key(|module| {
            module
                .mappings
                .iter()
                .map(|mapping| mapping.runtime_start)
                .min()
                .unwrap_or_default()
        });
        other_mappings.sort_by_key(|mapping| mapping.runtime_start);

        self.state.modules = modules
            .iter()
            .map(|module| {
                let base_address = module
                    .mappings
                    .iter()
                    .map(|mapping| mapping.runtime_start)
                    .min()
                    .unwrap_or_default();
                let end_address = module
                    .mappings
                    .iter()
                    .map(|mapping| mapping.runtime_end)
                    .max()
                    .unwrap_or(base_address);
                (
                    base_address,
                    crate::debug::types::ModuleInfo {
                        base_address,
                        size: end_address.saturating_sub(base_address),
                        path: module.path.clone(),
                        name: module.name.clone(),
                    },
                )
            })
            .collect();

        let partial = !diagnostics.is_empty()
            || modules.iter().any(|module| {
                matches!(
                    module.address_transform,
                    ModuleAddressTransform::Unavailable { .. }
                )
            });
        Ok(ModuleListReport {
            schema_version: 1,
            pid,
            completeness: if partial {
                ModuleListCompleteness::Partial
            } else {
                ModuleListCompleteness::Complete
            },
            modules,
            other_mappings,
            diagnostics,
        })
    }

    fn set_timeline(&mut self, timeline: Arc<Mutex<Timeline>>) {
        self.ttd_timeline = Some(timeline);
    }

    fn enumerate_processes() -> Vec<ProcessInfo> {
        enumerate_processes()
    }

    fn attach(&mut self, pid: u32) -> FissionResult<()> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        self.state.status = DebugStatus::Attaching;

        ptrace::attach(Pid::from_raw(pid as i32)).map_err(|e| {
            FissionError::debug(format!("Failed to attach to process {}: {}", pid, e))
        })?;

        self.target_pid = Some(pid);
        self.state.attached_pid = Some(pid);
        self.state.status = DebugStatus::Suspended; // ptrace attach sends SIGSTOP
        self.state.last_event = Some(format!("Attached to PID {}", pid));
        if let Some(timeline) = &self.ttd_timeline {
            if let Ok(mut timeline) = timeline.lock() {
                timeline.start_recording();
            }
        }

        Ok(())
    }

    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        use nix::sys::ptrace;
        use nix::sys::signal::Signal;
        use nix::sys::wait::{WaitStatus, waitpid};
        use nix::unistd::Pid;
        use std::os::unix::process::CommandExt;
        use std::process::Command;

        if self.is_attached() {
            return Err(FissionError::debug(
                "Detach from the current process before launching another one",
            ));
        }

        let mut command = Command::new(path);
        command.args(args);
        // PTRACE_TRACEME makes the child stop with SIGTRAP after exec, before
        // it can execute its first user-space instruction. Keep the child-side
        // pre-exec hook limited to the ptrace syscall and error conversion.
        unsafe {
            command.pre_exec(|| {
                ptrace::traceme().map_err(|error| std::io::Error::from_raw_os_error(error as i32))
            });
        }

        let mut child = command.spawn().map_err(|error| {
            FissionError::debug(format!(
                "Failed to launch '{}' under ptrace: {}",
                path, error
            ))
        })?;
        let pid = child.id();
        let child_pid = Pid::from_raw(pid as i32);

        let initial_status = match waitpid(child_pid, None) {
            Ok(status) => status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(FissionError::debug(format!(
                    "Failed waiting for '{}' to stop after exec: {}",
                    path, error
                )));
            }
        };

        if !matches!(
            initial_status,
            WaitStatus::Stopped(stopped_pid, Signal::SIGTRAP)
                if stopped_pid.as_raw() == child_pid.as_raw()
        ) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FissionError::debug(format!(
                "Launched '{}' did not enter the expected ptrace exec stop: {:?}",
                path, initial_status
            )));
        }

        self.target_pid = Some(pid);
        let registers = match self.fetch_registers(pid) {
            Ok(registers) => registers,
            Err(error) => {
                self.target_pid = None;
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };

        let mut state = DebugState {
            attached_pid: Some(pid),
            main_thread_id: Some(pid),
            last_thread_id: Some(pid),
            current_thread_id: Some(pid),
            status: DebugStatus::Suspended,
            registers: Some(registers.clone()),
            last_event: Some(format!("Launched PID {} and stopped after exec", pid)),
            ..DebugState::default()
        };
        state.threads.insert(
            pid,
            ThreadInfo {
                thread_id: pid,
                start_address: registers.pc,
                suspended: true,
                is_main: true,
            },
        );
        self.state = state;
        self.pending_events.push_back(DebugEvent::ProcessCreated {
            pid,
            main_thread_id: pid,
        });

        if let Some(timeline) = &self.ttd_timeline {
            if let Ok(mut timeline) = timeline.lock() {
                timeline.start_recording();
            }
        }
        self.record_ttd_snapshot(pid, registers);

        Ok(pid)
    }

    fn detach(&mut self) -> FissionResult<()> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached to any process"))?;

        ptrace::detach(Pid::from_raw(pid as i32), None).map_err(|e| {
            FissionError::debug(format!("Failed to detach from process {}: {}", pid, e))
        })?;

        self.target_pid = None;
        self.state.attached_pid = None;
        self.state.status = DebugStatus::Detached;
        self.state.last_event = Some("Detached".to_string());
        self.stop_ttd_recording();

        Ok(())
    }

    fn is_attached(&self) -> bool {
        self.target_pid.is_some()
    }

    fn attached_pid(&self) -> Option<u32> {
        self.target_pid
    }

    fn continue_execution(&mut self) -> FissionResult<()> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;

        ptrace::cont(Pid::from_raw(pid as i32), None)
            .map_err(|e| FissionError::debug(format!("Continue failed: {}", e)))?;

        self.state.status = DebugStatus::Running;
        Ok(())
    }

    fn single_step(&mut self) -> FissionResult<()> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;

        ptrace::step(Pid::from_raw(pid as i32), None)
            .map_err(|e| FissionError::debug(format!("Single step failed: {}", e)))?;

        self.state.status = DebugStatus::Running;
        Ok(())
    }

    fn poll_event(
        &mut self,
        timeout_ms: u32,
    ) -> FissionResult<Option<crate::debug::types::DebugEvent>> {
        use nix::sys::signal::Signal;
        use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
        use nix::unistd::Pid;

        if let Some(event) = self.pending_events.pop_front() {
            self.state.event_count = self.state.event_count.saturating_add(1);
            return Ok(Some(event));
        }

        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;
        let start = Instant::now();
        let timeout = Duration::from_millis(u64::from(timeout_ms));

        loop {
            let status = waitpid(Pid::from_raw(pid as i32), Some(WaitPidFlag::WNOHANG))
                .map_err(|e| FissionError::debug(format!("waitpid failed: {}", e)))?;
            match status {
                WaitStatus::StillAlive => {
                    if timeout_ms == 0 || start.elapsed() >= timeout {
                        return Ok(None);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                WaitStatus::Exited(_, code) => {
                    self.target_pid = None;
                    self.state.attached_pid = None;
                    self.state.status = DebugStatus::Terminated;
                    self.state.event_count = self.state.event_count.saturating_add(1);
                    self.stop_ttd_recording();
                    return Ok(Some(crate::debug::types::DebugEvent::ProcessExited {
                        exit_code: code as u32,
                    }));
                }
                WaitStatus::Signaled(_, signal, _) => {
                    self.target_pid = None;
                    self.state.attached_pid = None;
                    self.state.status = DebugStatus::Terminated;
                    self.state.event_count = self.state.event_count.saturating_add(1);
                    self.stop_ttd_recording();
                    return Ok(Some(crate::debug::types::DebugEvent::ProcessExited {
                        exit_code: 128 + signal as u32,
                    }));
                }
                WaitStatus::Stopped(_, signal) => {
                    self.state.status = DebugStatus::Suspended;
                    self.state.last_thread_id = Some(pid);
                    self.state.current_thread_id = Some(pid);
                    self.state.event_count = self.state.event_count.saturating_add(1);
                    let registers = self.fetch_registers(pid)?;
                    self.state.registers = Some(registers.clone());
                    self.record_ttd_snapshot(pid, registers.clone());

                    if signal == Signal::SIGTRAP {
                        let breakpoint_address = registers
                            .pc
                            .checked_sub(1)
                            .filter(|address| self.state.breakpoints.contains_key(address));
                        if let Some(address) = breakpoint_address {
                            if let Some(breakpoint) = self.state.breakpoints.get_mut(&address) {
                                breakpoint.hits = breakpoint.hits.saturating_add(1);
                            }
                            return Ok(Some(crate::debug::types::DebugEvent::BreakpointHit {
                                address,
                                thread_id: pid,
                            }));
                        }
                        return Ok(Some(crate::debug::types::DebugEvent::SingleStep {
                            thread_id: pid,
                        }));
                    }

                    return Ok(Some(crate::debug::types::DebugEvent::Exception {
                        code: signal as u32,
                        address: registers.pc,
                        first_chance: true,
                    }));
                }
                _ => {}
            }
        }
    }

    fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;

        // Read original byte using ptrace PEEKDATA
        let original_word =
            ptrace::read(Pid::from_raw(pid as i32), address as *mut std::ffi::c_void).map_err(
                |e| FissionError::debug(format!("Failed to read memory at 0x{:x}: {}", address, e)),
            )?;

        let original_byte = (original_word & 0xFF) as u8;

        // Write INT3 (0xCC) using ptrace POKEDATA
        let new_word = (original_word & !0xFF) | 0xCC;
        unsafe {
            ptrace::write(
                Pid::from_raw(pid as i32),
                address as *mut std::ffi::c_void,
                new_word as *mut std::ffi::c_void,
            )
            .map_err(|e| {
                FissionError::debug(format!(
                    "Failed to write breakpoint at 0x{:x}: {}",
                    address, e
                ))
            })?;
        }

        let bp = Breakpoint {
            address,
            original_byte,
            enabled: true,
            temporary: false,
            kind: crate::debug::types::BreakpointKind::Software,
            hits: 0,
            condition: None,
        };
        self.state.breakpoints.insert(address, bp);
        self.state.last_event = Some(format!("Breakpoint set at 0x{:016x}", address));

        Ok(())
    }

    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;

        let bp = self
            .state
            .breakpoints
            .get(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not found"))?;

        // Read current word
        let current_word =
            ptrace::read(Pid::from_raw(pid as i32), address as *mut std::ffi::c_void).map_err(
                |e| FissionError::debug(format!("Failed to read memory at 0x{:x}: {}", address, e)),
            )?;

        // Restore original byte
        let new_word = (current_word & !0xFF) | (bp.original_byte as i64);
        unsafe {
            ptrace::write(
                Pid::from_raw(pid as i32),
                address as *mut std::ffi::c_void,
                new_word as *mut std::ffi::c_void,
            )
            .map_err(|e| {
                FissionError::debug(format!(
                    "Failed to restore breakpoint at 0x{:x}: {}",
                    address, e
                ))
            })?;
        }

        self.state.breakpoints.remove(&address);
        self.state.last_event = Some(format!("Breakpoint removed at 0x{:016x}", address));

        Ok(())
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;

        // Read from /proc/[pid]/mem
        use std::io::{Read, Seek, SeekFrom};

        let mem_path = format!("/proc/{}/mem", pid);
        let mut file = std::fs::File::open(&mem_path)
            .map_err(|e| FissionError::debug(format!("Failed to open {}: {}", mem_path, e)))?;

        file.seek(SeekFrom::Start(address)).map_err(|e| {
            FissionError::debug(format!("Failed to seek to 0x{:x}: {}", address, e))
        })?;

        let mut buffer = vec![0u8; size];
        file.read_exact(&mut buffer).map_err(|e| {
            FissionError::debug(format!(
                "Failed to read {} bytes at 0x{:x}: {}",
                size, address, e
            ))
        })?;

        Ok(buffer)
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;

        // Write to /proc/[pid]/mem
        use std::fs::OpenOptions;
        use std::io::{Seek, SeekFrom, Write};

        let mem_path = format!("/proc/{}/mem", pid);
        let mut file = OpenOptions::new()
            .write(true)
            .open(&mem_path)
            .map_err(|e| {
                FissionError::debug(format!("Failed to open {} for writing: {}", mem_path, e))
            })?;

        file.seek(SeekFrom::Start(address)).map_err(|e| {
            FissionError::debug(format!("Failed to seek to 0x{:x}: {}", address, e))
        })?;

        file.write_all(data).map_err(|e| {
            FissionError::debug(format!(
                "Failed to write {} bytes at 0x{:x}: {}",
                data.len(),
                address,
                e
            ))
        })?;

        Ok(())
    }

    fn fetch_registers(&mut self, _thread_id: u32) -> FissionResult<RegisterState> {
        use nix::sys::ptrace;
        use nix::unistd::Pid;

        let pid = self
            .target_pid
            .ok_or_else(|| FissionError::debug("Not attached"))?;

        // Get registers using ptrace GETREGS
        let regs = ptrace::getregs(Pid::from_raw(pid as i32))
            .map_err(|e| FissionError::debug(format!("Failed to get registers: {}", e)))?;

        // `user_regs_struct` is the host's, so these names are correct here
        // by construction -- this backend only runs on an x86-64 Linux host.
        Ok(RegisterState::at(regs.rip)
            .with("RAX", regs.rax)
            .with("RBX", regs.rbx)
            .with("RCX", regs.rcx)
            .with("RDX", regs.rdx)
            .with("RSI", regs.rsi)
            .with("RDI", regs.rdi)
            .with("RBP", regs.rbp)
            .with("RSP", regs.rsp)
            .with("R8", regs.r8)
            .with("R9", regs.r9)
            .with("R10", regs.r10)
            .with("R11", regs.r11)
            .with("R12", regs.r12)
            .with("R13", regs.r13)
            .with("R14", regs.r14)
            .with("R15", regs.r15)
            .with("RIP", regs.rip)
            .with("RFLAGS", regs.eflags))
    }

    fn get_state(&self) -> DebugState {
        self.state.clone()
    }
}

#[cfg(all(test, target_os = "linux", feature = "interactive_runtime"))]
mod tests {
    #[cfg(target_arch = "x86_64")]
    use super::LinuxDebugger;
    use super::derive_elf_load_bias;
    use crate::debug::platform::linux::memory::LinuxProcessMapping;
    #[cfg(target_arch = "x86_64")]
    use crate::debug::traits::ExecutionBackend;
    #[cfg(target_arch = "x86_64")]
    use crate::debug::types::ModuleAddressTransform;

    fn mapping(start: u64, end: u64, file_offset: u64) -> LinuxProcessMapping {
        LinuxProcessMapping {
            start,
            end,
            permissions: "r-xp".to_string(),
            file_offset,
            device: "08:02".to_string(),
            inode: 42,
            path: Some("/tmp/fixture.so".to_string()),
        }
    }

    #[test]
    fn load_bias_requires_consistent_evidence_from_multiple_load_segments() {
        let segments = [
            fission_loader::loader::elf::ElfLoadSegment {
                file_offset: 0,
                virtual_address: 0,
                file_size: 0x1000,
                memory_size: 0x1000,
            },
            fission_loader::loader::elf::ElfLoadSegment {
                file_offset: 0x1000,
                virtual_address: 0x2000,
                file_size: 0x1000,
                memory_size: 0x1000,
            },
        ];
        let mappings = [
            mapping(0x7f00_0000, 0x7f00_1000, 0),
            mapping(0x7f00_2000, 0x7f00_3000, 0x1000),
        ];

        assert_eq!(derive_elf_load_bias(&mappings, &segments), Ok(0x7f00_0000));
    }

    #[test]
    fn load_bias_rejects_inconsistent_segment_evidence() {
        let segments = [
            fission_loader::loader::elf::ElfLoadSegment {
                file_offset: 0,
                virtual_address: 0x400000,
                file_size: 0x1000,
                memory_size: 0x1000,
            },
            fission_loader::loader::elf::ElfLoadSegment {
                file_offset: 0x1000,
                virtual_address: 0x402000,
                file_size: 0x1000,
                memory_size: 0x1000,
            },
        ];
        let mappings = [
            mapping(0x400000, 0x401000, 0),
            mapping(0x403000, 0x404000, 0x1000),
        ];

        assert!(derive_elf_load_bias(&mappings, &segments).is_err());
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn linux_launch_pc_correlates_to_a_mapped_elf_address() {
        use fission_loader::loader::elf::ElfLoader;

        let mut debugger = LinuxDebugger::new();
        let pid = debugger
            .launch("/bin/true", &[])
            .expect("launch a child under ptrace");

        let outcome = (|| {
            let registers = debugger
                .fetch_registers(pid)
                .map_err(|error| error.to_string())?;
            let report = debugger.list_modules().map_err(|error| error.to_string())?;
            let module = report
                .modules
                .iter()
                .find(|module| {
                    module.mappings.iter().any(|mapping| {
                        mapping.runtime_start <= registers.pc && registers.pc < mapping.runtime_end
                    })
                })
                .ok_or_else(|| {
                    format!(
                        "launch PC {:#x} was not in a file-backed module",
                        registers.pc
                    )
                })?;
            let ModuleAddressTransform::Resolved { load_bias, .. } = &module.address_transform
            else {
                return Err(format!(
                    "launch-PC module {} has no resolved ELF transform: {:?}",
                    module.path, module.address_transform
                ));
            };
            let analysis_pc = u64::try_from(i128::from(registers.pc) - i128::from(*load_bias))
                .map_err(|_| "runtime PC is outside the ELF VA domain".to_string())?;
            let mut file = std::fs::File::open(&module.path)
                .map_err(|error| format!("could not reopen {}: {error}", module.path))?;
            let segments = ElfLoader::load_segments_from_reader(&mut file)
                .map_err(|error| format!("could not read {} PT_LOADs: {error}", module.path))?;
            if !segments.iter().any(|segment| {
                segment.virtual_address <= analysis_pc
                    && analysis_pc < segment.virtual_address.saturating_add(segment.memory_size)
            }) {
                return Err(format!(
                    "translated launch PC {analysis_pc:#x} is outside {} PT_LOAD ranges",
                    module.path
                ));
            }
            Ok::<(), String>(())
        })();

        let child = nix::unistd::Pid::from_raw(pid as i32);
        let detach = debugger.detach();
        let cleanup = if detach.is_ok() {
            nix::sys::wait::waitpid(child, None).map(|_| ())
        } else {
            match nix::sys::ptrace::kill(child) {
                Ok(()) => nix::sys::wait::waitpid(child, None).map(|_| ()),
                Err(error) => Err(error),
            }
        };

        assert!(outcome.is_ok(), "{}", outcome.unwrap_err());
        assert!(detach.is_ok(), "could not detach test child: {detach:?}");
        assert!(cleanup.is_ok(), "could not reap test child: {cleanup:?}");
    }
}
