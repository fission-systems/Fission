//! Windows API Function Signatures
//!
//! Contains type information for common Windows API functions
//! to improve decompiler output quality.

use std::collections::HashMap;

/// Parameter type information
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub type_name: String,
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
            params: params
                .iter()
                .map(|(n, t)| ParamInfo {
                    name: n.to_string(),
                    type_name: t.to_string(),
                })
                .collect(),
        }
    }
}

/// Windows API Signature Database
pub struct WinApiDatabase {
    signatures: HashMap<String, ApiSignature>,
}

impl WinApiDatabase {
    pub fn new() -> Self {
        let mut db = Self {
            signatures: HashMap::new(),
        };
        db.load_kernel32();
        db.load_user32();
        db.load_ntdll();
        db
    }

    fn add(&mut self, sig: ApiSignature) {
        self.signatures.insert(sig.name.clone(), sig);
    }

    fn load_kernel32(&mut self) {
        // Memory functions
        self.add(ApiSignature::new("VirtualAlloc", "LPVOID", &[
            ("lpAddress", "LPVOID"),
            ("dwSize", "SIZE_T"),
            ("flAllocationType", "DWORD"),
            ("flProtect", "DWORD"),
        ]));
        self.add(ApiSignature::new("VirtualFree", "BOOL", &[
            ("lpAddress", "LPVOID"),
            ("dwSize", "SIZE_T"),
            ("dwFreeType", "DWORD"),
        ]));
        self.add(ApiSignature::new("VirtualProtect", "BOOL", &[
            ("lpAddress", "LPVOID"),
            ("dwSize", "SIZE_T"),
            ("flNewProtect", "DWORD"),
            ("lpflOldProtect", "PDWORD"),
        ]));
        
        // Process/Thread
        self.add(ApiSignature::new("GetCurrentProcess", "HANDLE", &[]));
        self.add(ApiSignature::new("GetCurrentProcessId", "DWORD", &[]));
        self.add(ApiSignature::new("GetCurrentThread", "HANDLE", &[]));
        self.add(ApiSignature::new("GetCurrentThreadId", "DWORD", &[]));
        self.add(ApiSignature::new("CreateThread", "HANDLE", &[
            ("lpThreadAttributes", "LPSECURITY_ATTRIBUTES"),
            ("dwStackSize", "SIZE_T"),
            ("lpStartAddress", "LPTHREAD_START_ROUTINE"),
            ("lpParameter", "LPVOID"),
            ("dwCreationFlags", "DWORD"),
            ("lpThreadId", "LPDWORD"),
        ]));
        self.add(ApiSignature::new("ExitThread", "void", &[
            ("dwExitCode", "DWORD"),
        ]));
        self.add(ApiSignature::new("TerminateProcess", "BOOL", &[
            ("hProcess", "HANDLE"),
            ("uExitCode", "UINT"),
        ]));
        
        // File I/O
        self.add(ApiSignature::new("CreateFileA", "HANDLE", &[
            ("lpFileName", "LPCSTR"),
            ("dwDesiredAccess", "DWORD"),
            ("dwShareMode", "DWORD"),
            ("lpSecurityAttributes", "LPSECURITY_ATTRIBUTES"),
            ("dwCreationDisposition", "DWORD"),
            ("dwFlagsAndAttributes", "DWORD"),
            ("hTemplateFile", "HANDLE"),
        ]));
        self.add(ApiSignature::new("CreateFileW", "HANDLE", &[
            ("lpFileName", "LPCWSTR"),
            ("dwDesiredAccess", "DWORD"),
            ("dwShareMode", "DWORD"),
            ("lpSecurityAttributes", "LPSECURITY_ATTRIBUTES"),
            ("dwCreationDisposition", "DWORD"),
            ("dwFlagsAndAttributes", "DWORD"),
            ("hTemplateFile", "HANDLE"),
        ]));
        self.add(ApiSignature::new("ReadFile", "BOOL", &[
            ("hFile", "HANDLE"),
            ("lpBuffer", "LPVOID"),
            ("nNumberOfBytesToRead", "DWORD"),
            ("lpNumberOfBytesRead", "LPDWORD"),
            ("lpOverlapped", "LPOVERLAPPED"),
        ]));
        self.add(ApiSignature::new("WriteFile", "BOOL", &[
            ("hFile", "HANDLE"),
            ("lpBuffer", "LPCVOID"),
            ("nNumberOfBytesToWrite", "DWORD"),
            ("lpNumberOfBytesWritten", "LPDWORD"),
            ("lpOverlapped", "LPOVERLAPPED"),
        ]));
        self.add(ApiSignature::new("CloseHandle", "BOOL", &[
            ("hObject", "HANDLE"),
        ]));
        
        // Module
        self.add(ApiSignature::new("GetModuleHandleA", "HMODULE", &[
            ("lpModuleName", "LPCSTR"),
        ]));
        self.add(ApiSignature::new("GetModuleHandleW", "HMODULE", &[
            ("lpModuleName", "LPCWSTR"),
        ]));
        self.add(ApiSignature::new("LoadLibraryA", "HMODULE", &[
            ("lpLibFileName", "LPCSTR"),
        ]));
        self.add(ApiSignature::new("LoadLibraryW", "HMODULE", &[
            ("lpLibFileName", "LPCWSTR"),
        ]));
        self.add(ApiSignature::new("GetProcAddress", "FARPROC", &[
            ("hModule", "HMODULE"),
            ("lpProcName", "LPCSTR"),
        ]));
        self.add(ApiSignature::new("FreeLibrary", "BOOL", &[
            ("hLibModule", "HMODULE"),
        ]));
        
        // Heap
        self.add(ApiSignature::new("HeapAlloc", "LPVOID", &[
            ("hHeap", "HANDLE"),
            ("dwFlags", "DWORD"),
            ("dwBytes", "SIZE_T"),
        ]));
        self.add(ApiSignature::new("HeapFree", "BOOL", &[
            ("hHeap", "HANDLE"),
            ("dwFlags", "DWORD"),
            ("lpMem", "LPVOID"),
        ]));
        self.add(ApiSignature::new("GetProcessHeap", "HANDLE", &[]));
    }

