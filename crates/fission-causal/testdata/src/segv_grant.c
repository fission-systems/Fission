/* A sink reached only through a signal handler: grant() runs in the
 * SIGSEGV handler. req[0] selects a slot pointer and slot 2 is NULL, so the
 * store of req[1] through it faults and the handler grants. The selector
 * is the cause (an address dependence into the fault); the stored value
 * req[1] flows into the faulting instruction but never decides whether it
 * faults. The handler exits rather than returning: returning would re-run
 * the faulting store, and the emulator's signal frame has no ucontext or
 * rt_sigreturn to resume elsewhere (see #1112). */
#include <signal.h>
#include "common.h"

unsigned char slots[4][8];
unsigned char *table[4];
unsigned char req[8];
int grants;

NOINLINE void grant(void) { grants++; say("GRANT\n"); }

static void on_segv(int sig) {
    (void)sig;
    grant();
    _exit(3);
}

int main(void) {
    struct sigaction sa = {0};
    sa.sa_handler = on_segv;
    sigaction(SIGSEGV, &sa, 0);
    if (read(0, req, sizeof req) < 2) return 2;
    table[0] = slots[0];
    table[1] = slots[1];
    table[2] = 0;
    table[3] = slots[3];
    unsigned char *p = table[req[0] & 3];
    *p = req[1];
    say("OK\n");
    return 0;
}
