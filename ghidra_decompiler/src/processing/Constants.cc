#include <cstring>
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
    // ── POSIX constants ──────────────────────────────────────
    {"MMAP_PROT", {
        {0x00, "PROT_NONE"},
        {0x01, "PROT_READ"},
        {0x02, "PROT_WRITE"},
        {0x03, "PROT_READ | PROT_WRITE"},
        {0x04, "PROT_EXEC"},
        {0x05, "PROT_READ | PROT_EXEC"},
        {0x07, "PROT_READ | PROT_WRITE | PROT_EXEC"},
    }},
    {"MMAP_FLAGS", {
        {0x01, "MAP_SHARED"},
        {0x02, "MAP_PRIVATE"},
        {0x10, "MAP_FIXED"},
        {0x20, "MAP_ANONYMOUS"},        // Linux
        {0x22, "MAP_PRIVATE | MAP_ANONYMOUS"},
    }},
    {"OPEN_FLAGS", {
        {0x00, "O_RDONLY"},
        {0x01, "O_WRONLY"},
        {0x02, "O_RDWR"},
        {0x40, "O_CREAT"},              // Linux
        {0x80, "O_EXCL"},               // Linux
        {0x200, "O_TRUNC"},             // Linux
        {0x400, "O_APPEND"},            // Linux
        {0x800, "O_NONBLOCK"},          // Linux
    }},
    {"SIGNAL_NUM", {
        {1,  "SIGHUP"},
        {2,  "SIGINT"},
        {3,  "SIGQUIT"},
        {6,  "SIGABRT"},
        {9,  "SIGKILL"},
        {11, "SIGSEGV"},
        {13, "SIGPIPE"},
        {14, "SIGALRM"},
        {15, "SIGTERM"},
        {17, "SIGCHLD"},               // Linux
    }},
    {"DLOPEN_FLAGS", {
        {0x00, "RTLD_LOCAL"},
        {0x01, "RTLD_LAZY"},
        {0x02, "RTLD_NOW"},
        {0x100, "RTLD_GLOBAL"},         // Linux
    }},
    {"SEEK_WHENCE", {
        {0, "SEEK_SET"},
        {1, "SEEK_CUR"},
        {2, "SEEK_END"},
    }},
    {"FCNTL_CMD", {
        {0, "F_DUPFD"},
        {1, "F_GETFD"},
        {2, "F_SETFD"},
        {3, "F_GETFL"},
        {4, "F_SETFL"},
    }},
    {"SHUTDOWN_HOW", {
        {0, "SHUT_RD"},
        {1, "SHUT_WR"},
        {2, "SHUT_RDWR"},
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
    // ── POSIX API parameter mappings ────────────────────────
    // mmap / mprotect
    {"mmap",     2, "MMAP_PROT"},
    {"mmap",     3, "MMAP_FLAGS"},
    {"mmap64",   2, "MMAP_PROT"},
    {"mmap64",   3, "MMAP_FLAGS"},
    {"mprotect", 2, "MMAP_PROT"},
    // open
    {"open",     1, "OPEN_FLAGS"},
    {"open64",   1, "OPEN_FLAGS"},
    // signal / kill
    {"signal",   0, "SIGNAL_NUM"},
    {"kill",     1, "SIGNAL_NUM"},
    {"raise",    0, "SIGNAL_NUM"},
    // dlopen
    {"dlopen",   1, "DLOPEN_FLAGS"},
    // lseek
    {"lseek",    2, "SEEK_WHENCE"},
    {"lseek64",  2, "SEEK_WHENCE"},
    {"fseek",    2, "SEEK_WHENCE"},
    // fcntl
    {"fcntl",    1, "FCNTL_CMD"},
    // socket (POSIX too)
    {"shutdown", 1, "SHUTDOWN_HOW"},
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

    // ── POSIX API signatures ─────────────────────────────────
    // Memory mapping
    {"mmap",       {{"addr", "length", "prot", "flags", "fd", "offset"}}},
    {"mmap64",     {{"addr", "length", "prot", "flags", "fd", "offset"}}},
    {"munmap",     {{"addr", "length"}}},
    {"mprotect",   {{"addr", "len", "prot"}}},
    {"mremap",     {{"old_address", "old_size", "new_size", "flags"}}},

    // File I/O
    {"open",       {{"pathname", "flags", "mode"}}},
    {"open64",     {{"pathname", "flags", "mode"}}},
    {"close",      {{"fd"}}},
    {"read",       {{"fd", "buf", "count"}}},
    {"write",      {{"fd", "buf", "count"}}},
    {"lseek",      {{"fd", "offset", "whence"}}},
    {"lseek64",    {{"fd", "offset", "whence"}}},
    {"pread",      {{"fd", "buf", "count", "offset"}}},
    {"pwrite",     {{"fd", "buf", "count", "offset"}}},
    {"dup",        {{"oldfd"}}},
    {"dup2",       {{"oldfd", "newfd"}}},
    {"pipe",       {{"pipefd"}}},
    {"fcntl",      {{"fd", "cmd"}}},
    {"ioctl",      {{"fd", "request"}}},
    {"stat",       {{"pathname", "statbuf"}}},
    {"fstat",      {{"fd", "statbuf"}}},
    {"lstat",      {{"pathname", "statbuf"}}},

    // C standard library
    {"malloc",     {{"size"}}},
    {"calloc",     {{"nmemb", "size"}}},
    {"realloc",    {{"ptr", "size"}}},
    {"free",       {{"ptr"}}},
    {"memcpy",     {{"dest", "src", "n"}}},
    {"memset",     {{"s", "c", "n"}}},
    {"memmove",    {{"dest", "src", "n"}}},
    {"memcmp",     {{"s1", "s2", "n"}}},
    {"strlen",     {{"s"}}},
    {"strcpy",     {{"dest", "src"}}},
    {"strncpy",    {{"dest", "src", "n"}}},
    {"strcat",     {{"dest", "src"}}},
    {"strncat",    {{"dest", "src", "n"}}},
    {"strcmp",     {{"s1", "s2"}}},
    {"strncmp",    {{"s1", "s2", "n"}}},
    {"strchr",     {{"s", "c"}}},
    {"strrchr",    {{"s", "c"}}},
    {"strstr",     {{"haystack", "needle"}}},
    {"atoi",       {{"nptr"}}},
    {"atol",       {{"nptr"}}},
    {"strtol",     {{"nptr", "endptr", "base"}}},
    {"strtoul",    {{"nptr", "endptr", "base"}}},

    // stdio
    {"printf",     {{"format"}}},
    {"fprintf",    {{"stream", "format"}}},
    {"sprintf",    {{"str", "format"}}},
    {"snprintf",   {{"str", "size", "format"}}},
    {"puts",       {{"s"}}},
    {"fputs",      {{"s", "stream"}}},
    {"fgets",      {{"s", "size", "stream"}}},
    {"fopen",      {{"pathname", "mode"}}},
    {"fclose",     {{"stream"}}},
    {"fread",      {{"ptr", "size", "nmemb", "stream"}}},
    {"fwrite",     {{"ptr", "size", "nmemb", "stream"}}},
    {"fseek",      {{"stream", "offset", "whence"}}},
    {"ftell",      {{"stream"}}},
    {"fflush",     {{"stream"}}},

    // Process
    {"fork",       {{}}},
    {"execve",     {{"pathname", "argv", "envp"}}},
    {"execvp",     {{"file", "argv"}}},
    {"waitpid",    {{"pid", "wstatus", "options"}}},
    {"exit",       {{"status"}}},
    {"_exit",      {{"status"}}},
    {"getpid",     {{}}},
    {"getppid",    {{}}},
    {"kill",       {{"pid", "sig"}}},
    {"signal",     {{"signum", "handler"}}},
    {"raise",      {{"sig"}}},

    // Dynamic loading
    {"dlopen",     {{"filename", "flags"}}},
    {"dlsym",      {{"handle", "symbol"}}},
    {"dlclose",    {{"handle"}}},
    {"dlerror",    {{}}},

    // Threads (pthread)
    {"pthread_create",    {{"thread", "attr", "start_routine", "arg"}}},
    {"pthread_join",      {{"thread", "retval"}}},
    {"pthread_exit",      {{"retval"}}},
    {"pthread_mutex_init",    {{"mutex", "attr"}}},
    {"pthread_mutex_lock",    {{"mutex"}}},
    {"pthread_mutex_unlock",  {{"mutex"}}},
    {"pthread_mutex_destroy", {{"mutex"}}},

    // Networking (POSIX)
    {"shutdown",   {{"sockfd", "how"}}},
    {"setsockopt", {{"sockfd", "level", "optname", "optval", "optlen"}}},
    {"getsockopt", {{"sockfd", "level", "optname", "optval", "optlen"}}},
    {"getaddrinfo", {{"node", "service", "hints", "res"}}},
    {"freeaddrinfo", {{"res"}}},
    {"inet_pton",  {{"af", "src", "dst"}}},
    {"inet_ntop",  {{"af", "src", "dst", "size"}}},
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
