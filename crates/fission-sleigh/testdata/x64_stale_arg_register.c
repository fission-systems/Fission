/* A one-parameter callee called right after another call that set the next
   argument register. The leftover value is not an argument of the second call.
   Built with
     zig cc -target x86_64-linux-gnu -O0 -fno-pic -nostdlib -static \
       -fno-stack-protector -fno-sanitize=undefined -Wl,-e,caller -o x.elf this.c
   then `llvm-strip --strip-all`, so nothing names the callees. */
__attribute__((noinline)) long take_two(long *p, int n) { return *p + n; }
__attribute__((noinline)) long take_one(long *p) { return *p; }

long caller(long *a, int n) {
    long r = take_two(a, n);
    return r + take_one(a);
}
