//! macOS native debugger: `ptrace` for execution control, Mach for state.
//!
//! * Launch runs the child under `PT_TRACE_ME`, which stops it with SIGTRAP at
//!   `exec`, before its first user-space instruction -- the same contract as the
//!   Linux backend.
//! * Registers, memory and modules come from the target's Mach **task port**.
//!   macOS hands that out only to a caller that has been authorized for Developer
//!   Tools access (or is root), so acquiring it is the one step that can fail on
//!   a correctly written debugger. It is requested on a bounded wait and fails
//!   with the action a person must take, rather than blocking a headless caller
//!   on an authorization prompt.
//! * Execution control uses the BSD `ptrace` requests macOS provides
//!   (`PT_CONTINUE`, `PT_STEP`, `PT_DETACH`, `PT_KILL`); stops arrive through
//!   `waitpid`.
//!
//! Only the host's own architecture is supported: a translated x86-64 process on
//! Apple Silicon has a different thread-state flavor and is not handled.

use super::mach::{self, WaitOutcome};
use super::regs;
use crate::debug::timeline::Timeline;
use crate::debug::traits::ExecutionBackend;
use crate::debug::types::{
    Breakpoint, BreakpointKind, DebugEvent, DebugState, DebugStatus, ModuleAddressTransform,
    ModuleListCompleteness, ModuleListReport, ProcessInfo, ProcessMemoryMapping, ProcessModule,
    RegisterState, ThreadInfo,
};
use fission_core::{FissionError, Result as FissionResult};
use std::collections::{HashMap, VecDeque};
use std::os::unix::process::CommandExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SIGTRAP: i32 = 5;
const SIGSTOP: i32 = 17;
/// Single-step traps absorbed after stepping off a breakpoint before one is
/// reported as a real event.
const MAX_ABSORBED_STEP_TRAPS: u32 = 8;
/// How long to wait for the Mach task port before reporting that access is
/// not authorized. Long enough for a granted request, short enough that a
/// headless caller is not stuck on a prompt it cannot answer.
const TASK_PORT_TIMEOUT: Duration = Duration::from_secs(8);

/// macOS debugger implementation.
pub struct MacOSDebugger {
    state: DebugState,
    target_pid: Option<u32>,
    /// The target's task port while attached.
    task: Option<mach::mach_port_t>,
    /// Thread ports from the last stop, main thread first.
    threads: Vec<mach::mach_port_t>,
    /// Bytes the breakpoint instruction replaced, by address.
    inserted: HashMap<u64, Vec<u8>>,
    /// A signal the target stopped for that is delivered when it resumes.
    pending_signal: i32,
    /// Where the target stood right after stepping off a breakpoint on the way
    /// to a `continue`. macOS can report one more single-step trap there after
    /// the continue, with no instruction executed; it is not an event. More than
    /// one such trap has been seen in a row, so the mark stays until the target
    /// stops anywhere else.
    step_off_pc: Option<u64>,
    /// How many of those traps were absorbed since the step-off.
    absorbed_step_traps: u32,
    /// Events produced while establishing a stop, delivered by `poll_event`.
    pending_events: VecDeque<DebugEvent>,
    ttd_timeline: Option<Arc<Mutex<Timeline>>>,
}

impl MacOSDebugger {
    /// Create a new macOS debugger instance
    pub fn new() -> Self {
        Self {
            state: DebugState::default(),
            target_pid: None,
            task: None,
            threads: Vec::new(),
            inserted: HashMap::new(),
            pending_signal: 0,
            step_off_pc: None,
            absorbed_step_traps: 0,
            pending_events: VecDeque::new(),
            ttd_timeline: None,
        }
    }

    /// Get current state
    pub fn state(&self) -> &DebugState {
        &self.state
    }

    fn pid(&self) -> FissionResult<u32> {
        self.target_pid
            .ok_or_else(|| FissionError::debug("Not attached to a process"))
    }

    fn task(&self) -> FissionResult<mach::mach_port_t> {
        self.task
            .ok_or_else(|| FissionError::debug("Not attached to a process"))
    }

    fn acquire_task_port(&mut self, pid: u32) -> FissionResult<()> {
        match mach::task_port_for_pid(pid, TASK_PORT_TIMEOUT) {
            Ok(task) => {
                self.task = Some(task);
                Ok(())
            }
            Err(error) => Err(FissionError::debug(format!(
                "Could not get the Mach task port for PID {pid} ({error:?}): {}",
                error.remediation()
            ))),
        }
    }

    fn release_ports(&mut self) {
        for port in self.threads.drain(..) {
            mach::release_port(port);
        }
        if let Some(task) = self.task.take() {
            mach::release_port(task);
        }
    }

