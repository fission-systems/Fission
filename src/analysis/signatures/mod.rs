//! CRT Function Signature Database
//!
//! FLIRT-style pattern matching for recognizing CRT and standard library functions.
//! This helps the decompiler identify known functions without debug symbols.

pub mod win_api;
pub mod win_constants;
pub mod win_types;

// Re-export lazily-initialized global databases for efficient reuse
pub use win_api::WIN_API_DB;
pub use win_constants::WIN_CONSTANTS_DB;

use std::collections::HashMap;

/// A function signature pattern for matching
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Short name of the function
    pub name: String,
    /// Byte pattern (None = wildcard)
    pub pattern: Vec<Option<u8>>,
    /// Minimum function size
    pub min_size: usize,
    /// Parameter names (for annotation)
    pub params: Vec<String>,
    /// Return type description
    pub ret_type: String,
}

impl FunctionSignature {
    /// Create a new signature from a hex pattern string
    /// Use ?? for wildcards, e.g., "55 8B EC ?? ?? 6A"
    pub fn from_hex(name: &str, hex_pattern: &str) -> Self {
        let pattern: Vec<Option<u8>> = hex_pattern
            .split_whitespace()
            .map(|s| {
                if s == "??" {
                    None
                } else {
                    u8::from_str_radix(s, 16).ok()
                }
            })
            .collect();

        Self {
            name: name.to_string(),
            pattern,
            min_size: 16,
            params: Vec::new(),
            ret_type: String::new(),
        }
    }

    /// Match pattern against bytes
    pub fn matches(&self, bytes: &[u8]) -> bool {
        if bytes.len() < self.pattern.len() {
            return false;
        }

        for (i, &pat_byte) in self.pattern.iter().enumerate() {
            if let Some(expected) = pat_byte {
                if bytes[i] != expected {
                    return false;
                }
            }
        }
        true
    }
}

/// CRT Signature Database
/// 
/// Uses a first-byte index for faster signature matching. Most signatures
/// start with a unique or semi-unique first byte, so indexing by this byte
/// reduces the number of signatures to check from ~150 to typically 1-10.
pub struct SignatureDatabase {
    signatures: Vec<FunctionSignature>,
    /// Index of signatures by their first non-wildcard byte for O(1) initial filtering.
    /// Key: first byte value, Value: indices into signatures vec
    first_byte_index: HashMap<u8, Vec<usize>>,
}

impl SignatureDatabase {
    /// Create a new database with built-in signatures
    ///
    /// Performance: Pre-allocates vector capacity based on known signature count
    /// to avoid reallocations during loading (~150 signatures)
    pub fn new() -> Self {
        let mut db = Self {
            // Pre-allocate for ~150 known signatures to avoid reallocations
            signatures: Vec::with_capacity(160),
            first_byte_index: HashMap::with_capacity(64),
        };
        db.load_msvc_signatures();
        db.build_index();
        db
    }

    /// Build the first-byte index for faster lookups
    fn build_index(&mut self) {
        self.first_byte_index.clear();
        for (idx, sig) in self.signatures.iter().enumerate() {
            // Find the first non-wildcard byte in the pattern
            if let Some(&Some(first_byte)) = sig.pattern.first() {
                self.first_byte_index
                    .entry(first_byte)
                    .or_insert_with(Vec::new)
                    .push(idx);
            }
        }
    }

