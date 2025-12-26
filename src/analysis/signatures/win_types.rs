//! Windows Data Types and Structures
//!
//! Common Windows API structures for type annotation in decompiled code.
//! Based on Windows SDK headers and ghidra-data community definitions.

use std::collections::HashMap;

// ============================================================================
// Windows Base Types (for annotation purposes)
// ============================================================================

/// Windows base type sizes
pub mod base_types {
    /// Type size information for annotation
    #[derive(Debug, Clone, Copy)]
    pub struct TypeInfo {
        pub name: &'static str,
        pub size_32: usize,
        pub size_64: usize,
        pub is_pointer: bool,
        pub is_signed: bool,
    }
    
    pub const BYTE: TypeInfo = TypeInfo { name: "BYTE", size_32: 1, size_64: 1, is_pointer: false, is_signed: false };
    pub const WORD: TypeInfo = TypeInfo { name: "WORD", size_32: 2, size_64: 2, is_pointer: false, is_signed: false };
    pub const DWORD: TypeInfo = TypeInfo { name: "DWORD", size_32: 4, size_64: 4, is_pointer: false, is_signed: false };
    pub const QWORD: TypeInfo = TypeInfo { name: "QWORD", size_32: 8, size_64: 8, is_pointer: false, is_signed: false };
    pub const BOOL: TypeInfo = TypeInfo { name: "BOOL", size_32: 4, size_64: 4, is_pointer: false, is_signed: true };
    pub const LONG: TypeInfo = TypeInfo { name: "LONG", size_32: 4, size_64: 4, is_pointer: false, is_signed: true };
    pub const ULONG: TypeInfo = TypeInfo { name: "ULONG", size_32: 4, size_64: 4, is_pointer: false, is_signed: false };
    pub const INT: TypeInfo = TypeInfo { name: "INT", size_32: 4, size_64: 4, is_pointer: false, is_signed: true };
    pub const UINT: TypeInfo = TypeInfo { name: "UINT", size_32: 4, size_64: 4, is_pointer: false, is_signed: false };
    pub const CHAR: TypeInfo = TypeInfo { name: "CHAR", size_32: 1, size_64: 1, is_pointer: false, is_signed: true };
    pub const WCHAR: TypeInfo = TypeInfo { name: "WCHAR", size_32: 2, size_64: 2, is_pointer: false, is_signed: false };
    
