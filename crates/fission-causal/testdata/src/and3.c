/* Three-way AND: grant() only when req[0..3] spell "KEY". On a request
 * that gets all three wrong, no single byte change grants and no pair
 * does: only the joint intervention on all three, with those exact
 * values, flips the sink. req[3] is a decoy: it is compared (and printed)
 * before the check, so it sits on the path and in a static slice, but it
 * never decides the grant. */
#include "common.h"

unsigned char req[16];
int grants;
int quiet;

NOINLINE void grant(void) { grants++; say("GRANT\n"); }

int main(void) {
    if (read(0, req, sizeof req) < 4) return 2;
    if (req[3] == 'Q') { quiet = 1; say("Q\n"); }
    if (req[0] == 'K' && req[1] == 'E' && req[2] == 'Y') grant(); else say("DENY\n");
    return grants;
}
