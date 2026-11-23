/* Implicit flow into a call argument: grant() receives a constant chosen
 * by a branch on req[5]. No data flow connects req[5] to the argument. */
#include "common.h"

unsigned char req[16];
int granted_level;

NOINLINE void grant(int level) {
    granted_level = level;
    say(level == 7 ? "GRANT7\n" : "GRANT1\n");
}

int main(void) {
    if (read(0, req, sizeof req) < 8) return 2;
    if (req[5] == 'Z') grant(7); else grant(1);
    return granted_level;
}