    /// Refresh thread ports and the thread table. The main thread is reported
    /// under the pid, as the Linux backend does; the rest by kernel thread id.
    fn refresh_threads(&mut self) -> FissionResult<()> {
        let task = self.task()?;
        let pid = self.pid()?;
        let ports = mach::threads(task).map_err(FissionError::debug)?;
        for port in self.threads.drain(..) {
            mach::release_port(port);
        }
        self.state.threads.clear();
        for (index, port) in ports.iter().enumerate() {
            let is_main = index == 0;
            let id = if is_main {
                pid
            } else {
                mach::thread_unique_id(*port).unwrap_or(index as u64) as u32
            };
            self.state.threads.insert(
                id,
                ThreadInfo {
                    thread_id: id,
                    start_address: 0,
                    suspended: true,
                    is_main,
                },
            );
        }
        self.threads = ports;
        Ok(())
    }

    fn thread_port(&self, thread_id: u32) -> FissionResult<mach::mach_port_t> {
        let pid = self.pid()?;
        // The main thread is reported under the pid, and is the only thread of
        // most targets at the exec stop.
        if thread_id == pid || self.threads.len() <= 1 {
            return self
                .threads
                .first()
                .copied()
                .ok_or_else(|| FissionError::debug("The target has no threads"));
        }
        self.threads
            .iter()
            .copied()
            .skip(1)
            .find(|port| mach::thread_unique_id(*port).map(|id| id as u32) == Ok(thread_id))
            .ok_or_else(|| FissionError::debug(format!("No thread {thread_id}")))
    }

    fn state_words(&self, thread_id: u32) -> FissionResult<Vec<u32>> {
        let port = self.thread_port(thread_id)?;
        let mut words = vec![0u32; regs::STATE_WORDS];
        mach::get_thread_state(port, regs::THREAD_STATE_FLAVOR, &mut words)
            .map_err(FissionError::debug)?;
        Ok(words)
    }

    fn main_thread_id(&self) -> u32 {
        self.target_pid.unwrap_or_default()
    }

    fn record_ttd_snapshot(&mut self, thread_id: u32, registers: RegisterState) {
        if let Some(timeline) = &self.ttd_timeline {
            if let Ok(mut timeline) = timeline.lock() {
                if timeline.is_recording() {
                    timeline.record_event(registers, thread_id);
                }
            }
        }
    }

    fn start_ttd(&self) {
        if let Some(timeline) = &self.ttd_timeline {
            if let Ok(mut timeline) = timeline.lock() {
                timeline.start_recording();
            }
        }
    }

    fn stop_ttd(&self) {
        if let Some(timeline) = &self.ttd_timeline {
            if let Ok(mut timeline) = timeline.lock() {
                timeline.stop_recording();
            }
        }
    }

    fn mark_gone(&mut self, status: DebugStatus) {
        self.release_ports();
        self.inserted.clear();
        self.target_pid = None;
        self.state.attached_pid = None;
        self.state.status = status;
        self.stop_ttd();
    }

