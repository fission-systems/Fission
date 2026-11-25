/* The grant depends on the wall clock as well as on req[0]. The emulator
 * stubs time to a constant, so a replay looks deterministic; the native
 * program does not behave that way. CSIC must refuse to attribute. */
#include "common.h"
#include <time.h>

unsigned char req[16];
int grants;

NOINLINE void grant(void) { grants++; say("GRANT\n"); }

int main(void) {
    if (read(0, req, sizeof req) < 4) return 2;
    time_t t = time(0);
    if ((t & 1) == 0 && req[0] == 'T') grant(); else say("DENY\n");
    return grants;
}
