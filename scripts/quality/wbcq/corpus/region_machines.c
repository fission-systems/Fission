/*
 * Step-function state machines that read or write through a pointer the
 * context holds, for the WBCQ P4 evaluation (RFC #1093, P4 step 2:
 * data-computed addresses).
 *
 * Same shape as `state_machines.c` -- `unsigned step(struct ... *ctx,
 * unsigned in)`, context zeroed at the start -- except that some context
 * fields are pointers, and the machine's tables or buffer live behind them:
 * a table-driven lexer (a class table indexed by the input byte, a
 * transition table indexed by the state), a keyword matcher that reads its
 * keyword at the current match position, and a ring buffer written at its
 * head.
 *
 * What the pointed-to bytes hold is part of the environment, not of this
 * source: `region_tables.py` names them, the manifest says which field gets
 * which, and both the engine (`--region FIELD=HEX`) and the native ground
 * truth (`ground_truth.py`) are given the same bytes.
 */

/* ── Table-driven number lexer ─────────────────────────────────────────────
 * cls[byte] is the byte's class (0 other, 1 digit, 2 '.', 3 delimiter);
 * delta[state * 4 + class] is the next state, with bit 7 set when a number
 * token is accepted on that step. */

struct tdfa_ctx {
    const unsigned char *cls;
    const unsigned char *delta;
    unsigned state;
};

unsigned tdfa_step(struct tdfa_ctx *c, unsigned in)
{
    unsigned cls = c->cls[in & 0xff];
    unsigned char next = c->delta[c->state * 4 + cls];
    c->state = next & 0x7f;
    return next >> 7;
}

/* ── Keyword matcher ───────────────────────────────────────────────────────
 * word is a NUL-terminated keyword without a repeated first letter; the
 * step returns 1 when its last byte arrives. The keyword is read at the
 * match position, so the index is the machine's own state. */

struct kw_ctx {
    const unsigned char *word;
    unsigned matched;
};

unsigned kw_step(struct kw_ctx *c, unsigned in)
{
    unsigned ch = in & 0xff;
    if (ch == c->word[c->matched]) {
        c->matched++;
        if (c->word[c->matched] == 0) {
            c->matched = 0;
            return 1;
        }
        return 0;
    }
    c->matched = ch == c->word[0] ? 1 : 0;
    return 0;
}

/* ── Delay line over a ring buffer ─────────────────────────────────────────
 * ring holds the last four input bits; each step returns the one stored
 * four steps ago and stores the new one at the head. */

struct ring_ctx {
    unsigned char *ring;
    unsigned head;
};

unsigned ring_step(struct ring_ctx *c, unsigned in)
{
    unsigned out = c->ring[c->head];
    c->ring[c->head] = in & 1;
    c->head = (c->head + 1) & 3;
    return out;
}
