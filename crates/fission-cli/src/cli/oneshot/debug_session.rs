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

use crate::cli::args::{DebugCommand, DebugSessionArgs, MemoryBpKindArg};
use fission_dynamic::debug::traits::ExecutionBackend;
use fission_dynamic::debug::types::{DebugEvent, MemoryBpKind, RegisterState};

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
    if commands.is_empty() {
        bail!("no commands: pass at least one -c/--command, or --script");
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

fn event_json(event: &DebugEvent) -> Value {
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
            json!({
                "action": "continue",
                "pc": format!("0x{:x}", backend.fetch_registers(thread_id)?.pc),
                // Why it stopped, which `Suspended` cannot say: a breakpoint
                // and a program that ran out of code look the same otherwise.
                "stop": backend.stop_reason(),
            })
        }
        DebugCommand::Step => {
            backend.single_step()?;
            json!({
                "action": "step",
                "pc": format!("0x{:x}", backend.fetch_registers(thread_id)?.pc),
                // Why it stopped, which `Suspended` cannot say: a breakpoint
                // and a program that ran out of code look the same otherwise.
                "stop": backend.stop_reason(),
            })
        }
        DebugCommand::StepOver => {
            backend.step_over()?;
            json!({
                "action": "step_over",
                "pc": format!("0x{:x}", backend.fetch_registers(thread_id)?.pc),
                // Why it stopped, which `Suspended` cannot say: a breakpoint
                // and a program that ran out of code look the same otherwise.
                "stop": backend.stop_reason(),
            })
        }
        DebugCommand::StepOut => {
            backend.step_out()?;
            json!({
                "action": "step_out",
                "pc": format!("0x{:x}", backend.fetch_registers(thread_id)?.pc),
                // Why it stopped, which `Suspended` cannot say: a breakpoint
                // and a program that ran out of code look the same otherwise.
                "stop": backend.stop_reason(),
            })
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
        DebugCommand::Event => match backend.poll_event(0)? {
            Some(event) => json!({ "action": "event", "event": event_json(&event) }),
            None => json!({ "action": "event", "event": Value::Null }),
        },
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

pub fn run_session(args: DebugSessionArgs, use_emulator: bool) -> Result<()> {
    let commands = collect_commands(&args)?;

    let mut session = {
        let mut builder = fission_dynamic::debug::DebugSession::new();
        if use_emulator {
            builder = builder.with_emulator();
        }
        builder.build()
    };

    let pid = session
        .debugger
        .launch(&args.path, &[])
        .with_context(|| format!("failed to launch {}", args.path))?;
    let thread_id = session.debugger.get_state().main_thread_id.unwrap_or(1);

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

    let state = session.debugger.get_state();
    let final_pc = session
        .debugger
        .fetch_registers(thread_id)
        .ok()
        .map(|r| format!("0x{:x}", r.pc));
    let report = json!({
        "binary": args.path,
        "pid": pid,
        "backend": if use_emulator { "emulator" } else { "native" },
        "results": results,
        "final": {
            "status": format!("{:?}", state.status),
            "pc": final_pc,
            "stop": session.debugger.stop_reason(),
        },
    });

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