    // Pointer types
    pub const HANDLE: TypeInfo = TypeInfo { name: "HANDLE", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const PVOID: TypeInfo = TypeInfo { name: "PVOID", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const LPVOID: TypeInfo = TypeInfo { name: "LPVOID", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const LPSTR: TypeInfo = TypeInfo { name: "LPSTR", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const LPCSTR: TypeInfo = TypeInfo { name: "LPCSTR", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const LPWSTR: TypeInfo = TypeInfo { name: "LPWSTR", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const LPCWSTR: TypeInfo = TypeInfo { name: "LPCWSTR", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const SIZE_T: TypeInfo = TypeInfo { name: "SIZE_T", size_32: 4, size_64: 8, is_pointer: false, is_signed: false };
    pub const ULONG_PTR: TypeInfo = TypeInfo { name: "ULONG_PTR", size_32: 4, size_64: 8, is_pointer: false, is_signed: false };
    
    // Windows handle types
    pub const HMODULE: TypeInfo = TypeInfo { name: "HMODULE", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const HWND: TypeInfo = TypeInfo { name: "HWND", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const HINSTANCE: TypeInfo = TypeInfo { name: "HINSTANCE", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const HKEY: TypeInfo = TypeInfo { name: "HKEY", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const HFILE: TypeInfo = TypeInfo { name: "HFILE", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    pub const HGLOBAL: TypeInfo = TypeInfo { name: "HGLOBAL", size_32: 4, size_64: 8, is_pointer: true, is_signed: false };
    
    // NT status and security
    pub const NTSTATUS: TypeInfo = TypeInfo { name: "NTSTATUS", size_32: 4, size_64: 4, is_pointer: false, is_signed: true };
    pub const SECURITY_STATUS: TypeInfo = TypeInfo { name: "SECURITY_STATUS", size_32: 4, size_64: 4, is_pointer: false, is_signed: true };
}

// ============================================================================
// Windows Structure Definitions
// ============================================================================

/// Structure field definition
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: &'static str,
    pub type_name: &'static str,
    pub offset_32: usize,
    pub offset_64: usize,
    pub size_32: usize,
    pub size_64: usize,
}

/// Structure definition
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: &'static str,
    pub size_32: usize,
    pub size_64: usize,
    pub fields: Vec<FieldDef>,
}

/// Windows structures database
pub struct WindowsStructures {
    pub structures: HashMap<String, StructDef>,
}

impl WindowsStructures {
    pub fn new() -> Self {
        let mut db = Self {
            structures: HashMap::new(),
        };
        db.load_common_structures();
        db.load_extended_structures();
        db
    }
    
    fn add(&mut self, s: StructDef) {
        self.structures.insert(s.name.to_string(), s);
    }
    
    fn load_common_structures(&mut self) {
        // UNICODE_STRING
        self.add(StructDef {
            name: "UNICODE_STRING",
            size_32: 8,
            size_64: 16,
            fields: vec![
                FieldDef { name: "Length", type_name: "USHORT", offset_32: 0, offset_64: 0, size_32: 2, size_64: 2 },
                FieldDef { name: "MaximumLength", type_name: "USHORT", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "Buffer", type_name: "PWSTR", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
            ],
        });
        
        // LIST_ENTRY
        self.add(StructDef {
            name: "LIST_ENTRY",
            size_32: 8,
            size_64: 16,
            fields: vec![
                FieldDef { name: "Flink", type_name: "PLIST_ENTRY", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "Blink", type_name: "PLIST_ENTRY", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
            ],
        });
        
        // OVERLAPPED
        self.add(StructDef {
            name: "OVERLAPPED",
            size_32: 20,
            size_64: 32,
            fields: vec![
                FieldDef { name: "Internal", type_name: "ULONG_PTR", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "InternalHigh", type_name: "ULONG_PTR", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "Offset", type_name: "DWORD", offset_32: 8, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "OffsetHigh", type_name: "DWORD", offset_32: 12, offset_64: 20, size_32: 4, size_64: 4 },
                FieldDef { name: "hEvent", type_name: "HANDLE", offset_32: 16, offset_64: 24, size_32: 4, size_64: 8 },
            ],
        });
        
        // SECURITY_ATTRIBUTES
        self.add(StructDef {
            name: "SECURITY_ATTRIBUTES",
            size_32: 12,
            size_64: 24,
            fields: vec![
                FieldDef { name: "nLength", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "lpSecurityDescriptor", type_name: "LPVOID", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "bInheritHandle", type_name: "BOOL", offset_32: 8, offset_64: 16, size_32: 4, size_64: 4 },
            ],
        });
        
        // STARTUPINFOW
        self.add(StructDef {
            name: "STARTUPINFOW",
            size_32: 68,
            size_64: 104,
            fields: vec![
                FieldDef { name: "cb", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "lpReserved", type_name: "LPWSTR", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "lpDesktop", type_name: "LPWSTR", offset_32: 8, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "lpTitle", type_name: "LPWSTR", offset_32: 12, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "dwX", type_name: "DWORD", offset_32: 16, offset_64: 32, size_32: 4, size_64: 4 },
                FieldDef { name: "dwY", type_name: "DWORD", offset_32: 20, offset_64: 36, size_32: 4, size_64: 4 },
                FieldDef { name: "dwXSize", type_name: "DWORD", offset_32: 24, offset_64: 40, size_32: 4, size_64: 4 },
                FieldDef { name: "dwYSize", type_name: "DWORD", offset_32: 28, offset_64: 44, size_32: 4, size_64: 4 },
                FieldDef { name: "dwXCountChars", type_name: "DWORD", offset_32: 32, offset_64: 48, size_32: 4, size_64: 4 },
                FieldDef { name: "dwYCountChars", type_name: "DWORD", offset_32: 36, offset_64: 52, size_32: 4, size_64: 4 },
                FieldDef { name: "dwFillAttribute", type_name: "DWORD", offset_32: 40, offset_64: 56, size_32: 4, size_64: 4 },
                FieldDef { name: "dwFlags", type_name: "DWORD", offset_32: 44, offset_64: 60, size_32: 4, size_64: 4 },
                FieldDef { name: "wShowWindow", type_name: "WORD", offset_32: 48, offset_64: 64, size_32: 2, size_64: 2 },
                FieldDef { name: "cbReserved2", type_name: "WORD", offset_32: 50, offset_64: 66, size_32: 2, size_64: 2 },
                FieldDef { name: "lpReserved2", type_name: "LPBYTE", offset_32: 52, offset_64: 72, size_32: 4, size_64: 8 },
                FieldDef { name: "hStdInput", type_name: "HANDLE", offset_32: 56, offset_64: 80, size_32: 4, size_64: 8 },
                FieldDef { name: "hStdOutput", type_name: "HANDLE", offset_32: 60, offset_64: 88, size_32: 4, size_64: 8 },
                FieldDef { name: "hStdError", type_name: "HANDLE", offset_32: 64, offset_64: 96, size_32: 4, size_64: 8 },
            ],
        });
        
        // PROCESS_INFORMATION
        self.add(StructDef {
            name: "PROCESS_INFORMATION",
            size_32: 16,
            size_64: 24,
            fields: vec![
                FieldDef { name: "hProcess", type_name: "HANDLE", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "hThread", type_name: "HANDLE", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "dwProcessId", type_name: "DWORD", offset_32: 8, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "dwThreadId", type_name: "DWORD", offset_32: 12, offset_64: 20, size_32: 4, size_64: 4 },
            ],
        });
        
        // FILETIME
        self.add(StructDef {
            name: "FILETIME",
            size_32: 8,
            size_64: 8,
            fields: vec![
                FieldDef { name: "dwLowDateTime", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "dwHighDateTime", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
            ],
        });
        
        // GUID
        self.add(StructDef {
            name: "GUID",
            size_32: 16,
            size_64: 16,
            fields: vec![
                FieldDef { name: "Data1", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "Data2", type_name: "WORD", offset_32: 4, offset_64: 4, size_32: 2, size_64: 2 },
                FieldDef { name: "Data3", type_name: "WORD", offset_32: 6, offset_64: 6, size_32: 2, size_64: 2 },
                FieldDef { name: "Data4", type_name: "BYTE[8]", offset_32: 8, offset_64: 8, size_32: 8, size_64: 8 },
            ],
        });
        
        // CRITICAL_SECTION
        self.add(StructDef {
            name: "CRITICAL_SECTION",
            size_32: 24,
            size_64: 40,
            fields: vec![
                FieldDef { name: "DebugInfo", type_name: "PRTL_CRITICAL_SECTION_DEBUG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "LockCount", type_name: "LONG", offset_32: 4, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "RecursionCount", type_name: "LONG", offset_32: 8, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "OwningThread", type_name: "HANDLE", offset_32: 12, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "LockSemaphore", type_name: "HANDLE", offset_32: 16, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "SpinCount", type_name: "ULONG_PTR", offset_32: 20, offset_64: 32, size_32: 4, size_64: 8 },
            ],
        });
        
        // IMAGE_DOS_HEADER
        self.add(StructDef {
            name: "IMAGE_DOS_HEADER",
            size_32: 64,
            size_64: 64,
            fields: vec![
                FieldDef { name: "e_magic", type_name: "WORD", offset_32: 0, offset_64: 0, size_32: 2, size_64: 2 },
                FieldDef { name: "e_cblp", type_name: "WORD", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "e_cp", type_name: "WORD", offset_32: 4, offset_64: 4, size_32: 2, size_64: 2 },
                FieldDef { name: "e_lfanew", type_name: "LONG", offset_32: 60, offset_64: 60, size_32: 4, size_64: 4 },
            ],
        });
        
        // PEB (Process Environment Block) - simplified
        self.add(StructDef {
            name: "PEB",
            size_32: 0x480,
            size_64: 0x7C8,
            fields: vec![
                FieldDef { name: "InheritedAddressSpace", type_name: "BOOLEAN", offset_32: 0, offset_64: 0, size_32: 1, size_64: 1 },
                FieldDef { name: "ReadImageFileExecOptions", type_name: "BOOLEAN", offset_32: 1, offset_64: 1, size_32: 1, size_64: 1 },
                FieldDef { name: "BeingDebugged", type_name: "BOOLEAN", offset_32: 2, offset_64: 2, size_32: 1, size_64: 1 },
                FieldDef { name: "ImageBaseAddress", type_name: "PVOID", offset_32: 8, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "Ldr", type_name: "PPEB_LDR_DATA", offset_32: 12, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "ProcessParameters", type_name: "PRTL_USER_PROCESS_PARAMETERS", offset_32: 16, offset_64: 32, size_32: 4, size_64: 8 },
            ],
        });
        
        // TEB (Thread Environment Block) - simplified
        self.add(StructDef {
            name: "TEB",
            size_32: 0x1000,
            size_64: 0x1838,
            fields: vec![
                FieldDef { name: "NtTib", type_name: "NT_TIB", offset_32: 0, offset_64: 0, size_32: 28, size_64: 56 },
                FieldDef { name: "EnvironmentPointer", type_name: "PVOID", offset_32: 28, offset_64: 56, size_32: 4, size_64: 8 },
                FieldDef { name: "ProcessEnvironmentBlock", type_name: "PPEB", offset_32: 48, offset_64: 96, size_32: 4, size_64: 8 },
            ],
        });
    }
    
    /// Load extended structures (GUI, Memory, Loader, Network)
    fn load_extended_structures(&mut self) {
        
        // ====================================================================
        // GUI & Windowing Structures
        // ====================================================================

        // POINT
        self.add(StructDef {
            name: "POINT",
            size_32: 8,
            size_64: 8,
            fields: vec![
                FieldDef { name: "x", type_name: "LONG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "y", type_name: "LONG", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
            ],
        });

        // RECT
        self.add(StructDef {
            name: "RECT",
            size_32: 16,
            size_64: 16,
            fields: vec![
                FieldDef { name: "left", type_name: "LONG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "top", type_name: "LONG", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "right", type_name: "LONG", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "bottom", type_name: "LONG", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
            ],
        });

        // MSG (Window Message)
        self.add(StructDef {
            name: "MSG",
            size_32: 28,
            size_64: 48,
            fields: vec![
                FieldDef { name: "hwnd", type_name: "HWND", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "message", type_name: "UINT", offset_32: 4, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "wParam", type_name: "WPARAM", offset_32: 8, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "lParam", type_name: "LPARAM", offset_32: 12, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "time", type_name: "DWORD", offset_32: 16, offset_64: 32, size_32: 4, size_64: 4 },
                FieldDef { name: "pt", type_name: "POINT", offset_32: 20, offset_64: 36, size_32: 8, size_64: 8 },
            ],
        });

        // ====================================================================
        // System Information & Memory
        // ====================================================================

        // MEMORY_BASIC_INFORMATION
        self.add(StructDef {
            name: "MEMORY_BASIC_INFORMATION",
            size_32: 28,
            size_64: 48,
            fields: vec![
                FieldDef { name: "BaseAddress", type_name: "PVOID", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "AllocationBase", type_name: "PVOID", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "AllocationProtect", type_name: "DWORD", offset_32: 8, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "RegionSize", type_name: "SIZE_T", offset_32: 12, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "State", type_name: "DWORD", offset_32: 16, offset_64: 32, size_32: 4, size_64: 4 },
                FieldDef { name: "Protect", type_name: "DWORD", offset_32: 20, offset_64: 36, size_32: 4, size_64: 4 },
                FieldDef { name: "Type", type_name: "DWORD", offset_32: 24, offset_64: 40, size_32: 4, size_64: 4 },
            ],
        });

        // SYSTEM_INFO
        self.add(StructDef {
            name: "SYSTEM_INFO",
            size_32: 36,
            size_64: 48,
            fields: vec![
                FieldDef { name: "wProcessorArchitecture", type_name: "WORD", offset_32: 0, offset_64: 0, size_32: 2, size_64: 2 },
                FieldDef { name: "wReserved", type_name: "WORD", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "dwPageSize", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "lpMinimumApplicationAddress", type_name: "LPVOID", offset_32: 8, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "lpMaximumApplicationAddress", type_name: "LPVOID", offset_32: 12, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "dwActiveProcessorMask", type_name: "DWORD_PTR", offset_32: 16, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "dwNumberOfProcessors", type_name: "DWORD", offset_32: 20, offset_64: 32, size_32: 4, size_64: 4 },
                FieldDef { name: "dwProcessorType", type_name: "DWORD", offset_32: 24, offset_64: 36, size_32: 4, size_64: 4 },
                FieldDef { name: "dwAllocationGranularity", type_name: "DWORD", offset_32: 28, offset_64: 40, size_32: 4, size_64: 4 },
                FieldDef { name: "wProcessorLevel", type_name: "WORD", offset_32: 32, offset_64: 44, size_32: 2, size_64: 2 },
                FieldDef { name: "wProcessorRevision", type_name: "WORD", offset_32: 34, offset_64: 46, size_32: 2, size_64: 2 },
            ],
        });

        // ====================================================================
        // File System
        // ====================================================================

        // WIN32_FIND_DATAW
        self.add(StructDef {
            name: "WIN32_FIND_DATAW",
            size_32: 592,
            size_64: 592,
            fields: vec![
                FieldDef { name: "dwFileAttributes", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "ftCreationTime", type_name: "FILETIME", offset_32: 4, offset_64: 4, size_32: 8, size_64: 8 },
                FieldDef { name: "ftLastAccessTime", type_name: "FILETIME", offset_32: 12, offset_64: 12, size_32: 8, size_64: 8 },
                FieldDef { name: "ftLastWriteTime", type_name: "FILETIME", offset_32: 20, offset_64: 20, size_32: 8, size_64: 8 },
                FieldDef { name: "nFileSizeHigh", type_name: "DWORD", offset_32: 28, offset_64: 28, size_32: 4, size_64: 4 },
                FieldDef { name: "nFileSizeLow", type_name: "DWORD", offset_32: 32, offset_64: 32, size_32: 4, size_64: 4 },
                FieldDef { name: "dwReserved0", type_name: "DWORD", offset_32: 36, offset_64: 36, size_32: 4, size_64: 4 },
                FieldDef { name: "dwReserved1", type_name: "DWORD", offset_32: 40, offset_64: 40, size_32: 4, size_64: 4 },
                FieldDef { name: "cFileName", type_name: "WCHAR[260]", offset_32: 44, offset_64: 44, size_32: 520, size_64: 520 },
                FieldDef { name: "cAlternateFileName", type_name: "WCHAR[14]", offset_32: 564, offset_64: 564, size_32: 28, size_64: 28 },
            ],
        });

        // ====================================================================
        // NT Loader Internals (Important for Malware Analysis)
        // ====================================================================

        // PEB_LDR_DATA
        self.add(StructDef {
            name: "PEB_LDR_DATA",
            size_32: 48,
            size_64: 88,
            fields: vec![
                FieldDef { name: "Length", type_name: "ULONG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "Initialized", type_name: "BOOLEAN", offset_32: 4, offset_64: 4, size_32: 1, size_64: 1 },
                FieldDef { name: "SsHandle", type_name: "HANDLE", offset_32: 8, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "InLoadOrderModuleList", type_name: "LIST_ENTRY", offset_32: 12, offset_64: 16, size_32: 8, size_64: 16 },
                FieldDef { name: "InMemoryOrderModuleList", type_name: "LIST_ENTRY", offset_32: 20, offset_64: 32, size_32: 8, size_64: 16 },
                FieldDef { name: "InInitializationOrderModuleList", type_name: "LIST_ENTRY", offset_32: 28, offset_64: 48, size_32: 8, size_64: 16 },
                FieldDef { name: "EntryInProgress", type_name: "PVOID", offset_32: 36, offset_64: 64, size_32: 4, size_64: 8 },
                FieldDef { name: "ShutdownInProgress", type_name: "BOOLEAN", offset_32: 40, offset_64: 72, size_32: 1, size_64: 1 },
                FieldDef { name: "ShutdownThreadId", type_name: "HANDLE", offset_32: 44, offset_64: 80, size_32: 4, size_64: 8 },
            ],
        });

        // LDR_DATA_TABLE_ENTRY
        self.add(StructDef {
            name: "LDR_DATA_TABLE_ENTRY",
            size_32: 80,
            size_64: 144,
            fields: vec![
                FieldDef { name: "InLoadOrderLinks", type_name: "LIST_ENTRY", offset_32: 0, offset_64: 0, size_32: 8, size_64: 16 },
                FieldDef { name: "InMemoryOrderLinks", type_name: "LIST_ENTRY", offset_32: 8, offset_64: 16, size_32: 8, size_64: 16 },
                FieldDef { name: "InInitializationOrderLinks", type_name: "LIST_ENTRY", offset_32: 16, offset_64: 32, size_32: 8, size_64: 16 },
                FieldDef { name: "DllBase", type_name: "PVOID", offset_32: 24, offset_64: 48, size_32: 4, size_64: 8 },
                FieldDef { name: "EntryPoint", type_name: "PVOID", offset_32: 28, offset_64: 56, size_32: 4, size_64: 8 },
                FieldDef { name: "SizeOfImage", type_name: "ULONG", offset_32: 32, offset_64: 64, size_32: 4, size_64: 4 },
                FieldDef { name: "FullDllName", type_name: "UNICODE_STRING", offset_32: 36, offset_64: 72, size_32: 8, size_64: 16 },
                FieldDef { name: "BaseDllName", type_name: "UNICODE_STRING", offset_32: 44, offset_64: 88, size_32: 8, size_64: 16 },
            ],
        });

        // ====================================================================
        // PE Headers (Executable File Structure)
        // ====================================================================

        // IMAGE_FILE_HEADER
        self.add(StructDef {
            name: "IMAGE_FILE_HEADER",
            size_32: 20,
            size_64: 20,
            fields: vec![
                FieldDef { name: "Machine", type_name: "WORD", offset_32: 0, offset_64: 0, size_32: 2, size_64: 2 },
                FieldDef { name: "NumberOfSections", type_name: "WORD", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "TimeDateStamp", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "PointerToSymbolTable", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "NumberOfSymbols", type_name: "DWORD", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "SizeOfOptionalHeader", type_name: "WORD", offset_32: 16, offset_64: 16, size_32: 2, size_64: 2 },
                FieldDef { name: "Characteristics", type_name: "WORD", offset_32: 18, offset_64: 18, size_32: 2, size_64: 2 },
            ],
        });

        // IMAGE_DATA_DIRECTORY
        self.add(StructDef {
            name: "IMAGE_DATA_DIRECTORY",
            size_32: 8,
            size_64: 8,
            fields: vec![
                FieldDef { name: "VirtualAddress", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "Size", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
            ],
        });

        // IMAGE_SECTION_HEADER
        self.add(StructDef {
            name: "IMAGE_SECTION_HEADER",
            size_32: 40,
            size_64: 40,
            fields: vec![
                FieldDef { name: "Name", type_name: "BYTE[8]", offset_32: 0, offset_64: 0, size_32: 8, size_64: 8 },
                FieldDef { name: "VirtualSize", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "VirtualAddress", type_name: "DWORD", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "SizeOfRawData", type_name: "DWORD", offset_32: 16, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "PointerToRawData", type_name: "DWORD", offset_32: 20, offset_64: 20, size_32: 4, size_64: 4 },
                FieldDef { name: "PointerToRelocations", type_name: "DWORD", offset_32: 24, offset_64: 24, size_32: 4, size_64: 4 },
                FieldDef { name: "PointerToLinenumbers", type_name: "DWORD", offset_32: 28, offset_64: 28, size_32: 4, size_64: 4 },
                FieldDef { name: "NumberOfRelocations", type_name: "WORD", offset_32: 32, offset_64: 32, size_32: 2, size_64: 2 },
                FieldDef { name: "NumberOfLinenumbers", type_name: "WORD", offset_32: 34, offset_64: 34, size_32: 2, size_64: 2 },
                FieldDef { name: "Characteristics", type_name: "DWORD", offset_32: 36, offset_64: 36, size_32: 4, size_64: 4 },
            ],
        });

        // IMAGE_NT_HEADERS
        self.add(StructDef {
            name: "IMAGE_NT_HEADERS",
            size_32: 248,
            size_64: 264,
            fields: vec![
                FieldDef { name: "Signature", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "FileHeader", type_name: "IMAGE_FILE_HEADER", offset_32: 4, offset_64: 4, size_32: 20, size_64: 20 },
                FieldDef { name: "OptionalHeader_Magic", type_name: "WORD", offset_32: 24, offset_64: 24, size_32: 2, size_64: 2 },
                FieldDef { name: "AddressOfEntryPoint", type_name: "DWORD", offset_32: 40, offset_64: 40, size_32: 4, size_64: 4 },
                FieldDef { name: "ImageBase", type_name: "ULONGLONG", offset_32: 52, offset_64: 48, size_32: 4, size_64: 8 },
            ],
        });

        // IMAGE_IMPORT_DESCRIPTOR
        self.add(StructDef {
            name: "IMAGE_IMPORT_DESCRIPTOR",
            size_32: 20,
            size_64: 20,
            fields: vec![
                FieldDef { name: "OriginalFirstThunk", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "TimeDateStamp", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "ForwarderChain", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "Name", type_name: "DWORD", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "FirstThunk", type_name: "DWORD", offset_32: 16, offset_64: 16, size_32: 4, size_64: 4 },
            ],
        });

        // ====================================================================
        // Networking (Winsock)
        // ====================================================================

        // SOCKADDR_IN (IPv4)
        self.add(StructDef {
            name: "SOCKADDR_IN",
            size_32: 16,
            size_64: 16,
            fields: vec![
                FieldDef { name: "sin_family", type_name: "SHORT", offset_32: 0, offset_64: 0, size_32: 2, size_64: 2 },
                FieldDef { name: "sin_port", type_name: "USHORT", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "sin_addr", type_name: "ULONG", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "sin_zero", type_name: "CHAR[8]", offset_32: 8, offset_64: 8, size_32: 8, size_64: 8 },
            ],
        });

        // WSADATA (Winsock Init)
        self.add(StructDef {
            name: "WSADATA",
            size_32: 400,
            size_64: 408,
            fields: vec![
                FieldDef { name: "wVersion", type_name: "WORD", offset_32: 0, offset_64: 0, size_32: 2, size_64: 2 },
                FieldDef { name: "wHighVersion", type_name: "WORD", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "szDescription", type_name: "CHAR[257]", offset_32: 4, offset_64: 4, size_32: 257, size_64: 257 },
                FieldDef { name: "szSystemStatus", type_name: "CHAR[129]", offset_32: 261, offset_64: 261, size_32: 129, size_64: 129 },
            ],
        });

        // HOSTENT (DNS resolution)
        self.add(StructDef {
            name: "HOSTENT",
            size_32: 16,
            size_64: 32,
            fields: vec![
                FieldDef { name: "h_name", type_name: "PCHAR", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "h_aliases", type_name: "PCHAR*", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "h_addrtype", type_name: "SHORT", offset_32: 8, offset_64: 16, size_32: 2, size_64: 2 },
                FieldDef { name: "h_length", type_name: "SHORT", offset_32: 10, offset_64: 18, size_32: 2, size_64: 2 },
                FieldDef { name: "h_addr_list", type_name: "PCHAR*", offset_32: 12, offset_64: 24, size_32: 4, size_64: 8 },
            ],
        });

        // ====================================================================
        // Security & Privileges (Token Manipulation)
        // ====================================================================

        // LUID (Locally Unique Identifier)
        self.add(StructDef {
            name: "LUID",
            size_32: 8,
            size_64: 8,
            fields: vec![
                FieldDef { name: "LowPart", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "HighPart", type_name: "LONG", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
            ],
        });

        // LUID_AND_ATTRIBUTES
        self.add(StructDef {
            name: "LUID_AND_ATTRIBUTES",
            size_32: 12,
            size_64: 12,
            fields: vec![
                FieldDef { name: "Luid", type_name: "LUID", offset_32: 0, offset_64: 0, size_32: 8, size_64: 8 },
                FieldDef { name: "Attributes", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
            ],
        });

        // TOKEN_PRIVILEGES
        self.add(StructDef {
            name: "TOKEN_PRIVILEGES",
            size_32: 16,
            size_64: 16,
            fields: vec![
                FieldDef { name: "PrivilegeCount", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "Privileges", type_name: "LUID_AND_ATTRIBUTES[1]", offset_32: 4, offset_64: 4, size_32: 12, size_64: 12 },
            ],
        });

        // SID_IDENTIFIER_AUTHORITY
        self.add(StructDef {
            name: "SID_IDENTIFIER_AUTHORITY",
            size_32: 6,
            size_64: 6,
            fields: vec![
                FieldDef { name: "Value", type_name: "BYTE[6]", offset_32: 0, offset_64: 0, size_32: 6, size_64: 6 },
            ],
        });

        // ====================================================================
        // ToolHelp32 (Process Enumeration)
        // ====================================================================

        // PROCESSENTRY32W
        self.add(StructDef {
            name: "PROCESSENTRY32W",
            size_32: 556,
            size_64: 568,
            fields: vec![
                FieldDef { name: "dwSize", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "cntUsage", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "th32ProcessID", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "th32DefaultHeapID", type_name: "ULONG_PTR", offset_32: 12, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "th32ModuleID", type_name: "DWORD", offset_32: 16, offset_64: 24, size_32: 4, size_64: 4 },
                FieldDef { name: "cntThreads", type_name: "DWORD", offset_32: 20, offset_64: 28, size_32: 4, size_64: 4 },
                FieldDef { name: "th32ParentProcessID", type_name: "DWORD", offset_32: 24, offset_64: 32, size_32: 4, size_64: 4 },
                FieldDef { name: "pcPriClassBase", type_name: "LONG", offset_32: 28, offset_64: 36, size_32: 4, size_64: 4 },
                FieldDef { name: "dwFlags", type_name: "DWORD", offset_32: 32, offset_64: 40, size_32: 4, size_64: 4 },
                FieldDef { name: "szExeFile", type_name: "WCHAR[260]", offset_32: 36, offset_64: 44, size_32: 520, size_64: 520 },
            ],
        });

        // MODULEENTRY32W
        self.add(StructDef {
            name: "MODULEENTRY32W",
            size_32: 1064,
            size_64: 1080,
            fields: vec![
                FieldDef { name: "dwSize", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "th32ModuleID", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "th32ProcessID", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "GlblcntUsage", type_name: "DWORD", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "ProccntUsage", type_name: "DWORD", offset_32: 16, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "modBaseAddr", type_name: "BYTE*", offset_32: 20, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "modBaseSize", type_name: "DWORD", offset_32: 24, offset_64: 32, size_32: 4, size_64: 4 },
                FieldDef { name: "hModule", type_name: "HMODULE", offset_32: 28, offset_64: 40, size_32: 4, size_64: 8 },
                FieldDef { name: "szModule", type_name: "WCHAR[256]", offset_32: 32, offset_64: 48, size_32: 512, size_64: 512 },
                FieldDef { name: "szExePath", type_name: "WCHAR[260]", offset_32: 544, offset_64: 560, size_32: 520, size_64: 520 },
            ],
        });

        // THREADENTRY32
        self.add(StructDef {
            name: "THREADENTRY32",
            size_32: 28,
            size_64: 28,
            fields: vec![
                FieldDef { name: "dwSize", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "cntUsage", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "th32ThreadID", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "th32OwnerProcessID", type_name: "DWORD", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "tpBasePri", type_name: "LONG", offset_32: 16, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "tpDeltaPri", type_name: "LONG", offset_32: 20, offset_64: 20, size_32: 4, size_64: 4 },
                FieldDef { name: "dwFlags", type_name: "DWORD", offset_32: 24, offset_64: 24, size_32: 4, size_64: 4 },
            ],
        });

        // ====================================================================
        // Exception Handling
        // ====================================================================

        // EXCEPTION_RECORD
        self.add(StructDef {
            name: "EXCEPTION_RECORD",
            size_32: 80,
            size_64: 152,
            fields: vec![
                FieldDef { name: "ExceptionCode", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "ExceptionFlags", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "ExceptionRecord", type_name: "PEXCEPTION_RECORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "ExceptionAddress", type_name: "PVOID", offset_32: 12, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "NumberParameters", type_name: "DWORD", offset_32: 16, offset_64: 24, size_32: 4, size_64: 4 },
                FieldDef { name: "ExceptionInformation", type_name: "ULONG_PTR[15]", offset_32: 20, offset_64: 32, size_32: 60, size_64: 120 },
            ],
        });

        // CONTEXT_X86 (32-bit Architecture)
        self.add(StructDef {
            name: "CONTEXT_X86",
            size_32: 716,
            size_64: 0,
            fields: vec![
                FieldDef { name: "ContextFlags", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Dr0", type_name: "DWORD", offset_32: 4, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Dr1", type_name: "DWORD", offset_32: 8, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Dr2", type_name: "DWORD", offset_32: 12, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Dr3", type_name: "DWORD", offset_32: 16, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SegGs", type_name: "DWORD", offset_32: 140, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SegFs", type_name: "DWORD", offset_32: 144, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SegEs", type_name: "DWORD", offset_32: 148, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SegDs", type_name: "DWORD", offset_32: 152, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Edi", type_name: "DWORD", offset_32: 156, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Esi", type_name: "DWORD", offset_32: 160, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Ebx", type_name: "DWORD", offset_32: 164, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Edx", type_name: "DWORD", offset_32: 168, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Ecx", type_name: "DWORD", offset_32: 172, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Eax", type_name: "DWORD", offset_32: 176, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Ebp", type_name: "DWORD", offset_32: 180, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Eip", type_name: "DWORD", offset_32: 184, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SegCs", type_name: "DWORD", offset_32: 188, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "EFlags", type_name: "DWORD", offset_32: 192, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Esp", type_name: "DWORD", offset_32: 196, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SegSs", type_name: "DWORD", offset_32: 200, offset_64: 0, size_32: 4, size_64: 0 },
            ],
        });

        // CONTEXT_X64 (64-bit Architecture)
        self.add(StructDef {
            name: "CONTEXT_X64",
            size_32: 0,
            size_64: 1232,
            fields: vec![
                FieldDef { name: "P1Home", type_name: "DWORD64", offset_32: 0, offset_64: 0, size_32: 0, size_64: 8 },
                FieldDef { name: "P2Home", type_name: "DWORD64", offset_32: 0, offset_64: 8, size_32: 0, size_64: 8 },
                FieldDef { name: "P3Home", type_name: "DWORD64", offset_32: 0, offset_64: 16, size_32: 0, size_64: 8 },
                FieldDef { name: "P4Home", type_name: "DWORD64", offset_32: 0, offset_64: 24, size_32: 0, size_64: 8 },
                FieldDef { name: "P5Home", type_name: "DWORD64", offset_32: 0, offset_64: 32, size_32: 0, size_64: 8 },
                FieldDef { name: "P6Home", type_name: "DWORD64", offset_32: 0, offset_64: 40, size_32: 0, size_64: 8 },
                FieldDef { name: "ContextFlags", type_name: "DWORD", offset_32: 0, offset_64: 48, size_32: 0, size_64: 4 },
                FieldDef { name: "MxCsr", type_name: "DWORD", offset_32: 0, offset_64: 52, size_32: 0, size_64: 4 },
                FieldDef { name: "Rax", type_name: "DWORD64", offset_32: 0, offset_64: 120, size_32: 0, size_64: 8 },
                FieldDef { name: "Rcx", type_name: "DWORD64", offset_32: 0, offset_64: 128, size_32: 0, size_64: 8 },
                FieldDef { name: "Rdx", type_name: "DWORD64", offset_32: 0, offset_64: 136, size_32: 0, size_64: 8 },
                FieldDef { name: "Rbx", type_name: "DWORD64", offset_32: 0, offset_64: 144, size_32: 0, size_64: 8 },
                FieldDef { name: "Rsp", type_name: "DWORD64", offset_32: 0, offset_64: 152, size_32: 0, size_64: 8 },
                FieldDef { name: "Rbp", type_name: "DWORD64", offset_32: 0, offset_64: 160, size_32: 0, size_64: 8 },
                FieldDef { name: "Rsi", type_name: "DWORD64", offset_32: 0, offset_64: 168, size_32: 0, size_64: 8 },
                FieldDef { name: "Rdi", type_name: "DWORD64", offset_32: 0, offset_64: 176, size_32: 0, size_64: 8 },
                FieldDef { name: "R8", type_name: "DWORD64", offset_32: 0, offset_64: 184, size_32: 0, size_64: 8 },
                FieldDef { name: "R9", type_name: "DWORD64", offset_32: 0, offset_64: 192, size_32: 0, size_64: 8 },
                FieldDef { name: "R10", type_name: "DWORD64", offset_32: 0, offset_64: 200, size_32: 0, size_64: 8 },
                FieldDef { name: "R11", type_name: "DWORD64", offset_32: 0, offset_64: 208, size_32: 0, size_64: 8 },
                FieldDef { name: "R12", type_name: "DWORD64", offset_32: 0, offset_64: 216, size_32: 0, size_64: 8 },
                FieldDef { name: "R13", type_name: "DWORD64", offset_32: 0, offset_64: 224, size_32: 0, size_64: 8 },
                FieldDef { name: "R14", type_name: "DWORD64", offset_32: 0, offset_64: 232, size_32: 0, size_64: 8 },
                FieldDef { name: "R15", type_name: "DWORD64", offset_32: 0, offset_64: 240, size_32: 0, size_64: 8 },
                FieldDef { name: "Rip", type_name: "DWORD64", offset_32: 0, offset_64: 248, size_32: 0, size_64: 8 },
            ],
        });

        // ====================================================================
        // PE Optional Headers (32/64 split)
        // ====================================================================

        // IMAGE_OPTIONAL_HEADER32
        self.add(StructDef {
            name: "IMAGE_OPTIONAL_HEADER32",
            size_32: 224,
            size_64: 0,
            fields: vec![
                FieldDef { name: "Magic", type_name: "WORD", offset_32: 0, offset_64: 0, size_32: 2, size_64: 0 },
                FieldDef { name: "MajorLinkerVersion", type_name: "BYTE", offset_32: 2, offset_64: 0, size_32: 1, size_64: 0 },
                FieldDef { name: "MinorLinkerVersion", type_name: "BYTE", offset_32: 3, offset_64: 0, size_32: 1, size_64: 0 },
                FieldDef { name: "SizeOfCode", type_name: "DWORD", offset_32: 4, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "AddressOfEntryPoint", type_name: "DWORD", offset_32: 16, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "BaseOfCode", type_name: "DWORD", offset_32: 20, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "BaseOfData", type_name: "DWORD", offset_32: 24, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "ImageBase", type_name: "DWORD", offset_32: 28, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SectionAlignment", type_name: "DWORD", offset_32: 32, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "FileAlignment", type_name: "DWORD", offset_32: 36, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SizeOfImage", type_name: "DWORD", offset_32: 56, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "SizeOfHeaders", type_name: "DWORD", offset_32: 60, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "Subsystem", type_name: "WORD", offset_32: 68, offset_64: 0, size_32: 2, size_64: 0 },
                FieldDef { name: "NumberOfRvaAndSizes", type_name: "DWORD", offset_32: 92, offset_64: 0, size_32: 4, size_64: 0 },
                FieldDef { name: "DataDirectory", type_name: "IMAGE_DATA_DIRECTORY[16]", offset_32: 96, offset_64: 0, size_32: 128, size_64: 0 },
            ],
        });

        // IMAGE_OPTIONAL_HEADER64
        self.add(StructDef {
            name: "IMAGE_OPTIONAL_HEADER64",
            size_32: 0,
            size_64: 240,
            fields: vec![
                FieldDef { name: "Magic", type_name: "WORD", offset_32: 0, offset_64: 0, size_32: 0, size_64: 2 },
                FieldDef { name: "MajorLinkerVersion", type_name: "BYTE", offset_32: 0, offset_64: 2, size_32: 0, size_64: 1 },
                FieldDef { name: "MinorLinkerVersion", type_name: "BYTE", offset_32: 0, offset_64: 3, size_32: 0, size_64: 1 },
                FieldDef { name: "SizeOfCode", type_name: "DWORD", offset_32: 0, offset_64: 4, size_32: 0, size_64: 4 },
                FieldDef { name: "AddressOfEntryPoint", type_name: "DWORD", offset_32: 0, offset_64: 16, size_32: 0, size_64: 4 },
                FieldDef { name: "BaseOfCode", type_name: "DWORD", offset_32: 0, offset_64: 20, size_32: 0, size_64: 4 },
                FieldDef { name: "ImageBase", type_name: "ULONGLONG", offset_32: 0, offset_64: 24, size_32: 0, size_64: 8 },
                FieldDef { name: "SectionAlignment", type_name: "DWORD", offset_32: 0, offset_64: 32, size_32: 0, size_64: 4 },
                FieldDef { name: "FileAlignment", type_name: "DWORD", offset_32: 0, offset_64: 36, size_32: 0, size_64: 4 },
                FieldDef { name: "SizeOfImage", type_name: "DWORD", offset_32: 0, offset_64: 56, size_32: 0, size_64: 4 },
                FieldDef { name: "SizeOfHeaders", type_name: "DWORD", offset_32: 0, offset_64: 60, size_32: 0, size_64: 4 },
                FieldDef { name: "Subsystem", type_name: "WORD", offset_32: 0, offset_64: 68, size_32: 0, size_64: 2 },
                FieldDef { name: "NumberOfRvaAndSizes", type_name: "DWORD", offset_32: 0, offset_64: 108, size_32: 0, size_64: 4 },
                FieldDef { name: "DataDirectory", type_name: "IMAGE_DATA_DIRECTORY[16]", offset_32: 0, offset_64: 112, size_32: 0, size_64: 128 },
            ],
        });

        // ====================================================================
        // Windows Registry
        // ====================================================================

        // KEY_VALUE_PARTIAL_INFORMATION
        self.add(StructDef {
            name: "KEY_VALUE_PARTIAL_INFORMATION",
            size_32: 16,
            size_64: 16,
            fields: vec![
                FieldDef { name: "TitleIndex", type_name: "ULONG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "Type", type_name: "ULONG", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "DataLength", type_name: "ULONG", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "Data", type_name: "UCHAR[1]", offset_32: 12, offset_64: 12, size_32: 1, size_64: 1 },
            ],
        });

        // KEY_VALUE_FULL_INFORMATION
        self.add(StructDef {
            name: "KEY_VALUE_FULL_INFORMATION",
            size_32: 20,
            size_64: 20,
            fields: vec![
                FieldDef { name: "TitleIndex", type_name: "ULONG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "Type", type_name: "ULONG", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "DataOffset", type_name: "ULONG", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "DataLength", type_name: "ULONG", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "NameLength", type_name: "ULONG", offset_32: 16, offset_64: 16, size_32: 4, size_64: 4 },
            ],
        });

        // ====================================================================
        // PE Export/Import (Binary Analysis)
        // ====================================================================

        // IMAGE_EXPORT_DIRECTORY
        self.add(StructDef {
            name: "IMAGE_EXPORT_DIRECTORY",
            size_32: 40,
            size_64: 40,
            fields: vec![
                FieldDef { name: "Characteristics", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "TimeDateStamp", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "MajorVersion", type_name: "WORD", offset_32: 8, offset_64: 8, size_32: 2, size_64: 2 },
                FieldDef { name: "MinorVersion", type_name: "WORD", offset_32: 10, offset_64: 10, size_32: 2, size_64: 2 },
                FieldDef { name: "Name", type_name: "DWORD", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "Base", type_name: "DWORD", offset_32: 16, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "NumberOfFunctions", type_name: "DWORD", offset_32: 20, offset_64: 20, size_32: 4, size_64: 4 },
                FieldDef { name: "NumberOfNames", type_name: "DWORD", offset_32: 24, offset_64: 24, size_32: 4, size_64: 4 },
                FieldDef { name: "AddressOfFunctions", type_name: "DWORD", offset_32: 28, offset_64: 28, size_32: 4, size_64: 4 },
                FieldDef { name: "AddressOfNames", type_name: "DWORD", offset_32: 32, offset_64: 32, size_32: 4, size_64: 4 },
                FieldDef { name: "AddressOfNameOrdinals", type_name: "DWORD", offset_32: 36, offset_64: 36, size_32: 4, size_64: 4 },
            ],
        });

        // IMAGE_THUNK_DATA32
        self.add(StructDef {
            name: "IMAGE_THUNK_DATA32",
            size_32: 4,
            size_64: 0,
            fields: vec![
                FieldDef { name: "u1", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 0 },
            ],
        });

        // IMAGE_THUNK_DATA64
        self.add(StructDef {
            name: "IMAGE_THUNK_DATA64",
            size_32: 0,
            size_64: 8,
            fields: vec![
                FieldDef { name: "u1", type_name: "ULONGLONG", offset_32: 0, offset_64: 0, size_32: 0, size_64: 8 },
            ],
        });

        // IMAGE_IMPORT_BY_NAME
        self.add(StructDef {
            name: "IMAGE_IMPORT_BY_NAME",
            size_32: 4,
            size_64: 4,
            fields: vec![
                FieldDef { name: "Hint", type_name: "WORD", offset_32: 0, offset_64: 0, size_32: 2, size_64: 2 },
                FieldDef { name: "Name", type_name: "CHAR[1]", offset_32: 2, offset_64: 2, size_32: 1, size_64: 1 },
            ],
        });

        // IMAGE_BASE_RELOCATION
        self.add(StructDef {
            name: "IMAGE_BASE_RELOCATION",
            size_32: 8,
            size_64: 8,
            fields: vec![
                FieldDef { name: "VirtualAddress", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "SizeOfBlock", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
            ],
        });

        // IMAGE_RESOURCE_DIRECTORY
        self.add(StructDef {
            name: "IMAGE_RESOURCE_DIRECTORY",
            size_32: 16,
            size_64: 16,
            fields: vec![
                FieldDef { name: "Characteristics", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "TimeDateStamp", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "MajorVersion", type_name: "WORD", offset_32: 8, offset_64: 8, size_32: 2, size_64: 2 },
                FieldDef { name: "MinorVersion", type_name: "WORD", offset_32: 10, offset_64: 10, size_32: 2, size_64: 2 },
                FieldDef { name: "NumberOfNamedEntries", type_name: "WORD", offset_32: 12, offset_64: 12, size_32: 2, size_64: 2 },
                FieldDef { name: "NumberOfIdEntries", type_name: "WORD", offset_32: 14, offset_64: 14, size_32: 2, size_64: 2 },
            ],
        });

        // ====================================================================
        // NT Internals (Anti-debug/Injection Analysis)
        // ====================================================================

        // CLIENT_ID
        self.add(StructDef {
            name: "CLIENT_ID",
            size_32: 8,
            size_64: 16,
            fields: vec![
                FieldDef { name: "UniqueProcess", type_name: "HANDLE", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "UniqueThread", type_name: "HANDLE", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
            ],
        });

        // OBJECT_ATTRIBUTES
        self.add(StructDef {
            name: "OBJECT_ATTRIBUTES",
            size_32: 24,
            size_64: 48,
            fields: vec![
                FieldDef { name: "Length", type_name: "ULONG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "RootDirectory", type_name: "HANDLE", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "ObjectName", type_name: "PUNICODE_STRING", offset_32: 8, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "Attributes", type_name: "ULONG", offset_32: 12, offset_64: 24, size_32: 4, size_64: 4 },
                FieldDef { name: "SecurityDescriptor", type_name: "PVOID", offset_32: 16, offset_64: 32, size_32: 4, size_64: 8 },
                FieldDef { name: "SecurityQualityOfService", type_name: "PVOID", offset_32: 20, offset_64: 40, size_32: 4, size_64: 8 },
            ],
        });

        // IO_STATUS_BLOCK
        self.add(StructDef {
            name: "IO_STATUS_BLOCK",
            size_32: 8,
            size_64: 16,
            fields: vec![
                FieldDef { name: "Status", type_name: "NTSTATUS", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "Information", type_name: "ULONG_PTR", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
            ],
        });

        // RTL_USER_PROCESS_PARAMETERS
        self.add(StructDef {
            name: "RTL_USER_PROCESS_PARAMETERS",
            size_32: 0x290,
            size_64: 0x410,
            fields: vec![
                FieldDef { name: "MaximumLength", type_name: "ULONG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "Length", type_name: "ULONG", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "Flags", type_name: "ULONG", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "DebugFlags", type_name: "ULONG", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "ConsoleHandle", type_name: "HANDLE", offset_32: 16, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "ImagePathName", type_name: "UNICODE_STRING", offset_32: 56, offset_64: 96, size_32: 8, size_64: 16 },
                FieldDef { name: "CommandLine", type_name: "UNICODE_STRING", offset_32: 64, offset_64: 112, size_32: 8, size_64: 16 },
                FieldDef { name: "Environment", type_name: "PVOID", offset_32: 72, offset_64: 128, size_32: 4, size_64: 8 },
            ],
        });

        // ====================================================================
        // Debug/Injection
        // ====================================================================

        // DEBUG_EVENT
        self.add(StructDef {
            name: "DEBUG_EVENT",
            size_32: 96,
            size_64: 176,
            fields: vec![
                FieldDef { name: "dwDebugEventCode", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "dwProcessId", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "dwThreadId", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "u", type_name: "UNION", offset_32: 12, offset_64: 16, size_32: 84, size_64: 160 },
            ],
        });

        // EXCEPTION_POINTERS
        self.add(StructDef {
            name: "EXCEPTION_POINTERS",
            size_32: 8,
            size_64: 16,
            fields: vec![
                FieldDef { name: "ExceptionRecord", type_name: "PEXCEPTION_RECORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "ContextRecord", type_name: "PCONTEXT", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
            ],
        });

        // ====================================================================
        // Security (Privilege Escalation Analysis)
        // ====================================================================

        // ACL
        self.add(StructDef {
            name: "ACL",
            size_32: 8,
            size_64: 8,
            fields: vec![
                FieldDef { name: "AclRevision", type_name: "BYTE", offset_32: 0, offset_64: 0, size_32: 1, size_64: 1 },
                FieldDef { name: "Sbz1", type_name: "BYTE", offset_32: 1, offset_64: 1, size_32: 1, size_64: 1 },
                FieldDef { name: "AclSize", type_name: "WORD", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "AceCount", type_name: "WORD", offset_32: 4, offset_64: 4, size_32: 2, size_64: 2 },
                FieldDef { name: "Sbz2", type_name: "WORD", offset_32: 6, offset_64: 6, size_32: 2, size_64: 2 },
            ],
        });

        // ACE_HEADER
        self.add(StructDef {
            name: "ACE_HEADER",
            size_32: 4,
            size_64: 4,
            fields: vec![
                FieldDef { name: "AceType", type_name: "BYTE", offset_32: 0, offset_64: 0, size_32: 1, size_64: 1 },
                FieldDef { name: "AceFlags", type_name: "BYTE", offset_32: 1, offset_64: 1, size_32: 1, size_64: 1 },
                FieldDef { name: "AceSize", type_name: "WORD", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
            ],
        });

        // SECURITY_DESCRIPTOR
        self.add(StructDef {
            name: "SECURITY_DESCRIPTOR",
            size_32: 20,
            size_64: 40,
            fields: vec![
                FieldDef { name: "Revision", type_name: "BYTE", offset_32: 0, offset_64: 0, size_32: 1, size_64: 1 },
                FieldDef { name: "Sbz1", type_name: "BYTE", offset_32: 1, offset_64: 1, size_32: 1, size_64: 1 },
                FieldDef { name: "Control", type_name: "WORD", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "Owner", type_name: "PSID", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
                FieldDef { name: "Group", type_name: "PSID", offset_32: 8, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "Sacl", type_name: "PACL", offset_32: 12, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "Dacl", type_name: "PACL", offset_32: 16, offset_64: 32, size_32: 4, size_64: 8 },
            ],
        });

        // SID
        self.add(StructDef {
            name: "SID",
            size_32: 12,
            size_64: 12,
            fields: vec![
                FieldDef { name: "Revision", type_name: "BYTE", offset_32: 0, offset_64: 0, size_32: 1, size_64: 1 },
                FieldDef { name: "SubAuthorityCount", type_name: "BYTE", offset_32: 1, offset_64: 1, size_32: 1, size_64: 1 },
                FieldDef { name: "IdentifierAuthority", type_name: "SID_IDENTIFIER_AUTHORITY", offset_32: 2, offset_64: 2, size_32: 6, size_64: 6 },
                FieldDef { name: "SubAuthority", type_name: "DWORD[1]", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
            ],
        });

        // TOKEN_USER
        self.add(StructDef {
            name: "TOKEN_USER",
            size_32: 8,
            size_64: 16,
            fields: vec![
                FieldDef { name: "User", type_name: "SID_AND_ATTRIBUTES", offset_32: 0, offset_64: 0, size_32: 8, size_64: 16 },
            ],
        });

        // SID_AND_ATTRIBUTES
        self.add(StructDef {
            name: "SID_AND_ATTRIBUTES",
            size_32: 8,
            size_64: 16,
            fields: vec![
                FieldDef { name: "Sid", type_name: "PSID", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "Attributes", type_name: "DWORD", offset_32: 4, offset_64: 8, size_32: 4, size_64: 4 },
            ],
        });

        // ====================================================================
        // Networking (C2 Analysis)
        // ====================================================================

        // SOCKADDR_IN6
        self.add(StructDef {
            name: "SOCKADDR_IN6",
            size_32: 28,
            size_64: 28,
            fields: vec![
                FieldDef { name: "sin6_family", type_name: "SHORT", offset_32: 0, offset_64: 0, size_32: 2, size_64: 2 },
                FieldDef { name: "sin6_port", type_name: "USHORT", offset_32: 2, offset_64: 2, size_32: 2, size_64: 2 },
                FieldDef { name: "sin6_flowinfo", type_name: "ULONG", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "sin6_addr", type_name: "IN6_ADDR", offset_32: 8, offset_64: 8, size_32: 16, size_64: 16 },
                FieldDef { name: "sin6_scope_id", type_name: "ULONG", offset_32: 24, offset_64: 24, size_32: 4, size_64: 4 },
            ],
        });

        // IN_ADDR
        self.add(StructDef {
            name: "IN_ADDR",
            size_32: 4,
            size_64: 4,
            fields: vec![
                FieldDef { name: "S_addr", type_name: "ULONG", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
            ],
        });

        // IN6_ADDR
        self.add(StructDef {
            name: "IN6_ADDR",
            size_32: 16,
            size_64: 16,
            fields: vec![
                FieldDef { name: "Byte", type_name: "UCHAR[16]", offset_32: 0, offset_64: 0, size_32: 16, size_64: 16 },
            ],
        });

        // ADDRINFO
        self.add(StructDef {
            name: "ADDRINFO",
            size_32: 32,
            size_64: 48,
            fields: vec![
                FieldDef { name: "ai_flags", type_name: "INT", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "ai_family", type_name: "INT", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "ai_socktype", type_name: "INT", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "ai_protocol", type_name: "INT", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "ai_addrlen", type_name: "SIZE_T", offset_32: 16, offset_64: 16, size_32: 4, size_64: 8 },
                FieldDef { name: "ai_canonname", type_name: "PCHAR", offset_32: 20, offset_64: 24, size_32: 4, size_64: 8 },
                FieldDef { name: "ai_addr", type_name: "PSOCKADDR", offset_32: 24, offset_64: 32, size_32: 4, size_64: 8 },
                FieldDef { name: "ai_next", type_name: "PADDRINFO", offset_32: 28, offset_64: 40, size_32: 4, size_64: 8 },
            ],
        });

        // ====================================================================
        // Service (Persistence Analysis)
        // ====================================================================

        // SERVICE_STATUS
        self.add(StructDef {
            name: "SERVICE_STATUS",
            size_32: 28,
            size_64: 28,
            fields: vec![
                FieldDef { name: "dwServiceType", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "dwCurrentState", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "dwControlsAccepted", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "dwWin32ExitCode", type_name: "DWORD", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "dwServiceSpecificExitCode", type_name: "DWORD", offset_32: 16, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "dwCheckPoint", type_name: "DWORD", offset_32: 20, offset_64: 20, size_32: 4, size_64: 4 },
                FieldDef { name: "dwWaitHint", type_name: "DWORD", offset_32: 24, offset_64: 24, size_32: 4, size_64: 4 },
            ],
        });

        // SERVICE_TABLE_ENTRYW
        self.add(StructDef {
            name: "SERVICE_TABLE_ENTRYW",
            size_32: 8,
            size_64: 16,
            fields: vec![
                FieldDef { name: "lpServiceName", type_name: "LPWSTR", offset_32: 0, offset_64: 0, size_32: 4, size_64: 8 },
                FieldDef { name: "lpServiceProc", type_name: "LPSERVICE_MAIN_FUNCTION", offset_32: 4, offset_64: 8, size_32: 4, size_64: 8 },
            ],
        });

        // SERVICE_STATUS_PROCESS
        self.add(StructDef {
            name: "SERVICE_STATUS_PROCESS",
            size_32: 36,
            size_64: 36,
            fields: vec![
                FieldDef { name: "dwServiceType", type_name: "DWORD", offset_32: 0, offset_64: 0, size_32: 4, size_64: 4 },
                FieldDef { name: "dwCurrentState", type_name: "DWORD", offset_32: 4, offset_64: 4, size_32: 4, size_64: 4 },
                FieldDef { name: "dwControlsAccepted", type_name: "DWORD", offset_32: 8, offset_64: 8, size_32: 4, size_64: 4 },
                FieldDef { name: "dwWin32ExitCode", type_name: "DWORD", offset_32: 12, offset_64: 12, size_32: 4, size_64: 4 },
                FieldDef { name: "dwServiceSpecificExitCode", type_name: "DWORD", offset_32: 16, offset_64: 16, size_32: 4, size_64: 4 },
                FieldDef { name: "dwCheckPoint", type_name: "DWORD", offset_32: 20, offset_64: 20, size_32: 4, size_64: 4 },
                FieldDef { name: "dwWaitHint", type_name: "DWORD", offset_32: 24, offset_64: 24, size_32: 4, size_64: 4 },
                FieldDef { name: "dwProcessId", type_name: "DWORD", offset_32: 28, offset_64: 28, size_32: 4, size_64: 4 },
                FieldDef { name: "dwServiceFlags", type_name: "DWORD", offset_32: 32, offset_64: 32, size_32: 4, size_64: 4 },
            ],
        });
    }
    
    /// Get structure by name
    pub fn get(&self, name: &str) -> Option<&StructDef> {
        self.structures.get(name)
    }
    
    /// Get all structure names
    pub fn names(&self) -> Vec<&String> {
        self.structures.keys().collect()
    }
}

impl Default for WindowsStructures {
    fn default() -> Self {
        Self::new()
    }
}

