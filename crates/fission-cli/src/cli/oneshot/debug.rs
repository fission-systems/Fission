//! Debugger CLI dispatch across the selected native or emulator backend.
//!
//! One-shot model: each command loads the session state from a private directory,
//! executes the operation, prints output, and persists state back.

use crate::cli::args::DebugCommand;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

mod process_identity;
use process_identity::process_identity;

fn print_capabilities_text(report: &fission_dynamic::debug::DebugBackendCapabilities) {
    println!("Backend: {:?}", report.backend);
    println!("Host: {} ({})", report.host.os, report.host.architecture);
    match report.availability {
        fission_dynamic::debug::BackendAvailability::Available => {
            println!("Availability: available");
        }
        fission_dynamic::debug::BackendAvailability::Unavailable { reason } => {
            println!("Availability: unavailable ({reason:?})");
        }
    }
    println!("Operations:");
    for capability in &report.operations {
        let support = match &capability.support {
            fission_dynamic::debug::OperationSupport::Supported => "supported".to_string(),
            fission_dynamic::debug::OperationSupport::Conditional { requirements } => {
                format!("conditional ({requirements:?})")
            }
            fission_dynamic::debug::OperationSupport::Unsupported { reason } => {
                format!("unsupported ({reason:?})")
            }
        };
        println!("  {:?}: {support}", capability.operation);
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct DebugStateFile {
    pid: u32,
    #[serde(default)]
    process_identity: Option<String>,
    last_thread_id: Option<u32>,
    breakpoints: Vec<u64>,
}

impl DebugStateFile {
    fn verify_process(&self) -> Result<()> {
        let expected = self.process_identity.as_deref().context(
            "This debug session has no process identity. Run `fission_cli debug attach <pid>` again.",
        )?;
        let current = process_identity(self.pid).with_context(|| {
            format!(
                "cannot identify process {} for this debug session",
                self.pid
            )
        })?;
        anyhow::ensure!(
            current == expected,
            "Debug session for PID {} expired. Run `fission_cli debug attach <pid>` again.",
            self.pid
        );
        Ok(())
    }
}

/// Where one-shot sessions keep their state between invocations.
#[cfg(unix)]
fn state_dir() -> Result<PathBuf> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: geteuid takes no arguments and does not access caller memory.
    let owner = unsafe { libc::geteuid() };
    let temp = std::env::temp_dir().canonicalize()?;
    for parent in temp.ancestors() {
        let meta = std::fs::metadata(parent)?;
        anyhow::ensure!(
            (meta.uid() == owner || meta.uid() == 0)
                && (meta.mode() & 0o022 == 0 || meta.mode() & 0o1000 != 0),
            "Debugger session path has an untrusted parent: {}",
            parent.display()
        );
    }
    let dir = temp.join(format!("fission-debug-{owner}"));
    ensure_private_state_dir(&dir, owner)?;
    Ok(dir)
}

#[cfg(unix)]
fn ensure_private_state_dir(dir: &Path, owner: u32) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).context("create debugger session directory"),
    }
    let meta = std::fs::symlink_metadata(dir)?;
    anyhow::ensure!(
        meta.is_dir() && meta.uid() == owner && meta.mode() & 0o077 == 0,
        "Debugger session directory must be private and owned by the current user: {}",
        dir.display()
    );
    Ok(())
}

#[cfg(not(unix))]
fn state_dir() -> Result<PathBuf> {
    let dir = fission_core::core::resource_roots::fission_cache_dir()
        .context("No user cache directory for debugger sessions")?
        .join("debug-sessions");
    std::fs::create_dir_all(&dir)?;
    anyhow::ensure!(
        std::fs::symlink_metadata(&dir)?.is_dir(),
        "Invalid debugger session directory"
    );
    Ok(dir)
}

fn state_path_in(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("fission-debug-{pid}.json"))
}

/// Write the state file so that a reader sees the old file or the new one,
/// never a truncated one: the bytes go to a sibling file that is then renamed
/// over the target. `File::create` on the target itself empties it first, and
/// a `debug` command running at that moment failed to parse an empty file.
fn save_state_in(dir: &Path, state: &DebugStateFile) -> Result<()> {
    let path = state_path_in(dir, state.pid);
    let data = serde_json::to_string_pretty(state)?;
    // The leading dot keeps the scratch file out of `state_files_in`.
    let scratch = dir.join(format!(
        ".fission-debug-{}.json.{}.tmp",
        state.pid,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&scratch);
    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&scratch)?;
        file.write_all(data.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&scratch, &path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&scratch);
    }
    written.with_context(|| format!("write debug session state to {}", path.display()))
}

fn save_state(state: &DebugStateFile) -> Result<()> {
    save_state_in(&state_dir()?, state)
}

fn remove_state(dir: &Path, pid: u32) {
    let _ = std::fs::remove_file(state_path_in(dir, pid));
}

/// The pid a state file belongs to, if `name` is exactly
/// `fission-debug-<pid>.json`.
fn state_file_pid(name: &str) -> Option<u32> {
    let pid = name.strip_prefix("fission-debug-")?.strip_suffix(".json")?;
    // `parse` alone accepts a leading `+`.
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    pid.parse().ok()
}