    fn load_user32(&mut self) {
        self.add(ApiSignature::new("MessageBoxA", "int", &[
            ("hWnd", "HWND"),
            ("lpText", "LPCSTR"),
            ("lpCaption", "LPCSTR"),
            ("uType", "UINT"),
        ]));
        self.add(ApiSignature::new("MessageBoxW", "int", &[
            ("hWnd", "HWND"),
            ("lpText", "LPCWSTR"),
            ("lpCaption", "LPCWSTR"),
            ("uType", "UINT"),
        ]));
        self.add(ApiSignature::new("GetWindowTextA", "int", &[
            ("hWnd", "HWND"),
            ("lpString", "LPSTR"),
            ("nMaxCount", "int"),
        ]));
        self.add(ApiSignature::new("SetWindowTextA", "BOOL", &[
            ("hWnd", "HWND"),
            ("lpString", "LPCSTR"),
        ]));
    }

    fn load_ntdll(&mut self) {
        self.add(ApiSignature::new("NtAllocateVirtualMemory", "NTSTATUS", &[
            ("ProcessHandle", "HANDLE"),
            ("BaseAddress", "PVOID*"),
            ("ZeroBits", "ULONG_PTR"),
            ("RegionSize", "PSIZE_T"),
            ("AllocationType", "ULONG"),
            ("Protect", "ULONG"),
        ]));
        self.add(ApiSignature::new("NtProtectVirtualMemory", "NTSTATUS", &[
            ("ProcessHandle", "HANDLE"),
            ("BaseAddress", "PVOID*"),
            ("RegionSize", "PSIZE_T"),
            ("NewProtect", "ULONG"),
            ("OldProtect", "PULONG"),
        ]));
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
