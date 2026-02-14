#include <windows.h>
#include <stdio.h>

void test_api_calls() {
    LPVOID addr = VirtualAlloc(NULL, 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    if (addr) {
        printf("Allocated at: %p\n", addr);
        VirtualFree(addr, 0, MEM_RELEASE);
    }
    
    HANDLE hFile = CreateFileA("test.txt", GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (hFile != INVALID_HANDLE_VALUE) {
        CloseHandle(hFile);
    }
}

int main() {
    printf("Fission Windows Test\n");
    test_api_calls();
    return 0;
}
