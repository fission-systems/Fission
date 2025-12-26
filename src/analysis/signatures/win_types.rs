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
                // ... more fields (abbreviated for space)
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
