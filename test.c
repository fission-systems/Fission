#include <windows.h>

void __stdcall mainCRTStartup() {
    HMODULE h = LoadLibraryA("user32.dll");
    if (h) {
        GetProcAddress(h, "MessageBoxA");
    }
    ExitProcess(0);
}
