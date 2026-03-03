//! Live debugging — attach/detach, breakpoints, single-step, memory reads, and TTD integration.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use tauri::State;

// ============================================================================
// Windows-only: event-drain helper
// ============================================================================

/// Drain all pending OS debug events from the crossbeam channel and apply
/// them to `debug_state`.  Acquires the debugger lock first (for register
/// reads) then releases it before acquiring debug_state, so we never hold
/// both locks at the same time.
#[cfg(target_os = "windows")]
async fn drain_events_into_state(state: &AppState) {
    use fission_analysis::debug::traits::Debugger;
    use fission_analysis::debug::types::DebugEvent;

    // Step 1: non-blocking drain into a local Vec (very short lock)
    let events: Vec<DebugEvent> = {
        match state.debug_event_rx.lock() {
            Ok(guard) => guard
                .as_ref()
                .map(|rx| std::iter::from_fn(|| rx.try_recv().ok()).collect())
                .unwrap_or_default(),
            Err(_) => return,
        }
        // std::sync::MutexGuard dropped here
    };

    if events.is_empty() {
        return;
    }

    // Step 2: fetch registers for stop-events (debugger lock, brief)
    let mut reg_cache: std::collections::HashMap<u32, RegisterStateDto> =
        std::collections::HashMap::new();
    {
        let mut dbg = state.debugger.lock().await;
        if let Some(ref mut d) = *dbg {
            for evt in &events {
                let tid = match evt {
                    DebugEvent::BreakpointHit { thread_id, .. } => Some(*thread_id),
                    DebugEvent::SingleStep { thread_id } => Some(*thread_id),
                    _ => None,
                };
                if let Some(tid) = tid {
                    if let Ok(regs) = d.fetch_registers(tid) {
                        reg_cache.insert(tid, RegisterStateDto::from(regs));
                    }
                }
            }
        }
        // debugger lock dropped here
    }

    // Step 2b: record TTD steps if the timeline is actively recording.
    {
        let mut timeline = state.timeline.lock().await;
        if timeline.is_recording() {
            for evt in &events {
                if let DebugEvent::SingleStep { thread_id } = evt {
                    if let Some(regs) = reg_cache.get(thread_id) {
                        use fission_analysis::debug::types::RegisterState;
                        let rs = RegisterState {
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
                            rflags: regs.rflags,
                        };
                        timeline.record_step_internal(rs, *thread_id);
                    }
                }
            }
        }
        // timeline lock dropped here
    }

    // Step 3: apply events to DTO (debug_state lock)
    let mut ds = state.debug_state.lock().await;
    // Trim log to avoid unbounded growth
    if ds.events.len() + events.len() > 500 {
        let keep = ds.events.len().saturating_sub(events.len());
        let current_len = ds.events.len();
        let drain_end = current_len - keep.min(current_len);
        ds.events.drain(..drain_end);
    }
    for evt in events {
        match evt {
            DebugEvent::BreakpointHit { address, thread_id } => {
                ds.status = DebugStatusDto::Suspended;
                let msg = format!("[bp hit] 0x{:x} (tid {})", address, thread_id);
                ds.last_event = Some(msg.clone());
                ds.events.push(msg);
                ds.registers = reg_cache.remove(&thread_id);
            }
            DebugEvent::SingleStep { thread_id } => {
                ds.status = DebugStatusDto::Suspended;
                let msg = format!("[step] tid {}", thread_id);
                ds.last_event = Some(msg.clone());
                ds.events.push(msg);
                ds.registers = reg_cache.remove(&thread_id);
            }
            DebugEvent::ProcessExited { exit_code } => {
                ds.status = DebugStatusDto::Terminated;
                let msg = format!("[exit] Process exited (code {})", exit_code);
                ds.last_event = Some(msg.clone());
                ds.events.push(msg);
                ds.attached_pid = None;
                ds.registers = None;
            }
            DebugEvent::ProcessCreated {
                pid,
                main_thread_id,
            } => {
                let msg = format!("[create] PID {} main_tid {}", pid, main_thread_id);
                ds.events.push(msg.clone());
                ds.last_event = Some(msg);
            }
            DebugEvent::ThreadCreated { thread_id } => {
                ds.events.push(format!("[thread+] tid {}", thread_id));
            }
            DebugEvent::ThreadExited { thread_id } => {
                ds.events.push(format!("[thread-] tid {}", thread_id));
            }
            DebugEvent::DllLoaded { name, base_address } => {
                ds.events
                    .push(format!("[dll] {} @ 0x{:x}", name, base_address));
            }
            DebugEvent::Exception {
                code,
                address,
                first_chance,
            } => {
                let chance = if first_chance { "1st" } else { "2nd" };
                let msg = format!(
                    "[exc] 0x{:x} @ 0x{:x} ({})",
                    code, address, chance
                );
                ds.last_event = Some(msg.clone());
                ds.events.push(msg);
            }
        }
    }
}

