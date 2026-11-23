/* Crash: req[0] selects a slot pointer; slot 2 is NULL, so the store of
 * req[1] through it faults. The stored value is not a cause of the fault;
 * the selector is, through an address dependence. */
#include "common.h"

unsigned char slots[4][8];
unsigned char *table[4];
unsigned char req[8];

int main(void) {
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
