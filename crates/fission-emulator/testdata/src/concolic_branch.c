/* Source for x64_static_concolic_branch.elf
 *
 * Rebuild:
 *   zig cc -target x86_64-linux-musl -O1 -static -s \
 *     -o testdata/x64_static_concolic_branch.elf testdata/src/concolic_branch.c
 *
 * Reads one stdin byte; exits 0 if 'A', else 1. Used for stdin-taint → branch smoke.
 */
#include <unistd.h>

int main(void) {
    unsigned char b = 0;
    if (read(0, &b, 1) != 1) {
        return 2;
    }
    if (b == 'A') {
        return 0;
    }
    return 1;
}