    /// Load MSVC CRT signatures
    fn load_msvc_signatures(&mut self) {
        // ==================== x86 Patterns ====================

        // __security_check_cookie (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "__security_check_cookie",
            "3B 0D ?? ?? ?? ?? 74 ?? C3",
        ));

        // __security_init_cookie (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "__security_init_cookie",
            "8B FF 55 8B EC 83 EC 10 A1",
        ));

        // _initterm (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "_initterm",
            "56 8B 74 24 08 57 8B 7C 24 10",
        ));

        // _CRT_INIT (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "_CRT_INIT",
            "53 56 57 BB 01 00 00 00",
        ));

        // ==================== x64 Patterns ====================

        // __security_check_cookie (x64) - GS cookie check
        self.signatures.push(FunctionSignature::from_hex(
            "__security_check_cookie",
            "48 3B 0D ?? ?? ?? ?? 75 ?? C3",
        ));

        // __security_init_cookie (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "__security_init_cookie",
            "48 83 EC 28 48 8B 05",
        ));

        // _initterm (x64) - initializer list
        self.signatures.push(FunctionSignature::from_hex(
            "_initterm",
            "48 89 5C 24 08 57 48 83 EC 20 48 8B D9 48 8B FA",
        ));

        // _initterm_e (x64) - initializer with error
        self.signatures.push(FunctionSignature::from_hex(
            "_initterm_e",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18",
        ));

        // __GSHandlerCheck (x64) - exception handler GS check
        self.signatures.push(FunctionSignature::from_hex(
            "__GSHandlerCheck",
            "48 89 4C 24 08 48 89 54 24 10 4C 89 44 24 18",
        ));

        // __chkstk (x64) - stack probe
        self.signatures.push(FunctionSignature::from_hex(
            "__chkstk",
            "48 83 EC 10 4C 89 14 24 4C 89 5C 24 08",
        ));

        // __alloca_probe (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "__alloca_probe",
            "51 48 8D 4C 24 08 48 2B C8",
        ));

        // memset (x64) - common pattern
        self.signatures.push(FunctionSignature::from_hex(
            "memset",
            "40 53 48 83 EC 20 0F B6 C2 48 8B D9",
        ));

        // memcpy (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "memcpy",
            "48 8B C1 4C 8D 15 ?? ?? ?? ?? 49 83 F8 0F",
        ));

        // memmove (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "memmove",
            "48 8B C1 4C 8B D9 48 3B CA",
        ));

        // strlen (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strlen",
            "48 8B C1 48 F7 D0 48 83 C0 01",
        ));

        // strcmp (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strcmp",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B F2",
        ));

        // wcslen (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "wcslen",
            "48 8B C1 66 83 39 00 74",
        ));

        // wcscpy (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "wcscpy",
            "48 8B C1 66 44 89 01 66 45 85 C0",
        ));

        // _purecall (x64) - pure virtual call error
        self.signatures.push(FunctionSignature::from_hex(
            "_purecall",
            "48 83 EC 28 E8 ?? ?? ?? ?? 33 C0",
        ));

        // _amsg_exit (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "_amsg_exit",
            "48 83 EC 28 8B C1 B9 ?? 00 00 00",
        ));

        // _cexit (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "_cexit",
            "48 83 EC 28 E8 ?? ?? ?? ?? 85 C0 75",
        ));

        // _c_exit (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "_c_exit",
            "48 83 EC 28 E8 ?? ?? ?? ?? E8",
        ));

        // ~~ PyInstaller specific (observed in user binary) ~~

        // Python main entry stub
        self.signatures.push(FunctionSignature::from_hex(
            "_pyi_main",
            "48 89 5C 24 ?? 48 89 74 24 ?? 57 48 83 EC 20",
        ));

        // Common function prologue patterns (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "_crt_startup",
            "48 83 EC 28 48 8D 0D ?? ?? ?? ?? E8",
        ));

        // ==================== String Functions (x64) ====================

        // memcmp (x64) - memory comparison
        self.signatures.push(FunctionSignature::from_hex(
            "memcmp",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 4D 8B C8",
        ));

        // memcmp variant (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "memcmp",
            "4C 8B DC 49 89 5B 08 49 89 6B 10 49 89 73 18",
        ));

        // strncmp (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strncmp",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 4C 8B CA",
        ));

        // strncpy (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strncpy",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 20 49 8B F0",
        ));

        // strcpy (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strcpy",
            "48 8B C1 0F B6 12 88 11 48 FF C1",
        ));

        // strcat (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strcat",
            "48 8B C1 80 39 00 74 ?? 48 FF C1 EB",
        ));

        // strncat (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strncat",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 20 49 8B D8",
        ));

        // strchr (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strchr",
            "40 53 48 83 EC 20 0F B6 DA 48 8B C1",
        ));

        // strrchr (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strrchr",
            "48 89 5C 24 08 57 48 83 EC 20 0F B6 FA 48 8B D9 E8",
        ));

        // strstr (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strstr",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B F9 48 8B F2",
        ));

        // ==================== Formatting Functions (x64) ====================

        // sprintf (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "sprintf",
            "48 89 4C 24 08 48 89 54 24 10 4C 89 44 24 18 4C 89 4C 24 20 48 83 EC 38",
        ));

        // snprintf / _snprintf (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "snprintf",
            "4C 89 4C 24 20 4C 89 44 24 18 48 89 54 24 10 48 89 4C 24 08 48 83 EC 38",
        ));

        // sscanf (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "sscanf",
            "48 89 54 24 10 4C 89 44 24 18 4C 89 4C 24 20 48 83 EC 28",
        ));

        // printf (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "printf",
            "48 89 4C 24 08 48 89 54 24 10 4C 89 44 24 18 4C 89 4C 24 20 48 83 EC 28",
        ));

        // ==================== Conversion Functions (x64) ====================

        // atoi (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "atoi",
            "48 83 EC 28 45 33 C0 45 33 C9 33 D2",
        ));

        // atol (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "atol",
            "48 83 EC 28 45 33 C0 45 33 C9",
        ));

        // strtol (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strtol",
            "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 48 83 EC 20 41 8B E8",
        ));

        // strtoul (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strtoul",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 20 83 FA 24",
        ));

        // ==================== Memory Allocation (x64) ====================

        // malloc (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "malloc",
            "48 83 EC 28 48 85 C9 75 ?? B9 01 00 00 00",
        ));

        // calloc (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "calloc",
            "48 89 5C 24 08 57 48 83 EC 20 48 8B FA 48 8B D9 48 0F AF FB",
        ));

        // realloc (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "realloc",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B F2 48 8B F9 48 85 C9",
        ));

        // free (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "free",
            "48 85 C9 74 ?? 48 83 EC 28 4C 8B C1",
        ));

        // free variant (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "free",
            "48 83 EC 28 48 85 C9 74 ?? E8",
        ));

        // ==================== File I/O (x64) ====================

        // fopen (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "fopen",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 50 48 8B FA 48 8B F1",
        ));

        // fclose (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "fclose",
            "48 89 5C 24 08 57 48 83 EC 20 48 8B F9 33 D2",
        ));

        // fread (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "fread",
            "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 48 83 EC 30 49 8B E8",
        ));

        // fwrite (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "fwrite",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 20 49 8B D9",
        ));

        // fseek (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "fseek",
            "48 89 5C 24 08 57 48 83 EC 20 41 8B F8 48 8B D9 48 63 C2",
        ));

        // ftell (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "ftell",
            "48 83 EC 28 48 8B 49 18 48 83 C1 08",
        ));

        // ==================== Wide String Functions (x64) ====================

        // wcscmp (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "wcscmp",
            "48 89 5C 24 08 57 48 83 EC 20 48 8B DA 48 8B F9 66 39 11",
        ));

        // wcsncmp (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "wcsncmp",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 4C 8B C2",
        ));

        // wcscat (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "wcscat",
            "48 8B C1 66 83 39 00 74 ?? 48 83 C1 02 EB",
        ));

        // wcsstr (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "wcsstr",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B FA 48 8B F1 66 83 3A 00",
        ));

        // _wcsicmp (x64) - case insensitive wide compare
        self.signatures.push(FunctionSignature::from_hex(
            "_wcsicmp",
            "48 89 5C 24 08 57 48 83 EC 20 48 8B DA 48 8B F9 0F B7 01",
        ));

        // ==================== C++ Runtime (x64) ====================

        // operator new (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "operator_new",
            "48 83 EC 28 48 85 C9 75 ?? B9 01 00 00 00",
        ));

        // operator delete (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "operator_delete",
            "48 85 C9 74 ?? 48 83 EC 28 E8",
        ));

        // __CxxFrameHandler3 (x64 SEH)
        self.signatures.push(FunctionSignature::from_hex(
            "__CxxFrameHandler3",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57",
        ));

        // __CxxFrameHandler4 (x64 newer)
        self.signatures.push(FunctionSignature::from_hex(
            "__CxxFrameHandler4",
            "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 41 54 41 55",
        ));

        // _RTDynamicCast (RTTI)
        self.signatures.push(FunctionSignature::from_hex(
            "__RTDynamicCast",
            "48 89 5C 24 08 48 89 74 24 10 48 89 7C 24 18 55 41 54 41 55",
        ));

        // __std_exception_copy (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "__std_exception_copy",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B 02",
        ));

        // __std_exception_destroy (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "__std_exception_destroy",
            "48 85 D2 74 ?? 48 89 5C 24 08 57 48 83 EC 20",
        ));

        // ==================== Anti-Debug Patterns ====================

        // IsDebuggerPresent via PEB
        self.signatures.push(FunctionSignature::from_hex(
            "antidebug_peb_BeingDebugged",
            "65 48 8B 04 25 60 00 00 00 0F B6 40 02",
        ));

        // NtGlobalFlag check
        self.signatures.push(FunctionSignature::from_hex(
            "antidebug_peb_NtGlobalFlag",
            "65 48 8B 04 25 60 00 00 00 8B 80 BC 00 00 00",
        ));

        // RDTSC timing check
        self.signatures.push(FunctionSignature::from_hex(
            "timing_rdtsc",
            "0F 31 48 C1 E2 20 48 0B C2",
        ));

        // GetTickCount timing
        self.signatures.push(FunctionSignature::from_hex(
            "timing_GetTickCount",
            "FF 15 ?? ?? ?? ?? 8B D8 FF 15 ?? ?? ?? ?? 2B C3",
        ));

        // QueryPerformanceCounter timing
        self.signatures.push(FunctionSignature::from_hex(
            "timing_QueryPerformanceCounter",
            "48 8D 4C 24 ?? FF 15 ?? ?? ?? ?? 48 8B 44 24",
        ));

        // ==================== Crypto Patterns ====================

        // AES S-box lookup
        self.signatures.push(FunctionSignature::from_hex(
            "aes_sbox_lookup",
            "0F B6 C0 48 8D 0D ?? ?? ?? ?? 0F B6 04 01",
        ));

        // MD5 init constants
        self.signatures.push(FunctionSignature::from_hex(
            "md5_init",
            "C7 01 01 23 45 67 C7 41 04 89 AB CD EF",
        ));

        // SHA256 init
        self.signatures.push(FunctionSignature::from_hex(
            "sha256_init",
            "C7 01 67 E6 09 6A C7 41 04 85 AE 67 BB",
        ));

        // SHA1 init
        self.signatures.push(FunctionSignature::from_hex(
            "sha1_init",
            "C7 01 01 23 45 67 C7 41 04 89 AB CD EF C7 41 08 FE DC BA 98",
        ));

        // RC4 key schedule
        self.signatures.push(FunctionSignature::from_hex(
            "rc4_init",
            "33 C0 89 01 89 41 04 48 8D 49 04 3D 00 01 00 00 72",
        ));

        // Base64 encode pattern
        self.signatures.push(FunctionSignature::from_hex(
            "base64_encode",
            "48 8D 05 ?? ?? ?? ?? 0F B6 14 08 C1 E9 02",
        ));

        // XOR loop pattern
        self.signatures.push(FunctionSignature::from_hex(
            "xor_decrypt_loop",
            "30 04 0A 48 FF C2 48 3B D1 72",
        ));

        // ==================== Compression ====================

        // zlib inflate
        self.signatures.push(FunctionSignature::from_hex(
            "zlib_inflate",
            "55 48 8B EC 48 83 EC 50 48 89 5D F0 48 89 75 F8",
        ));

        // zlib deflate
        self.signatures.push(FunctionSignature::from_hex(
            "zlib_deflate",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 48 89 7C 24 20 41 54",
        ));

        // LZ4 decompress
        self.signatures.push(FunctionSignature::from_hex(
            "lz4_decompress_safe",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 48 89 7C 24 20 41 54",
        ));

        // LZMA decode
        self.signatures.push(FunctionSignature::from_hex(
            "lzma_decode",
            "41 57 41 56 41 55 41 54 55 57 56 53 48 81 EC",
        ));

        // ==================== Framework Patterns ====================

        // Python Py_Initialize
        self.signatures.push(FunctionSignature::from_hex(
            "Py_Initialize",
            "40 53 48 83 EC 20 48 8B D9 33 C9 E8",
        ));

        // Python PyRun_SimpleString
        self.signatures.push(FunctionSignature::from_hex(
            "PyRun_SimpleString",
            "48 89 5C 24 08 57 48 83 EC 20 48 8B F9 BA",
        ));

        // .NET CorExeMain
        self.signatures.push(FunctionSignature::from_hex(
            "_CorExeMain",
            "48 83 EC 28 48 8B 05 ?? ?? ?? ?? 48 85 C0 75",
        ));

        // Golang runtime.main
        self.signatures.push(FunctionSignature::from_hex(
            "runtime_main",
            "65 48 8B 0C 25 28 00 00 00 48 8D 44 24",
        ));

        // Rust std::rt::lang_start
        self.signatures.push(FunctionSignature::from_hex(
            "rust_lang_start",
            "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 48 83 EC 30 49 8B F8",
        ));

        // ==================== Windows Internals ====================

        // RtlAllocateHeap pattern
        self.signatures.push(FunctionSignature::from_hex(
            "RtlAllocateHeap",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 41 8B F0 48 8B DA",
        ));

        // RtlFreeHeap pattern
        self.signatures.push(FunctionSignature::from_hex(
            "RtlFreeHeap",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 49 8B F0 8B FA",
        ));

        // NtAllocateVirtualMemory pattern
        self.signatures.push(FunctionSignature::from_hex(
            "NtAllocateVirtualMemory",
            "4C 8B DC 49 89 5B 10 49 89 73 18 57 48 83 EC 50",
        ));

        // LdrLoadDll pattern
        self.signatures.push(FunctionSignature::from_hex(
            "LdrLoadDll",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 30 49 8B F9",
        ));

        // ==================== Syscall Stubs (EDR Evasion) ====================

        // Direct syscall (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "syscall_stub",
            "4C 8B D1 B8 ?? ?? 00 00 0F 05 C3",
        ));

        // Wow64 syscall (32-bit on 64-bit)
        self.signatures.push(FunctionSignature::from_hex(
            "wow64_syscall",
            "B8 ?? ?? 00 00 BA ?? ?? ?? ?? FF D2",
        ));

        // Syscall with jmp to ntdll
        self.signatures.push(FunctionSignature::from_hex(
            "syscall_jmp_ntdll",
            "4C 8B D1 B8 ?? ?? 00 00 49 BB ?? ?? ?? ?? ?? ?? 00 00 41 FF E3",
        ));

        // ==================== Process Injection Patterns ====================

        // NtCreateThreadEx stub
        self.signatures.push(FunctionSignature::from_hex(
            "NtCreateThreadEx",
            "4C 8B D1 B8 C7 00 00 00",
        ));

        // NtWriteVirtualMemory
        self.signatures.push(FunctionSignature::from_hex(
            "NtWriteVirtualMemory",
            "4C 8B D1 B8 3A 00 00 00",
        ));

        // NtProtectVirtualMemory
        self.signatures.push(FunctionSignature::from_hex(
            "NtProtectVirtualMemory",
            "4C 8B D1 B8 50 00 00 00",
        ));

        // APC injection pattern
        self.signatures.push(FunctionSignature::from_hex(
            "apc_injection",
            "48 8B D1 48 8B CA 48 8B C2 4C 8D 4C 24",
        ));

        // ==================== VM/Sandbox Detection ====================

        // CPUID VM detection
        self.signatures.push(FunctionSignature::from_hex(
            "vm_detect_cpuid",
            "B8 01 00 00 00 0F A2 81 E1 00 00 00 80",
        ));

        // CPUID hypervisor brand check
        self.signatures.push(FunctionSignature::from_hex(
            "vm_detect_cpuid_hypervisor",
            "B8 40 00 00 00 0F A2",
        ));

        // In instruction (VMware backdoor)
        self.signatures.push(FunctionSignature::from_hex(
            "vm_detect_vmware",
            "B8 58 4D 56 56 BB 00 00 00 00 B9 0A 00 00 00 BA 58 56 00 00 ED",
        ));

        // ==================== Network Patterns (C2) ====================

        // WSAStartup pattern
        self.signatures.push(FunctionSignature::from_hex(
            "wsa_startup",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 30 8B F9 48 8D 54 24 20",
        ));

        // socket() pattern
        self.signatures.push(FunctionSignature::from_hex(
            "socket_create",
            "44 8B C2 8B D1 B9 02 00 00 00 FF 15",
        ));

        // connect() pattern
        self.signatures.push(FunctionSignature::from_hex(
            "socket_connect",
            "44 8B 4C 24 ?? 44 8B 44 24 ?? 8B 54 24 ?? 8B 4C 24 ?? FF 15",
        ));

        // send() pattern
        self.signatures.push(FunctionSignature::from_hex(
            "socket_send",
            "45 33 C9 44 8B 44 24 ?? 48 8B 54 24 ?? 8B 4C 24 ?? FF 15",
        ));

        // recv() pattern
        self.signatures.push(FunctionSignature::from_hex(
            "socket_recv",
            "45 33 C9 45 8B C0 48 8B D1 8B C9 FF 15",
        ));

        // ==================== Packer Stubs ====================

        // UPX stub (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "upx_stub",
            "60 BE ?? ?? ?? ?? 8D BE ?? ?? ?? ?? 57 83 CD FF",
        ));

        // UPX stub (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "upx_stub_x64",
            "53 51 52 48 8D 05 ?? ?? ?? ?? 48 8D 0D",
        ));

        // Themida/WinLicense entry
        self.signatures.push(FunctionSignature::from_hex(
            "themida_entry",
            "55 8B EC 83 C4 ?? B8 ?? ?? ?? ?? E8",
        ));

        // VMProtect stub
        self.signatures.push(FunctionSignature::from_hex(
            "vmp_stub",
            "68 ?? ?? ?? ?? E8 ?? ?? ?? ?? 00 00 00 00 00",
        ));

        // ASPack stub
        self.signatures.push(FunctionSignature::from_hex(
            "aspack_stub",
            "60 E8 00 00 00 00 5D 81 ED ?? ?? ?? ?? B8 ?? ?? ?? ??",
        ));

        // ==================== TLS Callback ====================

        // TLS callback prologue (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "tls_callback",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 83 FA 01",
        ));

        // TLS callback DLL_PROCESS_ATTACH check
        self.signatures.push(FunctionSignature::from_hex(
            "tls_callback_attach",
            "83 FA 01 75 ?? 48 89 5C 24",
        ));

        // ==================== Math Functions (SSE) ====================

        // sqrtf (x64 SSE)
        self.signatures
            .push(FunctionSignature::from_hex("sqrtf", "0F 51 C0 C3"));

        // sqrtsd (x64 double)
        self.signatures
            .push(FunctionSignature::from_hex("sqrt", "F2 0F 51 C0 C3"));

        // sinf (x64 UCRT)
        self.signatures.push(FunctionSignature::from_hex(
            "sinf",
            "48 83 EC 28 0F 28 D0 F3 0F 5A C0",
        ));

        // cosf (x64 UCRT)
        self.signatures.push(FunctionSignature::from_hex(
            "cosf",
            "48 83 EC 28 0F 28 D0 F3 0F 5A C8",
        ));

        // fabsf (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "fabsf",
            "0F 54 05 ?? ?? ?? ?? C3",
        ));

        // floorf (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "floorf",
            "66 0F 3A 0A C0 01 C3",
        ));

        // ceilf (x64)
        self.signatures
            .push(FunctionSignature::from_hex("ceilf", "66 0F 3A 0A C0 02 C3"));

        // ==================== x86 Patterns (32-bit) ====================

        // malloc (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "malloc",
            "55 8B EC 83 7D 08 00 75 ?? 6A 01",
        ));

        // free (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "free",
            "55 8B EC 83 7D 08 00 74 ?? 8B 45 08",
        ));

        // memcpy (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "memcpy",
            "55 8B EC 57 56 8B 75 0C 8B 4D 10 8B 7D 08",
        ));

        // memset (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "memset",
            "55 8B EC 57 8B 7D 08 0F B6 45 0C",
        ));

        // strlen (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "strlen",
            "8B 4C 24 04 F7 C1 03 00 00 00 74",
        ));

        // strcmp (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "strcmp",
            "55 8B EC 56 8B 75 08 57 8B 7D 0C 8A 06",
        ));

        // ==================== MinGW/GCC ====================

        // __main (GCC CRT)
        self.signatures.push(FunctionSignature::from_hex(
            "__main",
            "55 48 89 E5 48 83 EC 20 E8 ?? ?? ?? ?? 48 83 C4 20 5D C3",
        ));

        // __mingw_CRTStartup
        self.signatures.push(FunctionSignature::from_hex(
            "__mingw_CRTStartup",
            "48 83 EC 28 48 8B 05 ?? ?? ?? ?? 48 85 C0",
        ));

        // __gcc_personality_v0
        self.signatures.push(FunctionSignature::from_hex(
            "__gcc_personality_v0",
            "55 48 89 E5 41 57 41 56 41 55 41 54 53",
        ));

        // ==================== WinHTTP/WinInet (C2) ====================

        // WinHttpOpen pattern
        self.signatures.push(FunctionSignature::from_hex(
            "winhttp_open",
            "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57 41 56 41 57 48 83 EC 40",
        ));

        // WinHttpConnect
        self.signatures.push(FunctionSignature::from_hex(
            "winhttp_connect",
            "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 48 83 EC 30",
        ));

        // InternetOpenA
        self.signatures.push(FunctionSignature::from_hex(
            "internet_open",
            "48 89 5C 24 08 48 89 74 24 10 48 89 7C 24 18 55 41 56 41 57",
        ));

        // ==================== Registry (Persistence) ====================

        // RegOpenKeyExW pattern
        self.signatures.push(FunctionSignature::from_hex(
            "reg_open_key",
            "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57 48 83 EC 30 49 8B F9",
        ));

        // RegSetValueExW pattern
        self.signatures.push(FunctionSignature::from_hex(
            "reg_set_value",
            "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 48 89 7C 24 ?? 41 56",
        ));

        // RegQueryValueExW pattern
        self.signatures.push(FunctionSignature::from_hex(
            "reg_query_value",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 41 54 41 55",
        ));

        // ==================== Additional Crypto ====================

        // ChaCha20 quarter round
        self.signatures.push(FunctionSignature::from_hex(
            "chacha20_quarter",
            "01 C1 31 C8 C1 C0 10 01 C3",
        ));

        // CRC32 table lookup
        self.signatures.push(FunctionSignature::from_hex(
            "crc32_update",
            "33 C0 8A 44 24 ?? 32 01 48 FF C1 48 8D 15",
        ));

        // Blowfish F function
        self.signatures.push(FunctionSignature::from_hex(
            "blowfish_f",
            "8B C1 C1 E8 18 8B 44 82 ?? 03 44 8A",
        ));

        // TEA encrypt
        self.signatures.push(FunctionSignature::from_hex(
            "tea_encrypt",
            "8B 01 8B 49 04 8D 14 30 C1 E0 04 C1 E9 05",
        ));
    }

    /// Try to match a function's bytes against known signatures
    /// 
    /// Performance: Uses first-byte index to reduce candidates from ~150 to typically 1-10,
    /// providing significant speedup for large binaries with many functions.
    pub fn identify(&self, bytes: &[u8]) -> Option<&FunctionSignature> {
        if bytes.is_empty() {
            return None;
        }
        
        let first_byte = bytes[0];
        
        // Use the index to only check signatures that start with the same first byte
        if let Some(indices) = self.first_byte_index.get(&first_byte) {
            for &idx in indices {
                if let Some(sig) = self.signatures.get(idx) {
                    if sig.matches(bytes) {
                        return Some(sig);
                    }
                }
            }
        }
        
        // Fall back to checking signatures that start with wildcards
        // (these won't be in the first_byte_index)
        for sig in &self.signatures {
            // Only check if pattern starts with wildcard (None)
            if sig.pattern.first() == Some(&None) && sig.matches(bytes) {
                return Some(sig);
            }
        }
        
        None
    }

    /// Get all signatures
    pub fn signatures(&self) -> &[FunctionSignature] {
        &self.signatures
    }

    /// Add a custom signature
    pub fn add_signature(&mut self, sig: FunctionSignature) {
        let idx = self.signatures.len();
        // Update index if signature has a non-wildcard first byte
        if let Some(&Some(first_byte)) = sig.pattern.first() {
            self.first_byte_index
                .entry(first_byte)
                .or_insert_with(Vec::new)
                .push(idx);
        }
        self.signatures.push(sig);
    }

    /// Scan binary bytes and identify known functions at given addresses
    /// Returns a map of address -> function name for matched signatures
    pub fn identify_functions_in_binary(
        &self,
        binary_data: &[u8],
        function_addresses: &[(u64, String)], // (address, current_name)
        image_base: u64,
    ) -> HashMap<u64, String> {
        let mut identified = HashMap::new();

        for (addr, _current_name) in function_addresses {
            // Calculate file offset from virtual address
            // For memory-mapped data, the address should be usable directly
            let offset = if *addr >= image_base {
                (*addr - image_base) as usize
            } else {
                continue;
            };

            // Skip if offset is out of bounds
            if offset >= binary_data.len() {
                continue;
            }

            // Get function bytes (first 32 bytes should be enough for matching)
            let end_offset = (offset + 32).min(binary_data.len());
            let func_bytes = &binary_data[offset..end_offset];

            // Try to identify
            if let Some(sig) = self.identify(func_bytes) {
                identified.insert(*addr, sig.name.clone());
            }
        }

        identified
    }
}

impl Default for SignatureDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_match() {
        let sig = FunctionSignature::from_hex("test", "55 8B EC ?? 6A");

        assert!(sig.matches(&[0x55, 0x8B, 0xEC, 0x00, 0x6A]));
        assert!(sig.matches(&[0x55, 0x8B, 0xEC, 0xFF, 0x6A])); // wildcard
        assert!(!sig.matches(&[0x55, 0x8B, 0xED, 0x00, 0x6A])); // wrong byte
        assert!(!sig.matches(&[0x55, 0x8B])); // too short
    }

    #[test]
    fn test_database_creation() {
        let db = SignatureDatabase::new();
        assert!(!db.signatures().is_empty());
    }
}
