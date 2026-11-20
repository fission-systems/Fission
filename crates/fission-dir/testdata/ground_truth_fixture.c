/*
 * Functions for fission-dir's ground-truth tests. Built into
 * `ground_truth_fixture.elf`, which is checked in next to this file:
 *
 *   clang --target=x86_64-linux-gnu -O1 -fno-inline -fstack-protector-all -fno-pic \
 *     -nostdlib -static -fuse-ld=lld -Wl,--build-id=none -Wl,-e,_start \
 *     -o ground_truth_fixture.elf ground_truth_fixture.c
 *
 * Each function exercises one part of the shared initial state: memory read
 * through a pointer argument, a pointer chain, a bool parameter, a narrow
 * parameter the ABI extends, a writable global, and the stack canary read
 * through the FS segment base.
 */
#include <stdbool.h>

/* Only `guarded_sum` keeps the canary `-fstack-protector-all` adds. */
#define PLAIN __attribute__((no_stack_protector))

struct node {
    struct node *next;
    int value;
};

long counter = 7;

PLAIN unsigned sum_bytes(const unsigned char *p, unsigned n)
{
    unsigned total = 0;
    for (unsigned i = 0; i < (n & 15); i++)
        total += p[i];
    return total;
}

PLAIN int first_plus_fourth(const int *p)
{
    return p[0] + p[3];
}

PLAIN int list_len(const struct node *p)
{
    int n = 0;
    while (p && n < 64) {
        n++;
        p = p->next;
    }
    return n;
}

PLAIN int pick(bool b, int x)
{
    return b ? x : -x;
}

PLAIN int widen(signed char c)
{
    return c * 3;
}

PLAIN long read_counter(void)
{
    return counter + 1;
}

int guarded_sum(int x)
{
    volatile int a[4] = {x, x + 1, x + 2, x + 3};
    return a[0] + a[3];
}

/* Memory (slice 2): a local array indexed by a run's value, a local whose
 * address a call receives, and string literals. */
PLAIN int pick_local(unsigned i, int a, int b)
{
    volatile int v[4];
    v[0] = a;
    v[1] = b;
    v[2] = a + b;
    v[3] = a ^ b;
    return v[i & 3];
}

PLAIN __attribute__((noinline, weak)) void bump(int *p)
{
    *p += 1;
}

PLAIN int pass_local(int a)
{
    int x = a;
    bump(&x);
    return x + 1;
}

PLAIN int give_local(int a)
{
    int x = a * 3;
    bump(&x);
    return a + 1;
}

PLAIN __attribute__((noinline)) long take_text(const char *s)
{
    return (long)s;
}

PLAIN long pass_literal(int i)
{
    return take_text(i ? "hello, fixture" : "goodbye, fixture");
}

PLAIN int literal_char(int i)
{
    const char *s = "fixture text";
    return s[i & 7];
}

/* Calls (slice 3): a global read on both sides of a call, which a stub that
 * rewrites writable data catches if the decompiled code keeps the first
 * read; and a call whose caller-saved registers the stub overwrites. */
long g_value = 5;

PLAIN __attribute__((noinline, weak)) void touch(void)
{
}

PLAIN __attribute__((noinline, weak)) void note(long x)
{
    (void)x;
}

PLAIN long reload_after_call(void)
{
    long before = g_value;
    note(before);
    return before * 3 + g_value;
}

PLAIN long call_then_return(long a)
{
    touch();
    return a;
}

/* The canary check's failure path; nothing here links a libc. */
PLAIN void __stack_chk_fail(void)
{
    for (;;)
        ;
}

PLAIN void _start(void)
{
    for (;;)
        ;
}