// ============================================================================
// Commands
// ============================================================================

/// Return the current debug session state (always safe to call).
/// On Windows, drains any pending OS debug events before returning.
#[tauri::command]
pub async fn debug_get_state(state: State<'_, AppState>) -> CmdResult<DebugStateDto> {
    #[cfg(target_os = "windows")]
    drain_events_into_state(&state).await;
    let ds = state.debug_state.lock().await;
    Ok(ds.clone())
}

/// Attach to a running process by PID.
/// On non-Windows builds returns an error immediately.
#[tauri::command]
pub async fn debug_attach(pid: u32, state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::{
            traits::Debugger,
            windows::{start_event_loop, WindowsDebugger},
        };

        // Guard: already attached?
        {
            let ds = state.debug_state.lock().await;
            if ds.attached_pid.is_some() {
                return Err(CmdError::other("Already attached to a process"));
            }
        }

        // Create & attach debugger
        {
            let mut dbg = state.debugger.lock().await;
            let d = dbg.get_or_insert_with(WindowsDebugger::new);
            d.attach(pid).map_err(CmdError::from)?;
        }

        // Wire up background event loop
        let (tx_events, rx_events) =
            crossbeam_channel::unbounded::<fission_analysis::debug::types::DebugEvent>();
        let (tx_stop, rx_stop) = crossbeam_channel::bounded::<()>(1);
        start_event_loop(pid, tx_events, rx_stop);
        
        // Safe: Handle poisoned mutex by recovering
        *state.debug_event_rx.lock().unwrap_or_else(|e| e.into_inner()) = Some(rx_events);
        *state.debug_stop_tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx_stop);

        // Update DTO
        let mut ds = state.debug_state.lock().await;
        ds.status = DebugStatusDto::Running;
        ds.attached_pid = Some(pid);
        ds.events.push(format!("[attach] Attached to PID {}", pid));
        ds.last_event = Some(format!("Attached to PID {}", pid));
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (pid, state);
        Err(CmdError::other(
            "Dynamic debugging is only supported on Windows",
        ))
    }
}

/// Detach from the currently attached process.
#[tauri::command]
pub async fn debug_detach(state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        // Signal event loop to stop, then clear channel handles
        {
            if let Ok(guard) = state.debug_stop_tx.lock() {
                if let Some(tx) = guard.as_ref() {
                    let _ = tx.send(());
                }
            }
        }
        
        // Safe: Handle poisoned mutex by recovering
        *state.debug_stop_tx.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *state.debug_event_rx.lock().unwrap_or_else(|e| e.into_inner()) = None;

        // Detach debugger
        {
            let mut dbg = state.debugger.lock().await;
            if let Some(ref mut d) = *dbg {
                d.detach().map_err(CmdError::from)?;
            }
            *dbg = None;
        }

        // Update DTO
        let mut ds = state.debug_state.lock().await;
        let pid = ds.attached_pid.unwrap_or_default();
        ds.events
            .push(format!("[detach] Detached from PID {}", pid));
        ds.status = DebugStatusDto::Detached;
        ds.attached_pid = None;
        ds.registers = None;
        ds.last_event = Some("Detached".to_string());
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
        Err(CmdError::other(
            "Dynamic debugging is only supported on Windows",
        ))
    }
}

/// Resume execution of a suspended process.
#[tauri::command]
pub async fn debug_continue(state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        let mut dbg = state.debugger.lock().await;
        let d = dbg.as_mut().ok_or_else(|| CmdError::other("Not attached"))?;
        d.continue_execution().map_err(CmdError::from)?;
        drop(dbg);

        let mut ds = state.debug_state.lock().await;
        ds.status = DebugStatusDto::Running;
        ds.last_event = Some("Continued".to_string());
        ds.events.push("[continue] Resumed execution".to_string());
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
        Err(CmdError::other(
            "Dynamic debugging is only supported on Windows",
        ))
    }
}

