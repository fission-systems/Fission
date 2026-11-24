/* The grant depends on kernel entropy. Same refusal as nondet_time. */
#include "common.h"
#include <sys/random.h>

unsigned char req[16];
int grants;

NOINLINE void grant(void) { grants++; say("GRANT\n"); }

int main(void) {
    if (read(0, req, sizeof req) < 4) return 2;
    unsigned r = 0;
    if (getrandom(&r, sizeof r, 0) != sizeof r) return 3;
    if ((r & 1) == 0 && req[0] == 'R') grant(); else say("DENY\n");
    return grants;
}
