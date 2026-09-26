//! One invocation, one live machine, a list of commands, structured output.
//!
//! Every other `debug` subcommand builds a session, attaches to a pid, does
//! one thing and exits. That works because the OS keeps a native process alive
//! between invocations -- and it cannot work for the emulator, whose machine
//! exists only inside the command that launched it. `debug --emulator bp
//! 0x401000` followed by `debug --emulator continue` is two different programs,
//! each at its entry point, and the second has never heard of the breakpoint.
//!
//! So the session is the command. The vocabulary is the same one the
//! subcommands use -- the strings are parsed by the same clap definitions, so
//! there is one spelling of `bp` and one of `read --size`, not two that can
//! drift.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};

use crate::cli::args::{DebugCommand, DebugSessionArgs, MemoryBpKindArg};
use fission_dynamic::debug::traits::ExecutionBackend;
use fission_dynamic::debug::types as debug_types_alias;
use fission_dynamic::debug::types::{DebugEvent, DebugStatus, MemoryBpKind, RegisterState};

/// Split a command line on whitespace, honouring double quotes.
///
/// Enough for this vocabulary: the only argument that can carry a space is a
/// hex byte string, and `write 0x1000 "CC 90"` should mean what it looks like.
fn split_command(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut current));
                    any = false;
                }
            }
            c => {
                current.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(current);
    }
    out
}

/// The commands to run, from `-c` flags then `--script`, in that order.
fn collect_commands(args: &DebugSessionArgs) -> Result<Vec<String>> {
    let mut commands = args.commands.clone();
    if let Some(script) = &args.script {
        let text = if script == "-" {
            std::io::read_to_string(std::io::stdin())
                .context("read commands from standard input")?
        } else {
            std::fs::read_to_string(script)
                .with_context(|| format!("read commands from {script}"))?
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            commands.push(line.to_string());
        }
    }
    if commands.is_empty() && args.rhai.is_none() {
        bail!("no commands: pass at least one -c/--command, --script, or --rhai");
    }
    Ok(commands)
}

/// A command line holding one of the `debug` subcommands and nothing else.
///
/// The point of routing through clap rather than hand-parsing is that the
/// session's vocabulary *is* the CLI's: one spelling of `bp`, one of
/// `read --size`, one hex parser, and no second definition to drift.
#[derive(clap::Parser, Debug)]
#[command(no_binary_name = true, disable_help_flag = true)]
struct SessionCommandLine {
    #[command(subcommand)]
    command: DebugCommand,
}

fn parse_command(line: &str) -> Result<DebugCommand> {
    let words = split_command(line);
    if words.is_empty() {
        bail!("empty command");
    }
    let parsed = <SessionCommandLine as clap::Parser>::try_parse_from(words)
        .map_err(|e| anyhow::anyhow!("{}", e.render()))
        .with_context(|| format!("in command {line:?}"))?;
    Ok(parsed.command)
}

fn registers_json(regs: &RegisterState) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("pc".into(), json!(format!("0x{:x}", regs.pc)));
    for (name, value) in regs.iter() {
        map.insert(name.to_ascii_lowercase(), json!(format!("0x{value:x}")));
    }
    Value::Object(map)
}

