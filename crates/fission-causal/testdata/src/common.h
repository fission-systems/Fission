/* Shared helpers for the CSIC fixtures. Every fixture reads its request
 * from stdin (fd 0) and makes its sink visible natively through stdout and
 * the exit status, so a native run can confirm what the emulator replays. */
#include <unistd.h>

#define NOINLINE __attribute__((noinline))

static NOINLINE void say(const char *s) {
    unsigned n = 0;
    while (s[n]) n++;
    write(1, s, n);
}
