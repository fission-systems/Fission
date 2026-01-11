//! RR Trace Management and Debugger Implementation
//!
//! Provides recording and replay of program execution using rr.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use super::gdb_mi::{GdbMiParser, MiResponse, MiValue};
use crate::debug::traits::TimeTravelDebugger;
use crate::debug::ttd::ExecutionSnapshot;
use crate::debug::types::RegisterState;

/// RR debugger state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RRState {
    /// Not connected to any trace
    Disconnected,
    /// Recording a new trace
    Recording,
    /// Replaying an existing trace
    Replaying,
    /// Paused at a specific point
    Paused,
    /// Process has terminated
    Terminated,
}

/// Information about an RR trace
#[derive(Debug, Clone)]
pub struct TraceInfo {
    /// Path to the trace directory
    pub path: PathBuf,
    /// Name of the recorded binary
    pub binary_name: String,
    /// Number of events in the trace
    pub event_count: u64,
    /// Recording timestamp
    pub timestamp: String,
}

/// RR Debugger - Integrates with Mozilla's Record and Replay debugger
///
/// # Linux Only
///
/// RR only works on Linux. On other platforms, this will return errors.
#[derive(Debug)]
pub struct RRDebugger {
    /// Path to the trace directory
    trace_dir: Option<PathBuf>,
    /// GDB subprocess (rr replay runs under GDB)
    gdb_process: Option<Child>,
    /// GDB/MI protocol parser
    mi_parser: GdbMiParser,
    /// Current state
    state: RRState,
    /// Current position in timeline
    current_position: u64,
    /// Maximum position (total events)
    max_position: u64,
    /// Last known registers
    last_registers: RegisterState,
    /// Command token counter
    token_counter: u32,
}

impl RRDebugger {
    /// Create a new RR debugger (disconnected state)
    pub fn new() -> Self {
        Self {
            trace_dir: None,
            gdb_process: None,
            mi_parser: GdbMiParser::new(),
            state: RRState::Disconnected,
            current_position: 0,
            max_position: 0,
            last_registers: RegisterState::default(),
            token_counter: 0,
        }
    }

    /// Check if rr is available on this system
    pub fn is_available() -> bool {
        #[cfg(target_os = "linux")]
        {
            Command::new("rr")
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Get the path to the latest RR trace
    pub fn latest_trace() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        let latest = PathBuf::from(home).join(".rr").join("latest-trace");
        if latest.exists() { Some(latest) } else { None }
    }

    /// List all available traces
    pub fn list_traces() -> Vec<TraceInfo> {
        let mut traces = Vec::new();

        if let Ok(home) = std::env::var("HOME") {
            let rr_dir = PathBuf::from(home).join(".rr");
            if let Ok(entries) = std::fs::read_dir(&rr_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();

                        // Skip symlinks like "latest-trace"
                        if name.starts_with("latest") {
                            continue;
                        }

                        traces.push(TraceInfo {
                            path: path.clone(),
                            binary_name: name,
                            event_count: 0, // Would need to parse trace
                            timestamp: String::new(),
                        });
                    }
                }
            }
        }

        traces
    }

    /// Record a new trace
    #[cfg(target_os = "linux")]
    pub fn record<P: AsRef<Path>>(binary: P, args: &[&str]) -> Result<PathBuf, String> {
        let mut cmd = Command::new("rr");
        cmd.arg("record").arg(binary.as_ref()).args(args);

        let status = cmd
            .status()
            .map_err(|e| format!("Failed to start rr record: {}", e))?;

        if status.success() {
            Self::latest_trace().ok_or_else(|| "No trace created".to_string())
        } else {
            Err(format!("rr record failed with status: {}", status))
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn record<P: AsRef<Path>>(_binary: P, _args: &[&str]) -> Result<PathBuf, String> {
        Err("RR is only available on Linux".to_string())
    }

    /// Start replaying a trace
    #[cfg(target_os = "linux")]
    pub fn replay<P: AsRef<Path>>(&mut self, trace_dir: P) -> Result<(), String> {
        let trace_path = trace_dir.as_ref().to_path_buf();

        if !trace_path.exists() {
            return Err(format!("Trace directory does not exist: {:?}", trace_path));
        }

        // Start rr replay with GDB in MI mode
        let mut child = Command::new("rr")
            .arg("replay")
            .arg("-i") // MI mode
            .arg(&trace_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start rr replay: {}", e))?;

        // Wait for initial prompt
        self.gdb_process = Some(child);
        self.trace_dir = Some(trace_path);
        self.state = RRState::Replaying;

        // Send initial setup commands
        self.send_command("set confirm off")?;
        self.send_command("set pagination off")?;

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn replay<P: AsRef<Path>>(&mut self, _trace_dir: P) -> Result<(), String> {
        Err("RR is only available on Linux".to_string())
    }

    /// Send a GDB/MI command
    fn send_command(&mut self, cmd: &str) -> Result<Vec<MiResponse>, String> {
        let gdb = self.gdb_process.as_mut().ok_or("No GDB process")?;

        let stdin = gdb.stdin.as_mut().ok_or("No stdin")?;

        self.token_counter += 1;
        let full_cmd = format!("{}-{}\n", self.token_counter, cmd);

        stdin
            .write_all(full_cmd.as_bytes())
            .map_err(|e| format!("Failed to write command: {}", e))?;
        stdin
            .flush()
            .map_err(|e| format!("Failed to flush: {}", e))?;

        // Read responses until we get a result
        self.read_until_result()
    }

    /// Read responses until we get a result record
    fn read_until_result(&mut self) -> Result<Vec<MiResponse>, String> {
        let gdb = self.gdb_process.as_mut().ok_or("No GDB process")?;

        let stdout = gdb.stdout.as_mut().ok_or("No stdout")?;

        let mut reader = BufReader::new(stdout);
        let mut responses = Vec::new();

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    self.mi_parser.feed(&line);
                    let parsed = self.mi_parser.parse();

                    for resp in parsed {
                        let is_result = matches!(&resp, MiResponse::Result { .. });
                        responses.push(resp);

                        if is_result {
                            return Ok(responses);
                        }
                    }
                }
                Err(e) => return Err(format!("Read error: {}", e)),
            }
        }

        Ok(responses)
    }

    /// Parse register state from GDB/MI response
    fn parse_registers(
        &self,
        _results: &std::collections::HashMap<String, MiValue>,
    ) -> RegisterState {
        // TODO: Parse actual register values from MI response
        RegisterState::default()
    }

    /// Get current execution state
    pub fn state(&self) -> RRState {
        self.state
    }

    /// Stop replay and disconnect
    pub fn disconnect(&mut self) {
        if let Some(mut process) = self.gdb_process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
        self.state = RRState::Disconnected;
        self.trace_dir = None;
    }
}