pub(super) fn event_json(event: &DebugEvent) -> Value {
    match event {
        DebugEvent::ProcessCreated {
            pid,
            main_thread_id,
        } => json!({
            "event": "process_created", "pid": pid, "main_thread_id": main_thread_id,
        }),
        DebugEvent::ProcessExited { exit_code } => json!({
            "event": "process_exited", "exit_code": exit_code,
        }),
        DebugEvent::ThreadCreated { thread_id } => json!({
            "event": "thread_created", "thread_id": thread_id,
        }),
        DebugEvent::ThreadExited { thread_id } => json!({
            "event": "thread_exited", "thread_id": thread_id,
        }),
        DebugEvent::DllLoaded { base_address, name } => json!({
            "event": "dll_loaded", "base": format!("0x{base_address:x}"), "name": name,
        }),
        DebugEvent::DllUnloaded { base_address } => json!({
            "event": "dll_unloaded", "base": format!("0x{base_address:x}"),
        }),
        DebugEvent::BreakpointHit { address, thread_id } => json!({
            "event": "breakpoint_hit", "address": format!("0x{address:x}"), "thread_id": thread_id,
        }),
        DebugEvent::WatchpointHit {
            address,
            size,
            write,
            pc,
            thread_id,
        } => json!({
            "event": "watchpoint_hit",
            "address": format!("0x{address:x}"),
            "size": size,
            "write": write,
            "pc": format!("0x{pc:x}"),
            "thread_id": thread_id,
        }),
        DebugEvent::SingleStep { thread_id } => json!({
            "event": "single_step", "thread_id": thread_id,
        }),
        DebugEvent::Exception {
            code,
            address,
            first_chance,
        } => json!({
            "event": "exception",
            "code": code,
            "address": format!("0x{address:x}"),
            "first_chance": first_chance,
        }),
        DebugEvent::OutputString { message } => json!({
            "event": "output", "message": message,
        }),
    }
}

/// Drain whatever the backend has queued, so every command's result carries
/// what happened during it.
fn drain_events(backend: &mut dyn ExecutionBackend) -> Vec<Value> {
    let mut out = Vec::new();
    while let Ok(Some(event)) = backend.poll_event(0) {
        out.push(event_json(&event));
    }
    out
}