    /// Turn a `waitpid` result into the event it means, updating the state.
    ///
    /// `None` means the stop was a trap the debugger absorbed and the target has
    /// been resumed; the caller keeps waiting.
    fn event_for_stop(&mut self, outcome: WaitOutcome) -> FissionResult<Option<DebugEvent>> {
        let pid = self.pid()?;
        match outcome {
            WaitOutcome::Exited(code) => {
                self.mark_gone(DebugStatus::Terminated);
                self.state.event_count = self.state.event_count.saturating_add(1);
                Ok(Some(DebugEvent::ProcessExited {
                    exit_code: code as u32,
                }))
            }
            WaitOutcome::Signaled(signal) => {
                self.mark_gone(DebugStatus::Terminated);
                self.state.event_count = self.state.event_count.saturating_add(1);
                Ok(Some(DebugEvent::ProcessExited {
                    exit_code: 128 + signal as u32,
                }))
            }
            WaitOutcome::Stopped(signal) => {
                self.state.status = DebugStatus::Suspended;
                self.state.event_count = self.state.event_count.saturating_add(1);
                self.refresh_threads()?;
                let mut words = self.state_words(pid)?;
                let mut pc = regs::program_counter(&words);
                let stepped_off_at = self.step_off_pc;
                self.state.last_thread_id = Some(pid);
                self.state.current_thread_id = Some(pid);

                if signal == SIGTRAP {
                    let candidate = pc.checked_sub(regs::PC_ADVANCE_AFTER_BREAKPOINT);
                    if let Some(address) = candidate.filter(|a| self.inserted.contains_key(a)) {
                        if pc != address {
                            // INT3 leaves the PC one byte past the breakpoint;
                            // put it back so the original instruction runs.
                            regs::set_program_counter(&mut words, address);
                            mach::set_thread_state(
                                self.thread_port(pid)?,
                                regs::THREAD_STATE_FLAVOR,
                                &words,
                            )
                            .map_err(FissionError::debug)?;
                            pc = address;
                        }
                        if let Some(breakpoint) = self.state.breakpoints.get_mut(&address) {
                            breakpoint.hits = breakpoint.hits.saturating_add(1);
                        }
                        let registers = regs::to_registers(&words);
                        self.state.registers = Some(registers.clone());
                        self.record_ttd_snapshot(pid, registers);
                        return Ok(Some(DebugEvent::BreakpointHit {
                            address: pc,
                            thread_id: pid,
                        }));
                    }
                    // Bounded, so a target genuinely trapping here is reported.
                    if stepped_off_at == Some(pc)
                        && self.absorbed_step_traps < MAX_ABSORBED_STEP_TRAPS
                    {
                        // The extra trap right after a step-off: nothing ran,
                        // so go on with the continue the caller asked for.
                        self.absorbed_step_traps += 1;
                        mach::ptrace_request(mach::PT_CONTINUE, pid, 1, 0).map_err(|errno| {
                            FissionError::debug(format!("Continue failed: errno {errno}"))
                        })?;
                        self.state.status = DebugStatus::Running;
                        return Ok(None);
                    }
                    self.step_off_pc = None;
                    let registers = regs::to_registers(&words);
                    self.state.registers = Some(registers.clone());
                    self.record_ttd_snapshot(pid, registers);
                    return Ok(Some(DebugEvent::SingleStep { thread_id: pid }));
                }

                self.step_off_pc = None;
                // Any other signal: report it and deliver it on resume. SIGSTOP
                // is not passed on: delivering it again would just stop the
                // target a second time, and a debugger resuming from a stop it
                // reported is what lifts it.
                self.pending_signal = if signal == SIGSTOP { 0 } else { signal };
                let registers = regs::to_registers(&words);
                self.state.registers = Some(registers.clone());
                self.record_ttd_snapshot(pid, registers);
                Ok(Some(DebugEvent::Exception {
                    code: signal as u32,
                    address: pc,
                    first_chance: true,
                }))
            }
        }
    }

    fn wait_blocking(&mut self, timeout: Duration) -> FissionResult<WaitOutcome> {
        let pid = self.pid()?;
        let start = Instant::now();
        loop {
            match mach::wait_pid(pid, mach::WNOHANG | mach::WUNTRACED) {
                Ok(Some(status)) => return Ok(mach::decode_wait_status(status)),
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        return Err(FissionError::debug("Timed out waiting for the target"));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(errno) => {
                    return Err(FissionError::debug(format!(
                        "waitpid failed: errno {errno}"
                    )));
                }
            }
        }
    }

    fn write_original(&self, address: u64) -> FissionResult<()> {
        let original = self
            .inserted
            .get(&address)
            .ok_or_else(|| FissionError::debug("Breakpoint not inserted"))?;
        mach::write_memory(self.task()?, address, original).map_err(FissionError::debug)
    }

    fn write_breakpoint(&self, address: u64) -> FissionResult<()> {
        mach::write_memory(self.task()?, address, regs::BREAKPOINT_INSTRUCTION)
            .map_err(FissionError::debug)
    }

    fn current_pc(&self) -> FissionResult<u64> {
        Ok(regs::program_counter(
            &self.state_words(self.main_thread_id())?,
        ))
    }

    /// Resume the target, first stepping off a breakpoint it is stopped on.
    ///
    /// Resuming with the breakpoint instruction still in place would trap on
    /// the same instruction forever; the original is put back for one step.
    /// Returns an event when that step already ended the run.
    fn resume(&mut self, step: bool) -> FissionResult<()> {
        let pid = self.pid()?;
        let signal = std::mem::take(&mut self.pending_signal);
        let pc = self.current_pc()?;
        let on_breakpoint = self.inserted.contains_key(&pc);

        if on_breakpoint {
            self.write_original(pc)?;
            mach::ptrace_request(mach::PT_STEP, pid, 1, signal)
                .map_err(|errno| FissionError::debug(format!("PT_STEP failed: errno {errno}")))?;
            self.state.status = DebugStatus::Running;
            let outcome = self.wait_blocking(Duration::from_secs(10))?;
            // The target may have exited or faulted during the step.
            if !matches!(outcome, WaitOutcome::Stopped(SIGTRAP)) {
                if let Some(event) = self.event_for_stop(outcome)? {
                    self.pending_events.push_back(event);
                }
                return Ok(());
            }
            self.write_breakpoint(pc)?;
            if step {
                if let Some(event) = self.event_for_stop(outcome)? {
                    self.pending_events.push_back(event);
                }
                return Ok(());
            }
            // Stepped off the breakpoint; now run.
            self.step_off_pc = Some(self.current_pc()?);
            self.absorbed_step_traps = 0;
            mach::ptrace_request(mach::PT_CONTINUE, pid, 1, 0)
                .map_err(|errno| FissionError::debug(format!("Continue failed: errno {errno}")))?;
        } else {
            let request = if step {
                mach::PT_STEP
            } else {
                mach::PT_CONTINUE
            };
            mach::ptrace_request(request, pid, 1, signal)
                .map_err(|errno| FissionError::debug(format!("Resume failed: errno {errno}")))?;
        }
        self.state.status = DebugStatus::Running;
        Ok(())
    }
}

