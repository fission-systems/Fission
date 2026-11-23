/* Branch-outcome sink with direct data flow: classify() branches on the
 * declared length req[0]. Positive control for dynamic taint. */
#include "common.h"

unsigned char req[16];
int longs;

NOINLINE int classify(unsigned n) {
    if (n > 8) { longs++; return 1; }
    return 0;
}

int main(void) {
    if (read(0, req, sizeof req) < 4) return 2;
    int r = classify(req[0]);
    say(r ? "LONG\n" : "SHORT\n");
    return r;
}