impl Default for RRDebugger {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RRDebugger {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl TimeTravelDebugger for RRDebugger {
    fn start_recording(&mut self) -> Result<(), String> {
        Err("Use RRDebugger::record() static method to record a new trace".to_string())
    }

    fn stop_recording(&mut self) -> Result<(), String> {
        Err("Recording is handled externally by rr".to_string())
    }

    fn is_recording(&self) -> bool {
        self.state == RRState::Recording
    }

    fn is_replay_mode(&self) -> bool {
        matches!(self.state, RRState::Replaying | RRState::Paused)
    }

    fn seek_to(&mut self, position: u64) -> Result<ExecutionSnapshot, String> {
        if !self.is_replay_mode() {
            return Err("Not in replay mode".to_string());
        }

        // Use rr's "when" command to get current event, and "run <N>" to seek
        let cmd = format!("interpreter-exec mi \"run {}\"", position);
        let _responses = self.send_command(&cmd)?;

        self.current_position = position;

        Ok(ExecutionSnapshot::new(
            position,
            self.last_registers.clone(),
            0,
        ))
    }

    fn reverse_step(&mut self) -> Result<ExecutionSnapshot, String> {
        if !self.is_replay_mode() {
            return Err("Not in replay mode".to_string());
        }

        let _responses = self.send_command("reverse-stepi")?;
        self.current_position = self.current_position.saturating_sub(1);

        // Fetch registers after step
        let _reg_resp = self.send_command("-data-list-register-values x")?;

        Ok(ExecutionSnapshot::new(
            self.current_position,
            self.last_registers.clone(),
            0,
        ))
    }

    fn reverse_continue(&mut self) -> Result<ExecutionSnapshot, String> {
        if !self.is_replay_mode() {
            return Err("Not in replay mode".to_string());
        }

        let _responses = self.send_command("reverse-continue")?;

        // Position will be updated from async response
        Ok(ExecutionSnapshot::new(
            self.current_position,
            self.last_registers.clone(),
            0,
        ))
    }

    fn forward_step(&mut self) -> Result<ExecutionSnapshot, String> {
        if !self.is_replay_mode() {
            return Err("Not in replay mode".to_string());
        }

        let _responses = self.send_command("-exec-step-instruction")?;
        self.current_position += 1;

        Ok(ExecutionSnapshot::new(
            self.current_position,
            self.last_registers.clone(),
            0,
        ))
    }

    fn forward_continue(&mut self) -> Result<ExecutionSnapshot, String> {
        if !self.is_replay_mode() {
            return Err("Not in replay mode".to_string());
        }

        let _responses = self.send_command("-exec-continue")?;

        Ok(ExecutionSnapshot::new(
            self.current_position,
            self.last_registers.clone(),
            0,
        ))
    }

    fn current_position(&self) -> Option<u64> {
        if self.is_replay_mode() {
            Some(self.current_position)
        } else {
            None
        }
    }

    fn current_snapshot(&self) -> Option<&ExecutionSnapshot> {
        None // RR doesn't store snapshots in memory
    }

    fn timeline_range(&self) -> Option<(u64, u64)> {
        if self.is_replay_mode() && self.max_position > 0 {
            Some((0, self.max_position))
        } else {
            None
        }
    }

    fn step_count(&self) -> usize {
        self.max_position as usize
    }

    fn clear_timeline(&mut self) {
        // Cannot clear RR traces from here
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rr_availability() {
        // Just check the function works
        let _available = RRDebugger::is_available();
    }

    #[test]
    fn test_rr_new() {
        let rr = RRDebugger::new();
        assert_eq!(rr.state(), RRState::Disconnected);
    }
}
