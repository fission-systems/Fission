/* Conjunctive grant: both req[0] == 'K' and req[1] == 'Y' are needed, so
 * each byte alone is a but-for cause. Contrast case for joint_or. */
#include "common.h"

unsigned char req[16];
int grants;

NOINLINE void grant(void) { grants++; say("GRANT\n"); }

int main(void) {
    if (read(0, req, sizeof req) < 4) return 2;
    if (req[0] == 'K' && req[1] == 'Y') grant(); else say("DENY\n");
    return grants;
}
