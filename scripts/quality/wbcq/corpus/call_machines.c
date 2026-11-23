/*
 * Step-function state machines that make calls, for the WBCQ P4 evaluation
 * (RFC #1093, P4 step 1: calls are effects, not refusals).
 *
 * Same shape as `state_machines.c` -- `unsigned step(struct ... *ctx,
 * unsigned in)`, context zeroed at the start -- but each one calls out: a
 * byte classifier, a logging call per transition, a call that does not
 * return, a tail call. The callees are C library functions (so the
 * signature library knows their prototypes) and one function no library
 * knows (`wbcq_log`).
 *
 * Neither side of the evaluation runs a callee. The engine stubs every call
 * and treats what it returns as an input the environment chooses;
 * `ground_truth.py` does the same natively (`call_env_native.c` stands in
 * for every callee, records the call and returns a scripted value). For the
 * corpus ELF, `call_env.c` supplies definitions only so the static link
 * resolves.
 */

int isdigit(int c);
int isalpha(int c);
int putchar(int c);
void wbcq_log(unsigned event);
__attribute__((noreturn)) void abort(void);

/* ── Lexer: classifies each byte with the C library's classifiers ───────── */

enum { LX_START, LX_NUMBER, LX_WORD };
enum { TOK_NONE, TOK_NUMBER_STARTS, TOK_WORD_STARTS, TOK_ENDS };

struct lex_ctx { unsigned state; };

unsigned lex_step(struct lex_ctx *c, unsigned in)
{
    int ch = (int)(in & 0xff);
    unsigned s = c->state;
    unsigned out = TOK_NONE;
    if (isdigit(ch)) {
        /* Digits continue a word (`x1`) but start a number elsewhere. */
        if (s != LX_WORD) {
            if (s != LX_NUMBER)
                out = TOK_NUMBER_STARTS;
            s = LX_NUMBER;
        }
    } else if (isalpha(ch)) {
        if (s != LX_WORD)
            out = TOK_WORD_STARTS;
        s = LX_WORD;
    } else {
        if (s != LX_START)
            out = TOK_ENDS;
        s = LX_START;
    }
    c->state = s;
    return out;
}

/* ── Garage door: one button, two limit switches; logs each change ──────── */

enum { DOOR_CLOSED, DOOR_OPENING, DOOR_OPEN, DOOR_CLOSING, DOOR_STOPPED };
enum { IN_BUTTON = 1, IN_TOP = 2, IN_BOTTOM = 3 };

struct door_ctx { unsigned state; unsigned last_dir; };

unsigned door_step(struct door_ctx *c, unsigned in)
{
    unsigned s = c->state, next = s;
    switch (s) {
    case DOOR_CLOSED:
        if (in == IN_BUTTON) next = DOOR_OPENING;
        break;
    case DOOR_OPENING:
        if (in == IN_TOP) next = DOOR_OPEN;
        else if (in == IN_BUTTON) next = DOOR_STOPPED;
        break;
    case DOOR_OPEN:
        if (in == IN_BUTTON) next = DOOR_CLOSING;
        break;
    case DOOR_CLOSING:
        if (in == IN_BOTTOM) next = DOOR_CLOSED;
        else if (in == IN_BUTTON) next = DOOR_STOPPED;
        break;
    default: /* DOOR_STOPPED: the button reverses the last motion */
        if (in == IN_BUTTON)
            next = c->last_dir == DOOR_OPENING ? DOOR_CLOSING : DOOR_OPENING;
        break;
    }
    if (next != s) {
        if (next == DOOR_OPENING || next == DOOR_CLOSING)
            c->last_dir = next;
        putchar('a' + (int)next);
    }
    c->state = next;
    return next == DOOR_OPEN; /* the lamp */
}

/* ── Coin turnstile: an application logger no library describes ────────── */

enum { TS_LOCKED, TS_UNLOCKED };
enum { TS_COIN, TS_PUSH };
enum { LOG_UNLOCK = 1, LOG_ALARM, LOG_LOCK, LOG_REFUND };

struct ts_ctx { unsigned state; };

unsigned turnstile_step(struct ts_ctx *c, unsigned in)
{
    if (c->state == TS_LOCKED) {
        if (in == TS_COIN) {
            c->state = TS_UNLOCKED;
            wbcq_log(LOG_UNLOCK);
            return 1;
        }
        if (in == TS_PUSH)
            wbcq_log(LOG_ALARM);
        return 0;
    }
    if (in == TS_PUSH) {
        c->state = TS_LOCKED;
        wbcq_log(LOG_LOCK);
        return 0;
    }
    if (in == TS_COIN)
        wbcq_log(LOG_REFUND);
    return 1;
}

/* ── Greeting handshake: a protocol violation aborts the server ─────────── */

struct hs_ctx { unsigned state; };

unsigned hs_step(struct hs_ctx *c, unsigned in)
{
    unsigned ch = in & 0xff;
    switch (c->state) {
    case 0:
        if (ch == 'H') {
            c->state = 1;
            return 0;
        }
        if (ch == 0)
            abort();
        return 0;
    case 1:
        if (ch == 'I') {
            c->state = 2;
            return 1;
        }
        if (ch == 0)
            abort();
        c->state = 0;
        return 0;
    default: /* greeted: echo */
        return ch;
    }
}

/* ── Parity reporter: the result is what the terminal call returned ─────── */

struct emit_ctx { unsigned parity; };

unsigned emit_step(struct emit_ctx *c, unsigned in)
{
    c->parity ^= in & 1;
    return (unsigned)putchar('0' + (int)c->parity);
}
