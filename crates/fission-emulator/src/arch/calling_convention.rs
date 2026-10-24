/// Describes how a particular ABI passes arguments, receives return values,
/// and manages the return address for HLE function interception.
pub trait CallingConvention: Send + Sync {
    /// Names of the registers used for the first N integer arguments (in order).
    /// If the caller needs more args than this list covers, `stack_arg_offset` is used.
    fn arg_regs(&self) -> &[&'static str];

    /// Name of the register that holds the integer return value.
    fn return_reg(&self) -> &'static str;

    /// Name of the register that holds the return address, if the architecture
    /// stores it in a link register (ARM lr, MIPS ra). Returns `None` for
    /// stack-based return (x86 call/ret).
    fn return_addr_reg(&self) -> Option<&'static str>;

    /// Byte offset from the stack pointer of the `n`-th *extra* argument
    /// (i.e., arguments beyond `arg_regs().len()`). `n` is zero-indexed
    /// within the overflow region.
    ///
    /// x86-64 Win64: offset = 0x28 + n * 8  (4 slots shadow space + retaddr)
    /// x86-64 SysV:  offset = 0x00 + n * 8  (no shadow space)
    fn stack_arg_offset(&self, n: usize) -> u64;

    /// Extra stack bytes popped by the callee on return (e.g. x86-32 `stdcall` `ret N`).
    /// Defaults to 0 for caller-cleanup conventions (`cdecl`, `SysV64`, `Win64`, RISC ABIs).
    fn callee_pop_bytes(&self, _func_name: &str) -> u64 {
        0
    }
}

// --- Concrete implementations ---

/// x86-64 Windows ABI (Win64 / MSVC fastcall)
/// Integer args: RCX, RDX, R8, R9; shadow space = 0x20 bytes
pub struct Win64Cc;
impl CallingConvention for Win64Cc {
    fn arg_regs(&self) -> &[&'static str] {
        &["rcx", "rdx", "r8", "r9"]
    }
    fn return_reg(&self) -> &'static str {
        "rax"
    }
    fn return_addr_reg(&self) -> Option<&'static str> {
        None
    }
    fn stack_arg_offset(&self, n: usize) -> u64 {
        0x28 + n as u64 * 8
    }
}

/// x86-64 System V ABI (Linux / macOS)
/// Integer args: RDI, RSI, RDX, RCX, R8, R9; no shadow space
pub struct SysV64Cc;
impl CallingConvention for SysV64Cc {
    fn arg_regs(&self) -> &[&'static str] {
        &["rdi", "rsi", "rdx", "rcx", "r8", "r9"]
    }
    fn return_reg(&self) -> &'static str {
        "rax"
    }
    fn return_addr_reg(&self) -> Option<&'static str> {
        None
    }
    fn stack_arg_offset(&self, n: usize) -> u64 {
        0x08 + n as u64 * 8
    }
}

/// x86-32 cdecl (all args on stack, caller cleans up)
pub struct Cdecl32Cc;
impl CallingConvention for Cdecl32Cc {
    fn arg_regs(&self) -> &[&'static str] {
        &[]
    }
    fn return_reg(&self) -> &'static str {
        "eax"
    }
    fn return_addr_reg(&self) -> Option<&'static str> {
        None
    }
    fn stack_arg_offset(&self, n: usize) -> u64 {
        0x04 + n as u64 * 4
    }
}

/// x86-32 stdcall (args on stack, callee cleans up — Win32 API default)
pub struct Stdcall32Cc;
impl CallingConvention for Stdcall32Cc {
    fn arg_regs(&self) -> &[&'static str] {
        &[]
    }
    fn return_reg(&self) -> &'static str {
        "eax"
    }
    fn return_addr_reg(&self) -> Option<&'static str> {
        None
    }
    fn stack_arg_offset(&self, n: usize) -> u64 {
        0x04 + n as u64 * 4
    }
    fn callee_pop_bytes(&self, func_name: &str) -> u64 {
        win32_stdcall_pop_bytes(func_name)
    }
}