fn hex_bytes(s: &str) -> Result<Vec<u8>> {
    let s: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();
    if s.len() % 2 != 0 {
        bail!("hex data length must be even (got {})", s.len());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .with_context(|| format!("invalid hex data: {s}"))
}

/// Run one command against the live session.
///
/// Commands that only make sense against an OS process -- attaching to a pid,
/// listing loaded modules, allocating memory in a foreign address space -- are
/// refused by name rather than silently doing nothing.
fn execute(
    backend: &mut dyn ExecutionBackend,
    thread_id: u32,
    command: DebugCommand,
) -> Result<Value> {
    let value = match command {
        DebugCommand::Capabilities(_) => serde_json::to_value(backend.capabilities())?,
        DebugCommand::Bp(a) => {
            backend.set_sw_breakpoint(a.addr)?;
            json!({ "action": "set_breakpoint", "address": format!("0x{:x}", a.addr) })
        }
        DebugCommand::RmBp(a) => {
            backend.remove_sw_breakpoint(a.addr)?;
            json!({ "action": "remove_breakpoint", "address": format!("0x{:x}", a.addr) })
        }
        DebugCommand::MemBp(a) => {
            let kind = match a.kind {
                MemoryBpKindArg::Read => MemoryBpKind::Read,
                MemoryBpKindArg::Write => MemoryBpKind::Write,
                MemoryBpKindArg::Execute => MemoryBpKind::Execute,
                MemoryBpKindArg::Access => MemoryBpKind::Access,
            };
            backend.set_memory_breakpoint(a.addr, a.size, kind)?;
            json!({
                "action": "set_memory_breakpoint",
                "address": format!("0x{:x}", a.addr),
                "size": a.size,
                "kind": format!("{:?}", a.kind).to_lowercase(),
            })
        }
        DebugCommand::RmMemBp(a) => {
            backend.remove_memory_breakpoint(a.addr)?;
            json!({ "action": "remove_memory_breakpoint", "address": format!("0x{:x}", a.addr) })
        }
        DebugCommand::Continue => {
            backend.continue_execution()?;
            execution_progress(backend, thread_id, "continue")?
        }
        DebugCommand::Step => {
            backend.single_step()?;
            execution_progress(backend, thread_id, "step")?
        }
        DebugCommand::StepOver => {
            backend.step_over()?;
            execution_progress(backend, thread_id, "step_over")?
        }
        DebugCommand::StepOut => {
            backend.step_out()?;
            execution_progress(backend, thread_id, "step_out")?
        }
        DebugCommand::Regs => {
            let regs = backend.fetch_registers(thread_id)?;
            json!({ "action": "regs", "registers": registers_json(&regs) })
        }
        DebugCommand::SetReg(a) => {
            let mut regs = backend.fetch_registers(thread_id)?;
            if regs.get(&a.name).is_none() {
                bail!("this machine has no register {}", a.name);
            }
            regs.set(&a.name, a.value);
            backend.set_registers(thread_id, &regs)?;
            json!({ "action": "set_reg", "name": a.name, "value": format!("0x{:x}", a.value) })
        }
        DebugCommand::Read(a) => {
            let bytes = backend.read_memory(a.addr, a.size)?;
            json!({
                "action": "read",
                "address": format!("0x{:x}", a.addr),
                "size": bytes.len(),
                "bytes": bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            })
        }
        DebugCommand::Write(a) => {
            let bytes = hex_bytes(&a.data)?;
            backend.write_memory(a.addr, &bytes)?;
            json!({
                "action": "write",
                "address": format!("0x{:x}", a.addr),
                "size": bytes.len(),
            })
        }
        DebugCommand::StackPeek(a) => {
            let value = backend.stack_peek(a.offset)?;
            json!({ "action": "stack_peek", "offset": a.offset, "value": format!("0x{value:x}") })
        }
        DebugCommand::Event(args) => match backend.poll_event(args.timeout_ms)? {
            Some(event) => json!({ "action": "event", "event": event_json(&event) }),
            None => json!({
                "action": "event",
                "event": Value::Null,
                "timeout_ms": args.timeout_ms,
            }),
        },
        DebugCommand::Modules(_) => serde_json::to_value(backend.list_modules()?)?,
        DebugCommand::Threads => {
            let state = backend.get_state();
            let threads: Vec<Value> = state
                .threads
                .values()
                .map(|t| {
                    json!({
                        "thread_id": t.thread_id,
                        "start_address": format!("0x{:x}", t.start_address),
                        "is_main": t.is_main,
                    })
                })
                .collect();
            json!({ "action": "threads", "threads": threads })
        }
        DebugCommand::SwitchThread(a) => {
            backend.set_current_thread(a.tid)?;
            json!({ "action": "switch_thread", "thread_id": a.tid })
        }
        DebugCommand::Stop | DebugCommand::Detach => {
            backend.detach()?;
            json!({ "action": "detach" })
        }
        other => {
            // Named the way the caller spelled it: clap renders the variant
            // `RmDllBp`, the command is `rm-dll-bp`, and an error naming the
            // wrong one sends a reader looking for a command that does not
            // exist.
            let variant = format!("{other:?}");
            let variant = variant.split(['(', ' ']).next().unwrap_or("that");
            let mut verb = String::new();
            for (i, c) in variant.chars().enumerate() {
                if c.is_uppercase() && i > 0 {
                    verb.push('-');
                }
                verb.extend(c.to_lowercase());
            }
            bail!(
                "`{verb}` needs an operating-system process and has no meaning inside a session; \
                 run it as its own subcommand against a native target"
            )
        }
    };
    Ok(value)
}

/// Some native backends return from `continue` or `step` while the target is
/// still running. A register read at that point is unsupported by ptrace; use
/// null for the PC until a stop event makes the register context readable.
fn execution_progress(
    backend: &mut dyn ExecutionBackend,
    thread_id: u32,
    action: &str,
) -> Result<Value> {
    let pc = if backend.get_state().status == DebugStatus::Running {
        Value::Null
    } else {
        json!(format!("0x{:x}", backend.fetch_registers(thread_id)?.pc))
    };
    Ok(json!({
        "action": action,
        "pc": pc,
        // Why it stopped, which `Suspended` cannot say: a breakpoint and a
        // program that ran out of code look the same otherwise.
        "stop": backend.stop_reason(),
    }))
}

fn start_session(
    args: &DebugSessionArgs,
    use_emulator: bool,
) -> Result<(fission_dynamic::debug::DebugSession, u32, u32)> {
    if use_emulator && args.attach.is_some() {
        bail!(
            "--attach requires a native debugger backend; it cannot attach to an emulator session"
        );
    }
    if args.rhai.is_some() && args.path.is_none() {
        bail!("--rhai requires a launched binary path and cannot be used with --attach");
    }

    let mut builder = fission_dynamic::debug::DebugSession::new();
    if use_emulator {
        builder = builder.with_emulator();
    }
    let mut session = builder.build();
    let pid = if let Some(pid) = args.attach {
        session
            .debugger
            .attach(pid)
            .with_context(|| format!("failed to attach to PID {pid}"))?;
        pid
    } else {
        let path = args
            .path
            .as_deref()
            .context("a binary path is required unless --attach is used")?;
        session
            .debugger
            .launch(path, &[])
            .with_context(|| format!("failed to launch {path}"))?
    };
    let thread_id = session.debugger.get_state().main_thread_id.unwrap_or(pid);
    Ok((session, pid, thread_id))
}

/// Breakpoints and watchpoints someone recorded earlier, put back before the
/// first command runs. Finding an address worth stopping at is the expensive
/// part of a session; having to find it again next time is what makes a
/// debugger a thing you use once.
fn restore_saved_breakpoints(backend: &mut dyn ExecutionBackend, path: Option<&str>) -> Vec<Value> {
    let Some(path) = path else {
        return Vec::new();
    };
    let mut restored = Vec::new();
    if let Ok(Some(project)) = fission_project::Project::read(
        &fission_project::Project::default_path(std::path::Path::new(path)),
    ) {
        for address in &project.breakpoints {
            if backend.set_sw_breakpoint(*address).is_ok() {
                restored.push(json!({ "breakpoint": format!("0x{address:x}") }));
            }
        }
        for watch in &project.watchpoints {
            let kind = match (watch.on_read, watch.on_write) {
                (true, true) => debug_types_alias::MemoryBpKind::Access,
                (true, false) => debug_types_alias::MemoryBpKind::Read,
                _ => debug_types_alias::MemoryBpKind::Write,
            };
            if backend
                .set_memory_breakpoint(watch.address, watch.size as usize, kind)
                .is_ok()
            {
                restored.push(json!({ "watchpoint": format!("0x{:x}", watch.address) }));
            }
        }
    }
    restored
}

fn session_state_json(backend: &mut dyn ExecutionBackend, thread_id: u32) -> Value {
    let state = backend.get_state();
    let pc = if state.status == DebugStatus::Running {
        Value::Null
    } else {
        backend
            .fetch_registers(thread_id)
            .ok()
            .map(|registers| json!(format!("0x{:x}", registers.pc)))
            .unwrap_or(Value::Null)
    };
    json!({
        "status": format!("{:?}", state.status).to_ascii_lowercase(),
        "attached_pid": state.attached_pid,
        "thread_id": state.current_thread_id.or(state.main_thread_id),
        "pc": pc,
        "stop": backend.stop_reason(),
    })
}

fn write_json_frame(writer: &mut impl Write, frame: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, frame)
        .context("serialize interactive debugger response")?;
    writer
        .write_all(b"\n")
        .context("write interactive debugger response")?;
    writer
        .flush()
        .context("flush interactive debugger response")?;
    Ok(())
}