/// The session state files in `dir`, most recently written first.
///
/// Only regular files named `fission-debug-<pid>.json` count. The order is
/// fixed so that the active session is the one written most recently.
fn state_files_in(dir: &Path) -> Result<Vec<(u32, PathBuf)>> {
    let mut files: Vec<(std::time::SystemTime, u32, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)?.filter_map(|e| e.ok()) {
        let Some(pid) = entry.file_name().to_str().and_then(state_file_pid) else {
            continue;
        };
        // `metadata` follows a symbolic link; `file_type` does not, and a link
        // is not a state file this command wrote.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        files.push((modified, pid, entry.path()));
    }
    files.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    Ok(files
        .into_iter()
        .map(|(_, pid, path)| (pid, path))
        .collect())
}

fn find_active_state_in(dir: &Path) -> Result<DebugStateFile> {
    let Some((pid, path)) = state_files_in(dir)?.into_iter().next() else {
        anyhow::bail!("No active debug session. Run `fission_cli debug attach <pid>` first.");
    };
    read_state(pid, &path)
}

fn read_state(pid: u32, path: &Path) -> Result<DebugStateFile> {
    let data = std::fs::read_to_string(path)
        .with_context(|| format!("read debug session state {}", path.display()))?;
    let state: DebugStateFile = serde_json::from_str(&data)
        .with_context(|| format!("debug session state {} is not valid", path.display()))?;
    anyhow::ensure!(
        pid == state.pid,
        "Debug session PID does not match {}",
        path.display()
    );
    Ok(state)
}

fn find_active_state() -> Result<DebugStateFile> {
    find_active_state_in(&state_dir()?)
}

/// Hex digits to bytes; whitespace and commas between them are ignored.
///
/// Works on bytes, not on `str` indices: slicing two bytes at a time out of
/// the text panicked when a multi-byte character straddled the cut (`"0é0"`
/// is four bytes, an even length, and not hex).
pub(super) fn hex_bytes_from_str(s: &str) -> Result<Vec<u8>> {
    let digits: Vec<u8> = s
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b',')
        .collect();
    if digits.len() % 2 != 0 {
        anyhow::bail!("hex data length must be even (got {})", digits.len());
    }
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            Some((high * 16 + low) as u8)
        })
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(|| anyhow::anyhow!("Invalid hex data: {}", s))
}

/// `value` as a JSON string literal, quotes included.
///
/// A name typed on the command line goes into `--json` output; pasted between
/// two quote characters as it was, a `"` or a backslash in it ended the string
/// early and the line was no longer JSON.
fn json_string(value: &str) -> String {
    serde_json::Value::from(value).to_string()
}

fn dll_breakpoint_json(action: &str, dll: &str) -> String {
    format!(
        "{{\"status\":\"ok\",\"action\":{},\"dll\":{}}}",
        json_string(action),
        json_string(dll)
    )
}

fn set_register_json(register: &str, value: u64) -> String {
    format!(
        "{{\"status\":\"ok\",\"action\":\"set_register\",\"register\":{},\"value\":\"0x{:x}\"}}",
        json_string(register),
        value
    )
}

fn get_flag_json(flag: &str, value: bool) -> String {
    format!(
        "{{\"status\":\"ok\",\"flag\":{},\"value\":{}}}",
        json_string(flag),
        value
    )
}

fn set_flag_json(flag: &str, value: bool) -> String {
    format!(
        "{{\"status\":\"ok\",\"action\":\"set_flag\",\"flag\":{},\"value\":{}}}",
        json_string(flag),
        value
    )
}

fn print_regs(regs: &fission_dynamic::debug::types::RegisterState, json: bool) {
    // Whatever the machine reported, named the way it named it. Printing a
    // fixed x86-64 list showed sixteen zeroes for an aarch64 target and gave
    // no sign that the names were the problem.
    if json {
        let mut obj = serde_json::Map::new();
        obj.insert("pc".to_string(), serde_json::json!(regs.pc));
        for (name, value) in regs.iter() {
            obj.insert(name.to_ascii_lowercase(), serde_json::json!(value));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Object(obj)).unwrap()
        );
        return;
    }

    println!("PC ={:016x}", regs.pc);
    let mut column = 0;
    for (name, value) in regs.iter() {
        print!("{name:<4}={value:016x}");
        column += 1;
        if column % 4 == 0 {
            println!();
        } else {
            print!(" ");
        }
    }
    if column % 4 != 0 {
        println!();
    }
}

