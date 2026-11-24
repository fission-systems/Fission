/* XOR-style cause: grant() when req[0] ^ req[1] == 0x5a. Each byte alone
 * is a but-for cause (any change to one breaks the XOR), yet flipping the
 * same bit in both restores it: the pair is not a joint cause, and its
 * union must not be reported as one. req[2] is a decoy compared on the
 * path. */
#include "common.h"

unsigned char req[16];
int grants;
int seen_d;

NOINLINE void grant(void) { grants++; say("GRANT\n"); }

int main(void) {
    if (read(0, req, sizeof req) < 4) return 2;
    if (req[2] == 'D') seen_d++;
    if ((req[0] ^ req[1]) == 0x5a) grant(); else say("DENY\n");
    return grants;
}
