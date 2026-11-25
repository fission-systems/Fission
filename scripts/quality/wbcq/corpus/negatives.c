/*
 * Negative cases for the WBCQ P3 evaluation (RFC #1093, section 11): code
 * a behavioural quotient cannot summarize. The engine must say so -- a
 * partial frontier or unsupported semantics -- and never return a small,
 * wrong machine.
 */

/* One SHA-256 compression round (FIPS 180-4, 6.2.2 step 3) with the     */
/* round constant fixed and the message word as input. The result is a  */
/* byte of the new state: every state word reaches it.                  */

struct sha_ctx { unsigned a, b, c, d, e, f, g, h; };

static unsigned rotr(unsigned x, unsigned n) { return (x >> n) | (x << (32 - n)); }

unsigned sha256_round_step(struct sha_ctx *s, unsigned w)
{
    unsigned s1 = rotr(s->e, 6) ^ rotr(s->e, 11) ^ rotr(s->e, 25);
    unsigned ch = (s->e & s->f) ^ (~s->e & s->g);
    unsigned t1 = s->h + s1 + ch + 0x428a2f98u + w;
    unsigned s0 = rotr(s->a, 2) ^ rotr(s->a, 13) ^ rotr(s->a, 22);
    unsigned maj = (s->a & s->b) ^ (s->a & s->c) ^ (s->b & s->c);
    unsigned t2 = s0 + maj;
    s->h = s->g; s->g = s->f; s->f = s->e; s->e = s->d + t1;
    s->d = s->c; s->c = s->b; s->b = s->a; s->a = t1 + t2;
    return s->a & 0xff;
}

/* A linked-list walk: the next address comes from memory the step reads. */

struct node { struct node *next; unsigned value; };
struct list_ctx { struct node *cursor; unsigned total; };

unsigned list_walk_step(struct list_ctx *c, unsigned limit)
{
    unsigned n = 0;
    while (c->cursor && n < (limit & 7)) {
        c->total += c->cursor->value;
        c->cursor = c->cursor->next;
        n++;
    }
    return c->total & 1;
}

/* Effects outside the P-code: a time stamp and a system call. */

struct clock_ctx { unsigned last; };

unsigned clock_step(struct clock_ctx *c, unsigned in)
{
    unsigned long long t = __builtin_readcyclecounter();
    unsigned odd = (unsigned)(t ^ in) & 1;
    c->last = odd;
    return odd;
}

struct sys_ctx { unsigned calls; };

unsigned syscall_step(struct sys_ctx *c, unsigned in)
{
    long ret;
#if defined(__x86_64__)
    __asm__ volatile("syscall" : "=a"(ret) : "a"(39L) : "rcx", "r11", "memory"); /* getpid */
#elif defined(__aarch64__)
    register long x8 __asm__("x8") = 172; /* getpid */
    register long x0 __asm__("x0");
    __asm__ volatile("svc #0" : "=r"(x0) : "r"(x8) : "memory");
    ret = x0;
#else
    ret = 0;
#endif
    c->calls += 1;
    return (unsigned)(ret ^ in) & 1;
}
