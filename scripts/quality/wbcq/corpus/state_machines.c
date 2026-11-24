/*
 * Step-function state machines for the WBCQ P3 evaluation (RFC #1093).
 *
 * Each function is one step of a protocol or controller in the shape the
 * quotient boundary takes: `unsigned step(struct ... *ctx, unsigned in)`.
 * The context persists between steps and starts zeroed; the result is
 * what a caller observes. Functions are leaves without globals or lookup
 * tables, so a table-free compiler build (`-fno-jump-tables`) keeps them
 * inside the model. The machines follow their published descriptions; none
 * was written against Fission's output.
 *
 * `build_corpus.py` compiles this file per ISA and obfuscation variant, and
 * natively for the ground-truth minimization in `ground_truth.py`.
 */

/* ── TCP connection (RFC 793 section 3.2, simplified events) ─────────── */

enum tcp_state {
    TCP_CLOSED, TCP_LISTEN, TCP_SYN_SENT, TCP_SYN_RCVD, TCP_ESTABLISHED,
    TCP_FIN_WAIT_1, TCP_FIN_WAIT_2, TCP_CLOSING, TCP_TIME_WAIT,
    TCP_CLOSE_WAIT, TCP_LAST_ACK
};
enum tcp_event {
    EV_PASSIVE_OPEN, EV_ACTIVE_OPEN, EV_SYN, EV_SYN_ACK, EV_ACK, EV_FIN,
    EV_CLOSE, EV_TIMEOUT, EV_RST
};
enum tcp_action { ACT_NONE, ACT_SYN, ACT_SYN_ACK, ACT_ACK, ACT_FIN, ACT_RST };

struct tcp_ctx { unsigned state; };

unsigned tcp_step(struct tcp_ctx *c, unsigned ev)
{
    unsigned s = c->state;
    unsigned act = ACT_NONE;
    if (ev == EV_RST && s != TCP_CLOSED && s != TCP_LISTEN) {
        c->state = TCP_CLOSED;
        return ACT_NONE;
    }
    switch (s) {
    case TCP_CLOSED:
        if (ev == EV_PASSIVE_OPEN) s = TCP_LISTEN;
        else if (ev == EV_ACTIVE_OPEN) { s = TCP_SYN_SENT; act = ACT_SYN; }
        else if (ev != EV_RST) act = ACT_RST;
        break;
    case TCP_LISTEN:
        if (ev == EV_SYN) { s = TCP_SYN_RCVD; act = ACT_SYN_ACK; }
        else if (ev == EV_CLOSE) s = TCP_CLOSED;
        break;
    case TCP_SYN_SENT:
        if (ev == EV_SYN_ACK) { s = TCP_ESTABLISHED; act = ACT_ACK; }
        else if (ev == EV_SYN) { s = TCP_SYN_RCVD; act = ACT_SYN_ACK; }
        else if (ev == EV_CLOSE || ev == EV_TIMEOUT) s = TCP_CLOSED;
        break;
    case TCP_SYN_RCVD:
        if (ev == EV_ACK) s = TCP_ESTABLISHED;
        else if (ev == EV_CLOSE) { s = TCP_FIN_WAIT_1; act = ACT_FIN; }
        break;
    case TCP_ESTABLISHED:
        if (ev == EV_FIN) { s = TCP_CLOSE_WAIT; act = ACT_ACK; }
        else if (ev == EV_CLOSE) { s = TCP_FIN_WAIT_1; act = ACT_FIN; }
        break;
    case TCP_FIN_WAIT_1:
        if (ev == EV_ACK) s = TCP_FIN_WAIT_2;
        else if (ev == EV_FIN) { s = TCP_CLOSING; act = ACT_ACK; }
        break;
    case TCP_FIN_WAIT_2:
        if (ev == EV_FIN) { s = TCP_TIME_WAIT; act = ACT_ACK; }
        break;
    case TCP_CLOSING:
        if (ev == EV_ACK) s = TCP_TIME_WAIT;
        break;
    case TCP_TIME_WAIT:
        if (ev == EV_TIMEOUT) s = TCP_CLOSED;
        break;
    case TCP_CLOSE_WAIT:
        if (ev == EV_CLOSE) { s = TCP_LAST_ACK; act = ACT_FIN; }
        break;
    case TCP_LAST_ACK:
        if (ev == EV_ACK) s = TCP_CLOSED;
        break;
    default:
        s = TCP_CLOSED;
        break;
    }
    c->state = s;
    return act;
}

