#include <stdio.h>
#include <unistd.h>

int main() {
    char buf[10];
    // Read 3 bytes from stdin
    read(0, buf, 3);
    
    // Simple conditional branch based on input
    if (buf[0] == 'F') {
        if (buf[1] == 'I') {
            if (buf[2] == 'S') {
                printf("Target Path Reached!\n");
                return 0;
            }
        }
    }
    
    printf("Failed Path.\n");
    return 1;
}