/// Single-step the suspended process.
#[tauri::command]
pub async fn debug_step(state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        let mut dbg = state.debugger.lock().await;
        let d = dbg.as_mut().ok_or_else(|| CmdError::other("Not attached"))?;
        d.single_step().map_err(CmdError::from)?;
        drop(dbg);

        let mut ds = state.debug_state.lock().await;
        ds.last_event = Some("Single-step issued".to_string());
        ds.events.push(
            "[step] Single-step issued (waiting for SINGLE_STEP event)".to_string(),
        );
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = state;
        Err(CmdError::other(
            "Dynamic debugging is only supported on Windows",
        ))
    }
}

/// Add a software breakpoint at `address`.
#[tauri::command]
pub async fn debug_add_breakpoint(address: u64, state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        let mut dbg = state.debugger.lock().await;
        let d = dbg.as_mut().ok_or_else(|| CmdError::other("Not attached"))?;
        d.set_sw_breakpoint(address).map_err(CmdError::from)?;
        drop(dbg);

        let mut ds = state.debug_state.lock().await;
        let addr_str = format!("0x{:x}", address);
        if !ds.breakpoints.iter().any(|bp| bp.address == addr_str) {
            ds.breakpoints.push(BreakpointInfoDto {
                address: addr_str.clone(),
                enabled: true,
            });
        }
        ds.events.push(format!("[bp+] Breakpoint at {}", addr_str));
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (address, state);
        Err(CmdError::other(
            "Dynamic debugging is only supported on Windows",
        ))
    }
}

/// Remove a software breakpoint at `address`.
#[tauri::command]
pub async fn debug_remove_breakpoint(address: u64, state: State<'_, AppState>) -> CmdResult<()> {
    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;

        let mut dbg = state.debugger.lock().await;
        let d = dbg.as_mut().ok_or_else(|| CmdError::other("Not attached"))?;
        d.remove_sw_breakpoint(address).map_err(CmdError::from)?;
        drop(dbg);

        let mut ds = state.debug_state.lock().await;
        let addr_str = format!("0x{:x}", address);
        ds.breakpoints.retain(|bp| bp.address != addr_str);
        ds.events
            .push(format!("[bp-] Removed breakpoint at {}", addr_str));
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (address, state);
        Err(CmdError::other(
            "Dynamic debugging is only supported on Windows",
        ))
    }
}

/// Read `size` bytes from the attached process starting at `address` (hex
/// string, e.g. `"0x401000"`) and return a formatted hex dump.
///
/// On non-Windows platforms this command always returns an error because there
/// is no live-debugging backend.
#[tauri::command]
pub async fn debug_read_memory(
    address: String,
    size: usize,
    state: State<'_, AppState>,
) -> CmdResult<String> {
    let addr = u64::from_str_radix(address.trim_start_matches("0x"), 16)
        .map_err(|_| CmdError::other(format!("Invalid address: {address}")))?;

    if size == 0 || size > fission_core::MAX_HEX_READ {
        return Err(CmdError::other(format!("Size must be 1–{} bytes", fission_core::MAX_HEX_READ)));
    }

    #[cfg(target_os = "windows")]
    {
        use fission_analysis::debug::traits::Debugger;
        let mut dbg = state.debugger.lock().await;
        let d = dbg
            .as_mut()
            .ok_or_else(|| CmdError::other("No process attached"))?;

        let bytes = d
            .read_memory(addr, size)
            .map_err(|e| CmdError::other(format!("ReadProcessMemory failed: {e}")))?;

        // Format as classic hex dump: 16 bytes per line.
        let mut out = String::new();
        for (chunk_idx, chunk) in bytes.chunks(16).enumerate() {
            let line_addr = addr + (chunk_idx as u64) * 16;
            let hex_part: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
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
            out.push_str(&format!(
                "0x{:016x}  {:<47}  {}\n",
                line_addr,
                hex_part.join(" "),
                ascii_part
            ));
        }
        return Ok(out);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (addr, state);
        Err(CmdError::other(
            "Memory dump is only supported on Windows (live debugger required)",
        ))
    }
}