/* ── HTTP/1.x request line: METHOD SP TARGET SP HTTP/1.D CR LF ───────── */
/* Result: 0 more input needed, 1 request line complete, 2 malformed.    */
/* Methods: GET, PUT, POST, HEAD. Once complete or malformed it stays.  */

struct http_ctx { unsigned state; };

enum {
    H_START, H_G, H_GE, H_P, H_PU, H_PO, H_POS, H_H, H_HE, H_HEA,
    H_METHOD_SP, H_TARGET, H_V_H, H_V_HT, H_V_HTT, H_V_HTTP, H_V_SLASH,
    H_V_ONE, H_V_DOT, H_V_DIGIT, H_CR, H_LF, H_DONE, H_ERROR
};

unsigned http_reqline_step(struct http_ctx *c, unsigned in)
{
    unsigned ch = in & 0xff;
    unsigned s = c->state;
    unsigned n = H_ERROR;
    switch (s) {
    case H_START: n = ch == 'G' ? H_G : ch == 'P' ? H_P : ch == 'H' ? H_H : H_ERROR; break;
    case H_G: n = ch == 'E' ? H_GE : H_ERROR; break;
    case H_GE: n = ch == 'T' ? H_METHOD_SP : H_ERROR; break;
    case H_P: n = ch == 'U' ? H_PU : ch == 'O' ? H_PO : H_ERROR; break;
    case H_PU: n = ch == 'T' ? H_METHOD_SP : H_ERROR; break;
    case H_PO: n = ch == 'S' ? H_POS : H_ERROR; break;
    case H_POS: n = ch == 'T' ? H_METHOD_SP : H_ERROR; break;
    case H_H: n = ch == 'E' ? H_HE : H_ERROR; break;
    case H_HE: n = ch == 'A' ? H_HEA : H_ERROR; break;
    case H_HEA: n = ch == 'D' ? H_METHOD_SP : H_ERROR; break;
    case H_METHOD_SP: n = ch == ' ' ? H_TARGET : H_ERROR; break;
    case H_TARGET:
        if (ch == ' ') n = H_V_H;
        else if (ch > 0x20 && ch < 0x7f) n = H_TARGET;
        break;
    case H_V_H: n = ch == 'H' ? H_V_HT : H_ERROR; break;
    case H_V_HT: n = ch == 'T' ? H_V_HTT : H_ERROR; break;
    case H_V_HTT: n = ch == 'T' ? H_V_HTTP : H_ERROR; break;
    case H_V_HTTP: n = ch == 'P' ? H_V_SLASH : H_ERROR; break;
    case H_V_SLASH: n = ch == '/' ? H_V_ONE : H_ERROR; break;
    case H_V_ONE: n = ch == '1' ? H_V_DOT : H_ERROR; break;
    case H_V_DOT: n = ch == '.' ? H_V_DIGIT : H_ERROR; break;
    case H_V_DIGIT: n = (ch == '0' || ch == '1') ? H_CR : H_ERROR; break;
    case H_CR: n = ch == '\r' ? H_LF : H_ERROR; break;
    case H_LF: n = ch == '\n' ? H_DONE : H_ERROR; break;
    case H_DONE: n = H_DONE; break;
    default: n = H_ERROR; break;
    }
    c->state = n;
    return n == H_DONE ? 1 : n == H_ERROR ? 2 : 0;
}

/* ── UTF-8 validation (RFC 3629 section 4, byte by byte) ─────────────── */
/* Result: 0 at a character boundary, 1 inside a sequence, 2 invalid     */
/* (sticky).                                                             */

struct utf8_ctx { unsigned need; unsigned lo; unsigned hi; unsigned bad; };

