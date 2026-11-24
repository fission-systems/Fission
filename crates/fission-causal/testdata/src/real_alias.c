/* Real alias: a byte store at a request-chosen offset into acct can land
 * on account.is_admin (offset 8). With req[2] == 8 the store of req[9] is the
 * write that makes is_admin non-zero. */
#include "common.h"

struct acct { char note[8]; int is_admin; };
struct acct account;
unsigned char req[16];

int main(void) {
    if (read(0, req, sizeof req) < 12) return 2;
    unsigned idx = req[2] & 15;
    ((unsigned char *)&account)[idx] = req[9];
    say(account.is_admin ? "ADMIN\n" : "USER\n");
    return account.is_admin != 0;
}
