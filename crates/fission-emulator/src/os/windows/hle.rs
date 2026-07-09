use anyhow::Result;
use crate::core::Emulator;
use crate::os::env::{HleResult, OsEnvironment};
use crate::os::windows::heap::DummyHeap;
use crate::os::windows::imports::{ImportTable, SharedImportTable};
use crate::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;
use std::sync::Mutex;

/// Standard handle cookies returned by GetStdHandle (and accepted by WriteFile).
const STD_INPUT_HANDLE: u64 = 0x50;
const STD_OUTPUT_HANDLE: u64 = 0x51;
const STD_ERROR_HANDLE: u64 = 0x52;
/// Legacy cookie used by older HLE paths.
const LEGACY_STDOUT: u64 = 0x77777777;

/// Windows PE execution environment.
///
/// - Import patching: overwrites IAT entries with sequential magic trampolines.
/// - Stub resolution: maps magic address back to import name (O(1) table).
/// - GetProcAddress allocates new magic stubs into the same table.
/// - HLE dispatch: emulates Win32 API functions by name.
pub struct WindowsEnv {
    pub heap: Mutex<DummyHeap>,
    imports: SharedImportTable,
}

impl WindowsEnv {
    pub fn new() -> Self {
        Self {
            heap: Mutex::new(DummyHeap::new(0x20000000)), // Dummy heap base
            imports: Mutex::new(ImportTable::default()),
        }
    }
}

impl Default for WindowsEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl OsEnvironment for WindowsEnv {
    fn patch_imports(&self, state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
        self.imports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .patch_iat(state, binary)
    }

    fn resolve_stub(&self, _binary: &LoadedBinary, magic_addr: u64) -> Option<String> {
        self.imports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .resolve(magic_addr)
    }

