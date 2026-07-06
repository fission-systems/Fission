//! Debugger CLI dispatch (Windows only)
//!
//! One-shot model: each command loads the session state from a temp file,
//! executes the operation, prints output, and persists state back.

use crate::cli::args::DebugCommand;
#[cfg(target_os = "windows")]
use crate::cli::args::HwBpKindArg;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Default)]
struct DebugStateFile {
    pid: u32,
    last_thread_id: Option<u32>,
    breakpoints: Vec<u64>,
}

fn state_path(pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("fission-debug-{pid}.json"))
}

fn save_state(state: &DebugStateFile) -> Result<()> {
    let path = state_path(state.pid);
    let data = serde_json::to_string_pretty(state)?;
    let mut file = std::fs::File::create(&path)?;
    file.write_all(data.as_bytes())?;
    Ok(())
}

fn remove_state(pid: u32) {
    let _ = std::fs::remove_file(state_path(pid));
}

fn hex_bytes_from_str(s: &str) -> Result<Vec<u8>> {
    let s = s.replace(',', "").replace(' ', "");
    if s.len() % 2 != 0 {
        anyhow::bail!("hex data length must be even (got {})", s.len());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .with_context(|| format!("Invalid hex data: {}", s))
}

fn print_regs(regs: &fission_dynamic::debug::types::RegisterState, json: bool) {
    if json {
        let obj = serde_json::json!({
            "rax": regs.rax,
            "rbx": regs.rbx,
            "rcx": regs.rcx,
            "rdx": regs.rdx,
            "rsi": regs.rsi,
            "rdi": regs.rdi,
            "rbp": regs.rbp,
            "rsp": regs.rsp,
            "r8": regs.r8,
            "r9": regs.r9,
            "r10": regs.r10,
            "r11": regs.r11,
            "r12": regs.r12,
            "r13": regs.r13,
            "r14": regs.r14,
            "r15": regs.r15,
            "rip": regs.rip,
            "rflags": regs.rflags,
        });
        println!("{}", serde_json::to_string_pretty(&obj).unwrap());
    } else {
        println!(
            "RAX={:016x} RBX={:016x} RCX={:016x} RDX={:016x}",
            regs.rax, regs.rbx, regs.rcx, regs.rdx
        );
        println!(
            "RSI={:016x} RDI={:016x} RBP={:016x} RSP={:016x}",
            regs.rsi, regs.rdi, regs.rbp, regs.rsp
        );
        println!(
            "R8 ={:016x} R9 ={:016x} R10={:016x} R11={:016x}",
            regs.r8, regs.r9, regs.r10, regs.r11
        );
        println!(
            "R12={:016x} R13={:016x} R14={:016x} R15={:016x}",
            regs.r12, regs.r13, regs.r14, regs.r15
        );
        println!("RIP={:016x} RFLAGS={:016x}", regs.rip, regs.rflags);
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
    use fission_dynamic::debug::traits::ExecutionBackend;
    let emulator = args.emulator;
    let build_session = || {
        let mut builder = fission_dynamic::debug::DebugSession::new();
        if emulator {
            builder = builder.with_emulator();
        }
        builder.build()
    };

    match args.command {
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
            let entries: Vec<_> = std::fs::read_dir(std::env::temp_dir())?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map_or(false, |n| n.starts_with("fission-debug-"))
                })
                .collect();
            if entries.is_empty() {
                anyhow::bail!("No active debug session found.");
            }
            let mut detached = 0usize;
            for entry in entries {
                let name = entry.file_name();
                let fname = name.to_string_lossy();
                if let Some(pid_str) = fname
                    .strip_prefix("fission-debug-")
                    .and_then(|s| s.strip_suffix(".json"))
                {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        let mut session = build_session();
                        let _ = session.attach(pid);
                        let _ = session.detach();
                        remove_state(pid);
                        detached += 1;
                    }
                }
            }
            println!("Detached from {} session(s).", detached);
            Ok(())
        }

        DebugCommand::Continue => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.continue_execution()?;
            println!("Continuing PID {}...", state.pid);
            Ok(())
        }

        DebugCommand::Step => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.single_step()?;

            if let Some(tid) = state.last_thread_id {
                let regs = session.debugger.fetch_registers(tid)?;
                print_regs(&regs, false);
            }
            Ok(())
        }

        DebugCommand::Bp(args) => {
            let mut state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let mut state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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

        DebugCommand::HwBp(args) => {
            #[cfg(target_os = "windows")]
            {
                let state = find_active_state()?;
                let mut session = build_session();
                session.attach(state.pid)?;

                let kind = match args.kind {
                    HwBpKindArg::Execute => {
                        fission_dynamic::debug::types::HwBreakpointKind::Execute
                    }
                    HwBpKindArg::Write => fission_dynamic::debug::types::HwBreakpointKind::Write,
                    HwBpKindArg::ReadWrite => {
                        fission_dynamic::debug::types::HwBreakpointKind::ReadWrite
                    }
                };
                session.debugger.set_hw_breakpoint(args.addr, kind)?;
                if args.json {
                    println!(
                        "{{\"status\":\"ok\",\"action\":\"set_hw_breakpoint\",\"address\":\"0x{:x}\"}}",
                        args.addr
                    );
                } else {
                    println!("Hardware breakpoint set at 0x{:016x}", args.addr);
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = args;
                println!("Hardware breakpoints are only supported on Windows.");
            }
            Ok(())
        }

        DebugCommand::Regs => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            let tid = state
                .last_thread_id
                .or(session.debugger.get_state().main_thread_id)
                .ok_or_else(|| anyhow::anyhow!("No thread available"))?;
            let regs = session.debugger.fetch_registers(tid)?;
            print_regs(&regs, false);
            Ok(())
        }

        DebugCommand::Read(args) => {
            find_active_state()?;
            let session = build_session();
            let mem = session.debugger.read_memory(args.addr, args.size)?;
            print_hex_dump(args.addr, &mem, args.json);
            Ok(())
        }

        DebugCommand::Write(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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

        DebugCommand::Modules => {
            find_active_state()?;
            let session = build_session();
            let modules = session.debugger.get_state().modules.clone();
            println!("Loaded modules ({}):", modules.len());
            for (base, info) in modules {
                println!("  {:016x} - {:016x}  {}", base, base + info.size, info.name);
            }
            Ok(())
        }

        DebugCommand::Threads => {
            find_active_state()?;
            let session = build_session();
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
                last_thread_id: session.debugger.get_state().main_thread_id,
                breakpoints: Vec::new(),
            };
            save_state(&state)?;

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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.pause()?;
            println!("Break requested for PID {}.", state.pid);
            Ok(())
        }

        DebugCommand::Stop => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.terminate()?;
            remove_state(state.pid);
            println!("Terminated PID {}.", state.pid);
            Ok(())
        }

        DebugCommand::StepOver => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.step_over()?;
            if let Some(tid) = state.last_thread_id {
                let regs = session.debugger.fetch_registers(tid)?;
                print_regs(&regs, false);
            }
            Ok(())
        }

        DebugCommand::StepOut => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.step_out()?;
            if let Some(tid) = state.last_thread_id {
                let regs = session.debugger.fetch_registers(tid)?;
                print_regs(&regs, false);
            }
            Ok(())
        }

        DebugCommand::Skip => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.skip_instruction()?;
            if let Some(tid) = state.last_thread_id {
                let regs = session.debugger.fetch_registers(tid)?;
                print_regs(&regs, false);
            }
            Ok(())
        }

        DebugCommand::SwitchThread(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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

        DebugCommand::Event => {
            #[cfg(target_os = "windows")]
            {
                let state = find_active_state()?;
                let mut session = build_session();
                session.attach(state.pid)?;
                let event = session.debugger.poll_event(5000)?;
                if let Some(evt) = event {
                    println!("{:?}", evt);
                } else {
                    println!("No event within timeout.");
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                println!("poll_event is only supported on Windows.");
            }
            Ok(())
        }

        DebugCommand::BpEnable(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            find_active_state()?;
            let session = build_session();
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.set_dll_breakpoint(&args.name)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"set_dll_breakpoint\",\"dll\":\"{}\"}}",
                    args.name
                );
            } else {
                println!("DLL breakpoint set for '{}'", args.name);
            }
            Ok(())
        }

        DebugCommand::RmDllBp(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            session.debugger.remove_dll_breakpoint(&args.name)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"remove_dll_breakpoint\",\"dll\":\"{}\"}}",
                    args.name
                );
            } else {
                println!("DLL breakpoint removed for '{}'", args.name);
            }
            Ok(())
        }

        DebugCommand::ExBp(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            let tid = state
                .last_thread_id
                .ok_or_else(|| anyhow::anyhow!("No thread available"))?;
            session.debugger.set_register(tid, &args.name, args.value)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"set_register\",\"register\":\"{}\",\"value\":\"0x{:x}\"}}",
                    args.name, args.value
                );
            } else {
                println!("Register {} = 0x{:016x}", args.name, args.value);
            }
            Ok(())
        }

        DebugCommand::GetFlag(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            let value = session.debugger.get_flag(&args.name)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"flag\":\"{}\",\"value\":{}}}",
                    args.name, value
                );
            } else {
                println!("Flag {} = {}", args.name, value);
            }
            Ok(())
        }

        DebugCommand::SetFlag(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
            let value = args
                .value
                .ok_or_else(|| anyhow::anyhow!("Flag value required for set-flag"))?;
            session.debugger.set_flag(&args.name, value)?;
            if args.json {
                println!(
                    "{{\"status\":\"ok\",\"action\":\"set_flag\",\"flag\":\"{}\",\"value\":{}}}",
                    args.name, value
                );
            } else {
                println!("Flag {} = {}", args.name, value);
            }
            Ok(())
        }

        DebugCommand::Alloc(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            find_active_state()?;
            let session = build_session();
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
            find_active_state()?;
            let mut session = build_session();
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
            find_active_state()?;
            let mut session = build_session();
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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

        DebugCommand::Imports(args) => {
            let state = find_active_state()?;
            let mut session = build_session();
            session.attach(state.pid)?;
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

fn find_active_state() -> Result<DebugStateFile> {
    let entries: Vec<_> = std::fs::read_dir(std::env::temp_dir())?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map_or(false, |n| n.starts_with("fission-debug-"))
        })
        .collect();
    if entries.is_empty() {
        anyhow::bail!("No active debug session. Run `fission_cli debug attach <pid>` first.");
    }
    let entry = entries.into_iter().next().unwrap();
    let data = std::fs::read_to_string(entry.path())?;
    let state: DebugStateFile = serde_json::from_str(&data)?;
    Ok(state)
}