#[derive(Clone, Copy)]
enum InteractiveEndReason {
    InputEof,
    Detached,
    TargetExited,
}

impl InteractiveEndReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::InputEof => "input_eof",
            Self::Detached => "detached",
            Self::TargetExited => "target_exited",
        }
    }
}

fn interactive_command(
    backend: &mut dyn ExecutionBackend,
    thread_id: u32,
    command_index: usize,
    command_line: &str,
    session_id: &str,
    target: &Value,
    backend_name: &str,
    writer: &mut impl Write,
) -> Result<Option<InteractiveEndReason>> {
    let outcome =
        parse_command(command_line).and_then(|command| execute(backend, thread_id, command));
    let events = drain_events(backend);
    let state = session_state_json(backend, thread_id);
    let (status, result, error) = match outcome {
        Ok(result) => ("ok", result, Value::Null),
        Err(error) => ("error", Value::Null, json!(format!("{error:#}"))),
    };
    let frame = json!({
        "schema_version": 1,
        "type": "command_result",
        "session_id": session_id,
        "target": target,
        "backend": backend_name,
        "command_index": command_index,
        "command": command_line,
        "status": status,
        "result": result,
        "error": error,
        "events": events,
        "state": state,
    });
    write_json_frame(writer, &frame)?;

    match state["status"].as_str() {
        Some("terminated") => Ok(Some(InteractiveEndReason::TargetExited)),
        Some("detached") => Ok(Some(InteractiveEndReason::Detached)),
        _ => Ok(None),
    }
}

