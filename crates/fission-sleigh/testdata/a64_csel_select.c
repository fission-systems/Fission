/* Built with: zig cc -target aarch64-linux-gnu -O1 -shared -nostdlib -o a64_csel_select.so a64_csel_select.c */
int sel_zero(int n, int m) { return n < 0 ? 0 : m; }
int sel_two(int n, int a, int b) { return n < 0 ? a : b; }
int sel_inc(int n, int a) { return n < 0 ? a : a + 1; }
long sel_cst(long n) { return n > 10 ? 7 : 99; }
/* cinc into x0, which also carries the pointer parameter on entry. */
unsigned long count_char(const char *s, char c) { unsigned long n = 0; while (*s) { if (*s == c) n++; s++; } return n; }
