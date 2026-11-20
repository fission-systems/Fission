/*
 * Frame-pointer functions for fission-dir's ground-truth tests. Built at -O0,
 * so every local lives in an `rbp` frame, into `ground_truth_fixture_o0.elf`:
 *
 *   clang --target=x86_64-linux-gnu -O0 -fno-stack-protector -fno-pic \
 *     -nostdlib -static -fuse-ld=lld -Wl,--build-id=none -Wl,-e,_start \
 *     -o ground_truth_fixture_o0.elf ground_truth_fixture_o0.c
 *
 * A local array whose address a call receives is placed by the decompiler in
 * a whole-frame backing counted from the steady-state stack pointer, while
 * scalar locals count from the frame pointer: one frame, two bases.
 */

__attribute__((noinline, weak)) long fill(char *buf, long n)
{
    return (long)buf + n;
}

int frame_buffer(int a)
{
    char buf[64];
    int k = a + 1;
    fill(buf, 63);
    return k;
}

void _start(void)
{
    for (;;)
        ;
}
