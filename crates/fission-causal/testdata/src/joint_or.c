/* Overdetermined grant: either req[0] == 'A' or req[1] == 'B' suffices.
 * With both true, no single byte change prevents the grant; only the joint
 * intervention on both does. At -O0 the || short-circuits, so the recorded
 * run never reads req[1]. */
#include "common.h"

unsigned char req[16];
int grants;

NOINLINE void grant(void) { grants++; say("GRANT\n"); }

int main(void) {
    if (read(0, req, sizeof req) < 4) return 2;
    if (req[0] == 'A' || req[1] == 'B') grant(); else say("DENY\n");
    return grants;
}