fn print_hex_dump(addr: u64, data: &[u8], json: bool) {
    if json {
        let hex: String = data.iter().map(|b| format!("{:02x}", b)).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "address": addr,
                "size": data.len(),
                "hex": hex,
            }))
            .unwrap()
        );
    } else {
        for (i, chunk) in data.chunks(16).enumerate() {
            let line_addr = addr + (i * 16) as u64;
            let hex_part: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
            let ascii_part: String = chunk
                .iter()
                .map(|&b| {
                    if b.is_ascii_graphic() || b == b' ' {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();
            println!(
                "{:016x}  {:47}  {}",
                line_addr,
                hex_part.join(" "),
                ascii_part
            );
        }
    }
}

pub fn run_debug_command(args: crate::cli::args::DebugArgs) -> Result<()> {
    let emulator = args.emulator;
    let build_session = || {
        let mut builder = fission_dynamic::debug::DebugSession::new();
        if emulator {
            builder = builder.with_emulator();
        }
        builder.build()
    };

    // The active session's state and a backend attached to its process.
    //
    // Every command that works on a session goes through this, so none of
    // them can validate that a session exists and then talk to a backend that
    // was never attached to it -- which six of them did, and answered "Not
    // attached" to a session that was there.
    let attach_saved = |state: &DebugStateFile| -> Result<fission_dynamic::debug::DebugSession> {
        state.verify_process()?;
        let mut session = build_session();
        session.attach(state.pid)?;
        state.verify_process()?;
        Ok(session)
    };
    let attach_active = || -> Result<(DebugStateFile, fission_dynamic::debug::DebugSession)> {
        anyhow::ensure!(
            !emulator,
            "Emulated debug sessions exist only in the process that launched them"
        );
        let state = find_active_state()?;
        let session = attach_saved(&state)?;
        Ok((state, session))
    };

    match args.command {
        DebugCommand::Capabilities(capability_args) => {
            // Constructing a backend is side-effect free. In particular, this
            // path never launches or attaches to a target.
            let session = build_session();
            let report = session.debugger.capabilities();
            if capability_args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_capabilities_text(&report);
            }
            Ok(())
        }

        DebugCommand::Attach(args) => {
            let mut session = build_session();
            session.attach(args.pid).with_context(|| {
                format!(
                    "Failed to attach to PID {}. Is the process running?",
                    args.pid
                )
            })?;

            let state = DebugStateFile {
                pid: args.pid,
                process_identity: Some(process_identity(args.pid)?),
                last_thread_id: session.debugger.get_state().main_thread_id,
                breakpoints: Vec::new(),
            };
            save_state(&state)?;

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "status": "attached",
                        "pid": args.pid,
                        "main_thread_id": state.last_thread_id,
                    }))?
                );
            } else {
                println!("Attached to PID {}", args.pid);
                if let Some(tid) = state.last_thread_id {
                    println!("Main thread: {}", tid);
                }
            }
            Ok(())
        }

        DebugCommand::Detach => {
            anyhow::ensure!(
                !emulator,
                "Emulated debug sessions exist only in the process that launched them"
            );
            let dir = state_dir()?;
            let sessions = state_files_in(&dir)?;
            if sessions.is_empty() {
                anyhow::bail!("No active debug session found.");
            }
            let mut detached = 0usize;
            for (pid, path) in sessions {
                let state = read_state(pid, &path)?;
                if state.verify_process().is_err() {
                    remove_state(&dir, pid);
                    continue;
                }
                let mut session = attach_saved(&state)?;
                session.detach()?;
                remove_state(&dir, pid);
                detached += 1;
            }
            println!("Detached from {} session(s).", detached);
            Ok(())
        }

        DebugCommand::Continue => {
            let (state, mut session) = attach_active()?;
            session.debugger.continue_execution()?;
            println!("Continuing PID {}...", state.pid);
            Ok(())
        }

        DebugCommand::Step => {
            let (state, mut session) = attach_active()?;
            session.debugger.single_step()?;

            if let Some(tid) = state.last_thread_id {
                let regs = session.debugger.fetch_registers(tid)?;
                print_regs(&regs, false);
            }
            Ok(())
        }

        DebugCommand::BpWhen(_) | DebugCommand::Logpoint(_) => {
            anyhow::bail!("conditional breakpoints and logpoints require a persistent debug session")
        }

        DebugCommand::Bp(args) => {
            let (mut state, mut session) = attach_active()?;
            session.debugger.set_sw_breakpoint(args.addr)?;
            if !state.breakpoints.contains(&args.addr) {
                state.breakpoints.push(args.addr);
            }
            save_state(&state)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"set_sw_breakpoint\",\"address\":\"0x{:x}\"}}",
                    args.addr
                );
            } else {
                println!("Breakpoint set at 0x{:016x}", args.addr);
            }
            Ok(())
        }

        DebugCommand::RmBp(args) => {
            let (mut state, mut session) = attach_active()?;
            session.debugger.remove_sw_breakpoint(args.addr)?;
            state.breakpoints.retain(|&a| a != args.addr);
            save_state(&state)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"remove_sw_breakpoint\",\"address\":\"0x{:x}\"}}",
                    args.addr
                );
            } else {
                println!("Breakpoint removed at 0x{:016x}", args.addr);
            }
            Ok(())
        }

        DebugCommand::HwBp(_) | DebugCommand::RmHwBp(_) => {
            anyhow::bail!("hardware breakpoints require a persistent debug session; Windows x64 native backend only")
        }

        DebugCommand::TraceInspect(args) => {
            let report = super::debug_trace::trace_inspect(&args)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }

        DebugCommand::TraceDiff(args) => {
            let report = super::debug_trace::trace_diff(&args)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }

        DebugCommand::TraceDecomp(args) => {
            let report = super::debug_trace::trace_decomp(&args)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }

        DebugCommand::RrStatus(args) => {
            let available = fission_dynamic::debug::rr::RRDebugger::is_available();
            let traces = fission_dynamic::debug::rr::RRDebugger::list_traces()
                .into_iter()
                .map(|trace| serde_json::json!({
                    "path": trace.path,
                    "binary_name": trace.binary_name,
                    "event_count": trace.event_count,
                    "timestamp": trace.timestamp,
                }))
                .collect::<Vec<_>>();
            let report = serde_json::json!({
                "schema_version": 1,
                "backend": "rr",
                "available": available,
                "traces": traces,
            });
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            Ok(())
        }

        DebugCommand::RrRecord(args) => {
            anyhow::ensure!(
                fission_dynamic::debug::rr::RRDebugger::is_available(),
                "rr is unavailable on this host; Linux and the rr executable are required"
            );
            let refs = args.args.iter().map(String::as_str).collect::<Vec<_>>();
            let trace = fission_dynamic::debug::rr::RRDebugger::record(&args.binary, &refs)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let report = serde_json::json!({
                "schema_version": 1,
                "action": "rr_record",
                "binary": args.binary,
                "trace": trace,
            });
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            Ok(())
        }

        DebugCommand::RrReplay(args) => {
            anyhow::ensure!(
                fission_dynamic::debug::rr::RRDebugger::is_available(),
                "rr is unavailable on this host; Linux and the rr executable are required"
            );
            let mut rr = fission_dynamic::debug::rr::RRDebugger::new();
            rr.replay(&args.trace_dir)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let report = serde_json::json!({
                "schema_version": 1,
                "action": "rr_replay",
                "trace": args.trace_dir,
                "state": format!("{:?}", rr.state()),
                "architecture": format!("{:?}", rr.arch()),
            });
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            Ok(())
        }

        DebugCommand::Context(args) => {
            let (state, mut session) = attach_active()?;
            let tid = session.debugger.get_state().current_thread_id
                .or(state.last_thread_id)
                .or(session.debugger.get_state().main_thread_id)
                .context("No thread available for a context snapshot")?;
            let context = super::debug_context::run_context(
                session.debugger.as_mut(), tid, &args, None, false,
            )?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&context)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&context)?);
            }
            Ok(())
        }
        DebugCommand::Regs => {
            let (state, mut session) = attach_active()?;
            let tid = state
                .last_thread_id
                .or(session.debugger.get_state().main_thread_id)
                .ok_or_else(|| anyhow::anyhow!("No thread available"))?;
            let regs = session.debugger.fetch_registers(tid)?;
            print_regs(&regs, false);
            Ok(())
        }

        DebugCommand::Read(args) => {
            let (_, session) = attach_active()?;
            let mem = session.debugger.read_memory(args.addr, args.size)?;
            print_hex_dump(args.addr, &mem, args.json);
            Ok(())
        }

        DebugCommand::Write(args) => {
            let (_, mut session) = attach_active()?;
            let bytes = hex_bytes_from_str(&args.data)?;
            session.debugger.write_memory(args.addr, &bytes)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"write_memory\",\"address\":\"0x{:x}\",\"bytes_written\":{}}}",
                    args.addr,
                    bytes.len()
                );
            } else {
                println!("Wrote {} bytes to 0x{:016x}", bytes.len(), args.addr);
            }
            Ok(())
        }

        DebugCommand::Modules(args) => {
            let (_, mut session) = attach_active()?;
            let report_result = session.debugger.list_modules();
            let detach_result = session.debugger.detach();
            let report = report_result?;
            detach_result?;

            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }

            println!(
                "Loaded modules ({}; {:?} mapping completeness):",
                report.modules.len(),
                report.completeness
            );
            for module in &report.modules {
                println!("  {} ({})", module.name, module.path);
                match &module.address_transform {
                    fission_dynamic::debug::types::ModuleAddressTransform::Resolved {
                        load_bias,
                        ..
                    } => println!("    ELF runtime = analysis VA + {load_bias:#x}"),
                    fission_dynamic::debug::types::ModuleAddressTransform::NotApplicable {
                        reason,
                    }
                    | fission_dynamic::debug::types::ModuleAddressTransform::Unavailable {
                        reason,
                    } => println!("    address transform unavailable: {reason}"),
                }
                for mapping in &module.mappings {
                    println!(
                        "    {:#016x}-{:#016x} {} file_offset={}",
                        mapping.runtime_start,
                        mapping.runtime_end,
                        mapping.permissions.as_deref().unwrap_or("????"),
                        mapping
                            .file_offset
                            .map_or_else(|| "unknown".to_string(), |offset| format!("{offset:#x}"))
                    );
                }
            }
            if !report.other_mappings.is_empty() {
                println!("Other mappings: {}", report.other_mappings.len());
            }
            for diagnostic in &report.diagnostics {
                println!("  note: {diagnostic}");
            }
            Ok(())
        }

        DebugCommand::Threads => {
            let (_, session) = attach_active()?;
            let threads = session.debugger.get_state().threads.clone();
            println!("Active threads ({}):", threads.len());
            for (tid, info) in threads {
                println!(
                    "  {}  start_address={:x}  suspended={}  main={}",
                    tid, info.start_address, info.suspended, info.is_main
                );
            }
            Ok(())
        }

        DebugCommand::Init(args) => {
            let mut session = build_session();
            let pid = session.launch(&args.path, &args.args)?;

            let state = DebugStateFile {
                pid,
                process_identity: if emulator {
                    None
                } else {
                    Some(process_identity(pid)?)
                },
                last_thread_id: session.debugger.get_state().main_thread_id,
                breakpoints: Vec::new(),
            };
            if !emulator {
                save_state(&state)?;
            }

            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "status": "launched",
                        "pid": pid,
                        "path": args.path,
                        "main_thread_id": state.last_thread_id,
                    }))?
                );
            } else {
                println!("Launched PID {} ({})", pid, args.path);
                if let Some(tid) = state.last_thread_id {
                    println!("Main thread: {}", tid);
                }
            }
            Ok(())
        }

        DebugCommand::Pause => {
            let (state, mut session) = attach_active()?;
            session.debugger.pause()?;
            println!("Break requested for PID {}.", state.pid);
            Ok(())
        }

        DebugCommand::Stop => {
            let (state, mut session) = attach_active()?;
            session.debugger.terminate()?;
            remove_state(&state_dir()?, state.pid);
            println!("Terminated PID {}.", state.pid);
            Ok(())
        }

        DebugCommand::StepOver => {
            let (state, mut session) = attach_active()?;
            session.debugger.step_over()?;
            if let Some(tid) = state.last_thread_id {
                let regs = session.debugger.fetch_registers(tid)?;
                print_regs(&regs, false);
            }
            Ok(())
        }

        DebugCommand::StepOut => {
            let (state, mut session) = attach_active()?;
            session.debugger.step_out()?;
            if let Some(tid) = state.last_thread_id {
                let regs = session.debugger.fetch_registers(tid)?;
                print_regs(&regs, false);
            }
            Ok(())
        }

        DebugCommand::Skip => {
            let (state, mut session) = attach_active()?;
            session.debugger.skip_instruction()?;
            if let Some(tid) = state.last_thread_id {
                let regs = session.debugger.fetch_registers(tid)?;
                print_regs(&regs, false);
            }
            Ok(())
        }

        DebugCommand::SwitchThread(args) => {
            let (_, mut session) = attach_active()?;
            session.debugger.set_current_thread(args.tid)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"switch_thread\",\"tid\":{}}}",
                    args.tid
                );
            } else {
                println!("Switched to thread {}", args.tid);
            }
            Ok(())
        }

        DebugCommand::Event(args) => {
            let (_, mut session) = attach_active()?;
            let event = session.debugger.poll_event(args.timeout_ms)?;
            if args.json {
                let report = match event {
                    Some(event) => serde_json::json!({
                        "status": "event",
                        "event": super::debug_session::event_json(&event),
                    }),
                    None => serde_json::json!({
                        "status": "timeout",
                        "event": null,
                        "timeout_ms": args.timeout_ms,
                    }),
                };
                println!("{}", serde_json::to_string(&report)?);
            } else if let Some(evt) = event {
                println!("{:?}", evt);
            } else {
                println!("No event within {} ms.", args.timeout_ms);
            }
            Ok(())
        }

        DebugCommand::BpEnable(args) => {
            let (_, mut session) = attach_active()?;
            session.debugger.enable_breakpoint(args.addr)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"enable_breakpoint\",\"address\":\"0x{:x}\"}}",
                    args.addr
                );
            } else {
                println!("Breakpoint enabled at 0x{:016x}", args.addr);
            }
            Ok(())
        }

        DebugCommand::BpDisable(args) => {
            let (_, mut session) = attach_active()?;
            session.debugger.disable_breakpoint(args.addr)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"disable_breakpoint\",\"address\":\"0x{:x}\"}}",
                    args.addr
                );
            } else {
                println!("Breakpoint disabled at 0x{:016x}", args.addr);
            }
            Ok(())
        }

        DebugCommand::BpList(args) => {
            let (state, mut session) = attach_active()?;
            // A backend built for this invocation knows no breakpoints; the
            // ones this session set are in its state file. Put them back for
            // the listing, so it shows the session's breakpoints and what the
            // backend recorded for each. One that can no longer be set (its
            // module was unloaded) is simply not listed.
            for address in &state.breakpoints {
                let _ = session.debugger.set_sw_breakpoint(*address);
            }
            let bps = session.debugger.list_breakpoints();
            if args.json {
                let arr: Vec<_> = bps
                    .iter()
                    .map(|bp| {
                        serde_json::json!({
                            "address": bp.address,
                            "enabled": bp.enabled,
                            "temporary": bp.temporary,
                            "kind": format!("{:?}", bp.kind),
                            "hits": bp.hits,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&arr)?);
            } else {
                println!("Breakpoints ({}):", bps.len());
                for bp in bps {
                    let kind_str = format!("{:?}", bp.kind);
                    println!(
                        "  0x{:016x}  {}  {}  hits={}  {}",
                        bp.address,
                        if bp.enabled { "enabled" } else { "disabled" },
                        if bp.temporary { "temp" } else { "perm" },
                        bp.hits,
                        kind_str
                    );
                }
            }
            Ok(())
        }

        DebugCommand::MemBp(args) => {
            let (_, mut session) = attach_active()?;
            let kind = match args.kind {
                crate::cli::args::MemoryBpKindArg::Read => {
                    fission_dynamic::debug::types::MemoryBpKind::Read
                }
                crate::cli::args::MemoryBpKindArg::Write => {
                    fission_dynamic::debug::types::MemoryBpKind::Write
                }
                crate::cli::args::MemoryBpKindArg::Execute => {
                    fission_dynamic::debug::types::MemoryBpKind::Execute
                }
                crate::cli::args::MemoryBpKindArg::Access => {
                    fission_dynamic::debug::types::MemoryBpKind::Access
                }
            };
            session
                .debugger
                .set_memory_breakpoint(args.addr, args.size, kind)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"set_memory_breakpoint\",\"address\":\"0x{:x}\",\"size\":{}}}",
                    args.addr, args.size
                );
            } else {
                println!(
                    "Memory breakpoint set at 0x{:016x} (size {})",
                    args.addr, args.size
                );
            }
            Ok(())
        }

        DebugCommand::RmMemBp(args) => {
            let (_, mut session) = attach_active()?;
            session.debugger.remove_memory_breakpoint(args.addr)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"remove_memory_breakpoint\",\"address\":\"0x{:x}\"}}",
                    args.addr
                );
            } else {
                println!("Memory breakpoint removed at 0x{:016x}", args.addr);
            }
            Ok(())
        }

        DebugCommand::DllBp(args) => {
            let (_, mut session) = attach_active()?;
            session.debugger.set_dll_breakpoint(&args.name)?;
            if args.json {
                println!("{}", dll_breakpoint_json("set_dll_breakpoint", &args.name));
            } else {
                println!("DLL breakpoint set for '{}'", args.name);
            }
            Ok(())
        }

        DebugCommand::RmDllBp(args) => {
            let (_, mut session) = attach_active()?;
            session.debugger.remove_dll_breakpoint(&args.name)?;
            if args.json {
                println!(
                    "{}",
                    dll_breakpoint_json("remove_dll_breakpoint", &args.name)
                );
            } else {
                println!("DLL breakpoint removed for '{}'", args.name);
            }
            Ok(())
        }

        DebugCommand::ExBp(args) => {
            let (_, mut session) = attach_active()?;
            session
                .debugger
                .set_exception_breakpoint(args.code as u32)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"set_exception_breakpoint\",\"code\":\"0x{:x}\"}}",
                    args.code
                );
            } else {
                println!("Exception breakpoint set for code 0x{:08x}", args.code);
            }
            Ok(())
        }

        DebugCommand::RmExBp(args) => {
            let (_, mut session) = attach_active()?;
            session
                .debugger
                .remove_exception_breakpoint(args.code as u32)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"remove_exception_breakpoint\",\"code\":\"0x{:x}\"}}",
                    args.code
                );
            } else {
                println!("Exception breakpoint removed for code 0x{:08x}", args.code);
            }
            Ok(())
        }

        DebugCommand::SetReg(args) => {
            let (state, mut session) = attach_active()?;
            let tid = state
                .last_thread_id
                .ok_or_else(|| anyhow::anyhow!("No thread available"))?;
            session.debugger.set_register(tid, &args.name, args.value)?;
            if args.json {
                println!("{}", set_register_json(&args.name, args.value));
            } else {
                println!("Register {} = 0x{:016x}", args.name, args.value);
            }
            Ok(())
        }

        DebugCommand::GetFlag(args) => {
            let (_, session) = attach_active()?;
            let value = session.debugger.get_flag(&args.name)?;
            if args.json {
                println!("{}", get_flag_json(&args.name, value));
            } else {
                println!("Flag {} = {}", args.name, value);
            }
            Ok(())
        }

        DebugCommand::SetFlag(args) => {
            let (_, mut session) = attach_active()?;
            let value = args
                .value
                .ok_or_else(|| anyhow::anyhow!("Flag value required for set-flag"))?;
            session.debugger.set_flag(&args.name, value)?;
            if args.json {
                println!("{}", set_flag_json(&args.name, value));
            } else {
                println!("Flag {} = {}", args.name, value);
            }
            Ok(())
        }

        DebugCommand::Alloc(args) => {
            let (_, mut session) = attach_active()?;
            let addr = session.debugger.remote_alloc(args.addr, args.size)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"alloc\",\"address\":\"0x{:x}\",\"size\":{}}}",
                    addr, args.size
                );
            } else {
                println!("Allocated {} bytes at 0x{:016x}", args.size, addr);
            }
            Ok(())
        }

        DebugCommand::Free(args) => {
            let (_, mut session) = attach_active()?;
            session.debugger.remote_free(args.addr)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"free\",\"address\":\"0x{:x}\"}}",
                    args.addr
                );
            } else {
                println!("Freed memory at 0x{:016x}", args.addr);
            }
            Ok(())
        }

        DebugCommand::GetProtect(args) => {
            let (_, session) = attach_active()?;
            let rights = session.debugger.get_page_rights(args.addr)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"address\":\"0x{:x}\",\"protect\":{}}}",
                    args.addr, rights
                );
            } else {
                println!("Page rights at 0x{:016x} = 0x{:08x}", args.addr, rights);
            }
            Ok(())
        }

        DebugCommand::SetProtect(args) => {
            let (_, mut session) = attach_active()?;
            let protect = args
                .protect
                .ok_or_else(|| anyhow::anyhow!("Protection flags required for set-protect"))?;
            session
                .debugger
                .set_page_rights(args.addr, args.size, protect)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"set_protect\",\"address\":\"0x{:x}\",\"size\":{},\"protect\":{}}}",
                    args.addr, args.size, protect
                );
            } else {
                println!(
                    "Set page rights at 0x{:016x} (size {}, protect 0x{:08x})",
                    args.addr, args.size, protect
                );
            }
            Ok(())
        }

        DebugCommand::StackPeek(args) => {
            let (_, session) = attach_active()?;
            let value = session.debugger.stack_peek(args.offset)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"offset\":{},\"value\":\"0x{:x}\"}}",
                    args.offset, value
                );
            } else {
                println!("Stack[{}] = 0x{:016x}", args.offset, value);
            }
            Ok(())
        }

        DebugCommand::StackPop(args) => {
            let (_, mut session) = attach_active()?;
            let value = session.debugger.stack_pop()?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"pop\",\"value\":\"0x{:x}\"}}",
                    value
                );
            } else {
                println!("Popped 0x{:016x}", value);
            }
            Ok(())
        }

        DebugCommand::StackPush(args) => {
            let (_, mut session) = attach_active()?;
            session.debugger.stack_push(args.value)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"push\",\"value\":\"0x{:x}\"}}",
                    args.value
                );
            } else {
                println!("Pushed 0x{:016x}", args.value);
            }
            Ok(())
        }

        DebugCommand::Find(args) => {
            let (_, session) = attach_active()?;
            let pattern = hex_bytes_from_str(&args.pattern)?;
            let results = session
                .debugger
                .find_pattern(args.start, args.size, &pattern)?;
            if args.json {
                let arr: Vec<_> = results
                    .iter()
                    .map(|&addr| serde_json::json!(format!("0x{:x}", addr)))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&arr)?);
            } else {
                println!("Found {} match(es):", results.len());
                for addr in results {
                    println!("  0x{:016x}", addr);
                }
            }
            Ok(())
        }

        DebugCommand::Exports(args) => {
            let (_, session) = attach_active()?;
            let exports = session.debugger.get_module_exports(args.base)?;
            if args.json {
                let arr: Vec<_> = exports
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "name": e.name,
                            "address": format!("0x{:x}", e.address),
                            "ordinal": e.ordinal,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&arr)?);
            } else {
                println!("Exports ({}):", exports.len());
                for e in exports {
                    println!("  0x{:016x}  ord={:?}  {}", e.address, e.ordinal, e.name);
                }
            }
            Ok(())
        }

        // The one command that owns a live machine for its whole duration.
        // Every arm around it re-attaches to a pid between invocations, which
        // an emulated process cannot survive.
        DebugCommand::Session(args) => super::debug_session::run_session(args, emulator),

        DebugCommand::Imports(args) => {
            let (_, session) = attach_active()?;
            let imports = session.debugger.get_module_imports(args.base)?;
            if args.json {
                let arr: Vec<_> = imports
                    .iter()
                    .map(|i| {
                        serde_json::json!({
                            "module": i.module,
                            "name": i.name,
                            "ordinal": i.ordinal,
                            "address": format!("0x{:x}", i.address),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&arr)?);
            } else {
                println!("Imports ({}):", imports.len());
                for i in imports {
                    println!(
                        "  0x{:016x}  {}!{}  ord={:?}",
                        i.address,
                        i.module,
                        i.name.as_deref().unwrap_or(""),
                        i.ordinal
                    );
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fission-cli-debug-state-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn state(pid: u32) -> DebugStateFile {
        DebugStateFile {
            pid,
            process_identity: None,
            last_thread_id: Some(pid),
            breakpoints: vec![0x1000],
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn a_session_can_verify_its_current_process() {
        let pid = std::process::id();
        let mut current = state(pid);
        current.process_identity = Some(process_identity(pid).unwrap());
        assert!(current.verify_process().is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn debugger_state_directory_is_private() {
        use std::os::unix::fs::MetadataExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session");
        let owner = temp.path().metadata().unwrap().uid();
        ensure_private_state_dir(&path, owner).unwrap();
        ensure_private_state_dir(&path, owner).unwrap();
        let meta = path.metadata().unwrap();
        assert_eq!(meta.uid(), owner);
        assert_eq!(meta.mode() & 0o077, 0);
    }

    #[test]
    fn hex_with_a_multi_byte_character_is_an_error_not_a_panic() {
        // Four bytes, so the even-length check passed, and the two-byte slice
        // then cut `é` in half.
        for text in ["0é0", "é", "00é1", "ｆｆ"] {
            let error = hex_bytes_from_str(text).expect_err(text);
            assert!(error.to_string().contains("hex"), "{text}: {error}");
        }
    }

    #[test]
    fn hex_parses_with_and_without_separators() {
        // Positive control: what parsed before still parses to the same bytes.
        assert_eq!(hex_bytes_from_str("CC90").unwrap(), [0xCC, 0x90]);
        assert_eq!(
            hex_bytes_from_str("cc 90,0f\t0B").unwrap(),
            [0xCC, 0x90, 0x0F, 0x0B]
        );
        assert_eq!(hex_bytes_from_str("").unwrap(), Vec::<u8>::new());
        assert!(hex_bytes_from_str("abc").is_err(), "odd length");
        assert!(hex_bytes_from_str("zz").is_err(), "not hex");
        assert!(hex_bytes_from_str("+1").is_err(), "a sign is not a digit");
    }

    fn parsed(line: &str) -> serde_json::Value {
        serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    #[test]
    fn a_name_with_quotes_and_backslashes_stays_one_json_string() {
        let hostile = "a\"b\\c\n\",\"injected\":\"x";
        let line = dll_breakpoint_json("set_dll_breakpoint", hostile);
        assert_eq!(parsed(&line)["dll"], hostile);
        assert!(parsed(&line).get("injected").is_none());

        let line = set_register_json(hostile, 0x10);
        assert_eq!(parsed(&line)["register"], hostile);
        assert_eq!(parsed(&line)["value"], "0x10");

        assert_eq!(parsed(&get_flag_json(hostile, true))["flag"], hostile);
        assert_eq!(parsed(&set_flag_json(hostile, false))["flag"], hostile);
        assert_eq!(parsed(&set_flag_json(hostile, false))["value"], false);
    }

    #[test]
    fn an_ordinary_name_prints_the_line_it_always_did() {
        // Positive control: the escaping changes nothing for a plain name.
        assert_eq!(
            dll_breakpoint_json("set_dll_breakpoint", "kernel32.dll"),
            r#"{"status":"ok","action":"set_dll_breakpoint","dll":"kernel32.dll"}"#
        );
        assert_eq!(
            dll_breakpoint_json("remove_dll_breakpoint", "kernel32.dll"),
            r#"{"status":"ok","action":"remove_dll_breakpoint","dll":"kernel32.dll"}"#
        );
        assert_eq!(
            set_register_json("rax", 0x2a),
            r#"{"status":"ok","action":"set_register","register":"rax","value":"0x2a"}"#
        );
        assert_eq!(
            get_flag_json("zf", true),
            r#"{"status":"ok","flag":"zf","value":true}"#
        );
        assert_eq!(
            set_flag_json("cf", false),
            r#"{"status":"ok","action":"set_flag","flag":"cf","value":false}"#
        );
    }

    #[test]
    fn only_a_regular_pid_named_json_file_is_a_session() {
        let dir = scratch_dir("filter");
        // Things that start with the prefix and are not a session's state.
        std::fs::create_dir(dir.join("fission-debug-backup")).unwrap();
        std::fs::create_dir(dir.join("fission-debug-12.json")).unwrap();
        std::fs::write(dir.join("fission-debug-notes.txt"), "x").unwrap();
        std::fs::write(dir.join("fission-debug-abc.json"), "{}").unwrap();
        std::fs::write(dir.join("fission-debug-+5.json"), "{}").unwrap();
        std::fs::write(dir.join("fission-debug-.json"), "{}").unwrap();

        let error = find_active_state_in(&dir).expect_err("nothing here is a session");
        assert!(
            error.to_string().contains("No active debug session"),
            "{error:#}"
        );

        // With a real one among them, that is the one found.
        save_state_in(&dir, &state(77)).unwrap();
        assert_eq!(find_active_state_in(&dir).unwrap().pid, 77);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_session_written_last_is_the_active_one() {
        let dir = scratch_dir("order");
        save_state_in(&dir, &state(300)).unwrap();
        save_state_in(&dir, &state(100)).unwrap();
        save_state_in(&dir, &state(200)).unwrap();
        let at = |seconds: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
        for (pid, seconds) in [(300, 1_000), (100, 3_000), (200, 2_000)] {
            std::fs::File::options()
                .write(true)
                .open(state_path_in(&dir, pid))
                .unwrap()
                .set_modified(at(seconds))
                .unwrap();
        }
        let order: Vec<u32> = state_files_in(&dir)
            .unwrap()
            .into_iter()
            .map(|(pid, _)| pid)
            .collect();
        assert_eq!(order, [100, 200, 300]);
        assert_eq!(find_active_state_in(&dir).unwrap().pid, 100);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_leaves_one_complete_file_and_no_scratch_file() {
        let dir = scratch_dir("save");
        save_state_in(&dir, &state(41)).unwrap();
        let mut second = state(41);
        second.breakpoints = vec![0x2000, 0x3000];
        save_state_in(&dir, &second).unwrap();

        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["fission-debug-41.json"]);
        assert_eq!(
            find_active_state_in(&dir).unwrap().breakpoints,
            [0x2000, 0x3000]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A reader running while the state is being rewritten must never see a
    /// file it cannot parse. Writing in place empties the file before the new
    /// bytes arrive, and a reader in that window got an empty document.
    #[test]
    fn a_reader_never_sees_a_half_written_state_file() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = scratch_dir("atomic");
        save_state_in(&dir, &state(9)).unwrap();
        let path = state_path_in(&dir, 9);
        let done = Arc::new(AtomicBool::new(false));

        let reader = {
            let done = Arc::clone(&done);
            let path = path.clone();
            std::thread::spawn(move || {
                let mut reads = 0u32;
                let mut bad = Vec::new();
                while !done.load(Ordering::Relaxed) {
                    // The file is replaced, never absent.
                    match std::fs::read_to_string(&path) {
                        Ok(text) => {
                            reads += 1;
                            if serde_json::from_str::<DebugStateFile>(&text).is_err() {
                                bad.push(text);
                            }
                        }
                        Err(error) => bad.push(format!("read failed: {error}")),
                    }
                }
                (reads, bad)
            })
        };

        let mut next = state(9);
        for round in 0..400u64 {
            next.breakpoints = (0..64).map(|i| round * 0x1000 + i).collect();
            save_state_in(&dir, &next).unwrap();
        }
        done.store(true, Ordering::Relaxed);
        let (reads, bad) = reader.join().unwrap();
        assert!(reads > 0, "the reader never ran");
        assert!(
            bad.is_empty(),
            "{} of {reads} reads saw a broken state file, first: {:?}",
            bad.len(),
            bad.first()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
