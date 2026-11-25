/* RFC #1101 case: req_buf[3] == 0x7f controls a write to session.is_admin
 * (implicit flow: the stored value is the constant 1). Request bytes 8..11
 * go to a log buffer through a variable index, which a static slice with no
 * points-to information must assume may alias session.is_admin. */
#include "common.h"

struct session {
    char user[8];
    int is_admin;
    int uid;
};

struct session session;
char log_buf[64];
unsigned log_pos;
unsigned char req_buf[32];

NOINLINE void log_byte(unsigned char v) { log_buf[log_pos++ & 63] = v; }

int main(void) {
    if (read(0, req_buf, sizeof req_buf) < 12) return 2;
    for (int i = 8; i < 12; i++) log_byte(req_buf[i]);
    if (req_buf[3] == 0x7f) session.is_admin = 1;
    if (session.is_admin) say("ADMIN\n"); else say("USER\n");
    return session.is_admin;
}
