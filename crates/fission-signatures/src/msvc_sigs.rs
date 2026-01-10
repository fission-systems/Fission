//! MSVC and CRT Function Signatures
//!
//! Collection of binary patterns for identifying MSVC CRT functions,
//! standard library functions, and common patterns in Windows binaries.

use super::signature::FunctionSignature;

/// Load all MSVC/CRT signatures into the provided vector
pub fn load_msvc_signatures(signatures: &mut Vec<FunctionSignature>) {
    // ==================== x86 Patterns ====================

    // __security_check_cookie (x86)
    signatures.push(FunctionSignature::from_hex(
        "__security_check_cookie",
        "3B 0D ?? ?? ?? ?? 74 ?? C3",
    ));

    // __security_init_cookie (x86)
    signatures.push(FunctionSignature::from_hex(
        "__security_init_cookie",
        "8B FF 55 8B EC 83 EC 10 A1",
    ));

    // _initterm (x86)
    signatures.push(FunctionSignature::from_hex(
        "_initterm",
        "56 8B 74 24 08 57 8B 7C 24 10",
    ));

    // _CRT_INIT (x86)
    signatures.push(FunctionSignature::from_hex(
        "_CRT_INIT",
        "53 56 57 BB 01 00 00 00",
    ));

    // ==================== x64 Patterns ====================

    // __security_check_cookie (x64) - GS cookie check
    signatures.push(FunctionSignature::from_hex(
        "__security_check_cookie",
        "48 3B 0D ?? ?? ?? ?? 75 ?? C3",
    ));

    // __security_init_cookie (x64)
    signatures.push(FunctionSignature::from_hex(
        "__security_init_cookie",
        "48 83 EC 28 48 8B 05",
    ));

    // _initterm (x64) - initializer list
    signatures.push(FunctionSignature::from_hex(
        "_initterm",
        "48 89 5C 24 08 57 48 83 EC 20 48 8B D9 48 8B FA",
    ));

    // _initterm_e (x64) - initializer with error
    signatures.push(FunctionSignature::from_hex(
        "_initterm_e",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18",
    ));

    // __GSHandlerCheck (x64) - exception handler GS check
    signatures.push(FunctionSignature::from_hex(
        "__GSHandlerCheck",
        "48 89 4C 24 08 48 89 54 24 10 4C 89 44 24 18",
    ));

    // __chkstk (x64) - stack probe
    signatures.push(FunctionSignature::from_hex(
        "__chkstk",
        "48 83 EC 10 4C 89 14 24 4C 89 5C 24 08",
    ));

    // __alloca_probe (x64)
    signatures.push(FunctionSignature::from_hex(
        "__alloca_probe",
        "51 48 8D 4C 24 08 48 2B C8",
    ));

    // memset (x64) - common pattern
    signatures.push(FunctionSignature::from_hex(
        "memset",
        "40 53 48 83 EC 20 0F B6 C2 48 8B D9",
    ));

    // memcpy (x64)
    signatures.push(FunctionSignature::from_hex(
        "memcpy",
        "48 8B C1 4C 8D 15 ?? ?? ?? ?? 49 83 F8 0F",
    ));

    // memmove (x64)
    signatures.push(FunctionSignature::from_hex(
        "memmove",
        "48 8B C1 4C 8B D9 48 3B CA",
    ));

    // strlen (x64)
    signatures.push(FunctionSignature::from_hex(
        "strlen",
        "48 8B C1 48 F7 D0 48 83 C0 01",
    ));

    // strcmp (x64)
    signatures.push(FunctionSignature::from_hex(
        "strcmp",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B F2",
    ));

    // wcslen (x64)
    signatures.push(FunctionSignature::from_hex(
        "wcslen",
        "48 8B C1 66 83 39 00 74",
    ));

    // wcscpy (x64)
    signatures.push(FunctionSignature::from_hex(
        "wcscpy",
        "48 8B C1 66 44 89 01 66 45 85 C0",
    ));

    // _purecall (x64) - pure virtual call error
    signatures.push(FunctionSignature::from_hex(
        "_purecall",
        "48 83 EC 28 E8 ?? ?? ?? ?? 33 C0",
    ));

    // _amsg_exit (x64)
    signatures.push(FunctionSignature::from_hex(
        "_amsg_exit",
        "48 83 EC 28 8B C1 B9 ?? 00 00 00",
    ));

    // _cexit (x64)
    signatures.push(FunctionSignature::from_hex(
        "_cexit",
        "48 83 EC 28 E8 ?? ?? ?? ?? 85 C0 75",
    ));

    // _c_exit (x64)
    signatures.push(FunctionSignature::from_hex(
        "_c_exit",
        "48 83 EC 28 E8 ?? ?? ?? ?? E8",
    ));

    // ~~ PyInstaller specific (observed in user binary) ~~

    // Python main entry stub
    signatures.push(FunctionSignature::from_hex(
        "_pyi_main",
        "48 89 5C 24 ?? 48 89 74 24 ?? 57 48 83 EC 20",
    ));

    // Common function prologue patterns (x64)
    signatures.push(FunctionSignature::from_hex(
        "_crt_startup",
        "48 83 EC 28 48 8D 0D ?? ?? ?? ?? E8",
    ));

    // ==================== String Functions (x64) ====================

    // memcmp (x64) - memory comparison
    signatures.push(FunctionSignature::from_hex(
        "memcmp",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 4D 8B C8",
    ));

    // memcmp variant (x64)
    signatures.push(FunctionSignature::from_hex(
        "memcmp",
        "4C 8B DC 49 89 5B 08 49 89 6B 10 49 89 73 18",
    ));

    // strncmp (x64)
    signatures.push(FunctionSignature::from_hex(
        "strncmp",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 4C 8B CA",
    ));

    // strncpy (x64)
    signatures.push(FunctionSignature::from_hex(
        "strncpy",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 20 49 8B F0",
    ));

    // strcpy (x64)
    signatures.push(FunctionSignature::from_hex(
        "strcpy",
        "48 8B C1 0F B6 12 88 11 48 FF C1",
    ));

    // strcat (x64)
    signatures.push(FunctionSignature::from_hex(
        "strcat",
        "48 8B C1 80 39 00 74 ?? 48 FF C1 EB",
    ));

    // strncat (x64)
    signatures.push(FunctionSignature::from_hex(
        "strncat",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 20 49 8B D8",
    ));

    // strchr (x64)
    signatures.push(FunctionSignature::from_hex(
        "strchr",
        "40 53 48 83 EC 20 0F B6 DA 48 8B C1",
    ));

    // strrchr (x64)
    signatures.push(FunctionSignature::from_hex(
        "strrchr",
        "48 89 5C 24 08 57 48 83 EC 20 0F B6 FA 48 8B D9 E8",
    ));

    // strstr (x64)
    signatures.push(FunctionSignature::from_hex(
        "strstr",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B F9 48 8B F2",
    ));

    // ==================== Formatting Functions (x64) ====================

    // sprintf (x64)
    signatures.push(FunctionSignature::from_hex(
        "sprintf",
        "48 89 4C 24 08 48 89 54 24 10 4C 89 44 24 18 4C 89 4C 24 20 48 83 EC 38",
    ));

    // snprintf / _snprintf (x64)
    signatures.push(FunctionSignature::from_hex(
        "snprintf",
        "4C 89 4C 24 20 4C 89 44 24 18 48 89 54 24 10 48 89 4C 24 08 48 83 EC 38",
    ));

    // sscanf (x64)
    signatures.push(FunctionSignature::from_hex(
        "sscanf",
        "48 89 54 24 10 4C 89 44 24 18 4C 89 4C 24 20 48 83 EC 28",
    ));

    // printf (x64)
    signatures.push(FunctionSignature::from_hex(
        "printf",
        "48 89 4C 24 08 48 89 54 24 10 4C 89 44 24 18 4C 89 4C 24 20 48 83 EC 28",
    ));

    // ==================== Conversion Functions (x64) ====================

    // atoi (x64)
    signatures.push(FunctionSignature::from_hex(
        "atoi",
        "48 83 EC 28 45 33 C0 45 33 C9 33 D2",
    ));

    // atol (x64)
    signatures.push(FunctionSignature::from_hex(
        "atol",
        "48 83 EC 28 45 33 C0 45 33 C9",
    ));

    // strtol (x64)
    signatures.push(FunctionSignature::from_hex(
        "strtol",
        "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 48 83 EC 20 41 8B E8",
    ));

    // strtoul (x64)
    signatures.push(FunctionSignature::from_hex(
        "strtoul",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 20 83 FA 24",
    ));

    // ==================== Memory Allocation (x64) ====================

    // malloc (x64)
    signatures.push(FunctionSignature::from_hex(
        "malloc",
        "48 83 EC 28 48 85 C9 75 ?? B9 01 00 00 00",
    ));

    // calloc (x64)
    signatures.push(FunctionSignature::from_hex(
        "calloc",
        "48 89 5C 24 08 57 48 83 EC 20 48 8B FA 48 8B D9 48 0F AF FB",
    ));

    // realloc (x64)
    signatures.push(FunctionSignature::from_hex(
        "realloc",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B F2 48 8B F9 48 85 C9",
    ));

    // free (x64)
    signatures.push(FunctionSignature::from_hex(
        "free",
        "48 85 C9 74 ?? 48 83 EC 28 4C 8B C1",
    ));

    // free variant (x64)
    signatures.push(FunctionSignature::from_hex(
        "free",
        "48 83 EC 28 48 85 C9 74 ?? E8",
    ));

    // ==================== File I/O (x64) ====================

    // fopen (x64)
    signatures.push(FunctionSignature::from_hex(
        "fopen",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 50 48 8B FA 48 8B F1",
    ));

    // fclose (x64)
    signatures.push(FunctionSignature::from_hex(
        "fclose",
        "48 89 5C 24 08 57 48 83 EC 20 48 8B F9 33 D2",
    ));

    // fread (x64)
    signatures.push(FunctionSignature::from_hex(
        "fread",
        "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 48 83 EC 30 49 8B E8",
    ));

    // fwrite (x64)
    signatures.push(FunctionSignature::from_hex(
        "fwrite",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 20 49 8B D9",
    ));

    // fseek (x64)
    signatures.push(FunctionSignature::from_hex(
        "fseek",
        "48 89 5C 24 08 57 48 83 EC 20 41 8B F8 48 8B D9 48 63 C2",
    ));

    // ftell (x64)
    signatures.push(FunctionSignature::from_hex(
        "ftell",
        "48 83 EC 28 48 8B 49 18 48 83 C1 08",
    ));

    // ==================== Wide String Functions (x64) ====================

    // wcscmp (x64)
    signatures.push(FunctionSignature::from_hex(
        "wcscmp",
        "48 89 5C 24 08 57 48 83 EC 20 48 8B DA 48 8B F9 66 39 11",
    ));

    // wcsncmp (x64)
    signatures.push(FunctionSignature::from_hex(
        "wcsncmp",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 4C 8B C2",
    ));

    // wcscat (x64)
    signatures.push(FunctionSignature::from_hex(
        "wcscat",
        "48 8B C1 66 83 39 00 74 ?? 48 83 C1 02 EB",
    ));

    // wcsstr (x64)
    signatures.push(FunctionSignature::from_hex(
        "wcsstr",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B FA 48 8B F1 66 83 3A 00",
    ));

    // _wcsicmp (x64) - case insensitive wide compare
    signatures.push(FunctionSignature::from_hex(
        "_wcsicmp",
        "48 89 5C 24 08 57 48 83 EC 20 48 8B DA 48 8B F9 0F B7 01",
    ));

    // ==================== C++ Runtime (x64) ====================

    // operator new (x64)
    signatures.push(FunctionSignature::from_hex(
        "operator_new",
        "48 83 EC 28 48 85 C9 75 ?? B9 01 00 00 00",
    ));

    // operator delete (x64)
    signatures.push(FunctionSignature::from_hex(
        "operator_delete",
        "48 85 C9 74 ?? 48 83 EC 28 E8",
    ));

    // __CxxFrameHandler3 (x64 SEH)
    signatures.push(FunctionSignature::from_hex(
        "__CxxFrameHandler3",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 41 54 41 55 41 56 41 57",
    ));

    // __CxxFrameHandler4 (x64 newer)
    signatures.push(FunctionSignature::from_hex(
        "__CxxFrameHandler4",
        "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 41 54 41 55",
    ));

    // _RTDynamicCast (RTTI)
    signatures.push(FunctionSignature::from_hex(
        "__RTDynamicCast",
        "48 89 5C 24 08 48 89 74 24 10 48 89 7C 24 18 55 41 54 41 55",
    ));

    // __std_exception_copy (x64)
    signatures.push(FunctionSignature::from_hex(
        "__std_exception_copy",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B 02",
    ));

    // __std_exception_destroy (x64)
    signatures.push(FunctionSignature::from_hex(
        "__std_exception_destroy",
        "48 85 D2 74 ?? 48 89 5C 24 08 57 48 83 EC 20",
    ));

    // ==================== Anti-Debug Patterns ====================

    // IsDebuggerPresent via PEB
    signatures.push(FunctionSignature::from_hex(
        "antidebug_peb_BeingDebugged",
        "65 48 8B 04 25 60 00 00 00 0F B6 40 02",
    ));

    // NtGlobalFlag check
    signatures.push(FunctionSignature::from_hex(
        "antidebug_peb_NtGlobalFlag",
        "65 48 8B 04 25 60 00 00 00 8B 80 BC 00 00 00",
    ));

    // RDTSC timing check
    signatures.push(FunctionSignature::from_hex(
        "timing_rdtsc",
        "0F 31 48 C1 E2 20 48 0B C2",
    ));

    // GetTickCount timing
    signatures.push(FunctionSignature::from_hex(
        "timing_GetTickCount",
        "FF 15 ?? ?? ?? ?? 8B D8 FF 15 ?? ?? ?? ?? 2B C3",
    ));

    // QueryPerformanceCounter timing
    signatures.push(FunctionSignature::from_hex(
        "timing_QueryPerformanceCounter",
        "48 8D 4C 24 ?? FF 15 ?? ?? ?? ?? 48 8B 44 24",
    ));

    // ==================== Crypto Patterns ====================

    // AES S-box lookup
    signatures.push(FunctionSignature::from_hex(
        "aes_sbox_lookup",
        "0F B6 C0 48 8D 0D ?? ?? ?? ?? 0F B6 04 01",
    ));

    // MD5 init constants
    signatures.push(FunctionSignature::from_hex(
        "md5_init",
        "C7 01 01 23 45 67 C7 41 04 89 AB CD EF",
    ));

    // SHA256 init
    signatures.push(FunctionSignature::from_hex(
        "sha256_init",
        "C7 01 67 E6 09 6A C7 41 04 85 AE 67 BB",
    ));

    // SHA1 init
    signatures.push(FunctionSignature::from_hex(
        "sha1_init",
        "C7 01 01 23 45 67 C7 41 04 89 AB CD EF C7 41 08 FE DC BA 98",
    ));

    // RC4 key schedule
    signatures.push(FunctionSignature::from_hex(
        "rc4_init",
        "33 C0 89 01 89 41 04 48 8D 49 04 3D 00 01 00 00 72",
    ));

    // Base64 encode pattern
    signatures.push(FunctionSignature::from_hex(
        "base64_encode",
        "48 8D 05 ?? ?? ?? ?? 0F B6 14 08 C1 E9 02",
    ));

    // XOR loop pattern
    signatures.push(FunctionSignature::from_hex(
        "xor_decrypt_loop",
        "30 04 0A 48 FF C2 48 3B D1 72",
    ));

    // ==================== Compression ====================

    // zlib inflate
    signatures.push(FunctionSignature::from_hex(
        "zlib_inflate",
        "55 48 8B EC 48 83 EC 50 48 89 5D F0 48 89 75 F8",
    ));

    // zlib deflate
    signatures.push(FunctionSignature::from_hex(
        "zlib_deflate",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 48 89 7C 24 20 41 54",
    ));

    // LZ4 decompress
    signatures.push(FunctionSignature::from_hex(
        "lz4_decompress_safe",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 48 89 7C 24 20 41 54",
    ));

    // LZMA decode
    signatures.push(FunctionSignature::from_hex(
        "lzma_decode",
        "41 57 41 56 41 55 41 54 55 57 56 53 48 81 EC",
    ));

    // ==================== Framework Patterns ====================

    // Python Py_Initialize
    signatures.push(FunctionSignature::from_hex(
        "Py_Initialize",
        "40 53 48 83 EC 20 48 8B D9 33 C9 E8",
    ));

    // Python PyRun_SimpleString
    signatures.push(FunctionSignature::from_hex(
        "PyRun_SimpleString",
        "48 89 5C 24 08 57 48 83 EC 20 48 8B F9 BA",
    ));

    // .NET CorExeMain
    signatures.push(FunctionSignature::from_hex(
        "_CorExeMain",
        "48 83 EC 28 48 8B 05 ?? ?? ?? ?? 48 85 C0 75",
    ));

    // Golang runtime.main
    signatures.push(FunctionSignature::from_hex(
        "runtime_main",
        "65 48 8B 0C 25 28 00 00 00 48 8D 44 24",
    ));

    // Rust std::rt::lang_start
    signatures.push(FunctionSignature::from_hex(
        "rust_lang_start",
        "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 48 83 EC 30 49 8B F8",
    ));

    // ==================== Windows Internals ====================

    // RtlAllocateHeap pattern
    signatures.push(FunctionSignature::from_hex(
        "RtlAllocateHeap",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 41 8B F0 48 8B DA",
    ));

    // RtlFreeHeap pattern
    signatures.push(FunctionSignature::from_hex(
        "RtlFreeHeap",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 49 8B F0 8B FA",
    ));

    // NtAllocateVirtualMemory pattern
    signatures.push(FunctionSignature::from_hex(
        "NtAllocateVirtualMemory",
        "4C 8B DC 49 89 5B 10 49 89 73 18 57 48 83 EC 50",
    ));

    // LdrLoadDll pattern
    signatures.push(FunctionSignature::from_hex(
        "LdrLoadDll",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 48 83 EC 30 49 8B F9",
    ));

    // ==================== Syscall Stubs (EDR Evasion) ====================

    // Direct syscall (x64)
    signatures.push(FunctionSignature::from_hex(
        "syscall_stub",
        "4C 8B D1 B8 ?? ?? 00 00 0F 05 C3",
    ));

    // Wow64 syscall (32-bit on 64-bit)
    signatures.push(FunctionSignature::from_hex(
        "wow64_syscall",
        "B8 ?? ?? 00 00 BA ?? ?? ?? ?? FF D2",
    ));

    // Syscall with jmp to ntdll
    signatures.push(FunctionSignature::from_hex(
        "syscall_jmp_ntdll",
        "4C 8B D1 B8 ?? ?? 00 00 49 BB ?? ?? ?? ?? ?? ?? 00 00 41 FF E3",
    ));

    // ==================== Process Injection Patterns ====================

    // NtCreateThreadEx stub
    signatures.push(FunctionSignature::from_hex(
        "NtCreateThreadEx",
        "4C 8B D1 B8 C7 00 00 00",
    ));

    // NtWriteVirtualMemory
    signatures.push(FunctionSignature::from_hex(
        "NtWriteVirtualMemory",
        "4C 8B D1 B8 3A 00 00 00",
    ));

    // NtProtectVirtualMemory
    signatures.push(FunctionSignature::from_hex(
        "NtProtectVirtualMemory",
        "4C 8B D1 B8 50 00 00 00",
    ));

    // APC injection pattern
    signatures.push(FunctionSignature::from_hex(
        "apc_injection",
        "48 8B D1 48 8B CA 48 8B C2 4C 8D 4C 24",
    ));

    // ==================== VM/Sandbox Detection ====================

    // CPUID VM detection
    signatures.push(FunctionSignature::from_hex(
        "vm_detect_cpuid",
        "B8 01 00 00 00 0F A2 81 E1 00 00 00 80",
    ));

    // CPUID hypervisor brand check
    signatures.push(FunctionSignature::from_hex(
        "vm_detect_cpuid_hypervisor",
        "B8 40 00 00 00 0F A2",
    ));

    // In instruction (VMware backdoor)
    signatures.push(FunctionSignature::from_hex(
        "vm_detect_vmware",
        "B8 58 4D 56 56 BB 00 00 00 00 B9 0A 00 00 00 BA 58 56 00 00 ED",
    ));

    // ==================== Network Patterns (C2) ====================

    // WSAStartup pattern
    signatures.push(FunctionSignature::from_hex(
        "wsa_startup",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 30 8B F9 48 8D 54 24 20",
    ));

    // socket() pattern
    signatures.push(FunctionSignature::from_hex(
        "socket_create",
        "44 8B C2 8B D1 B9 02 00 00 00 FF 15",
    ));

    // connect() pattern
    signatures.push(FunctionSignature::from_hex(
        "socket_connect",
        "44 8B 4C 24 ?? 44 8B 44 24 ?? 8B 54 24 ?? 8B 4C 24 ?? FF 15",
    ));

    // send() pattern
    signatures.push(FunctionSignature::from_hex(
        "socket_send",
        "45 33 C9 44 8B 44 24 ?? 48 8B 54 24 ?? 8B 4C 24 ?? FF 15",
    ));

    // recv() pattern
    signatures.push(FunctionSignature::from_hex(
        "socket_recv",
        "45 33 C9 45 8B C0 48 8B D1 8B C9 FF 15",
    ));

    // ==================== Packer Stubs ====================

    // UPX stub (x86)
    signatures.push(FunctionSignature::from_hex(
        "upx_stub",
        "60 BE ?? ?? ?? ?? 8D BE ?? ?? ?? ?? 57 83 CD FF",
    ));

    // UPX stub (x64)
    signatures.push(FunctionSignature::from_hex(
        "upx_stub_x64",
        "53 51 52 48 8D 05 ?? ?? ?? ?? 48 8D 0D",
    ));

    // Themida/WinLicense entry
    signatures.push(FunctionSignature::from_hex(
        "themida_entry",
        "55 8B EC 83 C4 ?? B8 ?? ?? ?? ?? E8",
    ));

    // VMProtect stub
    signatures.push(FunctionSignature::from_hex(
        "vmp_stub",
        "68 ?? ?? ?? ?? E8 ?? ?? ?? ?? 00 00 00 00 00",
    ));

    // ASPack stub
    signatures.push(FunctionSignature::from_hex(
        "aspack_stub",
        "60 E8 00 00 00 00 5D 81 ED ?? ?? ?? ?? B8 ?? ?? ?? ??",
    ));

    // ==================== TLS Callback ====================

    // TLS callback prologue (x64)
    signatures.push(FunctionSignature::from_hex(
        "tls_callback",
        "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 83 FA 01",
    ));

    // TLS callback DLL_PROCESS_ATTACH check
    signatures.push(FunctionSignature::from_hex(
        "tls_callback_attach",
        "83 FA 01 75 ?? 48 89 5C 24",
    ));

    // ==================== Math Functions (SSE) ====================

    // sqrtf (x64 SSE)
    signatures.push(FunctionSignature::from_hex("sqrtf", "0F 51 C0 C3"));

    // sqrtsd (x64 double)
    signatures.push(FunctionSignature::from_hex("sqrt", "F2 0F 51 C0 C3"));

    // sinf (x64 UCRT)
    signatures.push(FunctionSignature::from_hex(
        "sinf",
        "48 83 EC 28 0F 28 D0 F3 0F 5A C0",
    ));

    // cosf (x64 UCRT)
    signatures.push(FunctionSignature::from_hex(
        "cosf",
        "48 83 EC 28 0F 28 D0 F3 0F 5A C8",
    ));

    // fabsf (x64)
    signatures.push(FunctionSignature::from_hex(
        "fabsf",
        "0F 54 05 ?? ?? ?? ?? C3",
    ));

    // floorf (x64)
    signatures.push(FunctionSignature::from_hex(
        "floorf",
        "66 0F 3A 0A C0 01 C3",
    ));

    // ceilf (x64)
    signatures.push(FunctionSignature::from_hex("ceilf", "66 0F 3A 0A C0 02 C3"));

    // ==================== x86 Patterns (32-bit) ====================

    // malloc (x86)
    signatures.push(FunctionSignature::from_hex(
        "malloc",
        "55 8B EC 83 7D 08 00 75 ?? 6A 01",
    ));

    // free (x86)
    signatures.push(FunctionSignature::from_hex(
        "free",
        "55 8B EC 83 7D 08 00 74 ?? 8B 45 08",
    ));

    // memcpy (x86)
    signatures.push(FunctionSignature::from_hex(
        "memcpy",
        "55 8B EC 57 56 8B 75 0C 8B 4D 10 8B 7D 08",
    ));

    // memset (x86)
    signatures.push(FunctionSignature::from_hex(
        "memset",
        "55 8B EC 57 8B 7D 08 0F B6 45 0C",
    ));

    // strlen (x86)
    signatures.push(FunctionSignature::from_hex(
        "strlen",
        "8B 4C 24 04 F7 C1 03 00 00 00 74",
    ));

    // strcmp (x86)
    signatures.push(FunctionSignature::from_hex(
        "strcmp",
        "55 8B EC 56 8B 75 08 57 8B 7D 0C 8A 06",
    ));

    // ==================== MinGW/GCC ====================

    // __main (GCC CRT)
    signatures.push(FunctionSignature::from_hex(
        "__main",
        "55 48 89 E5 48 83 EC 20 E8 ?? ?? ?? ?? 48 83 C4 20 5D C3",
    ));

    // __mingw_CRTStartup
    signatures.push(FunctionSignature::from_hex(
        "__mingw_CRTStartup",
        "48 83 EC 28 48 8B 05 ?? ?? ?? ?? 48 85 C0",
    ));

    // __gcc_personality_v0
    signatures.push(FunctionSignature::from_hex(
        "__gcc_personality_v0",
        "55 48 89 E5 41 57 41 56 41 55 41 54 53",
    ));

    // ==================== WinHTTP/WinInet (C2) ====================

    // WinHttpOpen pattern
    signatures.push(FunctionSignature::from_hex(
        "winhttp_open",
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57 41 56 41 57 48 83 EC 40",
    ));

    // WinHttpConnect
    signatures.push(FunctionSignature::from_hex(
        "winhttp_connect",
        "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 48 83 EC 30",
    ));

    // InternetOpenA
    signatures.push(FunctionSignature::from_hex(
        "internet_open",
        "48 89 5C 24 08 48 89 74 24 10 48 89 7C 24 18 55 41 56 41 57",
    ));

    // ==================== Registry (Persistence) ====================

    // RegOpenKeyExW pattern
    signatures.push(FunctionSignature::from_hex(
        "reg_open_key",
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 57 48 83 EC 30 49 8B F9",
    ));

    // RegSetValueExW pattern
    signatures.push(FunctionSignature::from_hex(
        "reg_set_value",
        "48 89 5C 24 ?? 48 89 6C 24 ?? 48 89 74 24 ?? 48 89 7C 24 ?? 41 56",
    ));

    // RegQueryValueExW pattern
    signatures.push(FunctionSignature::from_hex(
        "reg_query_value",
        "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 41 54 41 55",
    ));

    // ==================== Additional Crypto ====================

    // ChaCha20 quarter round
    signatures.push(FunctionSignature::from_hex(
        "chacha20_quarter",
        "01 C1 31 C8 C1 C0 10 01 C3",
    ));

    // CRC32 table lookup
    signatures.push(FunctionSignature::from_hex(
        "crc32_update",
        "33 C0 8A 44 24 ?? 32 01 48 FF C1 48 8D 15",
    ));

    // Blowfish F function
    signatures.push(FunctionSignature::from_hex(
        "blowfish_f",
        "8B C1 C1 E8 18 8B 44 82 ?? 03 44 8A",
    ));

    // TEA encrypt
    signatures.push(FunctionSignature::from_hex(
        "tea_encrypt",
        "8B 01 8B 49 04 8D 14 30 C1 E0 04 C1 E9 05",
    ));
}