fn interactive_end_frame(
    backend: &mut dyn ExecutionBackend,
    thread_id: u32,
    session_id: &str,
    target: &Value,
    backend_name: &str,
    reason: &str,
    cleanup: Value,
    events: Vec<Value>,
) -> Value {
    let mut state = session_state_json(backend, thread_id);
    if cleanup["status"] == "failed" {
        let observed = state["status"].clone();
        state["status"] = json!("unknown_after_cleanup_failure");
        state["last_observed_status"] = observed;
    }
    json!({
        "schema_version": 1,
        "type": "session_ended",
        "session_id": session_id,
        "target": target,
        "backend": backend_name,
        "reason": reason,
        "cleanup": cleanup,
        "events": events,
        "state": state,
    })
}

fn run_interactive_session(args: DebugSessionArgs, use_emulator: bool) -> Result<()> {
    if args.script.is_some() || args.rhai.is_some() || args.keep_going {
        bail!(
            "--interactive reads commands from stdin and cannot be combined with --script, --rhai, or --keep-going"
        );
    }
    let (mut session, pid, thread_id) = match start_session(&args, use_emulator) {
        Ok(session) => session,
        Err(error) => {
            let stdout = std::io::stdout();
            let mut output = stdout.lock();
            let frame = json!({
                "schema_version": 1,
                "type": "session_start_failed",
                "backend": if use_emulator { "emulator" } else { "native" },
                "target": { "path": args.path, "pid": args.attach },
                "status": "error",
                "error": format!("{error:#}"),
            });
            write_json_frame(&mut output, &frame)?;
            return Err(error);
        }
    };
    let backend_name = if use_emulator { "emulator" } else { "native" };
    let session_id = format!("{}-{pid}", std::process::id());
    let target = json!({ "path": args.path, "pid": pid });
    let restored = restore_saved_breakpoints(session.debugger.as_mut(), args.path.as_deref());

    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let run_result = (|| -> Result<InteractiveEndReason> {
        let events = drain_events(session.debugger.as_mut());
        let started = json!({
            "schema_version": 1,
            "type": "session_started",
            "session_id": session_id,
            "target": target,
            "backend": backend_name,
            "restored": restored,
            "events": events,
            "state": session_state_json(session.debugger.as_mut(), thread_id),
        });
        write_json_frame(&mut output, &started)?;

        let mut command_index = 0;
        for command_line in &args.commands {
            command_index += 1;
            if let Some(reason) = interactive_command(
                session.debugger.as_mut(),
                thread_id,
                command_index,
                command_line,
                &session_id,
                &target,
                backend_name,
                &mut output,
            )? {
                return Ok(reason);
            }
        }

        let stdin = std::io::stdin();
        let mut input = BufReader::new(stdin.lock());
        let mut line = String::new();
        loop {
            line.clear();
            if input
                .read_line(&mut line)
                .context("read interactive debugger command")?
                == 0
            {
                return Ok(InteractiveEndReason::InputEof);
            }
            let command_line = line.trim();
            if command_line.is_empty() || command_line.starts_with('#') {
                continue;
            }
            command_index += 1;
            if let Some(reason) = interactive_command(
                session.debugger.as_mut(),
                thread_id,
                command_index,
                command_line,
                &session_id,
                &target,
                backend_name,
                &mut output,
            )? {
                return Ok(reason);
            }
        }
    })();

    let mut reason = run_result
        .as_ref()
        .copied()
        .unwrap_or(InteractiveEndReason::InputEof);
    let mut end_events = Vec::new();
    if matches!(reason, InteractiveEndReason::InputEof) {
        // Check once for an exit that raced with stdin closing before deciding
        // whether there is a live process to detach.
        end_events = drain_events(session.debugger.as_mut());
        if session.debugger.get_state().status == DebugStatus::Terminated {
            reason = InteractiveEndReason::TargetExited;
        }
    }

    let terminal = matches!(
        session.debugger.get_state().status,
        DebugStatus::Terminated | DebugStatus::Detached
    );
    let cleanup = if terminal || !session.debugger.is_attached() {
        json!({ "status": "not_needed" })
    } else {
        match session.debugger.detach() {
            Ok(()) => json!({ "status": "detached" }),
            Err(error) => json!({ "status": "failed", "error": format!("{error:#}") }),
        }
    };
    let end = interactive_end_frame(
        session.debugger.as_mut(),
        thread_id,
        &session_id,
        &target,
        backend_name,
        if run_result.is_err() {
            "io_error"
        } else {
            reason.as_str()
        },
        cleanup,
        end_events,
    );

    match run_result {
        Ok(_) => write_json_frame(&mut output, &end),
        Err(error) => {
            // A broken output pipe still must not leave a native target traced.
            let _ = write_json_frame(&mut output, &end);
            Err(error)
        }
    }
}

