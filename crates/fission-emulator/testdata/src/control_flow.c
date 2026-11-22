/* Differential fixture for tests/interp_differential.rs: the JIT and the
 * interpreter must take the same instruction path through libc start-up and
 * this program, and agree on its result (27).
 *
 * Built static, so start-up runs the C library's own string routines: on
 * AArch64 musl's memset clears large blocks with DC ZVA, which is what the
 * test exercises there. Rebuild with build_control_flow.sh.
 */
#include <string.h>

static unsigned char buffer[4096];

static int classify(int value) {
    switch (value % 5) {
    case 0: return 3;
    case 1: return value & 7;
    case 2: return -1;
    case 3: return value >> 1;
    default: return 0;
    }
}

int main(void) {
    memset(buffer, 0xA5, sizeof buffer);   /* large enough for DC ZVA paths */
    memset(buffer, 0, sizeof buffer);
    int total = 3;
    for (int i = 0; i < 12; i++) {
        int c = classify(i);
        if (c < 0)
            continue;
        total += c;
        buffer[i] = (unsigned char)total;
    }
    while (total > 27)
        total -= 4;
    return total + buffer[0] - buffer[0];
}
