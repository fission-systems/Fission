/*
 * The environment of `call_machines.c` for the native ground truth: every
 * callee records the call (which function, its argument) and returns the
 * next value the explorer scripted, the way the engine's stubs treat a
 * call's return as an input. `abort` records itself and unwinds to
 * `wbcq_run`, which reports that the step did not return.
 *
 * Compiled into one shared library with `call_machines.c`, so the step
 * functions' calls bind to these definitions and not the C library's.
 */

#include <setjmp.h>

#define WBCQ_MAX_CALLS 8

enum { FN_ISDIGIT = 1, FN_ISALPHA, FN_PUTCHAR, FN_WBCQ_LOG, FN_ABORT };

long long wbcq_returns[WBCQ_MAX_CALLS];
int wbcq_returns_len;
int wbcq_calls;
int wbcq_call_fn[WBCQ_MAX_CALLS];
long long wbcq_call_arg[WBCQ_MAX_CALLS];

static jmp_buf wbcq_halt;

static long long wbcq_call(int fn, long long arg)
{
    int n = wbcq_calls++;
    if (n < WBCQ_MAX_CALLS) {
        wbcq_call_fn[n] = fn;
        wbcq_call_arg[n] = arg;
    }
    return n < wbcq_returns_len ? wbcq_returns[n] : 0;
}

int isdigit(int c) { return (int)wbcq_call(FN_ISDIGIT, c); }
int isalpha(int c) { return (int)wbcq_call(FN_ISALPHA, c); }
int putchar(int c) { return (int)wbcq_call(FN_PUTCHAR, c); }
void wbcq_log(unsigned event) { (void)wbcq_call(FN_WBCQ_LOG, event); }

__attribute__((noreturn)) void abort(void)
{
    (void)wbcq_call(FN_ABORT, 0);
    longjmp(wbcq_halt, 1);
}

typedef unsigned (*wbcq_step)(void *ctx, unsigned in);

/* Run one step; 0 when it returned (its result in *result), 1 when it
 * ended in a call that does not return. */
int wbcq_run(wbcq_step step, void *ctx, unsigned in, unsigned *result)
{
    wbcq_calls = 0;
    if (setjmp(wbcq_halt))
        return 1;
    *result = step(ctx, in);
    return 0;
}