pub fn run_session(args: DebugSessionArgs, use_emulator: bool) -> Result<()> {
    if args.interactive {
        return run_interactive_session(args, use_emulator);
    }
    let commands = collect_commands(&args)?;
    let (mut session, pid, thread_id) = start_session(&args, use_emulator)?;
    let restored = restore_saved_breakpoints(session.debugger.as_mut(), args.path.as_deref());

    let mut results: Vec<Value> = Vec::new();
    let mut failed = false;
    for line in &commands {
        let outcome = parse_command(line)
            .and_then(|command| execute(session.debugger.as_mut(), thread_id, command));
        let events = drain_events(session.debugger.as_mut());
        let mut entry = match outcome {
            Ok(mut value) => {
                if let Some(map) = value.as_object_mut() {
                    map.insert("status".into(), json!("ok"));
                }
                value
            }
            Err(error) => {
                failed = true;
                json!({ "status": "error", "error": format!("{error:#}") })
            }
        };
        if let Some(map) = entry.as_object_mut() {
            map.insert("command".into(), json!(line));
            if !events.is_empty() {
                map.insert("events".into(), json!(events));
            }
        }
        results.push(entry);
        if failed && !args.keep_going {
            break;
        }
    }

    // A script, on the machine this session has been driving: the command
    // list has no loops and no conditions, and handing the script its own
    // freshly launched copy of the program would throw away everything the
    // commands above just set up.
    let mut script_result = None;
    if let Some(path) = &args.rhai {
        if failed && !args.keep_going {
            // The session already stopped; running a script on a machine in
            // an unknown state would report something nobody asked for.
        } else {
            let binary_path = args
                .path
                .as_deref()
                .context("--rhai requires a launched binary path")?;
            match run_rhai(&mut session, binary_path, path) {
                Ok(value) => script_result = Some(value),
                Err(error) => {
                    failed = true;
                    script_result =
                        Some(json!({ "status": "error", "error": format!("{error:#}") }));
                }
            }
        }
    }

    let state = session.debugger.get_state();
    let final_pc = session
        .debugger
        .fetch_registers(thread_id)
        .ok()
        .map(|r| format!("0x{:x}", r.pc));
    let mut report = json!({
        "binary": args.path,
        "pid": pid,
        "restored": restored,
        "backend": if use_emulator { "emulator" } else { "native" },
        "results": results,
        "final": {
            "status": format!("{:?}", state.status),
            "pc": final_pc,
            "stop": session.debugger.stop_reason(),
        },
    });

    if let (Some(script), Some(map)) = (script_result, report.as_object_mut()) {
        map.insert("script".into(), script);
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }

    // A failed command is a failed session unless the caller asked to carry
    // on: an agent reading an exit code should not have to parse the JSON to
    // find out something went wrong.
    if failed && !args.keep_going {
        bail!("session stopped at a failed command");
    }
    Ok(())
}