/// Callee-cleaned stack byte count (`ret N`) for 32-bit Windows `__stdcall` APIs.
/// Returns `0` for `cdecl` CRT routines (`malloc`, `printf`, `memcpy`, etc.) or zero-arg Win32 APIs.
pub fn win32_stdcall_pop_bytes(func_name: &str) -> u64 {
    let base = func_name
        .rsplit_once('!')
        .or_else(|| func_name.rsplit_once('.'))
        .map(|(_, sym)| sym)
        .unwrap_or(func_name);
    if let Some((_, suffix)) = base.rsplit_once('@') {
        if let Ok(bytes) = suffix.parse::<u64>() {
            return bytes;
        }
    }
    let clean = base.strip_prefix('_').unwrap_or(base);
    match clean {
        // 0 args (0 bytes)
        "GetProcessHeap"
        | "GetACP"
        | "GetOEMCP"
        | "AllocConsole"
        | "FreeConsole"
        | "GetCurrentProcess"
        | "GetCurrentProcessId"
        | "GetCurrentThread"
        | "GetCurrentThreadId"
        | "TlsAlloc"
        | "GetLastError"
        | "IsDebuggerPresent"
        | "GetCommandLineA"
        | "GetCommandLineW"
        | "GetEnvironmentStrings"
        | "GetEnvironmentStringsA"
        | "GetEnvironmentStringsW"
        | "GetTickCount"
        | "GetTickCount64"
        | "GetConsoleCP"
        | "GetConsoleOutputCP" => 0,

        // 1 arg (4 bytes)
        "LoadLibraryA"
        | "LoadLibraryW"
        | "FreeLibrary"
        | "GetStdHandle"
        | "AttachConsole"
        | "FlushFileBuffers"
        | "SetEndOfFile"
        | "GetStartupInfoA"
        | "GetStartupInfoW"
        | "IsProcessorFeaturePresent"
        | "FlsAlloc"
        | "FlsGetValue"
        | "FlsFree"
        | "InitializeConditionVariable"
        | "WakeConditionVariable"
        | "WakeAllConditionVariable"
        | "GetSystemInfo"
        | "GetNativeSystemInfo"
        | "SetConsoleCP"
        | "SetConsoleOutputCP"
        | "CloseHandle"
        | "TlsGetValue"
        | "TlsFree"
        | "InitializeCriticalSection"
        | "EnterCriticalSection"
        | "LeaveCriticalSection"
        | "DeleteCriticalSection"
        | "TryEnterCriticalSection"
        | "InitializeSListHead"
        | "InterlockedFlushSList"
        | "InterlockedPopEntrySList"
        | "SetLastError"
        | "SetUnhandledExceptionFilter"
        | "UnhandledExceptionFilter"
        | "OutputDebugStringA"
        | "OutputDebugStringW"
        | "lstrlenA"
        | "lstrlenW"
        | "GetModuleHandleA"
        | "GetModuleHandleW"
        | "FreeEnvironmentStringsA"
        | "FreeEnvironmentStringsW"
        | "ExitProcess"
        | "ExitThread"
        | "Sleep"
        | "GetSystemTimeAsFileTime"
        | "GetSystemTimePreciseAsFileTime"
        | "GetSystemTime"
        | "GetLocalTime"
        | "QueryPerformanceCounter"
        | "QueryPerformanceFrequency"
        | "EncodePointer"
        | "DecodePointer"
        | "DeleteFileA"
        | "DeleteFileW"
        | "IsValidCodePage"
        | "RegCloseKey" => 4,

        // 2 args (8 bytes)
        "GetProcAddress"
        | "GetConsoleMode"
        | "SetConsoleMode"
        | "GetFileSize"
        | "GetFileSizeEx"
        | "FlsSetValue"
        | "SetConsoleCtrlHandler"
        | "SystemTimeToFileTime"
        | "GetConsoleScreenBufferInfo"
        | "SetConsoleTextAttribute"
        | "WaitForSingleObject"
        | "TlsSetValue"
        | "InitializeCriticalSectionAndSpinCount"
        | "InterlockedPushEntrySList"
        | "CheckRemoteDebuggerPresent"
        | "lstrcpyA"
        | "lstrcpyW"
        | "lstrcmpA"
        | "lstrcmpW"
        | "lstrcmpiA"
        | "lstrcmpiW"
        | "lstrcatA"
        | "lstrcatW"
        | "RtlZeroMemory"
        | "TerminateProcess"
        | "GetSystemDirectoryA"
        | "GetSystemDirectoryW"
        | "GetWindowsDirectoryA"
        | "GetWindowsDirectoryW"
        | "GetCPInfo" => 8,

        // 3 args (12 bytes)
        "LoadLibraryExA"
        | "LoadLibraryExW"
        | "VirtualFree"
        | "VirtualQuery"
        | "HeapCreate"
        | "HeapAlloc"
        | "RtlAllocateHeap"
        | "HeapFree"
        | "RtlFreeHeap"
        | "HeapSize"
        | "WaitForSingleObjectEx"
        | "InitializeCriticalSectionEx"
        | "lstrcpynA"
        | "lstrcpynW"
        | "RtlMoveMemory"
        | "RtlCompareMemory"
        | "GetModuleHandleExA"
        | "GetModuleHandleExW"
        | "GetModuleFileNameA"
        | "GetModuleFileNameW"
        | "GetEnvironmentVariableA"
        | "GetEnvironmentVariableW"
        | "SetFilePointer" => 12,

        // 4 args (16 bytes)
        "VirtualAlloc" | "VirtualProtect" | "VirtualFreeEx" | "HeapReAlloc"
        | "RtlReAllocateHeap" | "MessageBoxA" | "MessageBoxW" => 16,

        // 5 args / 20 bytes
        "VirtualAllocEx" | "WriteConsoleA" | "WriteConsoleW" | "WriteFile" | "ReadConsoleA"
        | "ReadConsoleW" | "ReadFile" | "SetFilePointerEx" | "GetStringTypeW" | "RegOpenKeyExA"
        | "RegOpenKeyExW" => 20,

        // 6 args (24 bytes)
        "CreateThread"
        | "MultiByteToWideChar"
        | "LCMapStringA"
        | "LCMapStringW"
        | "RegQueryValueExA"
        | "RegQueryValueExW" => 24,

        // 7 args (28 bytes)
        "CreateFileA" | "CreateFileW" => 28,

        // 8 args (32 bytes)
        "WideCharToMultiByte" => 32,

        // 9 args (36 bytes)
        "LCMapStringEx" => 36,

        _ => 0,
    }
}

