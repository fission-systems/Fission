/* A carry: req[0] + req[1] is stored as a 16-bit word and its high byte is
 * compared, so the grant needs the sum to overflow a byte (either req[0]
 * or req[1] made large) and req[2] == 'W'. On "abc", no single byte
 * grants; {req[0], req[2]} and {req[1], req[2]} are both joint causes.
 * The emulator's symbolic shadow tags every byte of a stored word with the
 * whole sum's expression, so what the solver is asked is not quite what
 * the program computes; the replay decides. */
#include "common.h"

unsigned char req[16];
unsigned short word;
int grants;

NOINLINE void grant(void) { grants++; say("GRANT\n"); }

int main(void) {
    if (read(0, req, sizeof req) < 4) return 2;
    word = (unsigned short)(req[0] + req[1]);
    unsigned char hi = ((volatile unsigned char *)&word)[1];
    if (hi == CARRY && req[2] == 'W') grant(); else say("DENY\n");
    return grants;
}
