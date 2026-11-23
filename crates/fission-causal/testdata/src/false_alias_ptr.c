/* False alias: request bytes 4..7 are stored through a pointer parameter
 * into scratch[]; cfg.flag gets req[0] & 1 by direct data flow. Without
 * points-to facts the pointer store may alias cfg.flag. */
#include "common.h"

struct cfg { int mode; int flag; };
struct cfg cfg;
unsigned char scratch[8];
unsigned char req[16];

NOINLINE void put(unsigned char *dst, unsigned i, unsigned char v) { dst[i] = v; }

int main(void) {
    if (read(0, req, sizeof req) < 8) return 2;
    for (unsigned i = 0; i < 4; i++) put(scratch, i, req[4 + i]);
    cfg.flag = req[0] & 1;
    say(cfg.flag ? "FLAG\n" : "NOFLAG\n");
    return cfg.flag;
}
