#include <windows.h>
#include <stdio.h>
#include <math.h>

int main() {
    float a = 3.14f;
    float b = 2.0f;
    float c = a * b;
    
    HANDLE hHeap = GetProcessHeap();
    void* ptr = HeapAlloc(hHeap, 0, 100);
    if (ptr) {
        lstrcpyA((char*)ptr, "Hello from Fission HLE!\n");
        DWORD written = 0;
        WriteConsoleA(GetStdHandle(STD_OUTPUT_HANDLE), ptr, lstrlenA((char*)ptr), &written, NULL);
        HeapFree(hHeap, 0, ptr);
    }
    
    if (c > 6.0f) {
        ExitProcess(0);
    } else {
        ExitProcess(1);
    }
    return 0;
}
