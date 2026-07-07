use anyhow::Result;
use crate::core::Emulator;
use crate::os::env::{HleResult, OsEnvironment};
use crate::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;
use std::sync::Mutex;
use crate::os::windows::heap::DummyHeap;

const MAGIC_BASE: u64 = 0xFFFFFFF000000000;

/// Windows PE execution environment.
///
/// - Import patching: overwrites IAT entries with sequential magic trampolines.
/// - Stub resolution: maps magic address back to import name.
/// - HLE dispatch: emulates Win32 API functions by name.
pub struct WindowsEnv {
    pub heap: Mutex<DummyHeap>,
}

impl WindowsEnv {
    pub fn new() -> Self {
        Self {
            heap: Mutex::new(DummyHeap::new(0x20000000)), // Dummy heap base
        }
    }
}

impl OsEnvironment for WindowsEnv {
    fn patch_imports(&self, state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
        if binary.format != "PE" {
            return Ok(());
        }
        let mut iat_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        iat_entries.sort_by_key(|&(&addr, _)| addr);
        for (i, (&addr, name)) in iat_entries.into_iter().enumerate() {
            let trampoline = MAGIC_BASE + (i as u64 * 8);
            tracing::debug!("IAT patch: {} @ 0x{:X} → trampoline 0x{:X}", name, addr, trampoline);
            state.write_space(3, addr, &trampoline.to_le_bytes())?;
        }
        Ok(())
    }

    fn resolve_stub(&self, binary: &LoadedBinary, magic_addr: u64) -> Option<String> {
        let index = ((magic_addr - MAGIC_BASE) / 8) as usize;
        let mut iat_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        iat_entries.sort_by_key(|&(&addr, _)| addr);
        iat_entries
            .into_iter()
            .nth(index)
            .map(|(_, name)| name.split('!').last().unwrap_or(name).to_string())
    }