unsigned utf8_step(struct utf8_ctx *c, unsigned in)
{
    unsigned b = in & 0xff;
    if (c->bad)
        return 2;
    if (c->need == 0) {
        c->lo = 0x80;
        c->hi = 0xbf;
        if (b < 0x80) return 0;
        if (b >= 0xc2 && b <= 0xdf) { c->need = 1; return 1; }
        if (b == 0xe0) { c->need = 2; c->lo = 0xa0; return 1; }
        if ((b >= 0xe1 && b <= 0xec) || b == 0xee || b == 0xef) { c->need = 2; return 1; }
        if (b == 0xed) { c->need = 2; c->hi = 0x9f; return 1; }
        if (b == 0xf0) { c->need = 3; c->lo = 0x90; return 1; }
        if (b >= 0xf1 && b <= 0xf3) { c->need = 3; return 1; }
        if (b == 0xf4) { c->need = 3; c->hi = 0x8f; return 1; }
        c->bad = 1;
        return 2;
    }
    if (b < c->lo || b > c->hi) {
        c->bad = 1;
        return 2;
    }
    c->lo = 0x80;
    c->hi = 0xbf;
    c->need -= 1;
    return c->need == 0 ? 0 : 1;
}

/* ── Telnet command parser (RFC 854 / RFC 855) ──────────────────────── */
/* Result: 0 the byte is data, 1 consumed by a command, 2 a command or   */
/* subnegotiation just ended.                                            */

enum { T_DATA, T_IAC, T_OPT, T_SB, T_SB_IAC };
struct telnet_ctx { unsigned state; };

unsigned telnet_step(struct telnet_ctx *c, unsigned in)
{
    unsigned b = in & 0xff;
    switch (c->state) {
    case T_DATA:
        if (b == 255) { c->state = T_IAC; return 1; }
        return 0;
    case T_IAC:
        if (b == 255) { c->state = T_DATA; return 0; }      /* escaped 0xff */
        if (b >= 251 && b <= 254) { c->state = T_OPT; return 1; } /* WILL..DONT */
        if (b == 250) { c->state = T_SB; return 1; }
        c->state = T_DATA;
        return 2;
    case T_OPT:
        c->state = T_DATA;
        return 2;
    case T_SB:
        if (b == 255) c->state = T_SB_IAC;
        return 1;
    case T_SB_IAC:
        if (b == 240) { c->state = T_DATA; return 2; }  /* SE */
        c->state = T_SB;
        return 1;
    default:
        c->state = T_DATA;
        return 1;
    }
}

/* ── SLIP decoding (RFC 1055) ───────────────────────────────────────── */
/* Result: 0x100 + byte for a decoded byte, 0x200 at frame end, 0 when   */
/* the byte only starts an escape.                                       */

struct slip_ctx { unsigned escaped; };

unsigned slip_step(struct slip_ctx *c, unsigned in)
{
    unsigned b = in & 0xff;
    if (c->escaped) {
        c->escaped = 0;
        if (b == 0xdc) return 0x100 | 0xc0;
        if (b == 0xdd) return 0x100 | 0xdb;
        return 0x100 | b;  /* protocol violation: passed through, as RFC 1055 suggests */
    }
    if (b == 0xc0) return 0x200;
    if (b == 0xdb) { c->escaped = 1; return 0; }
    return 0x100 | b;
}

/* ── Keypad lock: code 2-7-1-8, locked out after three wrong codes ──── */
/* Result: 0 idle, 1 unlocked by this key, 2 locked out.                 */

struct pin_ctx { unsigned pos; unsigned failures; };

unsigned pin_step(struct pin_ctx *c, unsigned key)
{
    if (c->failures >= 3)
        return 2;
    unsigned expect = c->pos == 0 ? 2 : c->pos == 1 ? 7 : c->pos == 2 ? 1 : 8;
    if (key != expect) {
        c->pos = 0;
        c->failures += 1;
        return c->failures >= 3 ? 2 : 0;
    }
    if (c->pos == 3) {
        c->pos = 0;
        c->failures = 0;
        return 1;
    }
    c->pos += 1;
    return 0;
}