impl Default for MacOSDebugger {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MacOSDebugger {
    fn drop(&mut self) {
        // A traced child that is simply abandoned stays stopped for ever.
        if let Some(pid) = self.target_pid {
            for address in self.inserted.keys().copied().collect::<Vec<_>>() {
                let _ = self.write_original(address);
            }
            let _ = mach::ptrace_request(mach::PT_DETACH, pid, 1, 0);
        }
        self.release_ports();
    }
}

/// Enumerate running processes on macOS
///
/// Uses sysctl to get process list (doesn't require special permissions)
pub fn enumerate_processes() -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    // Use ps command as a simple approach
    // A more robust implementation would use sysctl with KERN_PROC
    if let Ok(output) = std::process::Command::new("ps")
        .args(["-axo", "pid,comm"])
        .output()
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            for line in stdout.lines().skip(1) {
                let parts: Vec<&str> = line.trim().splitn(2, ' ').collect();
                if parts.len() >= 2 {
                    if let Ok(pid) = parts[0].trim().parse::<u32>() {
                        processes.push(ProcessInfo {
                            pid,
                            name: parts[1].trim().to_string(),
                            exe_path: None, // Would need to use proc_pidpath
                        });
                    }
                }
            }
        }
    }

    processes.sort_by_key(|p| p.pid);
    processes
}

/// A Mach-O image the target's dyld reports.
struct DyldImage {
    load_address: u64,
    path: String,
}