    fn dispatch_hle(&self, emu: &mut Emulator, func_name: &str) -> Result<HleResult> {
        tracing::info!("HLE Intercept: {}", func_name);
        match func_name {
            "LoadLibraryA"   => handle_load_library_a(emu)?,
            "LoadLibraryW"   => handle_load_library_w(emu)?,
            "GetProcAddress" => handle_get_proc_address(emu)?,
            "VirtualAlloc"   => handle_virtual_alloc(emu)?,
            "VirtualFree"    => { emu.write_return_val(1)?; } // always succeed
            
            // Heap
            "GetProcessHeap" => { emu.write_return_val(0x20000000)?; } // dummy handle
            "HeapAlloc"      => handle_heap_alloc(emu, self)?,
            "HeapFree"       => handle_heap_free(emu, self)?,
            "HeapReAlloc"    => handle_heap_realloc(emu, self)?,
            
            // Console
            "GetStdHandle"   => { emu.write_return_val(0x77777777)?; } // dummy handle
            "WriteConsoleA"  => handle_write_console_a(emu)?,
            "WriteConsoleW"  => handle_write_console_w(emu)?,
            "ReadConsoleA"   => handle_read_console_a(emu)?,
            "ReadConsoleW"   => handle_read_console_w(emu)?,
            "AllocConsole"   => { emu.write_return_val(1)?; }
            
            // Thread / Sync
            "CreateThread"   => handle_create_thread(emu)?,
            "WaitForSingleObject" => { emu.write_return_val(0)?; } // WAIT_OBJECT_0
            "CloseHandle"    => { emu.write_return_val(1)?; }
            "InitializeCriticalSection" | "EnterCriticalSection" | "LeaveCriticalSection" | "DeleteCriticalSection" => {}
            
            // Str / Mem
            "lstrcpyA"       => handle_lstrcpy_a(emu)?,
            "lstrcatA"       => handle_lstrcat_a(emu)?,
            "lstrlenA"       => handle_lstrlen_a(emu)?,
            "RtlMoveMemory"  => handle_rtl_move_memory(emu)?,
            
            // Module
            "GetModuleHandleA" | "GetModuleHandleW" => { emu.write_return_val(0x140000000)?; }
            "GetModuleFileNameA" => handle_get_module_file_name_a(emu)?,
            
            "ExitProcess"    => {
                let code = emu.read_arg(0).unwrap_or(0) as u32;
                tracing::info!("ExitProcess({}). Emulation finished.", code);
                return Ok(HleResult::Halt(code));
            }
            // Time / Process identity (deterministic, tick-based)
            "Sleep" => {
                let ms = emu.read_arg(0).unwrap_or(0);
                // Advance simulated tick by requested ms (no real sleep).
                emu.tick_count = emu.tick_count.wrapping_add(ms);
                tracing::debug!("Sleep({}ms) — tick_count now {}", ms, emu.tick_count);
                emu.write_return_val(0)?;
            }
            "GetTickCount" => {
                emu.tick_count = emu.tick_count.wrapping_add(15); // ~15ms per call
                emu.write_return_val(emu.tick_count & 0xFFFF_FFFF)?;
            }
            "GetTickCount64" => {
                emu.tick_count = emu.tick_count.wrapping_add(15);
                emu.write_return_val(emu.tick_count)?;
            }
            "GetSystemTimeAsFileTime" => {
                // lpSystemTimeAsFileTime: pointer in RCX (arg 0)
                // Return a deterministic fake FILETIME (100-ns intervals since 1601-01-01).
                // Use a fixed base (2025-01-01 00:00:00 UTC) plus tick_count offset.
                let file_time_base: u64 = 133_800_000_000_000_000; // ~2025-01-01 in FILETIME units
                let fake_time = file_time_base.wrapping_add(emu.tick_count.wrapping_mul(10_000));
                let ptr = emu.read_arg(0).unwrap_or(0);
                if ptr != 0 {
                    let _ = emu.state.write_space(3, ptr, &fake_time.to_le_bytes());
                }
                emu.tick_count = emu.tick_count.wrapping_add(1);
                // void return
            }
            "QueryPerformanceCounter" => {
                // lpPerformanceCount: pointer in RCX
                let ptr = emu.read_arg(0).unwrap_or(0);
                if ptr != 0 {
                    let counter = emu.tick_count.wrapping_mul(10_000);
                    let _ = emu.state.write_space(3, ptr, &counter.to_le_bytes());
                }
                emu.tick_count = emu.tick_count.wrapping_add(1);
                emu.write_return_val(1)?; // TRUE = success
            }
            "QueryPerformanceFrequency" => {
                let ptr = emu.read_arg(0).unwrap_or(0);
                if ptr != 0 {
                    // Return 10,000,000 Hz (100-ns resolution, matches FILETIME)
                    let freq: u64 = 10_000_000;
                    let _ = emu.state.write_space(3, ptr, &freq.to_le_bytes());
                }
                emu.write_return_val(1)?;
            }
            "GetCurrentProcessId" => {
                emu.write_return_val(1337)?; // Fixed fake PID
            }
            "GetCurrentThreadId" => {
                emu.write_return_val(1)?; // Fixed fake TID
            }
            "GetLastError" => {
                emu.write_return_val(0)?; // ERROR_SUCCESS
            }
            "SetLastError" => {
                // Ignore the error code set by the guest.
            }
            "IsDebuggerPresent" => {
                emu.write_return_val(0)?; // not being debugged
            }
            "CheckRemoteDebuggerPresent" => {
                let ptr = emu.read_arg(1).unwrap_or(0);
                if ptr != 0 {
                    let _ = emu.state.write_space(3, ptr, &[0u8; 4]); // FALSE
                }
                emu.write_return_val(1)?; // TRUE = success
            }
            "OutputDebugStringA" | "OutputDebugStringW" => {
                // Silently ignore debug output.
            }
            "GetEnvironmentVariableA" | "GetEnvironmentVariableW" => {
                emu.write_return_val(0)?; // not found
            }
            "TerminateProcess" => {
                let code = emu.read_arg(1).unwrap_or(0) as u32;
                tracing::info!("TerminateProcess(code={}). Emulation finished.", code);
                return Ok(HleResult::Halt(code));
            }
            _ => {
                tracing::warn!("Unimplemented Win32 API: {}. Returning 0.", func_name);
                emu.write_return_val(0)?;
            }
        }
        Ok(HleResult::Continue)
    }

