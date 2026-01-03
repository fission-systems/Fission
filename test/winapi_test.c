#include <windows.h>
#include <stdio.h>

// Test case for FID (Function ID) matching and constant substitution
// This should trigger automatic function recognition and flag resolution

// Test 1: File I/O operations (kernel32.dll)
void test_file_operations() {
    printf("Testing File Operations...\n");
    
    // CreateFileA should be recognized by FID
    HANDLE hFile = CreateFileA("test.txt", 
                               GENERIC_READ | GENERIC_WRITE,  // 0xC0000000
                               FILE_SHARE_READ,                // 0x1
                               NULL, 
                               CREATE_ALWAYS,                  // 0x2
                               FILE_ATTRIBUTE_NORMAL,          // 0x80
                               NULL);
    
    if (hFile == INVALID_HANDLE_VALUE) {
        printf("Failed to create file\n");
        return;
    }
    
    // WriteFile should be recognized
    const char* data = "Hello from Fission!";
    DWORD bytesWritten;
    WriteFile(hFile, data, strlen(data), &bytesWritten, NULL);
    
    printf("Wrote %d bytes\n", bytesWritten);
    
    // CloseHandle should be recognized
    CloseHandle(hFile);
}

// Test 2: Memory allocation (kernel32.dll, ntdll.dll)
void test_memory_operations() {
    printf("Testing Memory Operations...\n");
    
    // VirtualAlloc should be recognized by FID
    LPVOID mem = VirtualAlloc(NULL, 
                              0x1000,                          // 4096 bytes
                              MEM_COMMIT | MEM_RESERVE,        // 0x3000
                              PAGE_READWRITE);                 // 0x4
    
    if (mem) {
        printf("Allocated memory at: %p\n", mem);
        
        // Write some data
        memcpy(mem, "Test data", 10);
        
        // VirtualFree should be recognized
        VirtualFree(mem, 0, MEM_RELEASE);  // 0x8000
        printf("Memory freed\n");
    }
    
    // HeapAlloc alternative
    HANDLE hHeap = GetProcessHeap();
    LPVOID heapMem = HeapAlloc(hHeap, HEAP_ZERO_MEMORY, 256);  // 0x8
    
    if (heapMem) {
        printf("Heap allocated: %p\n", heapMem);
        HeapFree(hHeap, 0, heapMem);
    }
}

// Test 3: Registry operations (advapi32.dll)
void test_registry_operations() {
    printf("Testing Registry Operations...\n");
    
    HKEY hKey;
    LONG result;
    
    // RegOpenKeyExA should be recognized
    result = RegOpenKeyExA(HKEY_CURRENT_USER,              // 0x80000001
                          "Software",
                          0,
                          KEY_READ,                        // 0x20019
                          &hKey);
    
    if (result == ERROR_SUCCESS) {
        printf("Registry key opened\n");
        
        // Query a value
        char buffer[256];
        DWORD bufferSize = sizeof(buffer);
        DWORD type;
        
        // RegQueryValueExA should be recognized
        result = RegQueryValueExA(hKey, "TestValue", NULL, &type, 
                                 (LPBYTE)buffer, &bufferSize);
        
        // RegCloseKey should be recognized
        RegCloseKey(hKey);
        printf("Registry key closed\n");
    }
}

// Test 4: Network operations (ws2_32.dll)
void test_network_operations() {
    printf("Testing Network Operations...\n");
    
    WSADATA wsaData;
    
    // WSAStartup should be recognized
    if (WSAStartup(MAKEWORD(2, 2), &wsaData) != 0) {
        printf("WSAStartup failed\n");
        return;
    }
    
    // socket should be recognized
    SOCKET sock = socket(AF_INET,              // 0x2
                        SOCK_STREAM,           // 0x1
                        IPPROTO_TCP);          // 0x6
    
    if (sock != INVALID_SOCKET) {
        printf("Socket created: %lld\n", (long long)sock);
        
        // closesocket should be recognized
        closesocket(sock);
        printf("Socket closed\n");
    }
    
    // WSACleanup should be recognized
    WSACleanup();
}

// Test 5: Process/Thread operations (kernel32.dll)
void test_process_operations() {
    printf("Testing Process Operations...\n");
    
    // GetCurrentProcess should be recognized
    HANDLE hProcess = GetCurrentProcess();
    printf("Current process handle: %p\n", hProcess);
    
    // GetCurrentProcessId should be recognized
    DWORD pid = GetCurrentProcessId();
    printf("Current PID: %d\n", pid);
    
    // GetCurrentThreadId should be recognized
    DWORD tid = GetCurrentThreadId();
    printf("Current TID: %d\n", tid);
    
    // CreateThread test
    HANDLE hThread = CreateThread(NULL, 0, 
                                  (LPTHREAD_START_ROUTINE)test_file_operations,
                                  NULL, 
                                  CREATE_SUSPENDED,        // 0x4
                                  NULL);
    
    if (hThread) {
        printf("Thread created (suspended)\n");
        // ResumeThread should be recognized
        ResumeThread(hThread);
        // WaitForSingleObject should be recognized
        WaitForSingleObject(hThread, INFINITE);  // 0xFFFFFFFF
        CloseHandle(hThread);
    }
}

// Test 6: Cryptography operations (bcrypt.dll)
void test_crypto_operations() {
    printf("Testing Crypto Operations...\n");
    
    BCRYPT_ALG_HANDLE hAlg = NULL;
    NTSTATUS status;
    
    // BCryptOpenAlgorithmProvider should be recognized
    status = BCryptOpenAlgorithmProvider(&hAlg,
                                        BCRYPT_SHA256_ALGORITHM,
                                        NULL,
                                        0);
    
    if (status == 0) {  // STATUS_SUCCESS
        printf("Crypto provider opened\n");
        
        // BCryptCloseAlgorithmProvider should be recognized
        BCryptCloseAlgorithmProvider(hAlg, 0);
        printf("Crypto provider closed\n");
    }
}

int main(int argc, char* argv[]) {
    printf("=== Fission FID Matching Test Suite ===\n\n");
    
    // Run all test cases
    test_file_operations();
    printf("\n");
    
    test_memory_operations();
    printf("\n");
    
    test_registry_operations();
    printf("\n");
    
    test_network_operations();
    printf("\n");
    
    test_process_operations();
    printf("\n");
    
    test_crypto_operations();
    printf("\n");
    
    printf("=== Test Complete ===\n");
    printf("Expected FID recognition:\n");
    printf("- File API: CreateFileA, WriteFile, CloseHandle\n");
    printf("- Memory API: VirtualAlloc, VirtualFree, HeapAlloc, HeapFree\n");
    printf("- Registry API: RegOpenKeyExA, RegQueryValueExA, RegCloseKey\n");
    printf("- Network API: WSAStartup, socket, closesocket, WSACleanup\n");
    printf("- Process API: GetCurrentProcess, CreateThread, ResumeThread\n");
    printf("- Crypto API: BCryptOpenAlgorithmProvider, BCryptCloseAlgorithmProvider\n");
    
    return 0;
}
