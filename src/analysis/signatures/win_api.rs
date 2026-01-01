//! Windows API Function Signatures
//!
//! Contains type information for common Windows API functions
//! to improve decompiler output quality.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Global lazily-initialized Windows API database for efficient reuse.
/// This avoids recreating the database with 100+ signatures on each use.
pub static WIN_API_DB: LazyLock<WinApiDatabase> = LazyLock::new(WinApiDatabase::new);

/// Parameter type information with optional enum group for context-aware constant resolution
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub type_name: String,
    /// Optional enum group for context-aware constant substitution
    /// e.g., "PAGE_PROTECT" for VirtualAlloc's flProtect parameter
    pub enum_group: Option<String>,
}

impl ParamInfo {
    pub fn new(name: &str, type_name: &str) -> Self {
        Self {
            name: name.to_string(),
            type_name: type_name.to_string(),
            enum_group: None,
        }
    }

    pub fn with_enum(name: &str, type_name: &str, group: &str) -> Self {
        Self {
            name: name.to_string(),
            type_name: type_name.to_string(),
            enum_group: Some(group.to_string()),
        }
    }
}

/// Function signature with parameter and return types
#[derive(Debug, Clone)]
pub struct ApiSignature {
    pub name: String,
    pub return_type: String,
    pub params: Vec<ParamInfo>,
}

impl ApiSignature {
    pub fn new(name: &str, ret: &str, params: &[(&str, &str)]) -> Self {
        Self {
            name: name.to_string(),
            return_type: ret.to_string(),
            params: params.iter().map(|(n, t)| ParamInfo::new(n, t)).collect(),
        }
    }

    /// Create signature with enum groups for specific parameters
    pub fn with_enums(name: &str, ret: &str, params: Vec<ParamInfo>) -> Self {
        Self {
            name: name.to_string(),
            return_type: ret.to_string(),
            params,
        }
    }
}

/// Windows API Signature Database
pub struct WinApiDatabase {
    signatures: HashMap<String, ApiSignature>,
}

impl WinApiDatabase {
    /// Create a new WinApiDatabase with all built-in signatures
    ///
    /// Performance: Pre-allocates HashMap capacity based on known API count
    /// to avoid rehashing during loading (~130 APIs across all DLLs)
    pub fn new() -> Self {
        let mut db = Self {
            // Pre-allocate for ~130 known APIs to minimize HashMap rehashing
            signatures: HashMap::with_capacity(140),
        };
        db.load_kernel32();
        db.load_user32();
        db.load_ntdll();
        db.load_advapi32();
        db.load_ws2_32();
        db.load_winhttp();
        db.load_wininet();
        db.load_shell32();
        db.load_bcrypt();
        db
    }

    fn add(&mut self, sig: ApiSignature) {
        self.signatures.insert(sig.name.clone(), sig);
    }

