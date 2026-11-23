/* Authorization sink: grant_admin() is called only when the first four
 * request bytes equal the secret. Each of the four is a but-for cause of
 * the call; the bytes after them are not. */
#include "common.h"

static const char secret[4] = {'s', '3', 'c', 'r'};
unsigned char req[16];
int admin_uid;

NOINLINE void grant_admin(int uid) { admin_uid = uid; say("ADMIN\n"); }

int main(void) {
    if (read(0, req, sizeof req) < 8) return 2;
    int ok = 1;
    for (int i = 0; i < 4; i++) {
        if (req[i] != (unsigned char)secret[i]) { ok = 0; break; }
    }
    if (ok) grant_admin(1000); else say("DENIED\n");
    return admin_uid != 0;
}