    fn dispatch_userop(
        &self,
        _emu: &mut Emulator,
        userop_name: &str,
        inputs: &[u64],
        _output_size: u32,
    ) -> Result<HleResult> {
        match userop_name {
            "segment_gs" | "segment_fs" => {
                let offset = inputs.get(0).copied().unwrap_or(0);
                tracing::debug!("Win32 HLE: {} (offset=0x{:X})", userop_name, offset);
                // TEB is at fs/gs. We don't have a full TEB mapped yet,
                // but we could set an output varnode if we extended the architecture.
                // For now, logging it handles the requirement.
            }
            "lock" | "rep" | "repne" | "repe" => {
                tracing::debug!("Win32 HLE: Prefix userop '{}'", userop_name);
            }
            "rdtsc" | "cpuid" => {
                tracing::info!("Win32 HLE: Instruct userop '{}' called", userop_name);
            }
            _ => {
                tracing::debug!("Win32 HLE: Unhandled USEROP: {} (inputs: {:?})", userop_name, inputs);
            }
        }
        Ok(HleResult::Continue)
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn read_string(emu: &mut Emulator, addr: u64) -> Result<String> {
    let mut bytes = Vec::new();
    let mut cur = addr;
    loop {
        let b = emu.state.read_space(3, cur, 1)?[0];
        if b == 0 { break; }
        bytes.push(b);
        cur += 1;
        if bytes.len() > 4096 { break; }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_wide_string(emu: &mut Emulator, addr: u64) -> Result<String> {
    let mut chars = Vec::new();
    let mut cur = addr;
    loop {
        let pair = emu.state.read_space(3, cur, 2)?;
        let wc = pair[0] as u16 | ((pair[1] as u16) << 8);
        if wc == 0 { break; }
        chars.push(wc);
        cur += 2;
        if chars.len() > 4096 { break; }
    }
    Ok(String::from_utf16_lossy(&chars))
}

// ── Win32 API handlers ────────────────────────────────────────────────────────

fn handle_load_library_a(emu: &mut Emulator) -> Result<()> {
    let addr = emu.read_arg(0)?;
    let name = if addr == 0 { String::from("<null>") } else { read_string(emu, addr)? };
    tracing::info!("LoadLibraryA(\"{}\")", name);
    emu.write_return_val(0x10000000)?; // dummy HMODULE
    Ok(())
}

fn handle_load_library_w(emu: &mut Emulator) -> Result<()> {
    let addr = emu.read_arg(0)?;
    let name = if addr == 0 { String::from("<null>") } else { read_wide_string(emu, addr)? };
    tracing::info!("LoadLibraryW(\"{}\")", name);
    emu.write_return_val(0x10000001)?; // dummy HMODULE
    Ok(())
}

fn handle_get_proc_address(emu: &mut Emulator) -> Result<()> {
    let h_module = emu.read_arg(0)?;
    let name_ptr = emu.read_arg(1)?;
    let proc_name = if name_ptr < 0xFFFF {
        format!("Ordinal({})", name_ptr)
    } else {
        read_string(emu, name_ptr)?
    };
    tracing::info!("GetProcAddress(0x{:X}, \"{}\")", h_module, proc_name);
    emu.write_return_val(0x20000000)?; // dummy FARPROC
    Ok(())
}

fn handle_virtual_alloc(emu: &mut Emulator) -> Result<()> {
    let lp_address = emu.read_arg(0)?;
    let dw_size    = emu.read_arg(1)?;
    let alloc_type = emu.read_arg(2)?;
    let protect    = emu.read_arg(3)?;
    tracing::info!("VirtualAlloc(0x{:X}, 0x{:X}, 0x{:X}, 0x{:X})", lp_address, dw_size, alloc_type, protect);
    emu.write_return_val(0x30000000)?; // dummy allocated address
    Ok(())
}

fn handle_heap_alloc(emu: &mut Emulator, env: &WindowsEnv) -> Result<()> {
    let _h_heap = emu.read_arg(0)?;
    let flags   = emu.read_arg(1)?;
    let bytes   = emu.read_arg(2)? as usize;
    let addr = env.heap.lock().unwrap().alloc(bytes);
    if (flags & 8) != 0 { // HEAP_ZERO_MEMORY
        let zeros = vec![0u8; bytes];
        emu.state.write_space(3, addr, &zeros)?;
    }
    tracing::info!("HeapAlloc(size: {}) -> 0x{:X}", bytes, addr);
    emu.write_return_val(addr)?;
    Ok(())
}

fn handle_heap_free(emu: &mut Emulator, env: &WindowsEnv) -> Result<()> {
    let _h_heap = emu.read_arg(0)?;
    let _flags  = emu.read_arg(1)?;
    let addr    = emu.read_arg(2)?;
    let success = env.heap.lock().unwrap().free(addr);
    tracing::info!("HeapFree(0x{:X}) -> {}", addr, success);
    emu.write_return_val(if success { 1 } else { 0 })?;
    Ok(())
}

fn handle_heap_realloc(emu: &mut Emulator, env: &WindowsEnv) -> Result<()> {
    let _h_heap = emu.read_arg(0)?;
    let flags   = emu.read_arg(1)?;
    let addr    = emu.read_arg(2)?;
    let bytes   = emu.read_arg(3)? as usize;
    if let Some(new_addr) = env.heap.lock().unwrap().realloc(addr, bytes) {
        if (flags & 8) != 0 {
            // zeroing memory might be needed but for dummy it's fine to skip diff
        }
        tracing::info!("HeapReAlloc(0x{:X}, {}) -> 0x{:X}", addr, bytes, new_addr);
        emu.write_return_val(new_addr)?;
    } else {
        emu.write_return_val(0)?;
    }
    Ok(())
}

fn handle_write_console_a(emu: &mut Emulator) -> Result<()> {
    let _h_console = emu.read_arg(0)?;
    let buf_ptr    = emu.read_arg(1)?;
    let n_chars    = emu.read_arg(2)? as usize;
    let p_written  = emu.read_arg(3)?;
    
    let raw = emu.state.read_space(3, buf_ptr, n_chars)?;
    let s = String::from_utf8_lossy(&raw);
    print!("{}", s); // Print to real stdout
    
    if p_written != 0 {
        emu.state.write_space(3, p_written, &(n_chars as u32).to_le_bytes())?;
    }
    emu.write_return_val(1)?;
    Ok(())
}

fn handle_write_console_w(emu: &mut Emulator) -> Result<()> {
    let _h_console = emu.read_arg(0)?;
    let buf_ptr    = emu.read_arg(1)?;
    let n_chars    = emu.read_arg(2)? as usize;
    let p_written  = emu.read_arg(3)?;
    
    let raw = emu.state.read_space(3, buf_ptr, n_chars * 2)?;
    let chars: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let s = String::from_utf16_lossy(&chars);
    print!("{}", s);
    
    if p_written != 0 {
        emu.state.write_space(3, p_written, &(n_chars as u32).to_le_bytes())?;
    }
    emu.write_return_val(1)?;
    Ok(())
}

fn handle_create_thread(emu: &mut Emulator) -> Result<()> {
    let start_address = emu.read_arg(2)?;
    tracing::info!("CreateThread(start: 0x{:X})", start_address);
    emu.write_return_val(0x40000000)?; // dummy handle
    Ok(())
}

fn handle_lstrcpy_a(emu: &mut Emulator) -> Result<()> {
    let dst = emu.read_arg(0)?;
    let src = emu.read_arg(1)?;
    let s = read_string(emu, src)?;
    let mut bytes = s.into_bytes();
    bytes.push(0);
    emu.state.write_space(3, dst, &bytes)?;
    emu.write_return_val(dst)?;
    Ok(())
}

fn handle_lstrcat_a(emu: &mut Emulator) -> Result<()> {
    let dst = emu.read_arg(0)?;
    let src = emu.read_arg(1)?;
    let d = read_string(emu, dst)?;
    let s = read_string(emu, src)?;
    let mut bytes = format!("{}{}", d, s).into_bytes();
    bytes.push(0);
    emu.state.write_space(3, dst, &bytes)?;
    emu.write_return_val(dst)?;
    Ok(())
}

fn handle_lstrlen_a(emu: &mut Emulator) -> Result<()> {
    let src = emu.read_arg(0)?;
    let s = read_string(emu, src)?;
    emu.write_return_val(s.len() as u64)?;
    Ok(())
}

fn handle_rtl_move_memory(emu: &mut Emulator) -> Result<()> {
    let dst = emu.read_arg(0)?;
    let src = emu.read_arg(1)?;
    let len = emu.read_arg(2)? as usize;
    let data = emu.state.read_space(3, src, len)?;
    emu.state.write_space(3, dst, &data)?;
    emu.write_return_val(0)?; // void
    Ok(())
}

fn handle_get_module_file_name_a(emu: &mut Emulator) -> Result<()> {
    let _h_module = emu.read_arg(0)?;
    let buf       = emu.read_arg(1)?;
    let size      = emu.read_arg(2)? as usize;
    
    let path = "C:\\sandbox\\test.exe";
    let mut bytes = path.as_bytes().to_vec();
    bytes.push(0);
    let copied = std::cmp::min(bytes.len(), size);
    emu.state.write_space(3, buf, &bytes[..copied])?;
    emu.write_return_val((copied - 1) as u64)?;
    Ok(())
}

fn handle_read_console_a(emu: &mut Emulator) -> Result<()> {
    let _h_console = emu.read_arg(0)?;
    let buf        = emu.read_arg(1)?;
    let chars_to_read = emu.read_arg(2)? as usize;
    let p_chars_read  = emu.read_arg(3)?;
    let _p_input_ctrl = emu.read_arg(4)?;

    let mut data = vec![0u8; chars_to_read];
    let mut bytes_read = 0;
    if let Some(ref mut mock_buf) = emu.stdin_buffer {
        let to_read = std::cmp::min(chars_to_read, mock_buf.len());
        data[..to_read].copy_from_slice(&mock_buf[..to_read]);
        mock_buf.drain(..to_read);
        bytes_read = to_read;
    } else {
        use std::io::Read;
        if let Ok(n) = std::io::stdin().read(&mut data) {
            bytes_read = n;
        }
    }
    
    if bytes_read > 0 {
        emu.state.write_space(3, buf, &data[..bytes_read])?;
    }
    if p_chars_read != 0 {
        emu.state.write_space(3, p_chars_read, &(bytes_read as u32).to_le_bytes())?;
    }
    
    emu.write_return_val(1)?; // non-zero on success
    Ok(())
}

fn handle_read_console_w(emu: &mut Emulator) -> Result<()> {
    let _h_console = emu.read_arg(0)?;
    let buf        = emu.read_arg(1)?;
    let chars_to_read = emu.read_arg(2)? as usize;
    let p_chars_read  = emu.read_arg(3)?;
    let _p_input_ctrl = emu.read_arg(4)?;

    let mut data = vec![0u8; chars_to_read];
    let mut chars_read = 0;
    if let Some(ref mut mock_buf) = emu.stdin_buffer {
        let to_read = std::cmp::min(chars_to_read, mock_buf.len());
        data[..to_read].copy_from_slice(&mock_buf[..to_read]);
        mock_buf.drain(..to_read);
        chars_read = to_read;
    } else {
        use std::io::Read;
        if let Ok(n) = std::io::stdin().read(&mut data) {
            chars_read = n; // Note: for W, ideally read UTF-16, but reading bytes as ASCII usually works for crackmes
        }
    }
    
    if chars_read > 0 {
        // Expand ASCII to UTF-16
        let mut wdata = Vec::with_capacity(chars_read * 2);
        for &b in &data[..chars_read] {
            wdata.push(b);
            wdata.push(0);
        }
        emu.state.write_space(3, buf, &wdata)?;
    }
    if p_chars_read != 0 {
        emu.state.write_space(3, p_chars_read, &(chars_read as u32).to_le_bytes())?;
    }
    
    emu.write_return_val(1)?;
    Ok(())
}