    fn load_kernel32(&mut self) {
        // Memory functions with enum groups for context-aware constant substitution
        self.add(ApiSignature::with_enums(
            "VirtualAlloc",
            "LPVOID",
            vec![
                ParamInfo::new("lpAddress", "LPVOID"),
                ParamInfo::new("dwSize", "SIZE_T"),
                ParamInfo::with_enum("flAllocationType", "DWORD", "MEM_ALLOC"),
                ParamInfo::with_enum("flProtect", "DWORD", "PAGE_PROTECT"),
            ],
        ));
        self.add(ApiSignature::with_enums(
            "VirtualFree",
            "BOOL",
            vec![
                ParamInfo::new("lpAddress", "LPVOID"),
                ParamInfo::new("dwSize", "SIZE_T"),
                ParamInfo::with_enum("dwFreeType", "DWORD", "MEM_ALLOC"),
            ],
        ));
        self.add(ApiSignature::with_enums(
            "VirtualProtect",
            "BOOL",
            vec![
                ParamInfo::new("lpAddress", "LPVOID"),
                ParamInfo::new("dwSize", "SIZE_T"),
                ParamInfo::with_enum("flNewProtect", "DWORD", "PAGE_PROTECT"),
                ParamInfo::new("lpflOldProtect", "PDWORD"),
            ],
        ));

        // Process/Thread
        self.add(ApiSignature::new("GetCurrentProcess", "HANDLE", &[]));
        self.add(ApiSignature::new("GetCurrentProcessId", "DWORD", &[]));
        self.add(ApiSignature::new("GetCurrentThread", "HANDLE", &[]));
        self.add(ApiSignature::new("GetCurrentThreadId", "DWORD", &[]));
        self.add(ApiSignature::new(
            "CreateThread",
            "HANDLE",
            &[
                ("lpThreadAttributes", "LPSECURITY_ATTRIBUTES"),
                ("dwStackSize", "SIZE_T"),
                ("lpStartAddress", "LPTHREAD_START_ROUTINE"),
                ("lpParameter", "LPVOID"),
                ("dwCreationFlags", "DWORD"),
                ("lpThreadId", "LPDWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "ExitThread",
            "void",
            &[("dwExitCode", "DWORD")],
        ));
        self.add(ApiSignature::new(
            "TerminateProcess",
            "BOOL",
            &[("hProcess", "HANDLE"), ("uExitCode", "UINT")],
        ));

        // File I/O with enum groups
        self.add(ApiSignature::with_enums(
            "CreateFileA",
            "HANDLE",
            vec![
                ParamInfo::new("lpFileName", "LPCSTR"),
                ParamInfo::with_enum("dwDesiredAccess", "DWORD", "GENERIC_ACCESS"),
                ParamInfo::with_enum("dwShareMode", "DWORD", "FILE_SHARE"),
                ParamInfo::new("lpSecurityAttributes", "LPSECURITY_ATTRIBUTES"),
                ParamInfo::with_enum("dwCreationDisposition", "DWORD", "FILE_CREATE"),
                ParamInfo::with_enum("dwFlagsAndAttributes", "DWORD", "FILE_ATTRIBUTE"),
                ParamInfo::new("hTemplateFile", "HANDLE"),
            ],
        ));
        self.add(ApiSignature::with_enums(
            "CreateFileW",
            "HANDLE",
            vec![
                ParamInfo::new("lpFileName", "LPCWSTR"),
                ParamInfo::with_enum("dwDesiredAccess", "DWORD", "GENERIC_ACCESS"),
                ParamInfo::with_enum("dwShareMode", "DWORD", "FILE_SHARE"),
                ParamInfo::new("lpSecurityAttributes", "LPSECURITY_ATTRIBUTES"),
                ParamInfo::with_enum("dwCreationDisposition", "DWORD", "FILE_CREATE"),
                ParamInfo::with_enum("dwFlagsAndAttributes", "DWORD", "FILE_ATTRIBUTE"),
                ParamInfo::new("hTemplateFile", "HANDLE"),
            ],
        ));
        self.add(ApiSignature::new(
            "ReadFile",
            "BOOL",
            &[
                ("hFile", "HANDLE"),
                ("lpBuffer", "LPVOID"),
                ("nNumberOfBytesToRead", "DWORD"),
                ("lpNumberOfBytesRead", "LPDWORD"),
                ("lpOverlapped", "LPOVERLAPPED"),
            ],
        ));
        self.add(ApiSignature::new(
            "WriteFile",
            "BOOL",
            &[
                ("hFile", "HANDLE"),
                ("lpBuffer", "LPCVOID"),
                ("nNumberOfBytesToWrite", "DWORD"),
                ("lpNumberOfBytesWritten", "LPDWORD"),
                ("lpOverlapped", "LPOVERLAPPED"),
            ],
        ));
        self.add(ApiSignature::new(
            "CloseHandle",
            "BOOL",
            &[("hObject", "HANDLE")],
        ));

        // Module
        self.add(ApiSignature::new(
            "GetModuleHandleA",
            "HMODULE",
            &[("lpModuleName", "LPCSTR")],
        ));
        self.add(ApiSignature::new(
            "GetModuleHandleW",
            "HMODULE",
            &[("lpModuleName", "LPCWSTR")],
        ));
        self.add(ApiSignature::new(
            "LoadLibraryA",
            "HMODULE",
            &[("lpLibFileName", "LPCSTR")],
        ));
        self.add(ApiSignature::new(
            "LoadLibraryW",
            "HMODULE",
            &[("lpLibFileName", "LPCWSTR")],
        ));
        self.add(ApiSignature::new(
            "GetProcAddress",
            "FARPROC",
            &[("hModule", "HMODULE"), ("lpProcName", "LPCSTR")],
        ));
        self.add(ApiSignature::new(
            "FreeLibrary",
            "BOOL",
            &[("hLibModule", "HMODULE")],
        ));

        // Heap
        self.add(ApiSignature::new(
            "HeapAlloc",
            "LPVOID",
            &[
                ("hHeap", "HANDLE"),
                ("dwFlags", "DWORD"),
                ("dwBytes", "SIZE_T"),
            ],
        ));
        self.add(ApiSignature::new(
            "HeapFree",
            "BOOL",
            &[
                ("hHeap", "HANDLE"),
                ("dwFlags", "DWORD"),
                ("lpMem", "LPVOID"),
            ],
        ));
        self.add(ApiSignature::new("GetProcessHeap", "HANDLE", &[]));

        // Process Injection with enum groups
        self.add(ApiSignature::with_enums(
            "OpenProcess",
            "HANDLE",
            vec![
                ParamInfo::with_enum("dwDesiredAccess", "DWORD", "PROCESS_ACCESS"),
                ParamInfo::new("bInheritHandle", "BOOL"),
                ParamInfo::new("dwProcessId", "DWORD"),
            ],
        ));
        self.add(ApiSignature::with_enums(
            "VirtualAllocEx",
            "LPVOID",
            vec![
                ParamInfo::new("hProcess", "HANDLE"),
                ParamInfo::new("lpAddress", "LPVOID"),
                ParamInfo::new("dwSize", "SIZE_T"),
                ParamInfo::with_enum("flAllocationType", "DWORD", "MEM_ALLOC"),
                ParamInfo::with_enum("flProtect", "DWORD", "PAGE_PROTECT"),
            ],
        ));
        self.add(ApiSignature::with_enums(
            "VirtualFreeEx",
            "BOOL",
            vec![
                ParamInfo::new("hProcess", "HANDLE"),
                ParamInfo::new("lpAddress", "LPVOID"),
                ParamInfo::new("dwSize", "SIZE_T"),
                ParamInfo::with_enum("dwFreeType", "DWORD", "MEM_ALLOC"),
            ],
        ));
        self.add(ApiSignature::new(
            "WriteProcessMemory",
            "BOOL",
            &[
                ("hProcess", "HANDLE"),
                ("lpBaseAddress", "LPVOID"),
                ("lpBuffer", "LPCVOID"),
                ("nSize", "SIZE_T"),
                ("lpNumberOfBytesWritten", "SIZE_T*"),
            ],
        ));
        self.add(ApiSignature::new(
            "ReadProcessMemory",
            "BOOL",
            &[
                ("hProcess", "HANDLE"),
                ("lpBaseAddress", "LPCVOID"),
                ("lpBuffer", "LPVOID"),
                ("nSize", "SIZE_T"),
                ("lpNumberOfBytesRead", "SIZE_T*"),
            ],
        ));
        self.add(ApiSignature::new(
            "CreateRemoteThread",
            "HANDLE",
            &[
                ("hProcess", "HANDLE"),
                ("lpThreadAttributes", "LPSECURITY_ATTRIBUTES"),
                ("dwStackSize", "SIZE_T"),
                ("lpStartAddress", "LPTHREAD_START_ROUTINE"),
                ("lpParameter", "LPVOID"),
                ("dwCreationFlags", "DWORD"),
                ("lpThreadId", "LPDWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "CreateRemoteThreadEx",
            "HANDLE",
            &[
                ("hProcess", "HANDLE"),
                ("lpThreadAttributes", "LPSECURITY_ATTRIBUTES"),
                ("dwStackSize", "SIZE_T"),
                ("lpStartAddress", "LPTHREAD_START_ROUTINE"),
                ("lpParameter", "LPVOID"),
                ("dwCreationFlags", "DWORD"),
                ("lpAttributeList", "LPPROC_THREAD_ATTRIBUTE_LIST"),
                ("lpThreadId", "LPDWORD"),
            ],
        ));

        // Anti-Debug
        self.add(ApiSignature::new("IsDebuggerPresent", "BOOL", &[]));
        self.add(ApiSignature::new(
            "CheckRemoteDebuggerPresent",
            "BOOL",
            &[("hProcess", "HANDLE"), ("pbDebuggerPresent", "PBOOL")],
        ));
        self.add(ApiSignature::new(
            "OutputDebugStringA",
            "void",
            &[("lpOutputString", "LPCSTR")],
        ));
        self.add(ApiSignature::new(
            "OutputDebugStringW",
            "void",
            &[("lpOutputString", "LPCWSTR")],
        ));

        // Process enumeration with enum groups
        self.add(ApiSignature::with_enums(
            "CreateToolhelp32Snapshot",
            "HANDLE",
            vec![
                ParamInfo::with_enum("dwFlags", "DWORD", "TH32CS"),
                ParamInfo::new("th32ProcessID", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "Process32FirstW",
            "BOOL",
            &[("hSnapshot", "HANDLE"), ("lppe", "LPPROCESSENTRY32W")],
        ));
        self.add(ApiSignature::new(
            "Process32NextW",
            "BOOL",
            &[("hSnapshot", "HANDLE"), ("lppe", "LPPROCESSENTRY32W")],
        ));

        // CreateProcess with enum groups
        self.add(ApiSignature::with_enums(
            "CreateProcessA",
            "BOOL",
            vec![
                ParamInfo::new("lpApplicationName", "LPCSTR"),
                ParamInfo::new("lpCommandLine", "LPSTR"),
                ParamInfo::new("lpProcessAttributes", "LPSECURITY_ATTRIBUTES"),
                ParamInfo::new("lpThreadAttributes", "LPSECURITY_ATTRIBUTES"),
                ParamInfo::new("bInheritHandles", "BOOL"),
                ParamInfo::with_enum("dwCreationFlags", "DWORD", "CREATION_FLAGS"),
                ParamInfo::new("lpEnvironment", "LPVOID"),
                ParamInfo::new("lpCurrentDirectory", "LPCSTR"),
                ParamInfo::new("lpStartupInfo", "LPSTARTUPINFOA"),
                ParamInfo::new("lpProcessInformation", "LPPROCESS_INFORMATION"),
            ],
        ));
        self.add(ApiSignature::with_enums(
            "CreateProcessW",
            "BOOL",
            vec![
                ParamInfo::new("lpApplicationName", "LPCWSTR"),
                ParamInfo::new("lpCommandLine", "LPWSTR"),
                ParamInfo::new("lpProcessAttributes", "LPSECURITY_ATTRIBUTES"),
                ParamInfo::new("lpThreadAttributes", "LPSECURITY_ATTRIBUTES"),
                ParamInfo::new("bInheritHandles", "BOOL"),
                ParamInfo::with_enum("dwCreationFlags", "DWORD", "CREATION_FLAGS"),
                ParamInfo::new("lpEnvironment", "LPVOID"),
                ParamInfo::new("lpCurrentDirectory", "LPCWSTR"),
                ParamInfo::new("lpStartupInfo", "LPSTARTUPINFOW"),
                ParamInfo::new("lpProcessInformation", "LPPROCESS_INFORMATION"),
            ],
        ));

        // Wait/Sync
        self.add(ApiSignature::new(
            "WaitForSingleObject",
            "DWORD",
            &[("hHandle", "HANDLE"), ("dwMilliseconds", "DWORD")],
        ));
        self.add(ApiSignature::new(
            "WaitForMultipleObjects",
            "DWORD",
            &[
                ("nCount", "DWORD"),
                ("lpHandles", "const HANDLE*"),
                ("bWaitAll", "BOOL"),
                ("dwMilliseconds", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "Sleep",
            "void",
            &[("dwMilliseconds", "DWORD")],
        ));

        // File/System
        self.add(ApiSignature::new(
            "GetEnvironmentVariableA",
            "DWORD",
            &[
                ("lpName", "LPCSTR"),
                ("lpBuffer", "LPSTR"),
                ("nSize", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "GetTempPathA",
            "DWORD",
            &[("nBufferLength", "DWORD"), ("lpBuffer", "LPSTR")],
        ));
        self.add(ApiSignature::new(
            "GetSystemDirectoryA",
            "UINT",
            &[("lpBuffer", "LPSTR"), ("uSize", "UINT")],
        ));
        self.add(ApiSignature::new(
            "GetWindowsDirectoryA",
            "UINT",
            &[("lpBuffer", "LPSTR"), ("uSize", "UINT")],
        ));
        self.add(ApiSignature::new(
            "CopyFileA",
            "BOOL",
            &[
                ("lpExistingFileName", "LPCSTR"),
                ("lpNewFileName", "LPCSTR"),
                ("bFailIfExists", "BOOL"),
            ],
        ));
        self.add(ApiSignature::new(
            "DeleteFileA",
            "BOOL",
            &[("lpFileName", "LPCSTR")],
        ));
        self.add(ApiSignature::new(
            "MoveFileA",
            "BOOL",
            &[
                ("lpExistingFileName", "LPCSTR"),
                ("lpNewFileName", "LPCSTR"),
            ],
        ));
        self.add(ApiSignature::new(
            "GetModuleFileNameA",
            "DWORD",
            &[
                ("hModule", "HMODULE"),
                ("lpFilename", "LPSTR"),
                ("nSize", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new("GetCommandLineA", "LPSTR", &[]));

        // Timing
        self.add(ApiSignature::new("GetTickCount", "DWORD", &[]));
        self.add(ApiSignature::new("GetTickCount64", "ULONGLONG", &[]));
        self.add(ApiSignature::new(
            "QueryPerformanceCounter",
            "BOOL",
            &[("lpPerformanceCount", "LARGE_INTEGER*")],
        ));

        // Error
        self.add(ApiSignature::new("GetLastError", "DWORD", &[]));
        self.add(ApiSignature::new(
            "SetLastError",
            "void",
            &[("dwErrCode", "DWORD")],
        ));
        self.add(ApiSignature::new(
            "ExitProcess",
            "void",
            &[("uExitCode", "UINT")],
        ));

        // Thread context (Process Hollowing)
        self.add(ApiSignature::new(
            "GetThreadContext",
            "BOOL",
            &[("hThread", "HANDLE"), ("lpContext", "LPCONTEXT")],
        ));
        self.add(ApiSignature::new(
            "SetThreadContext",
            "BOOL",
            &[("hThread", "HANDLE"), ("lpContext", "const CONTEXT*")],
        ));
        self.add(ApiSignature::new(
            "ResumeThread",
            "DWORD",
            &[("hThread", "HANDLE")],
        ));
        self.add(ApiSignature::new(
            "SuspendThread",
            "DWORD",
            &[("hThread", "HANDLE")],
        ));
    }

    fn load_user32(&mut self) {
        // MessageBox with enum groups
        self.add(ApiSignature::with_enums(
            "MessageBoxA",
            "int",
            vec![
                ParamInfo::new("hWnd", "HWND"),
                ParamInfo::new("lpText", "LPCSTR"),
                ParamInfo::new("lpCaption", "LPCSTR"),
                ParamInfo::with_enum("uType", "UINT", "MB_TYPE"),
            ],
        ));
        self.add(ApiSignature::with_enums(
            "MessageBoxW",
            "int",
            vec![
                ParamInfo::new("hWnd", "HWND"),
                ParamInfo::new("lpText", "LPCWSTR"),
                ParamInfo::new("lpCaption", "LPCWSTR"),
                ParamInfo::with_enum("uType", "UINT", "MB_TYPE"),
            ],
        ));
        self.add(ApiSignature::new(
            "GetWindowTextA",
            "int",
            &[
                ("hWnd", "HWND"),
                ("lpString", "LPSTR"),
                ("nMaxCount", "int"),
            ],
        ));
        self.add(ApiSignature::new(
            "SetWindowTextA",
            "BOOL",
            &[("hWnd", "HWND"), ("lpString", "LPCSTR")],
        ));

        // Input/Keylogger detection
        self.add(ApiSignature::new(
            "GetAsyncKeyState",
            "SHORT",
            &[("vKey", "int")],
        ));
        self.add(ApiSignature::new(
            "GetKeyState",
            "SHORT",
            &[("nVirtKey", "int")],
        ));
        self.add(ApiSignature::new(
            "SetWindowsHookExA",
            "HHOOK",
            &[
                ("idHook", "int"),
                ("lpfn", "HOOKPROC"),
                ("hmod", "HINSTANCE"),
                ("dwThreadId", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "SetWindowsHookExW",
            "HHOOK",
            &[
                ("idHook", "int"),
                ("lpfn", "HOOKPROC"),
                ("hmod", "HINSTANCE"),
                ("dwThreadId", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "CallNextHookEx",
            "LRESULT",
            &[
                ("hhk", "HHOOK"),
                ("nCode", "int"),
                ("wParam", "WPARAM"),
                ("lParam", "LPARAM"),
            ],
        ));
        self.add(ApiSignature::new(
            "UnhookWindowsHookEx",
            "BOOL",
            &[("hhk", "HHOOK")],
        ));

        // Window enumeration
        self.add(ApiSignature::new("GetForegroundWindow", "HWND", &[]));
        self.add(ApiSignature::new(
            "GetWindowThreadProcessId",
            "DWORD",
            &[("hWnd", "HWND"), ("lpdwProcessId", "LPDWORD")],
        ));
        self.add(ApiSignature::new(
            "FindWindowA",
            "HWND",
            &[("lpClassName", "LPCSTR"), ("lpWindowName", "LPCSTR")],
        ));
        self.add(ApiSignature::new(
            "FindWindowW",
            "HWND",
            &[("lpClassName", "LPCWSTR"), ("lpWindowName", "LPCWSTR")],
        ));
        self.add(ApiSignature::new(
            "EnumWindows",
            "BOOL",
            &[("lpEnumFunc", "WNDENUMPROC"), ("lParam", "LPARAM")],
        ));
        self.add(ApiSignature::new("GetDesktopWindow", "HWND", &[]));
        self.add(ApiSignature::new(
            "PostMessageA",
            "BOOL",
            &[
                ("hWnd", "HWND"),
                ("Msg", "UINT"),
                ("wParam", "WPARAM"),
                ("lParam", "LPARAM"),
            ],
        ));
        self.add(ApiSignature::new(
            "SendMessageA",
            "LRESULT",
            &[
                ("hWnd", "HWND"),
                ("Msg", "UINT"),
                ("wParam", "WPARAM"),
                ("lParam", "LPARAM"),
            ],
        ));
    }

    fn load_ntdll(&mut self) {
        self.add(ApiSignature::new(
            "NtAllocateVirtualMemory",
            "NTSTATUS",
            &[
                ("ProcessHandle", "HANDLE"),
                ("BaseAddress", "PVOID*"),
                ("ZeroBits", "ULONG_PTR"),
                ("RegionSize", "PSIZE_T"),
                ("AllocationType", "ULONG"),
                ("Protect", "ULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtProtectVirtualMemory",
            "NTSTATUS",
            &[
                ("ProcessHandle", "HANDLE"),
                ("BaseAddress", "PVOID*"),
                ("RegionSize", "PSIZE_T"),
                ("NewProtect", "ULONG"),
                ("OldProtect", "PULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtWriteVirtualMemory",
            "NTSTATUS",
            &[
                ("ProcessHandle", "HANDLE"),
                ("BaseAddress", "PVOID"),
                ("Buffer", "PVOID"),
                ("NumberOfBytesToWrite", "SIZE_T"),
                ("NumberOfBytesWritten", "PSIZE_T"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtReadVirtualMemory",
            "NTSTATUS",
            &[
                ("ProcessHandle", "HANDLE"),
                ("BaseAddress", "PVOID"),
                ("Buffer", "PVOID"),
                ("NumberOfBytesToRead", "SIZE_T"),
                ("NumberOfBytesRead", "PSIZE_T"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtCreateThreadEx",
            "NTSTATUS",
            &[
                ("ThreadHandle", "PHANDLE"),
                ("DesiredAccess", "ACCESS_MASK"),
                ("ObjectAttributes", "POBJECT_ATTRIBUTES"),
                ("ProcessHandle", "HANDLE"),
                ("StartRoutine", "PVOID"),
                ("Argument", "PVOID"),
                ("CreateFlags", "ULONG"),
                ("ZeroBits", "SIZE_T"),
                ("StackSize", "SIZE_T"),
                ("MaximumStackSize", "SIZE_T"),
                ("AttributeList", "PVOID"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtQueryInformationProcess",
            "NTSTATUS",
            &[
                ("ProcessHandle", "HANDLE"),
                ("ProcessInformationClass", "PROCESSINFOCLASS"),
                ("ProcessInformation", "PVOID"),
                ("ProcessInformationLength", "ULONG"),
                ("ReturnLength", "PULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtQuerySystemInformation",
            "NTSTATUS",
            &[
                ("SystemInformationClass", "SYSTEM_INFORMATION_CLASS"),
                ("SystemInformation", "PVOID"),
                ("SystemInformationLength", "ULONG"),
                ("ReturnLength", "PULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "LdrLoadDll",
            "NTSTATUS",
            &[
                ("PathToFile", "PWSTR"),
                ("Flags", "PULONG"),
                ("ModuleFileName", "PUNICODE_STRING"),
                ("ModuleHandle", "PHANDLE"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtOpenProcess",
            "NTSTATUS",
            &[
                ("ProcessHandle", "PHANDLE"),
                ("DesiredAccess", "ACCESS_MASK"),
                ("ObjectAttributes", "POBJECT_ATTRIBUTES"),
                ("ClientId", "PCLIENT_ID"),
            ],
        ));

        // Section (Process Hollowing)
        self.add(ApiSignature::new(
            "NtCreateSection",
            "NTSTATUS",
            &[
                ("SectionHandle", "PHANDLE"),
                ("DesiredAccess", "ACCESS_MASK"),
                ("ObjectAttributes", "POBJECT_ATTRIBUTES"),
                ("MaximumSize", "PLARGE_INTEGER"),
                ("SectionPageProtection", "ULONG"),
                ("AllocationAttributes", "ULONG"),
                ("FileHandle", "HANDLE"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtMapViewOfSection",
            "NTSTATUS",
            &[
                ("SectionHandle", "HANDLE"),
                ("ProcessHandle", "HANDLE"),
                ("BaseAddress", "PVOID*"),
                ("ZeroBits", "ULONG_PTR"),
                ("CommitSize", "SIZE_T"),
                ("SectionOffset", "PLARGE_INTEGER"),
                ("ViewSize", "PSIZE_T"),
                ("InheritDisposition", "SECTION_INHERIT"),
                ("AllocationType", "ULONG"),
                ("Win32Protect", "ULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "NtUnmapViewOfSection",
            "NTSTATUS",
            &[("ProcessHandle", "HANDLE"), ("BaseAddress", "PVOID")],
        ));

        // APC Injection
        self.add(ApiSignature::new(
            "NtQueueApcThread",
            "NTSTATUS",
            &[
                ("ThreadHandle", "HANDLE"),
                ("ApcRoutine", "PPS_APC_ROUTINE"),
                ("ApcArgument1", "PVOID"),
                ("ApcArgument2", "PVOID"),
                ("ApcArgument3", "PVOID"),
            ],
        ));

        // Common
        self.add(ApiSignature::new(
            "NtClose",
            "NTSTATUS",
            &[("Handle", "HANDLE")],
        ));
        self.add(ApiSignature::new(
            "NtFreeVirtualMemory",
            "NTSTATUS",
            &[
                ("ProcessHandle", "HANDLE"),
                ("BaseAddress", "PVOID*"),
                ("RegionSize", "PSIZE_T"),
                ("FreeType", "ULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "RtlInitUnicodeString",
            "void",
            &[
                ("DestinationString", "PUNICODE_STRING"),
                ("SourceString", "PCWSTR"),
            ],
        ));

        // Anti-debug
        self.add(ApiSignature::new(
            "NtSetInformationThread",
            "NTSTATUS",
            &[
                ("ThreadHandle", "HANDLE"),
                ("ThreadInformationClass", "THREADINFOCLASS"),
                ("ThreadInformation", "PVOID"),
                ("ThreadInformationLength", "ULONG"),
            ],
        ));
    }

    fn load_advapi32(&mut self) {
        // Registry
        self.add(ApiSignature::new(
            "RegOpenKeyExA",
            "LSTATUS",
            &[
                ("hKey", "HKEY"),
                ("lpSubKey", "LPCSTR"),
                ("ulOptions", "DWORD"),
                ("samDesired", "REGSAM"),
                ("phkResult", "PHKEY"),
            ],
        ));
        self.add(ApiSignature::new(
            "RegOpenKeyExW",
            "LSTATUS",
            &[
                ("hKey", "HKEY"),
                ("lpSubKey", "LPCWSTR"),
                ("ulOptions", "DWORD"),
                ("samDesired", "REGSAM"),
                ("phkResult", "PHKEY"),
            ],
        ));
        self.add(ApiSignature::new(
            "RegSetValueExA",
            "LSTATUS",
            &[
                ("hKey", "HKEY"),
                ("lpValueName", "LPCSTR"),
                ("Reserved", "DWORD"),
                ("dwType", "DWORD"),
                ("lpData", "const BYTE*"),
                ("cbData", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "RegSetValueExW",
            "LSTATUS",
            &[
                ("hKey", "HKEY"),
                ("lpValueName", "LPCWSTR"),
                ("Reserved", "DWORD"),
                ("dwType", "DWORD"),
                ("lpData", "const BYTE*"),
                ("cbData", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "RegQueryValueExA",
            "LSTATUS",
            &[
                ("hKey", "HKEY"),
                ("lpValueName", "LPCSTR"),
                ("lpReserved", "LPDWORD"),
                ("lpType", "LPDWORD"),
                ("lpData", "LPBYTE"),
                ("lpcbData", "LPDWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "RegCloseKey",
            "LSTATUS",
            &[("hKey", "HKEY")],
        ));

        // Privileges
        self.add(ApiSignature::new(
            "OpenProcessToken",
            "BOOL",
            &[
                ("ProcessHandle", "HANDLE"),
                ("DesiredAccess", "DWORD"),
                ("TokenHandle", "PHANDLE"),
            ],
        ));
        self.add(ApiSignature::new(
            "AdjustTokenPrivileges",
            "BOOL",
            &[
                ("TokenHandle", "HANDLE"),
                ("DisableAllPrivileges", "BOOL"),
                ("NewState", "PTOKEN_PRIVILEGES"),
                ("BufferLength", "DWORD"),
                ("PreviousState", "PTOKEN_PRIVILEGES"),
                ("ReturnLength", "PDWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "LookupPrivilegeValueA",
            "BOOL",
            &[
                ("lpSystemName", "LPCSTR"),
                ("lpName", "LPCSTR"),
                ("lpLuid", "PLUID"),
            ],
        ));

        // Crypto
        self.add(ApiSignature::new(
            "CryptAcquireContextA",
            "BOOL",
            &[
                ("phProv", "HCRYPTPROV*"),
                ("szContainer", "LPCSTR"),
                ("szProvider", "LPCSTR"),
                ("dwProvType", "DWORD"),
                ("dwFlags", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "CryptEncrypt",
            "BOOL",
            &[
                ("hKey", "HCRYPTKEY"),
                ("hHash", "HCRYPTHASH"),
                ("Final", "BOOL"),
                ("dwFlags", "DWORD"),
                ("pbData", "BYTE*"),
                ("pdwDataLen", "DWORD*"),
                ("dwBufLen", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "CryptDecrypt",
            "BOOL",
            &[
                ("hKey", "HCRYPTKEY"),
                ("hHash", "HCRYPTHASH"),
                ("Final", "BOOL"),
                ("dwFlags", "DWORD"),
                ("pbData", "BYTE*"),
                ("pdwDataLen", "DWORD*"),
            ],
        ));

        // Services (Persistence)
        self.add(ApiSignature::new(
            "OpenSCManagerA",
            "SC_HANDLE",
            &[
                ("lpMachineName", "LPCSTR"),
                ("lpDatabaseName", "LPCSTR"),
                ("dwDesiredAccess", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "CreateServiceA",
            "SC_HANDLE",
            &[
                ("hSCManager", "SC_HANDLE"),
                ("lpServiceName", "LPCSTR"),
                ("lpDisplayName", "LPCSTR"),
                ("dwDesiredAccess", "DWORD"),
                ("dwServiceType", "DWORD"),
                ("dwStartType", "DWORD"),
                ("dwErrorControl", "DWORD"),
                ("lpBinaryPathName", "LPCSTR"),
                ("lpLoadOrderGroup", "LPCSTR"),
                ("lpdwTagId", "LPDWORD"),
                ("lpDependencies", "LPCSTR"),
                ("lpServiceStartName", "LPCSTR"),
                ("lpPassword", "LPCSTR"),
            ],
        ));
        self.add(ApiSignature::new(
            "OpenServiceA",
            "SC_HANDLE",
            &[
                ("hSCManager", "SC_HANDLE"),
                ("lpServiceName", "LPCSTR"),
                ("dwDesiredAccess", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "StartServiceA",
            "BOOL",
            &[
                ("hService", "SC_HANDLE"),
                ("dwNumServiceArgs", "DWORD"),
                ("lpServiceArgVectors", "LPCSTR*"),
            ],
        ));
        self.add(ApiSignature::new(
            "ControlService",
            "BOOL",
            &[
                ("hService", "SC_HANDLE"),
                ("dwControl", "DWORD"),
                ("lpServiceStatus", "LPSERVICE_STATUS"),
            ],
        ));
        self.add(ApiSignature::new(
            "DeleteService",
            "BOOL",
            &[("hService", "SC_HANDLE")],
        ));
        self.add(ApiSignature::new(
            "CloseServiceHandle",
            "BOOL",
            &[("hSCObject", "SC_HANDLE")],
        ));
    }

    fn load_ws2_32(&mut self) {
        self.add(ApiSignature::new(
            "WSAStartup",
            "int",
            &[("wVersionRequested", "WORD"), ("lpWSAData", "LPWSADATA")],
        ));
        self.add(ApiSignature::new("WSACleanup", "int", &[]));
        self.add(ApiSignature::new(
            "socket",
            "SOCKET",
            &[("af", "int"), ("type", "int"), ("protocol", "int")],
        ));
        self.add(ApiSignature::new(
            "connect",
            "int",
            &[
                ("s", "SOCKET"),
                ("name", "const sockaddr*"),
                ("namelen", "int"),
            ],
        ));
        self.add(ApiSignature::new(
            "send",
            "int",
            &[
                ("s", "SOCKET"),
                ("buf", "const char*"),
                ("len", "int"),
                ("flags", "int"),
            ],
        ));
        self.add(ApiSignature::new(
            "recv",
            "int",
            &[
                ("s", "SOCKET"),
                ("buf", "char*"),
                ("len", "int"),
                ("flags", "int"),
            ],
        ));
        self.add(ApiSignature::new("closesocket", "int", &[("s", "SOCKET")]));
        self.add(ApiSignature::new(
            "bind",
            "int",
            &[
                ("s", "SOCKET"),
                ("name", "const sockaddr*"),
                ("namelen", "int"),
            ],
        ));
        self.add(ApiSignature::new(
            "listen",
            "int",
            &[("s", "SOCKET"), ("backlog", "int")],
        ));
        self.add(ApiSignature::new(
            "accept",
            "SOCKET",
            &[("s", "SOCKET"), ("addr", "sockaddr*"), ("addrlen", "int*")],
        ));
        self.add(ApiSignature::new(
            "getaddrinfo",
            "INT",
            &[
                ("pNodeName", "PCSTR"),
                ("pServiceName", "PCSTR"),
                ("pHints", "const ADDRINFOA*"),
                ("ppResult", "PADDRINFOA*"),
            ],
        ));
        self.add(ApiSignature::new(
            "inet_addr",
            "unsigned long",
            &[("cp", "const char*")],
        ));
        self.add(ApiSignature::new(
            "htons",
            "u_short",
            &[("hostshort", "u_short")],
        ));
    }

    fn load_winhttp(&mut self) {
        self.add(ApiSignature::new(
            "WinHttpOpen",
            "HINTERNET",
            &[
                ("pszAgentW", "LPCWSTR"),
                ("dwAccessType", "DWORD"),
                ("pszProxyW", "LPCWSTR"),
                ("pszProxyBypassW", "LPCWSTR"),
                ("dwFlags", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "WinHttpConnect",
            "HINTERNET",
            &[
                ("hSession", "HINTERNET"),
                ("pswzServerName", "LPCWSTR"),
                ("nServerPort", "INTERNET_PORT"),
                ("dwReserved", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "WinHttpOpenRequest",
            "HINTERNET",
            &[
                ("hConnect", "HINTERNET"),
                ("pwszVerb", "LPCWSTR"),
                ("pwszObjectName", "LPCWSTR"),
                ("pwszVersion", "LPCWSTR"),
                ("pwszReferrer", "LPCWSTR"),
                ("ppwszAcceptTypes", "LPCWSTR*"),
                ("dwFlags", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "WinHttpSendRequest",
            "BOOL",
            &[
                ("hRequest", "HINTERNET"),
                ("lpszHeaders", "LPCWSTR"),
                ("dwHeadersLength", "DWORD"),
                ("lpOptional", "LPVOID"),
                ("dwOptionalLength", "DWORD"),
                ("dwTotalLength", "DWORD"),
                ("dwContext", "DWORD_PTR"),
            ],
        ));
        self.add(ApiSignature::new(
            "WinHttpReceiveResponse",
            "BOOL",
            &[("hRequest", "HINTERNET"), ("lpReserved", "LPVOID")],
        ));
        self.add(ApiSignature::new(
            "WinHttpReadData",
            "BOOL",
            &[
                ("hRequest", "HINTERNET"),
                ("lpBuffer", "LPVOID"),
                ("dwNumberOfBytesToRead", "DWORD"),
                ("lpdwNumberOfBytesRead", "LPDWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "WinHttpCloseHandle",
            "BOOL",
            &[("hInternet", "HINTERNET")],
        ));
    }

    fn load_wininet(&mut self) {
        self.add(ApiSignature::new(
            "InternetOpenA",
            "HINTERNET",
            &[
                ("lpszAgent", "LPCSTR"),
                ("dwAccessType", "DWORD"),
                ("lpszProxy", "LPCSTR"),
                ("lpszProxyBypass", "LPCSTR"),
                ("dwFlags", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "InternetConnectA",
            "HINTERNET",
            &[
                ("hInternet", "HINTERNET"),
                ("lpszServerName", "LPCSTR"),
                ("nServerPort", "INTERNET_PORT"),
                ("lpszUserName", "LPCSTR"),
                ("lpszPassword", "LPCSTR"),
                ("dwService", "DWORD"),
                ("dwFlags", "DWORD"),
                ("dwContext", "DWORD_PTR"),
            ],
        ));
        self.add(ApiSignature::new(
            "HttpOpenRequestA",
            "HINTERNET",
            &[
                ("hConnect", "HINTERNET"),
                ("lpszVerb", "LPCSTR"),
                ("lpszObjectName", "LPCSTR"),
                ("lpszVersion", "LPCSTR"),
                ("lpszReferrer", "LPCSTR"),
                ("lplpszAcceptTypes", "LPCSTR*"),
                ("dwFlags", "DWORD"),
                ("dwContext", "DWORD_PTR"),
            ],
        ));
        self.add(ApiSignature::new(
            "HttpSendRequestA",
            "BOOL",
            &[
                ("hRequest", "HINTERNET"),
                ("lpszHeaders", "LPCSTR"),
                ("dwHeadersLength", "DWORD"),
                ("lpOptional", "LPVOID"),
                ("dwOptionalLength", "DWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "InternetReadFile",
            "BOOL",
            &[
                ("hFile", "HINTERNET"),
                ("lpBuffer", "LPVOID"),
                ("dwNumberOfBytesToRead", "DWORD"),
                ("lpdwNumberOfBytesRead", "LPDWORD"),
            ],
        ));
        self.add(ApiSignature::new(
            "InternetCloseHandle",
            "BOOL",
            &[("hInternet", "HINTERNET")],
        ));
    }

    fn load_shell32(&mut self) {
        self.add(ApiSignature::new(
            "ShellExecuteA",
            "HINSTANCE",
            &[
                ("hwnd", "HWND"),
                ("lpOperation", "LPCSTR"),
                ("lpFile", "LPCSTR"),
                ("lpParameters", "LPCSTR"),
                ("lpDirectory", "LPCSTR"),
                ("nShowCmd", "INT"),
            ],
        ));
        self.add(ApiSignature::new(
            "ShellExecuteW",
            "HINSTANCE",
            &[
                ("hwnd", "HWND"),
                ("lpOperation", "LPCWSTR"),
                ("lpFile", "LPCWSTR"),
                ("lpParameters", "LPCWSTR"),
                ("lpDirectory", "LPCWSTR"),
                ("nShowCmd", "INT"),
            ],
        ));
        self.add(ApiSignature::new(
            "ShellExecuteExA",
            "BOOL",
            &[("pExecInfo", "SHELLEXECUTEINFOA*")],
        ));
        self.add(ApiSignature::new(
            "SHGetFolderPathA",
            "HRESULT",
            &[
                ("hwnd", "HWND"),
                ("csidl", "int"),
                ("hToken", "HANDLE"),
                ("dwFlags", "DWORD"),
                ("pszPath", "LPSTR"),
            ],
        ));
    }

    fn load_bcrypt(&mut self) {
        self.add(ApiSignature::new(
            "BCryptOpenAlgorithmProvider",
            "NTSTATUS",
            &[
                ("phAlgorithm", "BCRYPT_ALG_HANDLE*"),
                ("pszAlgId", "LPCWSTR"),
                ("pszImplementation", "LPCWSTR"),
                ("dwFlags", "ULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "BCryptGenerateSymmetricKey",
            "NTSTATUS",
            &[
                ("hAlgorithm", "BCRYPT_ALG_HANDLE"),
                ("phKey", "BCRYPT_KEY_HANDLE*"),
                ("pbKeyObject", "PUCHAR"),
                ("cbKeyObject", "ULONG"),
                ("pbSecret", "PUCHAR"),
                ("cbSecret", "ULONG"),
                ("dwFlags", "ULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "BCryptEncrypt",
            "NTSTATUS",
            &[
                ("hKey", "BCRYPT_KEY_HANDLE"),
                ("pbInput", "PUCHAR"),
                ("cbInput", "ULONG"),
                ("pPaddingInfo", "VOID*"),
                ("pbIV", "PUCHAR"),
                ("cbIV", "ULONG"),
                ("pbOutput", "PUCHAR"),
                ("cbOutput", "ULONG"),
                ("pcbResult", "ULONG*"),
                ("dwFlags", "ULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "BCryptDecrypt",
            "NTSTATUS",
            &[
                ("hKey", "BCRYPT_KEY_HANDLE"),
                ("pbInput", "PUCHAR"),
                ("cbInput", "ULONG"),
                ("pPaddingInfo", "VOID*"),
                ("pbIV", "PUCHAR"),
                ("cbIV", "ULONG"),
                ("pbOutput", "PUCHAR"),
                ("cbOutput", "ULONG"),
                ("pcbResult", "ULONG*"),
                ("dwFlags", "ULONG"),
            ],
        ));
        self.add(ApiSignature::new(
            "BCryptCloseAlgorithmProvider",
            "NTSTATUS",
            &[("hAlgorithm", "BCRYPT_ALG_HANDLE"), ("dwFlags", "ULONG")],
        ));
    }

    /// Look up a function signature by name
    pub fn get(&self, name: &str) -> Option<&ApiSignature> {
        self.signatures.get(name)
    }

    /// Get all signatures
    pub fn all(&self) -> &HashMap<String, ApiSignature> {
        &self.signatures
    }
}

impl Default for WinApiDatabase {
    fn default() -> Self {
        Self::new()
    }
}