fn print_human(report: &Value) {
    println!(
        "session: {} ({})",
        report["binary"].as_str().unwrap_or(""),
        report["backend"].as_str().unwrap_or("")
    );
    for entry in report["results"].as_array().into_iter().flatten() {
        let command = entry["command"].as_str().unwrap_or("");
        match entry["status"].as_str() {
            Some("ok") => {
                let detail: Vec<String> = entry
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter(|(k, _)| {
                        !matches!(k.as_str(), "status" | "command" | "action" | "events")
                    })
                    .map(|(k, v)| format!("{k}={}", compact(v)))
                    .collect();
                println!("  {command:<28} ok   {}", detail.join(" "));
            }
            _ => println!(
                "  {command:<28} FAILED  {}",
                entry["error"].as_str().unwrap_or("")
            ),
        }
        for event in entry["events"].as_array().into_iter().flatten() {
            println!("      · {}", compact(event));
        }
    }
    if let Some(script) = report.get("script") {
        println!("script: {}", script["status"].as_str().unwrap_or("error"));
        for finding in script["findings"].as_array().into_iter().flatten() {
            let kind = finding["kind"].as_str().unwrap_or("finding");
            let address = finding["address"].as_str().unwrap_or("");
            let message = finding["message"].as_str().unwrap_or("");
            let data = finding
                .get("data")
                .filter(|d| !d.is_null())
                .map(|d| d.to_string())
                .unwrap_or_default();
            println!("  {kind:<16} {address:<18} {message} {data}");
        }
        for d in script["diagnostics"].as_array().into_iter().flatten() {
            println!(
                "  [{}] {}",
                d["severity"].as_str().unwrap_or("error"),
                d["message"].as_str().unwrap_or("")
            );
        }
        if let Some(error) = script.get("error").and_then(|e| e.as_str()) {
            println!("  FAILED  {error}");
        }
    }
    println!(
        "final: {} at {}",
        report["final"]["status"].as_str().unwrap_or("?"),
        report["final"]["pc"].as_str().unwrap_or("?")
    );
}

fn compact(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Hand the session's machine to a Rhai script and fold its findings in.
///
/// The backend is taken out of the session for the duration: the script owns
/// it while it runs, and gives it back so the report's closing state is the
/// real one.
fn run_rhai(
    session: &mut fission_dynamic::debug::DebugSession,
    binary_path: &str,
    script_path: &str,
) -> Result<Value> {
    let source = std::fs::read_to_string(script_path)
        .with_context(|| format!("read the script at {script_path}"))?;
    let binary = fission_loader::loader::LoadedBinary::from_file(binary_path)
        .with_context(|| format!("re-read {binary_path} for the script's view of it"))?;

    let placeholder: Box<dyn ExecutionBackend> =
        Box::new(fission_dynamic::debug::emulator_backend::EmulatorBackend::new());
    let backend = std::mem::replace(&mut session.debugger, placeholder);
    let machine = fission_script::MachineHost::adopt(backend, &binary);

    let limits = fission_script::ScriptLimits {
        max_runtime_ms: 30_000,
        max_operations: 100_000_000,
        ..fission_script::ScriptLimits::default()
    };
    let result = fission_script::run_script_on_machine(
        &binary,
        &source,
        script_path,
        limits,
        machine.clone(),
    );

    if let Some(backend) = machine.into_backend() {
        session.debugger = backend;
    }

    let value = serde_json::to_value(&result).context("serialise the script result")?;
    match result.status {
        fission_script::ScriptRunStatus::Ok => Ok(value),
        other => Err(anyhow::anyhow!(
            "script finished with status {other:?}: {}",
            result
                .diagnostics
                .first()
                .map(|d| d.message.as_str())
                .unwrap_or("no diagnostic")
        )),
    }
}