fn read_c_string(task: mach::mach_port_t, address: u64) -> String {
    let mut bytes = Vec::new();
    let mut cursor = address;
    while bytes.len() < 4096 {
        // Read up to the end of the page so a string at its end is not refused.
        let chunk = (0x1000 - (cursor & 0xfff)).min(256) as usize;
        let Ok(data) = mach::read_memory(task, cursor, chunk) else {
            break;
        };
        if let Some(end) = data.iter().position(|b| *b == 0) {
            bytes.extend_from_slice(&data[..end]);
            break;
        }
        bytes.extend_from_slice(&data);
        cursor += chunk as u64;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The images dyld has registered, from `dyld_all_image_infos`.
fn dyld_images(task: mach::mach_port_t) -> Result<Vec<DyldImage>, String> {
    let (info_address, _) = mach::dyld_info(task)?;
    if info_address == 0 {
        return Ok(Vec::new());
    }
    // version: u32, infoArrayCount: u32, infoArray: pointer.
    let header = mach::read_memory(task, info_address, 16)?;
    let count = u32::from_le_bytes(header[4..8].try_into().expect("4 bytes")) as usize;
    let array = u64::from_le_bytes(header[8..16].try_into().expect("8 bytes"));
    if count == 0 || array == 0 || count > 100_000 {
        return Ok(Vec::new());
    }
    // dyld_image_info: imageLoadAddress, imageFilePath, imageFileModDate (8 bytes each).
    let table = mach::read_memory(task, array, count * 24)?;
    Ok(table
        .chunks_exact(24)
        .map(|entry| {
            let load_address = u64::from_le_bytes(entry[0..8].try_into().expect("8 bytes"));
            let path_pointer = u64::from_le_bytes(entry[8..16].try_into().expect("8 bytes"));
            DyldImage {
                load_address,
                path: read_c_string(task, path_pointer),
            }
        })
        .collect())
}

/// One `LC_SEGMENT_64` of a Mach-O image.
#[derive(Debug, PartialEq, Eq)]
struct MachSegment {
    name: String,
    vm_address: u64,
    vm_size: u64,
    file_offset: u64,
    protection: u32,
}

/// Parse the 64-bit load commands that follow `header` (a `mach_header_64`).
fn parse_segments(header: &[u8], commands: &[u8]) -> Vec<MachSegment> {
    const LC_SEGMENT_64: u32 = 0x19;
    let ncmds = u32::from_le_bytes(header[16..20].try_into().expect("4 bytes")) as usize;
    let mut segments = Vec::new();
    let mut offset = 0usize;
    for _ in 0..ncmds {
        let Some(head) = commands.get(offset..offset + 8) else {
            break;
        };
        let command = u32::from_le_bytes(head[0..4].try_into().expect("4 bytes"));
        let size = u32::from_le_bytes(head[4..8].try_into().expect("4 bytes")) as usize;
        if size < 8 || offset + size > commands.len() {
            break;
        }
        if command == LC_SEGMENT_64 && size >= 72 {
            let body = &commands[offset..offset + size];
            let name_end = body[8..24].iter().position(|b| *b == 0).unwrap_or(16);
            segments.push(MachSegment {
                name: String::from_utf8_lossy(&body[8..8 + name_end]).into_owned(),
                vm_address: u64::from_le_bytes(body[24..32].try_into().expect("8 bytes")),
                vm_size: u64::from_le_bytes(body[32..40].try_into().expect("8 bytes")),
                file_offset: u64::from_le_bytes(body[40..48].try_into().expect("8 bytes")),
                protection: u32::from_le_bytes(body[60..64].try_into().expect("4 bytes")),
            });
        }
        offset += size;
    }
    segments
}

fn protection_string(protection: u32) -> String {
    let flag = |bit: u32, c: char| if protection & bit != 0 { c } else { '-' };
    format!("{}{}{}", flag(1, 'r'), flag(2, 'w'), flag(4, 'x'))
}

fn module_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

impl ExecutionBackend for MacOSDebugger {
    fn capabilities(&self) -> crate::debug::capabilities::DebugBackendCapabilities {
        use crate::debug::capabilities::{
            BackendAvailability, DebugBackendCapabilities, DebugBackendKind, DebugOperation,
            RuntimeRequirement,
        };

        DebugBackendCapabilities::new(
            DebugBackendKind::MacOsNative,
            BackendAvailability::Available,
            &[
                DebugOperation::ProcessEnumeration,
                DebugOperation::Launch,
                DebugOperation::Attach,
                DebugOperation::Detach,
                DebugOperation::ContinueExecution,
                DebugOperation::PollEvent,
                DebugOperation::SingleStep,
                DebugOperation::Terminate,
                DebugOperation::SoftwareBreakpoints,
                DebugOperation::RegisterRead,
                DebugOperation::RegisterWrite,
                DebugOperation::MemoryRead,
                DebugOperation::MemoryWrite,
                DebugOperation::ModuleList,
                DebugOperation::ThreadList,
            ],
        )
        // Launch and attach need the Mach task port, which macOS grants only
        // to an authorized caller; everything after depends on it.
        .conditionally_supporting(
            DebugOperation::Launch,
            &[
                RuntimeRequirement::PtracePolicyAllowsLaunch,
                RuntimeRequirement::MachTaskPortAccess,
            ],
        )
        .conditionally_supporting(
            DebugOperation::Attach,
            &[
                RuntimeRequirement::PtracePolicyAllowsAttach,
                RuntimeRequirement::MachTaskPortAccess,
            ],
        )
        .conditionally_supporting(
            DebugOperation::ModuleList,
            &[RuntimeRequirement::TargetProcessAllowsDebugging],
        )
    }

    fn set_timeline(&mut self, timeline: Arc<Mutex<Timeline>>) {
        self.ttd_timeline = Some(timeline);
    }

    fn enumerate_processes() -> Vec<ProcessInfo> {
        enumerate_processes()
    }

    fn attach(&mut self, pid: u32) -> FissionResult<()> {
        if self.is_attached() {
            return Err(FissionError::debug(
                "Detach from the current process before attaching to another one",
            ));
        }
        self.state.status = DebugStatus::Attaching;
        mach::ptrace_request(mach::PT_ATTACH, pid, 0, 0).map_err(|errno| {
            self.state.status = DebugStatus::Detached;
            FissionError::debug(format!(
                "Failed to attach to process {pid}: errno {errno} (the process may be \
                 hardened, owned by another user, or already traced)"
            ))
        })?;
        // PT_ATTACH stops the target with SIGSTOP.
        self.target_pid = Some(pid);
        if let Err(error) = self.wait_blocking(Duration::from_secs(10)) {
            let _ = mach::ptrace_request(mach::PT_DETACH, pid, 1, 0);
            self.target_pid = None;
            self.state.status = DebugStatus::Detached;
            return Err(error);
        }
        if let Err(error) = self
            .acquire_task_port(pid)
            .and_then(|()| self.refresh_threads())
        {
            let _ = mach::ptrace_request(mach::PT_DETACH, pid, 1, 0);
            self.release_ports();
            self.target_pid = None;
            self.state.status = DebugStatus::Detached;
            return Err(error);
        }
        self.state.attached_pid = Some(pid);
        self.state.main_thread_id = Some(pid);
        self.state.current_thread_id = Some(pid);
        self.state.status = DebugStatus::Suspended;
        self.state.last_event = Some(format!("Attached to PID {pid}"));
        self.start_ttd();
        Ok(())
    }

    fn launch(&mut self, path: &str, args: &[String]) -> FissionResult<u32> {
        if self.is_attached() {
            return Err(FissionError::debug(
                "Detach from the current process before launching another one",
            ));
        }
        let mut command = std::process::Command::new(path);
        command.args(args);
        // PT_TRACE_ME stops the child with SIGTRAP after exec, before it runs
        // any of its own code. The hook does nothing else.
        // SAFETY: only the async-signal-safe ptrace call runs between fork and exec.
        unsafe {
            command.pre_exec(|| {
                mach::ptrace_request(mach::PT_TRACE_ME, 0, 0, 0)
                    .map(|_| ())
                    .map_err(std::io::Error::from_raw_os_error)
            });
        }
        let child = command.spawn().map_err(|error| {
            FissionError::debug(format!("Failed to launch '{path}' under ptrace: {error}"))
        })?;
        let pid = child.id();
        self.target_pid = Some(pid);

        let kill = |this: &mut Self| {
            let _ = mach::ptrace_request(mach::PT_KILL, pid, 0, 0);
            let _ = mach::wait_pid(pid, 0);
            this.release_ports();
            this.target_pid = None;
        };

        match self.wait_blocking(Duration::from_secs(10)) {
            Ok(WaitOutcome::Stopped(SIGTRAP)) => {}
            Ok(other) => {
                kill(self);
                return Err(FissionError::debug(format!(
                    "Launched '{path}' did not enter the expected ptrace exec stop: {other:?}"
                )));
            }
            Err(error) => {
                kill(self);
                return Err(error);
            }
        }
        if let Err(error) = self
            .acquire_task_port(pid)
            .and_then(|()| self.refresh_threads())
        {
            kill(self);
            return Err(error);
        }
        let words = match self.state_words(pid) {
            Ok(words) => words,
            Err(error) => {
                kill(self);
                return Err(error);
            }
        };
        let registers = regs::to_registers(&words);
        let mut state = DebugState {
            attached_pid: Some(pid),
            main_thread_id: Some(pid),
            last_thread_id: Some(pid),
            current_thread_id: Some(pid),
            status: DebugStatus::Suspended,
            registers: Some(registers.clone()),
            last_event: Some(format!("Launched PID {pid} and stopped after exec")),
            ..DebugState::default()
        };
        state.threads = std::mem::take(&mut self.state.threads);
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
        self.start_ttd();
        self.record_ttd_snapshot(pid, registers);
        Ok(pid)
    }

    fn detach(&mut self) -> FissionResult<()> {
        let pid = self.pid()?;
        // Leave no breakpoint instruction behind in a process we let go of.
        for address in self.inserted.keys().copied().collect::<Vec<_>>() {
            self.write_original(address)?;
        }
        mach::ptrace_request(mach::PT_DETACH, pid, 1, 0).map_err(|errno| {
            FissionError::debug(format!(
                "Failed to detach from process {pid}: errno {errno}"
            ))
        })?;
        self.state.breakpoints.clear();
        self.mark_gone(DebugStatus::Detached);
        self.state.last_event = Some("Detached".to_string());
        Ok(())
    }

    fn terminate(&mut self) -> FissionResult<()> {
        let pid = self.pid()?;
        mach::ptrace_request(mach::PT_KILL, pid, 0, 0).map_err(|errno| {
            FissionError::debug(format!("Failed to kill process {pid}: errno {errno}"))
        })?;
        let _ = mach::wait_pid(pid, 0);
        self.state.breakpoints.clear();
        self.mark_gone(DebugStatus::Terminated);
        self.state.last_event = Some("Terminated".to_string());
        Ok(())
    }

    fn is_attached(&self) -> bool {
        self.target_pid.is_some()
    }

    fn attached_pid(&self) -> Option<u32> {
        self.target_pid
    }

    fn continue_execution(&mut self) -> FissionResult<()> {
        self.resume(false)
    }

    fn single_step(&mut self) -> FissionResult<()> {
        self.resume(true)
    }

    fn poll_event(&mut self, timeout_ms: u32) -> FissionResult<Option<DebugEvent>> {
        if let Some(event) = self.pending_events.pop_front() {
            self.state.event_count = self.state.event_count.saturating_add(1);
            return Ok(Some(event));
        }
        let pid = self.pid()?;
        let start = Instant::now();
        let timeout = Duration::from_millis(u64::from(timeout_ms));
        loop {
            match mach::wait_pid(pid, mach::WNOHANG | mach::WUNTRACED) {
                Ok(Some(status)) => {
                    let outcome = mach::decode_wait_status(status);
                    if let Some(event) = self.event_for_stop(outcome)? {
                        return Ok(Some(event));
                    }
                }
                Ok(None) => {
                    if timeout_ms == 0 || start.elapsed() >= timeout {
                        return Ok(None);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(errno) => {
                    return Err(FissionError::debug(format!(
                        "waitpid failed: errno {errno}"
                    )));
                }
            }
        }
    }

    fn set_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        let task = self.task()?;
        if self.inserted.contains_key(&address) {
            return Err(FissionError::debug(format!(
                "A breakpoint already exists at 0x{address:x}"
            )));
        }
        let original = mach::read_memory(task, address, regs::BREAKPOINT_INSTRUCTION.len())
            .map_err(|e| FissionError::debug(format!("Failed to read 0x{address:x}: {e}")))?;
        if original == regs::BREAKPOINT_INSTRUCTION {
            return Err(FissionError::debug(format!(
                "0x{address:x} already holds a breakpoint instruction"
            )));
        }
        self.inserted.insert(address, original.clone());
        if let Err(error) = self.write_breakpoint(address) {
            self.inserted.remove(&address);
            return Err(FissionError::debug(format!(
                "Failed to write breakpoint at 0x{address:x}: {error}"
            )));
        }
        self.state.breakpoints.insert(
            address,
            Breakpoint {
                address,
                original_byte: original[0],
                enabled: true,
                temporary: false,
                kind: BreakpointKind::Software,
                hits: 0,
                condition: None,
            },
        );
        self.state.last_event = Some(format!("Breakpoint set at 0x{address:016x}"));
        Ok(())
    }

    fn remove_sw_breakpoint(&mut self, address: u64) -> FissionResult<()> {
        if !self.inserted.contains_key(&address) {
            return Err(FissionError::debug("Breakpoint not found"));
        }
        self.write_original(address)?;
        self.inserted.remove(&address);
        self.state.breakpoints.remove(&address);
        self.state.last_event = Some(format!("Breakpoint removed at 0x{address:016x}"));
        Ok(())
    }

    fn read_memory(&self, address: u64, size: usize) -> FissionResult<Vec<u8>> {
        let mut bytes = mach::read_memory(self.task()?, address, size).map_err(|e| {
            FissionError::debug(format!("Failed to read {size} bytes at 0x{address:x}: {e}"))
        })?;
        // The target's memory shows our breakpoint instructions; callers want
        // the program's bytes.
        for (&breakpoint, original) in &self.inserted {
            for (index, byte) in original.iter().enumerate() {
                let at = breakpoint + index as u64;
                if at >= address && at < address + size as u64 {
                    bytes[(at - address) as usize] = *byte;
                }
            }
        }
        Ok(bytes)
    }

    fn write_memory(&mut self, address: u64, data: &[u8]) -> FissionResult<()> {
        mach::write_memory(self.task()?, address, data).map_err(|e| {
            FissionError::debug(format!(
                "Failed to write {} bytes at 0x{address:x}: {e}",
                data.len()
            ))
        })
    }

    fn fetch_registers(&mut self, thread_id: u32) -> FissionResult<RegisterState> {
        Ok(regs::to_registers(&self.state_words(thread_id)?))
    }

    fn set_registers(&mut self, thread_id: u32, registers: &RegisterState) -> FissionResult<()> {
        let mut words = self.state_words(thread_id)?;
        regs::apply_registers(&mut words, registers);
        mach::set_thread_state(
            self.thread_port(thread_id)?,
            regs::THREAD_STATE_FLAVOR,
            &words,
        )
        .map_err(FissionError::debug)?;
        self.state.registers = Some(regs::to_registers(&words));
        Ok(())
    }

    fn list_breakpoints(&self) -> Vec<Breakpoint> {
        let mut breakpoints: Vec<Breakpoint> = self.state.breakpoints.values().cloned().collect();
        breakpoints.sort_by_key(|breakpoint| breakpoint.address);
        breakpoints
    }

    fn list_modules(&mut self) -> FissionResult<ModuleListReport> {
        let pid = self.pid()?;
        let task = self.task()?;
        let mut diagnostics = Vec::new();
        let images = dyld_images(task).map_err(FissionError::debug)?;
        if images.is_empty() {
            diagnostics.push(
                "dyld has not registered any images yet (the target is stopped before dyld \
                 ran); continue past dyld and list again"
                    .to_string(),
            );
        }

        let mut modules = Vec::with_capacity(images.len());
        for image in images {
            let header = mach::read_memory(task, image.load_address, 32);
            let Ok(header) = header else {
                diagnostics.push(format!("{}: could not read its Mach-O header", image.path));
                continue;
            };
            let sizeofcmds = u32::from_le_bytes(header[20..24].try_into().expect("4 bytes"));
            let commands = mach::read_memory(task, image.load_address + 32, sizeofcmds as usize)
                .unwrap_or_default();
            let segments = parse_segments(&header, &commands);
            let text = segments.iter().find(|segment| segment.name == "__TEXT");
            let transform = match text {
                Some(text) => ModuleAddressTransform::Resolved {
                    format: "Mach-O".to_string(),
                    analysis_address_domain: "preferred __TEXT vmaddr".to_string(),
                    runtime_address_domain: "process virtual address".to_string(),
                    formula: "runtime = analysis + slide".to_string(),
                    load_bias: image.load_address.wrapping_sub(text.vm_address) as i64,
                },
                None => ModuleAddressTransform::Unavailable {
                    reason: "no __TEXT segment in the image's load commands".to_string(),
                },
            };
            let slide = text
                .map(|text| image.load_address.wrapping_sub(text.vm_address))
                .unwrap_or(0);
            let mappings = segments
                .iter()
                .filter(|segment| segment.vm_size > 0 && segment.name != "__PAGEZERO")
                .map(|segment| ProcessMemoryMapping {
                    runtime_start: segment.vm_address.wrapping_add(slide),
                    runtime_end: segment.vm_address.wrapping_add(slide) + segment.vm_size,
                    file_offset: Some(segment.file_offset),
                    permissions: Some(protection_string(segment.protection)),
                    path: Some(image.path.clone()),
                })
                .collect();
            modules.push(ProcessModule {
                name: module_name(&image.path),
                path: image.path,
                device: None,
                inode: None,
                mappings,
                address_transform: transform,
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

        self.state.modules = modules
            .iter()
            .map(|module| {
                let base = module
                    .mappings
                    .iter()
                    .map(|mapping| mapping.runtime_start)
                    .min()
                    .unwrap_or_default();
                let end = module
                    .mappings
                    .iter()
                    .map(|mapping| mapping.runtime_end)
                    .max()
                    .unwrap_or(base);
                (
                    base,
                    crate::debug::types::ModuleInfo {
                        base_address: base,
                        size: end.saturating_sub(base),
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
            other_mappings: Vec::new(),
            diagnostics,
        })
    }

    fn get_state(&self) -> DebugState {
        self.state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment_command(
        name: &str,
        vmaddr: u64,
        vmsize: u64,
        fileoff: u64,
        initprot: u32,
    ) -> Vec<u8> {
        let mut command = vec![0u8; 72];
        command[0..4].copy_from_slice(&0x19u32.to_le_bytes());
        command[4..8].copy_from_slice(&72u32.to_le_bytes());
        command[8..8 + name.len()].copy_from_slice(name.as_bytes());
        command[24..32].copy_from_slice(&vmaddr.to_le_bytes());
        command[32..40].copy_from_slice(&vmsize.to_le_bytes());
        command[40..48].copy_from_slice(&fileoff.to_le_bytes());
        command[60..64].copy_from_slice(&initprot.to_le_bytes());
        command
    }

    #[test]
    fn segments_come_out_of_the_load_commands_in_order() {
        let mut commands = Vec::new();
        commands.extend(segment_command("__PAGEZERO", 0, 0x1_0000_0000, 0, 0));
        commands.extend(segment_command("__TEXT", 0x1_0000_0000, 0x4000, 0, 5));
        // A command that is not a segment is skipped by its size.
        let mut other = vec![0u8; 24];
        other[0..4].copy_from_slice(&0x2u32.to_le_bytes());
        other[4..8].copy_from_slice(&24u32.to_le_bytes());
        commands.extend(other);
        commands.extend(segment_command("__DATA", 0x1_0000_4000, 0x4000, 0x4000, 3));
        let mut header = vec![0u8; 32];
        header[16..20].copy_from_slice(&4u32.to_le_bytes());

        let segments = parse_segments(&header, &commands);
        assert_eq!(
            segments.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["__PAGEZERO", "__TEXT", "__DATA"]
        );
        assert_eq!(segments[1].vm_address, 0x1_0000_0000);
        assert_eq!(segments[2].file_offset, 0x4000);
        assert_eq!(protection_string(segments[1].protection), "r-x");
        assert_eq!(protection_string(segments[2].protection), "rw-");
    }

    #[test]
    fn a_truncated_command_list_stops_instead_of_reading_past_it() {
        let mut commands = segment_command("__TEXT", 0x1000, 0x1000, 0, 5);
        commands.truncate(40);
        let mut header = vec![0u8; 32];
        header[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert!(parse_segments(&header, &commands).is_empty());
    }

    #[test]
    fn module_names_are_the_last_path_component() {
        assert_eq!(
            module_name("/usr/lib/libSystem.B.dylib"),
            "libSystem.B.dylib"
        );
        assert_eq!(module_name("main"), "main");
    }

    #[test]
    fn an_unattached_debugger_refuses_everything_with_one_message() {
        let mut debugger = MacOSDebugger::new();
        assert!(!debugger.is_attached());
        assert!(debugger.continue_execution().is_err());
        assert!(debugger.single_step().is_err());
        assert!(debugger.read_memory(0x1000, 4).is_err());
        assert!(debugger.fetch_registers(1).is_err());
        assert!(debugger.set_sw_breakpoint(0x1000).is_err());
        assert!(debugger.detach().is_err());
        assert!(debugger.terminate().is_err());
    }
}
