//! Linux-specific debugger implementation using ptrace.
//!
//! This module provides debugging capabilities on Linux using the ptrace system call.

use super::traits::Debugger;
use super::types::{DebugState, DebugStatus, ProcessInfo, RegisterState};
use fission_core::{FissionError, Result as FissionResult};

/// Linux debugger implementation using ptrace
pub struct LinuxDebugger {
    /// Current debug state
    state: DebugState,
    /// Target process ID
    target_pid: Option<u32>,
}

impl LinuxDebugger {
    /// Create a new Linux debugger instance
    pub fn new() -> Self {
        Self {
            state: DebugState::default(),
            target_pid: None,
        }
    }

    /// Get current state
    pub fn state(&self) -> &DebugState {
        &self.state
    }
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

impl Debugger for LinuxDebugger {
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

        Ok(())
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

        let bp = super::types::Breakpoint {
            address,
            original_byte,
            enabled: true,
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
            format!(
                "Failed to write {} bytes at 0x{:x}: {}",
                data.len(),
                address,
                e
            )
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

        Ok(RegisterState {
            rax: regs.rax,
            rbx: regs.rbx,
            rcx: regs.rcx,
            rdx: regs.rdx,
            rsi: regs.rsi,
            rdi: regs.rdi,
            rbp: regs.rbp,
            rsp: regs.rsp,
            r8: regs.r8,
            r9: regs.r9,
            r10: regs.r10,
            r11: regs.r11,
            r12: regs.r12,
            r13: regs.r13,
            r14: regs.r14,
            r15: regs.r15,
            rip: regs.rip,
            rflags: regs.eflags,
        })
    }
}
