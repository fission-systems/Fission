# Constant Substitution System

## Overview

Fission's constant substitution system automatically replaces magic numbers in decompiled code with symbolic constant names based on API parameter context. This makes decompiled code significantly more readable and easier to understand.

**Example transformation:**
```c
// Before substitution
hFile = CreateFileA("config.txt", 0xC0000000, 0x1, NULL, 0x3, 0x80, NULL);
mem = VirtualAlloc(NULL, 0x1000, 0x3000, 0x40);

// After substitution
hFile = CreateFileA("config.txt", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ, 
                    NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
mem = VirtualAlloc(NULL, 0x1000, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
```

---

## Table of Contents

- [System Architecture](#system-architecture)
- [Enum Groups](#enum-groups)
- [API Mappings](#api-mappings)
- [Flag Resolution](#flag-resolution)
- [Implementation Details](#implementation-details)
- [Adding Custom Mappings](#adding-custom-mappings)
- [GDT Type Loading](#gdt-type-loading)
- [Performance Considerations](#performance-considerations)

---

## System Architecture

### Components

```
┌─────────────────────┐
│  Decompiled Code    │
│  (from Ghidra)      │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  API Recognition    │ ◄─── Identifies function calls
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  Parameter Mapping  │ ◄─── Maps params to enum groups
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  Flag Resolution    │ ◄─── Resolves constants/combinations
└──────────┬──────────┘
           │
           ▼
┌─────────────────────┐
│  Substituted Code   │
└─────────────────────┘
```

### Key Modules

| Module | Location | Purpose |
|--------|----------|---------|
| **WinConstantsDb** | `src/analysis/signatures/win_constants.rs` | Rust-side enum groups |
| **ENUM_GROUPS** | `ghidra_decompiler/src/processing/Constants.cc` | C++ enum definitions |
| **API_PARAM_MAPPINGS** | `ghidra_decompiler/src/processing/Constants.cc` | Function → parameter → group mappings |
| **WinApiDatabase** | `src/analysis/signatures/win_api.rs` | API signatures with enum hints |

---

## Enum Groups

### Complete List (16 Groups)

| Group | Purpose | Example Values |
|-------|---------|----------------|
| **PAGE_PROTECT** | Memory protection flags | `PAGE_EXECUTE_READWRITE` (0x40) |
| **MEM_ALLOC** | Memory allocation type | `MEM_COMMIT \| MEM_RESERVE` (0x3000) |
| **GENERIC_ACCESS** | File access rights | `GENERIC_READ \| GENERIC_WRITE` (0xC0000000) |
| **FILE_SHARE** | File sharing mode | `FILE_SHARE_READ` (0x1) |
| **FILE_CREATE** | File creation disposition | `CREATE_ALWAYS` (2) |
| **FILE_ATTRIBUTES** | File attributes and flags | `FILE_ATTRIBUTE_NORMAL` (0x80) |
| **PROCESS_ACCESS** | Process access rights | `PROCESS_ALL_ACCESS` (0x1FFFFF) |
| **THREAD_ACCESS** | Thread access rights | `THREAD_SUSPEND_RESUME` (0x0002) |
| **HKEY_ROOT** | Registry root keys | `HKEY_LOCAL_MACHINE` (0x80000002) |
| **REG_ACCESS** | Registry access rights | `KEY_READ` (0x20019) |
| **REG_TYPE** | Registry value types | `REG_DWORD` (4) |
| **MB_TYPE** | MessageBox types | `MB_OKCANCEL \| MB_ICONWARNING` (0x31) |
| **AF_FAMILY** | Socket address family | `AF_INET` (2) |
| **SOCK_TYPE** | Socket type | `SOCK_STREAM` (1) |
| **IPPROTO** | IP protocol | `IPPROTO_TCP` (6) |
| **CREATION_FLAGS** | Thread/process creation | `CREATE_SUSPENDED` (0x4) |

### PAGE_PROTECT Group

```rust
PAGE_NOACCESS             = 0x01
PAGE_READONLY             = 0x02
PAGE_READWRITE            = 0x04
PAGE_WRITECOPY            = 0x08
PAGE_EXECUTE              = 0x10
PAGE_EXECUTE_READ         = 0x20
PAGE_EXECUTE_READWRITE    = 0x40
PAGE_EXECUTE_WRITECOPY    = 0x80
PAGE_GUARD                = 0x100
PAGE_NOCACHE              = 0x200
PAGE_WRITECOMBINE         = 0x400
```

**Usage:**
- `VirtualAlloc` parameter 4 (flProtect)
- `VirtualProtect` parameter 3 (flNewProtect)
- `CreateFileMapping` parameter 3 (flProtect)

### MEM_ALLOC Group

```rust
MEM_COMMIT        = 0x1000
MEM_RESERVE       = 0x2000
MEM_DECOMMIT      = 0x4000
MEM_RELEASE       = 0x8000
MEM_FREE          = 0x10000
MEM_RESET         = 0x80000
MEM_TOP_DOWN      = 0x100000
MEM_WRITE_WATCH   = 0x200000
MEM_PHYSICAL      = 0x400000
MEM_LARGE_PAGES   = 0x20000000
```

**Combination examples:**
- `0x3000` → `MEM_COMMIT | MEM_RESERVE`
- `0x1000` → `MEM_COMMIT`

**Usage:**
- `VirtualAlloc` parameter 3 (flAllocationType)
- `VirtualAllocEx` parameter 4 (flAllocationType)
- `VirtualFree` parameter 3 (dwFreeType)

### GENERIC_ACCESS Group

```rust
GENERIC_READ      = 0x80000000
GENERIC_WRITE     = 0x40000000
GENERIC_EXECUTE   = 0x20000000
GENERIC_ALL       = 0x10000000
```

**Combination examples:**
- `0xC0000000` → `GENERIC_READ | GENERIC_WRITE`
- `0x80000000` → `GENERIC_READ`

**Usage:**
- `CreateFileA/W` parameter 2 (dwDesiredAccess)
- `CreateNamedPipe` parameter 2 (dwOpenMode)

### HKEY_ROOT Group

```rust
HKEY_CLASSES_ROOT      = 0x80000000
HKEY_CURRENT_USER      = 0x80000001
HKEY_LOCAL_MACHINE     = 0x80000002
HKEY_USERS             = 0x80000003
HKEY_CURRENT_CONFIG    = 0x80000005
```

**Usage:**
- `RegOpenKeyExA/W` parameter 1 (hKey)
- `RegCreateKeyExA/W` parameter 1 (hKey)
- `RegDeleteKeyA/W` parameter 1 (hKey)

---

## API Mappings

### Mapping Structure

Each API function has parameter-to-group mappings:

```cpp
// Format: {function_name, parameter_index, enum_group}
{"VirtualAlloc", 2, "MEM_ALLOC"},        // Parameter 2 (flAllocationType)
{"VirtualAlloc", 3, "PAGE_PROTECT"},     // Parameter 3 (flProtect)
{"CreateFileA", 1, "GENERIC_ACCESS"},    // Parameter 1 (dwDesiredAccess)
{"CreateFileA", 2, "FILE_SHARE"},        // Parameter 2 (dwShareMode)
```

### Coverage Statistics

**9 DLLs covered:**
- kernel32.dll - 50+ functions
- user32.dll - 10+ functions
- ntdll.dll - 5+ functions
- advapi32.dll - 15+ functions
- ws2_32.dll - 8+ functions
- winhttp.dll - 3 functions
- wininet.dll - 3 functions
- shell32.dll - 2 functions
- bcrypt.dll - 2 functions

**Total: 100+ API-parameter mappings**

### Common API Functions

#### Memory Management
```cpp
VirtualAlloc(lpAddress, dwSize, [MEM_ALLOC], [PAGE_PROTECT])
VirtualAllocEx(hProcess, lpAddress, dwSize, [MEM_ALLOC], [PAGE_PROTECT])
VirtualFree(lpAddress, dwSize, [MEM_ALLOC])
VirtualProtect(lpAddress, dwSize, [PAGE_PROTECT], lpflOldProtect)
HeapAlloc(hHeap, dwFlags, dwBytes)
HeapFree(hHeap, dwFlags, lpMem)
```

#### File Operations
```cpp
CreateFileA(lpFileName, [GENERIC_ACCESS], [FILE_SHARE], ..., [FILE_CREATE], [FILE_ATTRIBUTES], ...)
ReadFile(hFile, lpBuffer, nNumberOfBytesToRead, lpNumberOfBytesRead, lpOverlapped)
WriteFile(hFile, lpBuffer, nNumberOfBytesToWrite, lpNumberOfBytesWritten, lpOverlapped)
CloseHandle(hObject)
```

#### Process/Thread
```cpp
OpenProcess([PROCESS_ACCESS], bInheritHandle, dwProcessId)
CreateThread(lpThreadAttributes, dwStackSize, lpStartAddress, lpParameter, [CREATION_FLAGS], lpThreadId)
CreateRemoteThread(hProcess, ..., [CREATION_FLAGS], ...)
OpenThread([THREAD_ACCESS], bInheritHandle, dwThreadId)
```

#### Registry
```cpp
RegOpenKeyExA([HKEY_ROOT], lpSubKey, ulOptions, [REG_ACCESS], phkResult)
RegCreateKeyExA([HKEY_ROOT], lpSubKey, ...)
RegQueryValueExA(hKey, lpValueName, ..., [REG_TYPE], ...)
RegSetValueExA(hKey, lpValueName, ..., [REG_TYPE], ...)
RegCloseKey(hKey)
```

#### Network
```cpp
socket([AF_FAMILY], [SOCK_TYPE], [IPPROTO])
WSASocketA([AF_FAMILY], [SOCK_TYPE], [IPPROTO], ...)
connect(s, name, namelen)
send(s, buf, len, flags)
recv(s, buf, len, flags)
```

---

## Flag Resolution

### Single Value Resolution

```rust
// Exact match
0x40 in PAGE_PROTECT → "PAGE_EXECUTE_READWRITE"
0x1000 in MEM_ALLOC → "MEM_COMMIT"
2 in AF_FAMILY → "AF_INET"
```

### Bitwise OR Combination Resolution

The system detects flag combinations by testing each bit:

```rust
pub fn resolve_flags(&self, value: u64) -> Option<String> {
    // Try exact match first
    if let Some(name) = self.get_name(value) {
        return Some(name.to_string());
    }
    
    // Try combining flags
    let mut matches = Vec::new();
    let mut remaining = value;
    
    for (name, flag_value) in &self.values {
        if *flag_value != 0 && (value & flag_value) == *flag_value {
            matches.push(name.clone());
            remaining &= !flag_value;
        }
    }
    
    if remaining == 0 && !matches.is_empty() {
        Some(matches.join(" | "))
    } else {
        None
    }
}
```

**Examples:**
```rust
0x3000 → MEM_COMMIT (0x1000) | MEM_RESERVE (0x2000)
0xC0000000 → GENERIC_READ (0x80000000) | GENERIC_WRITE (0x40000000)
0x31 → MB_OKCANCEL (0x01) | MB_ICONWARNING (0x30)
```

### Precedence Rules

1. **Exact match** - If value exists in group, use it
2. **Combination** - Try to decompose into OR'd flags
3. **Fallback** - Keep original hex value if no match

---

## Implementation Details

### Rust Implementation

**Location:** `src/analysis/signatures/win_constants.rs`

```rust
use std::collections::HashMap;
use std::sync::LazyLock;

/// Global database (singleton pattern)
pub static WIN_CONSTANTS_DB: LazyLock<WinConstantsDb> = 
    LazyLock::new(WinConstantsDb::new);

pub struct EnumGroup {
    pub name: String,
    pub values: Vec<(String, u64)>,
}

impl EnumGroup {
    pub fn resolve_flags(&self, value: u64) -> Option<String> {
        // Implementation as shown above
    }
}

pub struct WinConstantsDb {
    groups: HashMap<String, EnumGroup>,
}

impl WinConstantsDb {
    pub fn new() -> Self {
        let mut db = Self {
            groups: HashMap::new(),
        };
        db.init_all_groups();
        db
    }
    
    pub fn resolve_in_group(&self, group_name: &str, value: u64) -> Option<String> {
        self.groups.get(group_name)?.resolve_flags(value)
    }
}
```

### C++ Implementation

**Location:** `ghidra_decompiler/src/processing/Constants.cc`

```cpp
namespace fission {
namespace processing {

// Enum groups definition
std::map<std::string, std::map<uint64_t, std::string>> ENUM_GROUPS = {
    {"PAGE_PROTECT", {
        {0x40, "PAGE_EXECUTE_READWRITE"},
        // ...
    }},
    {"MEM_ALLOC", {
        {0x1000, "MEM_COMMIT"},
        {0x2000, "MEM_RESERVE"},
        {0x3000, "MEM_COMMIT | MEM_RESERVE"},  // Pre-computed combination
        // ...
    }},
};

// Parameter mappings
std::vector<ApiParamMapping> API_PARAM_MAPPINGS = {
    {"VirtualAlloc", 2, "MEM_ALLOC"},
    {"VirtualAlloc", 3, "PAGE_PROTECT"},
    // ...
};

// Flag resolution function
std::string resolve_flag_combination(uint64_t value, 
                                    const std::map<uint64_t, std::string>& group) {
    // Check exact match
    auto it = group.find(value);
    if (it != group.end()) {
        return it->second;
    }
    
    // Try combination
    std::vector<std::string> matches;
    uint64_t remaining = value;
    
    for (const auto& [flag_val, flag_name] : group) {
        if (flag_val != 0 && (value & flag_val) == flag_val) {
            matches.push_back(flag_name);
            remaining &= ~flag_val;
        }
    }
    
    if (remaining == 0 && !matches.empty()) {
        // Join with " | "
        std::string result;
        for (size_t i = 0; i < matches.size(); ++i) {
            result += matches[i];
            if (i < matches.size() - 1) result += " | ";
        }
        return result;
    }
    
    return "";  // No match
}

}  // namespace processing
}  // namespace fission
```

---

## Adding Custom Mappings

### Step 1: Add Enum Group (Rust)

Edit `src/analysis/signatures/win_constants.rs`:

```rust
impl WinConstantsDb {
    fn init_all_groups(&mut self) {
        // ... existing groups ...
        
        // Add your custom group
        self.add_group(EnumGroup::new(
            "MY_CUSTOM_FLAGS",
            &[
                ("FLAG_A", 0x01),
                ("FLAG_B", 0x02),
                ("FLAG_C", 0x04),
                ("FLAG_D", 0x08),
            ],
        ));
    }
}
```

### Step 2: Add Enum Group (C++)

Edit `ghidra_decompiler/src/processing/Constants.cc`:

```cpp
std::map<std::string, std::map<uint64_t, std::string>> ENUM_GROUPS = {
    // ... existing groups ...
    
    {"MY_CUSTOM_FLAGS", {
        {0x01, "FLAG_A"},
        {0x02, "FLAG_B"},
        {0x04, "FLAG_C"},
        {0x08, "FLAG_D"},
        {0x03, "FLAG_A | FLAG_B"},  // Optional: pre-computed combinations
    }},
};
```

### Step 3: Add API Mapping

Edit `ghidra_decompiler/src/processing/Constants.cc`:

```cpp
std::vector<ApiParamMapping> API_PARAM_MAPPINGS = {
    // ... existing mappings ...
    
    // Map MyCustomFunction's 3rd parameter to MY_CUSTOM_FLAGS
    {"MyCustomFunction", 2, "MY_CUSTOM_FLAGS"},  // 0-indexed
};
```

### Step 4: Add API Signature (Optional)

Edit `src/analysis/signatures/win_api.rs`:

```rust
impl WinApiDatabase {
    fn load_custom(&mut self) {
        self.add(ApiSignature::with_enums(
            "MyCustomFunction",
            "BOOL",
            vec![
                ParamInfo::new("param1", "DWORD"),
                ParamInfo::new("param2", "LPVOID"),
                ParamInfo::with_enum("dwFlags", "DWORD", "MY_CUSTOM_FLAGS"),
            ],
        ));
    }
}
```

### Step 5: Rebuild

```bash
# Rebuild decompiler (C++ changes)
cd ghidra_decompiler/build
cmake --build .

# Rebuild Fission (Rust changes)
cd ../..
cargo build
```

---

## GDT Type Loading

### Overview

Fission loads type information from Ghidra Data Type (GDT) archives:

**Statistics:**
- **5,700+ structures** - Windows SDK types
- **6,500+ typedefs** - Common type aliases
- **Pre-analyzed** - Extracted from Windows headers

### Type Categories

| Category | Count | Examples |
|----------|-------|----------|
| Basic types | ~100 | `DWORD`, `HANDLE`, `LPVOID` |
| Structures | 5,700+ | `PROCESS_INFORMATION`, `SECURITY_ATTRIBUTES` |
| Unions | ~200 | `LARGE_INTEGER`, `ULARGE_INTEGER` |
| Enums | ~500 | `TOKEN_TYPE`, `SE_OBJECT_TYPE` |

### Loading Process

```
┌───────────────────┐
│ GDT Archive       │
│ (msvcrt.gdt)      │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Ghidra Parser     │ ◄─── Parse type definitions
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Type Database     │ ◄─── Store in memory
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Apply to          │ ◄─── Use during decompilation
│ Decompiled Code   │
└───────────────────┘
```

### Usage in Decompilation

When Ghidra decompiles a function, it uses GDT types to:
1. **Type function parameters** - `HANDLE hProcess` instead of `void* param1`
2. **Resolve structures** - Show field names and offsets
3. **Cast correctly** - `(LPVOID)` instead of `(void*)`

**Example:**
```c
// Without GDT types
int sub_140001000(void* param1, int param2) {
    *(int*)(param1 + 0x10) = param2;
}

// With GDT types
BOOL SetProcessInfo(PROCESS_INFORMATION* pInfo, DWORD dwValue) {
    pInfo->dwProcessId = dwValue;
}
```

---

## Performance Considerations

### Caching

**LazyLock singleton:**
```rust
pub static WIN_CONSTANTS_DB: LazyLock<WinConstantsDb> = 
    LazyLock::new(WinConstantsDb::new);
```

- **Initialized once** on first access
- **Reused** for all subsequent lookups
- **Thread-safe** - No locks needed (immutable after init)

### Lookup Performance

| Operation | Complexity | Time |
|-----------|------------|------|
| Exact match | O(1) | ~10 ns |
| Flag combination | O(n) | ~100 ns |
| Group lookup | O(1) | ~10 ns |

**Benchmarks:**
- 1 million lookups: ~10 ms
- Negligible impact on decompilation speed

### Memory Usage

- **Rust DB**: ~50 KB
- **C++ DB**: ~100 KB
- **GDT types**: ~20 MB (loaded by Ghidra)

---

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_flag() {
        let db = WinConstantsDb::new();
        assert_eq!(
            db.resolve_in_group("PAGE_PROTECT", 0x40),
            Some("PAGE_EXECUTE_READWRITE".to_string())
        );
    }

    #[test]
    fn test_combined_flags() {
        let db = WinConstantsDb::new();
        assert_eq!(
            db.resolve_in_group("MEM_ALLOC", 0x3000),
            Some("MEM_RESERVE | MEM_COMMIT".to_string())
        );
    }

    #[test]
    fn test_invalid_value() {
        let db = WinConstantsDb::new();
        assert_eq!(db.resolve_in_group("PAGE_PROTECT", 0x999), None);
    }
}
```

### Running Tests

```bash
cargo test win_constants
```

---

## Examples

### Before and After

#### Memory Allocation
```c
// Before
mem = VirtualAlloc(0, 4096, 12288, 64);

// After
mem = VirtualAlloc(NULL, 0x1000, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
```

#### File Creation
```c
// Before
hFile = CreateFileA("data.bin", 3221225472, 1, 0, 3, 128, 0);

// After
hFile = CreateFileA("data.bin", GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ, 
                    NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
```

#### Registry Access
```c
// Before
RegOpenKeyExA(2147483650, "Software\\MyApp", 0, 131097, &hKey);

// After
RegOpenKeyExA(HKEY_LOCAL_MACHINE, "Software\\MyApp", 0, KEY_READ, &hKey);
```

---

## Future Enhancements

### Planned Features

1. **More DLLs**
   - ntdll.dll (native API)
   - ole32.dll (COM)
   - gdi32.dll (graphics)

2. **Custom Mappings UI**
   - GUI for adding custom groups
   - Import/export mapping files

3. **Context Inference**
   - Detect enum groups from usage patterns
   - Machine learning for unknown constants

4. **Cross-Platform**
   - Linux syscalls
   - macOS frameworks

---

## Related Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md) - Extend with plugins
- [BUILD.md](BUILD.md) - Build instructions

---

## Summary

Fission's constant substitution system:
- ✅ **16 enum groups** covering common Windows APIs
- ✅ **100+ API mappings** for parameter context
- ✅ **Dynamic flag resolution** for OR combinations
- ✅ **Extensible** - Easy to add custom mappings
- ✅ **High performance** - Minimal overhead
- ✅ **GDT integration** - 5,700+ Windows types

**Result:** Significantly improved decompiled code readability.