    fn dispatch_hle(&self, emu: &mut Emulator, func_name: &str) -> Result<HleResult> {
        tracing::info!("HLE Intercept: {}", func_name);
        emu.metrics.note_userop(&format!("win32:{func_name}"));
        match func_name {
            "LoadLibraryA" | "LoadLibraryExA" => handle_load_library_a(emu)?,
            "LoadLibraryW" | "LoadLibraryExW" => handle_load_library_w(emu)?,
            "GetProcAddress" => handle_get_proc_address(emu, self)?,
            "FreeLibrary" => { emu.write_return_val(1)?; }
            "VirtualAlloc" | "VirtualAllocEx" => handle_virtual_alloc(emu)?,
            "VirtualFree" | "VirtualFreeEx" => {
                let addr = emu.read_arg(0).unwrap_or(0);
                let size = emu.read_arg(1).unwrap_or(0);
                if size > 0 {
                    emu.state.page_map.unmap_region(addr, size);
                }
                emu.write_return_val(1)?;
            }
            "VirtualProtect" => handle_virtual_protect(emu)?,
            "VirtualQuery" => handle_virtual_query(emu)?,

            // Heap
            "GetProcessHeap" => { emu.write_return_val(0x20000000)?; }
            "HeapCreate" => { emu.write_return_val(0x20000000)?; }
            "HeapAlloc" | "RtlAllocateHeap" => handle_heap_alloc(emu, self)?,
            "HeapFree" | "RtlFreeHeap" => handle_heap_free(emu, self)?,
            "HeapReAlloc" => handle_heap_realloc(emu, self)?,
            "HeapSize" => {
                let _h = emu.read_arg(0)?;
                let _f = emu.read_arg(1)?;
                let _p = emu.read_arg(2)?;
                emu.write_return_val(0x1000)?;
            }

            // Console / stdio / CRT bootstrap
            "GetStdHandle" => handle_get_std_handle(emu)?,
            "GetConsoleMode" => handle_get_console_mode(emu)?,
            "SetConsoleMode" => { emu.write_return_val(1)?; }
            "WriteConsoleA" => handle_write_console_a(emu)?,
            "WriteFile" => handle_write_file(emu)?,
            "WriteConsoleW" => handle_write_console_w(emu)?,
            "ReadConsoleA" | "ReadFile" => handle_read_console_a(emu)?,
            "ReadConsoleW" => handle_read_console_w(emu)?,
            "AllocConsole" | "AttachConsole" | "FreeConsole" => { emu.write_return_val(1)?; }
            "FlushFileBuffers" | "SetEndOfFile" => { emu.write_return_val(1)?; }
            "CreateFileA" => handle_create_file_a(emu)?,
            "CreateFileW" => handle_create_file_w(emu)?,
            "GetFileSize" | "GetFileSizeEx" => {
                emu.write_return_val(0)?;
            }
            "GetStartupInfoA" => handle_get_startup_info_a(emu)?,
            "GetStartupInfoW" => handle_get_startup_info_w(emu)?,
            "GetACP" | "GetOEMCP" => { emu.write_return_val(65001)?; } // UTF-8
            "IsProcessorFeaturePresent" => { emu.write_return_val(0)?; }
            "GetSystemDirectoryA" | "GetWindowsDirectoryA" => {
                // Return empty / failure — CRT often tolerates 0.
                emu.write_return_val(0)?;
            }
            "FlsAlloc" => { emu.write_return_val(1)?; }
            "FlsGetValue" => { emu.write_return_val(0)?; }
            "FlsSetValue" | "FlsFree" => { emu.write_return_val(1)?; }
            // Thread / Sync / process identity
            "CreateThread" => handle_create_thread(emu)?,
            "WaitForSingleObject" | "WaitForSingleObjectEx" => { emu.write_return_val(0)?; }
            "CloseHandle" => { emu.write_return_val(1)?; }
            "GetCurrentProcess" => { emu.write_return_val(!0u64)?; } // pseudo -1
            "GetCurrentProcessId" => { emu.write_return_val(1000)?; }
            "GetCurrentThread" => { emu.write_return_val(!1u64 + 1)?; }
            "GetCurrentThreadId" => { emu.write_return_val(1000)?; }
            "TlsAlloc" => { emu.write_return_val(1)?; }
            "TlsGetValue" => { emu.write_return_val(0)?; }
            "TlsSetValue" => { emu.write_return_val(1)?; }
            "TlsFree" => { emu.write_return_val(1)?; }
            "InitializeCriticalSection"
            | "InitializeCriticalSectionEx"
            | "InitializeCriticalSectionAndSpinCount"
            | "EnterCriticalSection"
            | "LeaveCriticalSection"
            | "DeleteCriticalSection"
            | "InitializeSListHead" => {
                emu.write_return_val(1)?;
            }

            // Error state
            "GetLastError" => {
                emu.write_return_val(emu.win_last_error as u64)?;
            }
            "SetLastError" => {
                emu.win_last_error = emu.read_arg(0).unwrap_or(0) as u32;
                emu.write_return_val(0)?;
            }
            "SetUnhandledExceptionFilter" => { emu.write_return_val(0)?; }
            "IsDebuggerPresent" => { emu.write_return_val(0)?; }
            "CheckRemoteDebuggerPresent" => {
                let p = emu.read_arg(1).unwrap_or(0);
                if p != 0 {
                    let _ = emu.state.write_space(emu.state.ram_space(), p, &[0u8; 4]);
                }
                emu.write_return_val(1)?;
            }
            "OutputDebugStringA" => {
                let p = emu.read_arg(0).unwrap_or(0);
                if p != 0 {
                    if let Ok(s) = read_string(emu, p) {
                        tracing::debug!("OutputDebugStringA: {}", s);
                    }
                }
                emu.write_return_val(0)?;
            }
            "OutputDebugStringW" => { emu.write_return_val(0)?; }

            // Str / Mem / codepage
            "lstrcpyA" | "strcpy" => handle_lstrcpy_a(emu)?,
            "lstrcatA" | "strcat" => handle_lstrcat_a(emu)?,
            "lstrlenA" | "strlen" => handle_lstrlen_a(emu)?,
            "RtlMoveMemory" | "memmove" | "memcpy" => handle_rtl_move_memory(emu)?,
            "RtlZeroMemory" | "memset" => handle_rtl_zero_memory(emu)?,
            "MultiByteToWideChar" => handle_multi_byte_to_wide_char(emu)?,
            "WideCharToMultiByte" => handle_wide_char_to_multi_byte(emu)?,

            // Module
            "GetModuleHandleA" | "GetModuleHandleW" | "GetModuleHandleExA" | "GetModuleHandleExW" => {
                emu.write_return_val(0x140000000)?;
            }
            "GetModuleFileNameA" => handle_get_module_file_name_a(emu)?,
            "GetModuleFileNameW" => handle_get_module_file_name_w(emu)?,
            "GetCommandLineA" => handle_get_command_line_a(emu)?,
            "GetCommandLineW" => handle_get_command_line_w(emu)?,
            "GetEnvironmentStringsW" => {
                // Return a dummy env block pointer in guest heap region.
                let base = 0x0000_0000_2100_0000u64;
                let data = "PATH=C:\\Windows\\System32\0\0".encode_utf16().flat_map(|c| c.to_le_bytes()).collect::<Vec<_>>();
                let _ = emu.state.write_space(emu.state.ram_space(), base, &data);
                emu.write_return_val(base)?;
            }
            "FreeEnvironmentStringsW" | "FreeEnvironmentStringsA" => { emu.write_return_val(1)?; }

            "ExitProcess" | "TerminateProcess" => {
                let code = emu.read_arg(0).unwrap_or(0) as u32;
                tracing::info!("ExitProcess({}). Emulation finished.", code);
                return Ok(HleResult::Halt(code));
            }
            "ExitThread" => {
                let code = emu.read_arg(0).unwrap_or(0) as u32;
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
                    let _ = emu.state.write_space(emu.state.ram_space(), ptr, &fake_time.to_le_bytes());
                }
                emu.tick_count = emu.tick_count.wrapping_add(1);
                // void return
            }
            "QueryPerformanceCounter" => {
                // lpPerformanceCount: pointer in RCX
                let ptr = emu.read_arg(0).unwrap_or(0);
                if ptr != 0 {
                    let counter = emu.tick_count.wrapping_mul(10_000);
                    let _ = emu.state.write_space(emu.state.ram_space(), ptr, &counter.to_le_bytes());
                }
                emu.tick_count = emu.tick_count.wrapping_add(1);
                emu.write_return_val(1)?; // TRUE = success
            }
            "QueryPerformanceFrequency" => {
                let ptr = emu.read_arg(0).unwrap_or(0);
                if ptr != 0 {
                    // Return 10,000,000 Hz (100-ns resolution, matches FILETIME)
                    let freq: u64 = 10_000_000;
                    let _ = emu.state.write_space(emu.state.ram_space(), ptr, &freq.to_le_bytes());
                }
                emu.write_return_val(1)?;
            }
            "GetEnvironmentVariableA" | "GetEnvironmentVariableW" => {
                emu.write_return_val(0)?; // not found
            }
            _ => {
                tracing::warn!("Unimplemented Win32 API: {}. Returning 0.", func_name);
                emu.win_last_error = 127; // ERROR_PROC_NOT_FOUND-ish
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
        let b = emu.state.read_space(emu.state.ram_space(), cur, 1)?[0];
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
        let pair = emu.state.read_space(emu.state.ram_space(), cur, 2)?;
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

fn handle_get_proc_address(emu: &mut Emulator, env: &WindowsEnv) -> Result<()> {
    let h_module = emu.read_arg(0)?;
    let name_ptr = emu.read_arg(1)?;
    let proc_name = if name_ptr < 0xFFFF {
        format!("Ordinal_{}", name_ptr)
    } else {
        read_string(emu, name_ptr)?
    };
    let magic = env
        .imports
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .alloc_stub(&proc_name);
    tracing::info!(
        "GetProcAddress(0x{:X}, \"{}\") -> trampoline 0x{:X}",
        h_module,
        proc_name,
        magic
    );
    emu.win_last_error = 0;
    emu.write_return_val(magic)?;
    Ok(())
}

fn handle_get_std_handle(emu: &mut Emulator) -> Result<()> {
    // nStdHandle: -10 stdin, -11 stdout, -12 stderr (as unsigned 32/64).
    let n = emu.read_arg(0)? as i32;
    let h = match n {
        -10 => STD_INPUT_HANDLE,
        -11 => STD_OUTPUT_HANDLE,
        -12 => STD_ERROR_HANDLE,
        _ => 0xFFFF_FFFF_FFFF_FFFF, // INVALID_HANDLE_VALUE
    };
    if h == 0xFFFF_FFFF_FFFF_FFFF {
        emu.win_last_error = 6; // ERROR_INVALID_HANDLE
    } else {
        emu.win_last_error = 0;
    }
    emu.write_return_val(h)?;
    Ok(())
}

fn handle_get_console_mode(emu: &mut Emulator) -> Result<()> {
    let _h = emu.read_arg(0)?;
    let mode_ptr = emu.read_arg(1)?;
    if mode_ptr != 0 {
        // ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT
        let mode: u32 = 0x3;
        let _ = emu
            .state
            .write_space(emu.state.ram_space(), mode_ptr, &mode.to_le_bytes());
    }
    emu.write_return_val(1)?;
    Ok(())
}

fn handle_get_startup_info_a(emu: &mut Emulator) -> Result<()> {
    // STARTUPINFOA: first field cb (DWORD). Zero the rest; set cb = 68.
    let ptr = emu.read_arg(0)?;
    if ptr != 0 {
        let mut buf = vec![0u8; 68];
        buf[0..4].copy_from_slice(&68u32.to_le_bytes());
        let _ = emu.state.write_space(emu.state.ram_space(), ptr, &buf);
    }
    Ok(())
}

fn handle_get_startup_info_w(emu: &mut Emulator) -> Result<()> {
    let ptr = emu.read_arg(0)?;
    if ptr != 0 {
        let mut buf = vec![0u8; 104];
        buf[0..4].copy_from_slice(&104u32.to_le_bytes());
        let _ = emu.state.write_space(emu.state.ram_space(), ptr, &buf);
    }
    Ok(())
}

fn is_console_or_stdout_handle(h: u64) -> bool {
    matches!(
        h,
        STD_INPUT_HANDLE | STD_OUTPUT_HANDLE | STD_ERROR_HANDLE | LEGACY_STDOUT
    )
}

/// WriteFile(hFile, lpBuffer, nNumberOfBytesToWrite, lpNumberOfBytesWritten, lpOverlapped)
fn handle_write_file(emu: &mut Emulator) -> Result<()> {
    let h = emu.read_arg(0)?;
    let buf_ptr = emu.read_arg(1)?;
    let n = emu.read_arg(2)? as usize;
    let p_written = emu.read_arg(3)?;
    let _overlapped = emu.read_arg(4).unwrap_or(0);

    if !is_console_or_stdout_handle(h) && h != 0x50000000 && h != 0x50000001 {
        // Unknown disk handle — still attempt a best-effort write to host for debugging.
        tracing::debug!("WriteFile: non-std handle 0x{:X}, treating as console-like", h);
    }

    let raw = if n == 0 || buf_ptr == 0 {
        Vec::new()
    } else {
        emu.state
            .read_space(emu.state.ram_space(), buf_ptr, n.min(0x10_0000))?
    };
    if is_console_or_stdout_handle(h) || h == LEGACY_STDOUT {
        let s = String::from_utf8_lossy(&raw);
        print!("{}", s);
    } else {
        // Route through SimVFS when available (best-effort path).
        let s = String::from_utf8_lossy(&raw);
        tracing::debug!("WriteFile(disk-ish): {} bytes: {:?}", raw.len(), s.chars().take(64).collect::<String>());
        print!("{}", s);
    }

    if p_written != 0 {
        emu.state.write_space(
            emu.state.ram_space(),
            p_written,
            &(raw.len() as u32).to_le_bytes(),
        )?;
    }
    emu.win_last_error = 0;
    emu.write_return_val(1)?;
    Ok(())
}

fn handle_virtual_alloc(emu: &mut Emulator) -> Result<()> {
    use crate::pcode::page_map::prot;
    let lp_address = emu.read_arg(0)?;
    let dw_size = emu.read_arg(1)?;
    let alloc_type = emu.read_arg(2)?;
    let protect = emu.read_arg(3)?;
    // Map PAGE_* protect bits loosely onto page_map prot.
    let mut page_prot = prot::VALID | prot::READ;
    if protect & 0x04 != 0 || protect & 0x40 != 0 {
        // PAGE_READWRITE / EXECUTE_READWRITE
        page_prot |= prot::WRITE;
    }
    if protect & 0x10 != 0 || protect & 0x20 != 0 || protect & 0x40 != 0 {
        page_prot |= prot::EXEC;
    }
    if protect & 0x02 != 0 {
        page_prot |= prot::READ;
    }
    let size = dw_size.max(0x1000);
    let base = if lp_address == 0 {
        emu.state.page_map.mmap_anon(size, page_prot)
    } else {
        emu.state.page_map.map_region(lp_address, size, page_prot, true);
        lp_address
    };
    let fill = (size as usize).min(0x10_0000);
    let zeros = vec![0u8; fill];
    let _ = emu.state.write_space(emu.state.ram_space(), base, &zeros);
    tracing::info!(
        "VirtualAlloc(0x{:X}, 0x{:X}, type=0x{:X}, prot=0x{:X}) -> 0x{:X}",
        lp_address,
        dw_size,
        alloc_type,
        protect,
        base
    );
    emu.win_last_error = 0;
    emu.write_return_val(base)?;
    Ok(())
}

fn handle_virtual_protect(emu: &mut Emulator) -> Result<()> {
    use crate::pcode::page_map::prot;
    let addr = emu.read_arg(0)?;
    let size = emu.read_arg(1)?;
    let new_prot = emu.read_arg(2)? as u8;
    let old_ptr = emu.read_arg(3)?;
    if old_ptr != 0 {
        let _ = emu
            .state
            .write_space(emu.state.ram_space(), old_ptr, &4u32.to_le_bytes());
    }
    let mut p = prot::VALID | prot::READ;
    if new_prot & 0x04 != 0 || new_prot & 0x40 != 0 {
        p |= prot::WRITE;
    }
    if new_prot & 0x10 != 0 || new_prot & 0x20 != 0 || new_prot & 0x40 != 0 {
        p |= prot::EXEC;
    }
    emu.state.page_map.mprotect(addr, size.max(1), p);
    // SMC
    let mut page = addr & !0xFFF;
    let end = (addr + size.max(1) + 0xFFF) & !0xFFF;
    while page < end {
        emu.jit_cache.invalidate_page(page);
        page = page.saturating_add(0x1000);
    }
    emu.write_return_val(1)?;
    Ok(())
}

fn handle_virtual_query(emu: &mut Emulator) -> Result<()> {
    // MEMORY_BASIC_INFORMATION lite: fill BaseAddress + RegionSize + Protect + State
    let addr = emu.read_arg(0)?;
    let buf = emu.read_arg(1)?;
    let _len = emu.read_arg(2)?;
    if buf != 0 {
        let mut mbi = vec![0u8; 48];
        mbi[0..8].copy_from_slice(&addr.to_le_bytes());
        mbi[16..24].copy_from_slice(&0x1000u64.to_le_bytes()); // RegionSize
        mbi[24..28].copy_from_slice(&0x1000u32.to_le_bytes()); // MEM_COMMIT
        mbi[28..32].copy_from_slice(&0x04u32.to_le_bytes()); // PAGE_READWRITE
        let _ = emu.state.write_space(emu.state.ram_space(), buf, &mbi);
        emu.write_return_val(48)?;
    } else {
        emu.write_return_val(0)?;
    }
    Ok(())
}

fn handle_create_file_a(emu: &mut Emulator) -> Result<()> {
    let name_ptr = emu.read_arg(0)?;
    let name = if name_ptr == 0 {
        "<null>".into()
    } else {
        read_string(emu, name_ptr)?
    };
    tracing::info!("CreateFileA(\"{}\")", name);
    // Return a pseudo handle
    emu.win_last_error = 0;
    emu.write_return_val(0x50000000)?;
    Ok(())
}

fn handle_create_file_w(emu: &mut Emulator) -> Result<()> {
    let name_ptr = emu.read_arg(0)?;
    let name = if name_ptr == 0 {
        "<null>".into()
    } else {
        read_wide_string(emu, name_ptr)?
    };
    tracing::info!("CreateFileW(\"{}\")", name);
    emu.win_last_error = 0;
    emu.write_return_val(0x50000001)?;
    Ok(())
}

fn handle_rtl_zero_memory(emu: &mut Emulator) -> Result<()> {
    let dst = emu.read_arg(0)?;
    let len = emu.read_arg(1)? as usize;
    let zeros = vec![0u8; len.min(0x10_0000)];
    emu.state.write_space(emu.state.ram_space(), dst, &zeros)?;
    emu.write_return_val(0)?;
    Ok(())
}

fn handle_multi_byte_to_wide_char(emu: &mut Emulator) -> Result<()> {
    // MultiByteToWideChar(CodePage, dwFlags, lpMultiByteStr, cbMultiByte, lpWideCharStr, cchWideChar)
    let src = emu.read_arg(2)?;
    let cb = emu.read_arg(3)? as i64;
    let dst = emu.read_arg(4)?;
    let cch = emu.read_arg(5)? as i64;
    let s = if src == 0 {
        String::new()
    } else if cb < 0 {
        read_string(emu, src)?
    } else {
        let raw = emu.state.read_space(emu.state.ram_space(), src, cb as usize)?;
        String::from_utf8_lossy(&raw).into_owned()
    };
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    if cch == 0 {
        emu.write_return_val(wide.len() as u64)?;
    } else if dst != 0 {
        let n = (cch as usize).min(wide.len());
        let mut bytes = Vec::with_capacity(n * 2);
        for &c in &wide[..n] {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
        emu.state.write_space(emu.state.ram_space(), dst, &bytes)?;
        emu.write_return_val(n as u64)?;
    } else {
        emu.write_return_val(0)?;
    }
    Ok(())
}

fn handle_wide_char_to_multi_byte(emu: &mut Emulator) -> Result<()> {
    let src = emu.read_arg(2)?;
    let cch = emu.read_arg(3)? as i64;
    let dst = emu.read_arg(4)?;
    let cb = emu.read_arg(5)? as i64;
    let s = if src == 0 {
        String::new()
    } else {
        read_wide_string(emu, src)?
    };
    let mut bytes = s.into_bytes();
    bytes.push(0);
    if cb == 0 {
        emu.write_return_val(bytes.len() as u64)?;
    } else if dst != 0 {
        let n = (cb as usize).min(bytes.len());
        emu.state.write_space(emu.state.ram_space(), dst, &bytes[..n])?;
        emu.write_return_val(n as u64)?;
    } else {
        emu.write_return_val(0)?;
    }
    let _ = cch;
    Ok(())
}

fn handle_get_module_file_name_w(emu: &mut Emulator) -> Result<()> {
    let _h = emu.read_arg(0)?;
    let buf = emu.read_arg(1)?;
    let size = emu.read_arg(2)? as usize;
    let path: Vec<u16> = "C:\\sandbox\\test.exe"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let n = path.len().min(size.max(1));
    let mut bytes = Vec::with_capacity(n * 2);
    for &c in &path[..n] {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    if buf != 0 {
        emu.state.write_space(emu.state.ram_space(), buf, &bytes)?;
    }
    emu.write_return_val((n.saturating_sub(1)) as u64)?;
    Ok(())
}

fn handle_get_command_line_a(emu: &mut Emulator) -> Result<()> {
    let base = 0x0000_0000_2110_0000u64;
    let data = b"test.exe\0";
    emu.state.write_space(emu.state.ram_space(), base, data)?;
    emu.write_return_val(base)?;
    Ok(())
}

fn handle_get_command_line_w(emu: &mut Emulator) -> Result<()> {
    let base = 0x0000_0000_2110_1000u64;
    let wide: Vec<u16> = "test.exe".encode_utf16().chain(std::iter::once(0)).collect();
    let mut bytes = Vec::new();
    for c in wide {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    emu.state.write_space(emu.state.ram_space(), base, &bytes)?;
    emu.write_return_val(base)?;
    Ok(())
}

fn handle_heap_alloc(emu: &mut Emulator, env: &WindowsEnv) -> Result<()> {
    let _h_heap = emu.read_arg(0)?;
    let flags   = emu.read_arg(1)?;
    let bytes   = emu.read_arg(2)? as usize;
    let addr = env.heap.lock().unwrap().alloc(bytes);
    if (flags & 8) != 0 { // HEAP_ZERO_MEMORY
        let zeros = vec![0u8; bytes];
        emu.state.write_space(emu.state.ram_space(), addr, &zeros)?;
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
    
    let raw = emu.state.read_space(emu.state.ram_space(), buf_ptr, n_chars)?;
    let s = String::from_utf8_lossy(&raw);
    print!("{}", s); // Print to real stdout
    
    if p_written != 0 {
        emu.state.write_space(emu.state.ram_space(), p_written, &(n_chars as u32).to_le_bytes())?;
    }
    emu.write_return_val(1)?;
    Ok(())
}

fn handle_write_console_w(emu: &mut Emulator) -> Result<()> {
    let _h_console = emu.read_arg(0)?;
    let buf_ptr    = emu.read_arg(1)?;
    let n_chars    = emu.read_arg(2)? as usize;
    let p_written  = emu.read_arg(3)?;
    
    let raw = emu.state.read_space(emu.state.ram_space(), buf_ptr, n_chars * 2)?;
    let chars: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let s = String::from_utf16_lossy(&chars);
    print!("{}", s);
    
    if p_written != 0 {
        emu.state.write_space(emu.state.ram_space(), p_written, &(n_chars as u32).to_le_bytes())?;
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
    emu.state.write_space(emu.state.ram_space(), dst, &bytes)?;
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
    emu.state.write_space(emu.state.ram_space(), dst, &bytes)?;
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
    let data = emu.state.read_space(emu.state.ram_space(), src, len)?;
    emu.state.write_space(emu.state.ram_space(), dst, &data)?;
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
    emu.state.write_space(emu.state.ram_space(), buf, &bytes[..copied])?;
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
        emu.state.write_space(emu.state.ram_space(), buf, &data[..bytes_read])?;
        // Taint stdin bytes for symbolic execution
        for i in 0..bytes_read {
            let node = emu.solver.register_var(format!("stdin_console_{}", buf+i as u64), 1);
            emu.state
                .set_shadow_memory(emu.state.ram_space(), buf + i as u64, node);
        }
    }
    if p_chars_read != 0 {
        emu.state.write_space(emu.state.ram_space(), p_chars_read, &(bytes_read as u32).to_le_bytes())?;
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
        emu.state.write_space(emu.state.ram_space(), buf, &wdata)?;
    }
    if p_chars_read != 0 {
        emu.state.write_space(emu.state.ram_space(), p_chars_read, &(chars_read as u32).to_le_bytes())?;
    }
    
    emu.write_return_val(1)?;
    Ok(())
}