/* ── Debouncer: the output follows the input after 4 equal samples ──── */

struct debounce_ctx { unsigned out; unsigned count; };

unsigned debounce_step(struct debounce_ctx *c, unsigned in)
{
    unsigned level = in & 1;
    if (level == c->out) {
        c->count = 0;
    } else if (++c->count >= 4) {
        c->out = level;
        c->count = 0;
    }
    return c->out;
}

/* ── Vending machine: 25-cent item; nickel=1, dime=2, quarter=3 ─────── */
/* Result: 0 waiting, 1 + change in nickels when an item is dispensed.   */

struct vend_ctx { unsigned credit; };

unsigned vending_step(struct vend_ctx *c, unsigned coin)
{
    unsigned value = coin == 1 ? 5 : coin == 2 ? 10 : coin == 3 ? 25 : 0;
    unsigned credit = c->credit + value;
    if (credit >= 25) {
        c->credit = 0;
        return 1 + (credit - 25) / 5;
    }
    c->credit = credit;
    return 0;
}

/* ── Traffic light: green 3 ticks, yellow 1, red 3; a pedestrian       */
/* request (in != 0) cuts green short to one more tick.                  */
/* Result: the light shown, 0 green / 1 yellow / 2 red.                  */

struct traffic_ctx { unsigned phase; unsigned ticks; };

unsigned traffic_step(struct traffic_ctx *c, unsigned in)
{
    unsigned limit = c->phase == 0 ? 3 : c->phase == 1 ? 1 : 3;
    c->ticks += 1;
    if (c->phase == 0 && in != 0 && c->ticks < limit)
        c->ticks = limit - 1;
    if (c->ticks >= limit) {
        c->ticks = 0;
        c->phase = c->phase == 2 ? 0 : c->phase + 1;
    }
    return c->phase;
}

/* ── Base64 decoding (RFC 4648): the phase is control, the bits are    */
/* data. Result: 1 when a byte completed (stored in ctx->byte), 0 when  */
/* not, 2 on a character outside the alphabet (and the phase resets).   */

struct base64_ctx { unsigned phase; unsigned bits; unsigned byte; };

unsigned base64_step(struct base64_ctx *c, unsigned in)
{
    unsigned ch = in & 0xff;
    unsigned v;
    if (ch >= 'A' && ch <= 'Z') v = ch - 'A';
    else if (ch >= 'a' && ch <= 'z') v = ch - 'a' + 26;
    else if (ch >= '0' && ch <= '9') v = ch - '0' + 52;
    else if (ch == '+') v = 62;
    else if (ch == '/') v = 63;
    else { c->phase = 0; return 2; }
    unsigned produced = 0;
    switch (c->phase) {
    case 0: c->bits = v; break;
    case 1: c->byte = (c->bits << 2) | (v >> 4); c->bits = v & 0xf; produced = 1; break;
    case 2: c->byte = (c->bits << 4) | (v >> 2); c->bits = v & 0x3; produced = 1; break;
    default: c->byte = (c->bits << 6) | v; produced = 1; break;
    }
    c->phase = (c->phase + 1) & 3;
    return produced;
}

/* ── Serial frame with an XOR checksum: STX len payload... checksum.   */
/* Whether a frame is valid depends on every payload byte, so the       */
/* minimal machine is as large as the checksum's range.                  */

struct frame_ctx { unsigned state; unsigned remaining; unsigned sum; };

unsigned frame_step(struct frame_ctx *c, unsigned in)
{
    unsigned b = in & 0xff;
    switch (c->state) {
    case 0:
        if (b == 0x02) c->state = 1;
        return 0;
    case 1:
        if (b == 0 || b > 8) { c->state = 0; return 2; }
        c->remaining = b;
        c->sum = 0;
        c->state = 2;
        return 0;
    case 2:
        c->sum ^= b;
        if (--c->remaining == 0) c->state = 3;
        return 0;
    default:
        c->state = 0;
        return b == c->sum ? 1 : 2;
    }
}
