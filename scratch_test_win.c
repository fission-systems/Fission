#include <windows.h>
#include <stdio.h>

int main() {
    HANDLE hStdin = GetStdHandle(STD_INPUT_HANDLE);
    HANDLE hStdout = GetStdHandle(STD_OUTPUT_HANDLE);
    
    char buf[10] = {0};
    DWORD readChars = 0;
    DWORD writtenChars = 0;
    
    // Read 3 characters from console
    ReadConsoleA(hStdin, buf, 3, &readChars, NULL);
    
    if (buf[0] == 'F') {
        if (buf[1] == 'I') {
            if (buf[2] == 'S') {
                WriteConsoleA(hStdout, "Target Path Reached!\n", 21, &writtenChars, NULL);
                return 0;
            }
        }
    }
    
    WriteConsoleA(hStdout, "Failed Path.\n", 13, &writtenChars, NULL);
    return 1;
}