/// ARM64 / AArch64 AAPCS
/// Integer args: X0–X7; return address in LR (X30)
pub struct Aarch64Cc;
impl CallingConvention for Aarch64Cc {
    fn arg_regs(&self) -> &[&'static str] {
        &["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7"]
    }
    fn return_reg(&self) -> &'static str {
        "x0"
    }
    fn return_addr_reg(&self) -> Option<&'static str> {
        Some("x30")
    } // LR
    fn stack_arg_offset(&self, n: usize) -> u64 {
        n as u64 * 8
    }
}

/// ARM32 AAPCS
/// Integer args: R0–R3; return address in LR (R14)
pub struct Arm32Cc;
impl CallingConvention for Arm32Cc {
    fn arg_regs(&self) -> &[&'static str] {
        &["r0", "r1", "r2", "r3"]
    }
    fn return_reg(&self) -> &'static str {
        "r0"
    }
    fn return_addr_reg(&self) -> Option<&'static str> {
        Some("r14")
    } // LR
    fn stack_arg_offset(&self, n: usize) -> u64 {
        n as u64 * 4
    }
}

/// MIPS o32 ABI
/// Integer args: $a0–$a3; return address in $ra
pub struct MipsO32Cc;
impl CallingConvention for MipsO32Cc {
    fn arg_regs(&self) -> &[&'static str] {
        &["a0", "a1", "a2", "a3"]
    }
    fn return_reg(&self) -> &'static str {
        "v0"
    }
    fn return_addr_reg(&self) -> Option<&'static str> {
        Some("ra")
    }
    fn stack_arg_offset(&self, n: usize) -> u64 {
        0x10 + n as u64 * 4
    }
}

/// PowerPC ELF ABI
/// Integer args: R3–R10; return address in LR
pub struct PpcElfCc;
impl CallingConvention for PpcElfCc {
    fn arg_regs(&self) -> &[&'static str] {
        &["r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10"]
    }
    fn return_reg(&self) -> &'static str {
        "r3"
    }
    fn return_addr_reg(&self) -> Option<&'static str> {
        Some("lr")
    }
    fn stack_arg_offset(&self, n: usize) -> u64 {
        0x08 + n as u64 * 4
    }
}

// --- helpers ---

pub(crate) fn le_bytes_to_u64(bytes: &[u8]) -> u64 {
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate() {
        val |= (b as u64) << (i * 8);
    }
    val
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::ArchInfo;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

    #[test]
    fn stdcall32_callee_pop_bytes_matches_win32_abi_and_keeps_crt_cdecl_zero() {
        let cc = Stdcall32Cc;
        assert_eq!(cc.callee_pop_bytes("GetLastError"), 0);
        assert_eq!(cc.callee_pop_bytes("GetStdHandle"), 4);
        assert_eq!(cc.callee_pop_bytes("GetProcAddress"), 8);
        assert_eq!(cc.callee_pop_bytes("HeapAlloc"), 12);
        assert_eq!(cc.callee_pop_bytes("VirtualAlloc"), 16);
        assert_eq!(cc.callee_pop_bytes("WriteFile"), 20);
        assert_eq!(cc.callee_pop_bytes("CreateFileA"), 28);
        assert_eq!(cc.callee_pop_bytes("WideCharToMultiByte"), 32);
        assert_eq!(cc.callee_pop_bytes("_CustomApi@24"), 24);
        assert_eq!(cc.callee_pop_bytes("KERNEL32.dll!VirtualProtect"), 16);
        // CRT functions are cdecl on 32-bit Windows (caller cleans up)
        assert_eq!(cc.callee_pop_bytes("malloc"), 0);
        assert_eq!(cc.callee_pop_bytes("printf"), 0);
        assert_eq!(cc.callee_pop_bytes("memcpy"), 0);
    }

    #[test]
    fn from_language_id_selects_stdcall32_for_pe_and_cdecl32_for_elf() {
        let pe = LoadedBinaryBuilder::new("app.exe".into(), DataBuffer::Heap(Vec::new()))
            .format("PE")
            .build()
            .expect("pe");
        let elf = LoadedBinaryBuilder::new("app.elf".into(), DataBuffer::Heap(Vec::new()))
            .format("ELF")
            .build()
            .expect("elf");

        let win_arch = ArchInfo::from_language_id("x86:LE:32:default", Some(&pe)).unwrap();
        let elf_arch = ArchInfo::from_language_id("x86:LE:32:default", Some(&elf)).unwrap();

        assert_eq!(win_arch.cc.callee_pop_bytes("VirtualAlloc"), 16);
        assert_eq!(elf_arch.cc.callee_pop_bytes("VirtualAlloc"), 0);
    }
}
