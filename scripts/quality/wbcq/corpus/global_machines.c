/*
 * Step-function state machines whose state lives in global memory, for the
 * WBCQ P4 evaluation (RFC #1093, P4 step 3: globals).
 *
 * Same shape as `state_machines.c` -- `unsigned step(struct ... *ctx,
 * unsigned in)` -- but the machine keeps its state, or its tables, in
 * globals the step reads and writes at their absolute addresses: a command
 * parser over a constant class table, an alternating-bit receiver, and a
 * delay line behind a global pointer. The context is there because the
 * boundary has one; these machines leave it alone.
 *
 * The native ground truth (`ground_truth.py`) treats the manifest's
 * `globals` as part of the state, restoring and reading them around every
 * step, as the engine's oracle does.
 */

struct unused_ctx { unsigned pad; };

/* ── Command parser over a constant class table ────────────────────────────
 * g_kind[byte]: 1 a letter (the command name), 2 a digit (an argument),
 * 3 ';' (end of command), 0 anything else (an error). g_mode: 0 idle,
 * 1 in a name, 2 in an argument, 3 error until ';'. ';' returns what ended
 * (1 a bare command, 2 a command with an argument, 3 an error). */

static const unsigned char g_kind[256] = {
    ['A'] = 1, ['B'] = 1, ['C'] = 1, ['D'] = 1, ['E'] = 1, ['F'] = 1, ['G'] = 1,
    ['H'] = 1, ['I'] = 1, ['J'] = 1, ['K'] = 1, ['L'] = 1, ['M'] = 1, ['N'] = 1,
    ['O'] = 1, ['P'] = 1, ['Q'] = 1, ['R'] = 1, ['S'] = 1, ['T'] = 1, ['U'] = 1,
    ['V'] = 1, ['W'] = 1, ['X'] = 1, ['Y'] = 1, ['Z'] = 1,
    ['0'] = 2, ['1'] = 2, ['2'] = 2, ['3'] = 2, ['4'] = 2, ['5'] = 2, ['6'] = 2,
    ['7'] = 2, ['8'] = 2, ['9'] = 2,
    [';'] = 3,
};
unsigned g_mode;

unsigned gcmd_step(struct unused_ctx *c, unsigned in)
{
    (void)c;
    unsigned kind = g_kind[in & 0xff];
    unsigned mode = g_mode;
    if (kind == 3) {
        g_mode = 0;
        return mode;
    }
    if (mode == 3 || kind == 0) {
        g_mode = 3;
    } else if (kind == 1) {
        g_mode = mode == 2 ? 3 : 1;
    } else if (mode != 0) {
        g_mode = 2;
    } else {
        g_mode = 3;
    }
    return 0;
}

/* ── Alternating-bit receiver ──────────────────────────────────────────────
 * Bit 7 of a frame is its sequence bit. A frame carrying the expected bit is
 * delivered (1) and the expectation flips; any other is a duplicate (2). */

unsigned g_expect;

unsigned abp_step(struct unused_ctx *c, unsigned in)
{
    (void)c;
    unsigned seq = (in >> 7) & 1;
    if (seq != g_expect)
        return 2;
    g_expect ^= 1;
    return 1;
}

/* ── Delay line behind a global pointer ────────────────────────────────────
 * g_ring points to four cells (g_cells); g_head is the next one. Each step
 * returns the bit stored four steps ago and stores the new one. */

unsigned char g_cells[4];
unsigned char *g_ring = g_cells;
unsigned g_head;

unsigned gring_step(struct unused_ctx *c, unsigned in)
{
    (void)c;
    unsigned out = g_ring[g_head];
    g_ring[g_head] = in & 1;
    g_head = (g_head + 1) & 3;
    return out;
}
