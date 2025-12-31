#include "fission/processing/Constants.h"
#include <algorithm>

namespace fission {
namespace processing {

// ============================================================================
// Enum Groups for Context-Aware Constant Substitution
// ============================================================================

// Enum group: value -> constant name
// Enum group: value -> constant name
std::map<std::string, std::map<uint64_t, std::string>> ENUM_GROUPS = {
    {"PAGE_PROTECT", {
        {0x01, "PAGE_NOACCESS"},
        {0x02, "PAGE_READONLY"},
        {0x04, "PAGE_READWRITE"},
        {0x08, "PAGE_WRITECOPY"},
        {0x10, "PAGE_EXECUTE"},
        {0x20, "PAGE_EXECUTE_READ"},
        {0x40, "PAGE_EXECUTE_READWRITE"},
        {0x80, "PAGE_EXECUTE_WRITECOPY"},
    }},
    {"MEM_ALLOC", {
        {0x1000, "MEM_COMMIT"},
        {0x2000, "MEM_RESERVE"},
        {0x3000, "MEM_COMMIT | MEM_RESERVE"},
        {0x4000, "MEM_DECOMMIT"},
        {0x8000, "MEM_RELEASE"},
    }},
    {"GENERIC_ACCESS", {
        {0x80000000, "GENERIC_READ"},
        {0x40000000, "GENERIC_WRITE"},
        {0x20000000, "GENERIC_EXECUTE"},
        {0x10000000, "GENERIC_ALL"},
        {0xC0000000, "GENERIC_READ | GENERIC_WRITE"},
    }},
    {"FILE_SHARE", {
        {0x01, "FILE_SHARE_READ"},
        {0x02, "FILE_SHARE_WRITE"},
        {0x03, "FILE_SHARE_READ | FILE_SHARE_WRITE"},
        {0x04, "FILE_SHARE_DELETE"},
    }},
    {"FILE_CREATE", {
        {1, "CREATE_NEW"},
        {2, "CREATE_ALWAYS"},
        {3, "OPEN_EXISTING"},
        {4, "OPEN_ALWAYS"},
        {5, "TRUNCATE_EXISTING"},
    }},
    {"PROCESS_ACCESS", {
        {0x0001, "PROCESS_TERMINATE"},
        {0x0002, "PROCESS_CREATE_THREAD"},
        {0x0008, "PROCESS_VM_OPERATION"},
        {0x0010, "PROCESS_VM_READ"},
        {0x0020, "PROCESS_VM_WRITE"},
        {0x0400, "PROCESS_QUERY_INFORMATION"},
        {0x1F0FFF, "PROCESS_ALL_ACCESS"},
        {0x1FFFFF, "PROCESS_ALL_ACCESS"},
    }},
    {"MB_TYPE", {
        {0x00, "MB_OK"},
        {0x01, "MB_OKCANCEL"},
        {0x02, "MB_ABORTRETRYIGNORE"},
        {0x03, "MB_YESNOCANCEL"},
        {0x04, "MB_YESNO"},
        {0x10, "MB_ICONERROR"},
        {0x20, "MB_ICONQUESTION"},
        {0x30, "MB_ICONWARNING"},
        {0x40, "MB_ICONINFORMATION"},
    }},
    {"TH32CS", {
        {0x01, "TH32CS_SNAPHEAPLIST"},
        {0x02, "TH32CS_SNAPPROCESS"},
        {0x04, "TH32CS_SNAPTHREAD"},
        {0x08, "TH32CS_SNAPMODULE"},
        {0x0F, "TH32CS_SNAPALL"},
        {0x1F, "TH32CS_SNAPALL"},
    }},
    {"CREATION_FLAGS", {
        {0x01, "DEBUG_PROCESS"},
        {0x04, "CREATE_SUSPENDED"},
        {0x08, "DETACHED_PROCESS"},
        {0x10, "CREATE_NEW_CONSOLE"},
        {0x08000000, "CREATE_NO_WINDOW"},
    }},
    {"HKEY_ROOT", {
        {0x80000000, "HKEY_CLASSES_ROOT"},
        {0x80000001, "HKEY_CURRENT_USER"},
        {0x80000002, "HKEY_LOCAL_MACHINE"},
        {0x80000003, "HKEY_USERS"},
        {0x80000005, "HKEY_CURRENT_CONFIG"},
    }},
    {"REG_ACCESS", {
        {0x0001, "KEY_QUERY_VALUE"},
        {0x0002, "KEY_SET_VALUE"},
        {0x0004, "KEY_CREATE_SUB_KEY"},
        {0x0008, "KEY_ENUMERATE_SUB_KEYS"},
        {0x20019, "KEY_READ"},
        {0x20006, "KEY_WRITE"},
        {0xF003F, "KEY_ALL_ACCESS"},
    }},
    {"WAIT_TIMEOUT", {
        {0, "0"},
        {0xFFFFFFFF, "INFINITE"},
    }},
    {"AF_FAMILY", {
        {2, "AF_INET"},
        {23, "AF_INET6"},
    }},
    {"SOCK_TYPE", {
        {1, "SOCK_STREAM"},
        {2, "SOCK_DGRAM"},
        {3, "SOCK_RAW"},
    }},
    {"IPPROTO", {
        {0, "IPPROTO_IP"},
        {6, "IPPROTO_TCP"},
        {17, "IPPROTO_UDP"},
    }},
    {"FILE_MAP", {
        {0x0001, "FILE_MAP_COPY"},
        {0x0002, "FILE_MAP_WRITE"},
        {0x0004, "FILE_MAP_READ"},
        {0x001F, "FILE_MAP_ALL_ACCESS"},
    }},
};

// ============================================================================
// API Parameter -> Enum Group Mapping
// ============================================================================

std::vector<ApiParamMapping> API_PARAM_MAPPINGS = {
    // VirtualAlloc
    {"VirtualAlloc", 2, "MEM_ALLOC"},
    {"VirtualAlloc", 3, "PAGE_PROTECT"},
    {"VirtualAllocEx", 3, "MEM_ALLOC"},
    {"VirtualAllocEx", 4, "PAGE_PROTECT"},
    {"VirtualFree", 2, "MEM_ALLOC"},
    {"VirtualProtect", 2, "PAGE_PROTECT"},
    // CreateFile
    {"CreateFileA", 1, "GENERIC_ACCESS"},
    {"CreateFileA", 2, "FILE_SHARE"},
    {"CreateFileA", 4, "FILE_CREATE"},
    {"CreateFileW", 1, "GENERIC_ACCESS"},
    {"CreateFileW", 2, "FILE_SHARE"},
    {"CreateFileW", 4, "FILE_CREATE"},
    // Process
    {"OpenProcess", 0, "PROCESS_ACCESS"},
    {"CreateProcessA", 5, "CREATION_FLAGS"},
    {"CreateProcessW", 5, "CREATION_FLAGS"},
    // MessageBox
    {"MessageBoxA", 3, "MB_TYPE"},
    {"MessageBoxW", 3, "MB_TYPE"},
    // Snapshot
    {"CreateToolhelp32Snapshot", 0, "TH32CS"},
    // Registry
    {"RegOpenKeyExA", 0, "HKEY_ROOT"},
    {"RegOpenKeyExA", 4, "REG_ACCESS"},
    {"RegOpenKeyExW", 0, "HKEY_ROOT"},
    {"RegOpenKeyExW", 4, "REG_ACCESS"},
    {"RegCreateKeyExA", 0, "HKEY_ROOT"},
    {"RegCreateKeyExW", 0, "HKEY_ROOT"},
    // Thread
    {"CreateThread", 4, "CREATION_FLAGS"},
    {"CreateRemoteThread", 5, "CREATION_FLAGS"},
    // File mapping
    {"CreateFileMappingA", 2, "PAGE_PROTECT"},
    {"CreateFileMappingW", 2, "PAGE_PROTECT"},
    {"MapViewOfFile", 1, "FILE_MAP"},
    // Socket
    {"socket", 0, "AF_FAMILY"},
    {"socket", 1, "SOCK_TYPE"},
    {"socket", 2, "IPPROTO"},
    {"WSASocketA", 0, "AF_FAMILY"},
    {"WSASocketA", 1, "SOCK_TYPE"},
    {"WSASocketW", 0, "AF_FAMILY"},
    {"WSASocketW", 1, "SOCK_TYPE"},
    // Wait
    {"WaitForSingleObject", 1, "WAIT_TIMEOUT"},
    {"WaitForMultipleObjects", 3, "WAIT_TIMEOUT"},
};

// ============================================================================
// API Function Signatures for Parameter Name Application
// ============================================================================

std::map<std::string, ApiSignature> API_SIGNATURES = {
    // Memory
    {"VirtualAlloc", {{"lpAddress", "dwSize", "flAllocationType", "flProtect"}}},
    {"VirtualAllocEx", {{"hProcess", "lpAddress", "dwSize", "flAllocationType", "flProtect"}}},
    {"VirtualFree", {{"lpAddress", "dwSize", "dwFreeType"}}},
    {"VirtualProtect", {{"lpAddress", "dwSize", "flNewProtect", "lpflOldProtect"}}},
    {"HeapAlloc", {{"hHeap", "dwFlags", "dwBytes"}}},
    {"HeapFree", {{"hHeap", "dwFlags", "lpMem"}}},
    
    // File
    {"CreateFileA", {{"lpFileName", "dwDesiredAccess", "dwShareMode", "lpSecurityAttributes", "dwCreationDisposition", "dwFlagsAndAttributes", "hTemplateFile"}}},
    {"CreateFileW", {{"lpFileName", "dwDesiredAccess", "dwShareMode", "lpSecurityAttributes", "dwCreationDisposition", "dwFlagsAndAttributes", "hTemplateFile"}}},
    {"ReadFile", {{"hFile", "lpBuffer", "nNumberOfBytesToRead", "lpNumberOfBytesRead", "lpOverlapped"}}},
    {"WriteFile", {{"hFile", "lpBuffer", "nNumberOfBytesToWrite", "lpNumberOfBytesWritten", "lpOverlapped"}}},
    {"CloseHandle", {{"hObject"}}},
    
    // Process
    {"CreateProcessA", {{"lpApplicationName", "lpCommandLine", "lpProcessAttributes", "lpThreadAttributes", "bInheritHandles", "dwCreationFlags", "lpEnvironment", "lpCurrentDirectory", "lpStartupInfo", "lpProcessInformation"}}},
    {"CreateProcessW", {{"lpApplicationName", "lpCommandLine", "lpProcessAttributes", "lpThreadAttributes", "bInheritHandles", "dwCreationFlags", "lpEnvironment", "lpCurrentDirectory", "lpStartupInfo", "lpProcessInformation"}}},
    {"OpenProcess", {{"dwDesiredAccess", "bInheritHandle", "dwProcessId"}}},
    {"TerminateProcess", {{"hProcess", "uExitCode"}}},
    {"GetCurrentProcess", {{}}},
    {"GetCurrentProcessId", {{}}},
    
    // Thread
    {"CreateThread", {{"lpThreadAttributes", "dwStackSize", "lpStartAddress", "lpParameter", "dwCreationFlags", "lpThreadId"}}},
    {"CreateRemoteThread", {{"hProcess", "lpThreadAttributes", "dwStackSize", "lpStartAddress", "lpParameter", "dwCreationFlags", "lpThreadId"}}},
    {"ExitThread", {{"dwExitCode"}}},
    
    // Module
    {"LoadLibraryA", {{"lpLibFileName"}}},
    {"LoadLibraryW", {{"lpLibFileName"}}},
    {"GetModuleHandleA", {{"lpModuleName"}}},
    {"GetModuleHandleW", {{"lpModuleName"}}},
    {"GetProcAddress", {{"hModule", "lpProcName"}}},
    {"FreeLibrary", {{"hLibModule"}}},
    
    // Memory Operations
    {"ReadProcessMemory", {{"hProcess", "lpBaseAddress", "lpBuffer", "nSize", "lpNumberOfBytesRead"}}},
    {"WriteProcessMemory", {{"hProcess", "lpBaseAddress", "lpBuffer", "nSize", "lpNumberOfBytesWritten"}}},
    
    // Snapshot
    {"CreateToolhelp32Snapshot", {{"dwFlags", "th32ProcessID"}}},
    
    // MessageBox
    {"MessageBoxA", {{"hWnd", "lpText", "lpCaption", "uType"}}},
    {"MessageBoxW", {{"hWnd", "lpText", "lpCaption", "uType"}}},
    
    // Registry
    {"RegOpenKeyExA", {{"hKey", "lpSubKey", "ulOptions", "samDesired", "phkResult"}}},
    {"RegOpenKeyExW", {{"hKey", "lpSubKey", "ulOptions", "samDesired", "phkResult"}}},
    {"RegCloseKey", {{"hKey"}}},
    {"RegQueryValueExA", {{"hKey", "lpValueName", "lpReserved", "lpType", "lpData", "lpcbData"}}},
    {"RegSetValueExA", {{"hKey", "lpValueName", "Reserved", "dwType", "lpData", "cbData"}}},
    
    // Socket
    {"socket", {{"af", "type", "protocol"}}},
    {"connect", {{"s", "name", "namelen"}}},
    {"send", {{"s", "buf", "len", "flags"}}},
    {"recv", {{"s", "buf", "len", "flags"}}},
    {"closesocket", {{"s"}}},
    {"bind", {{"s", "name", "namelen"}}},
    {"listen", {{"s", "backlog"}}},
    {"accept", {{"s", "addr", "addrlen"}}},
    
    // Wait
    {"WaitForSingleObject", {{"hHandle", "dwMilliseconds"}}},
    {"WaitForMultipleObjects", {{"nCount", "lpHandles", "bWaitAll", "dwMilliseconds"}}},
    {"Sleep", {{"dwMilliseconds"}}},
    
    // String
    {"lstrcpyA", {{"lpString1", "lpString2"}}},
    {"lstrcpyW", {{"lpString1", "lpString2"}}},
    {"lstrcatA", {{"lpString1", "lpString2"}}},
    {"lstrlenA", {{"lpString"}}},
    {"lstrlenW", {{"lpString"}}},
    
    // File Mapping
    {"CreateFileMappingA", {{"hFile", "lpFileMappingAttributes", "flProtect", "dwMaximumSizeHigh", "dwMaximumSizeLow", "lpName"}}},
    {"CreateFileMappingW", {{"hFile", "lpFileMappingAttributes", "flProtect", "dwMaximumSizeHigh", "dwMaximumSizeLow", "lpName"}}},
    {"MapViewOfFile", {{"hFileMappingObject", "dwDesiredAccess", "dwFileOffsetHigh", "dwFileOffsetLow", "dwNumberOfBytesToMap"}}},
    {"UnmapViewOfFile", {{"lpBaseAddress"}}},

    // Process Injection
    {"VirtualFreeEx", {{"hProcess", "lpAddress", "dwSize", "dwFreeType"}}},
    {"VirtualProtectEx", {{"hProcess", "lpAddress", "dwSize", "flNewProtect", "lpflOldProtect"}}},
    {"CreateRemoteThreadEx", {{"hProcess", "lpThreadAttributes", "dwStackSize", "lpStartAddress", "lpParameter", "dwCreationFlags", "lpAttributeList", "lpThreadId"}}},
};

// ============================================================================
// Utility Functions
// ============================================================================

// Dynamic flag combination resolver
std::string resolve_flag_combination(uint64_t value, const std::map<uint64_t, std::string>& group) {
    // Single value first
    auto it = group.find(value);
    if (it != group.end()) return it->second;
    
    // Try bit combinations
    std::vector<std::string> flags;
    uint64_t remaining = value;
    
    // Sort by value descending (greedy)
    std::vector<std::pair<uint64_t, std::string>> sorted(group.begin(), group.end());
    std::sort(sorted.begin(), sorted.end(), 
              [](const auto& a, const auto& b) { return a.first > b.first; });
    
    for (const auto& [v, name] : sorted) {
        if (v != 0 && (remaining & v) == v) {
            flags.push_back(name);
            remaining &= ~v;
        }
    }
    
    if (remaining == 0 && !flags.empty()) {
        std::string result;
        for (size_t i = 0; i < flags.size(); i++) {
            if (i > 0) result += " | ";
            result += flags[i];
        }
        return result;
    }
    
    return "";  // Combination failed
}

} // namespace processing
} // namespace fission
