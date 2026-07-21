/* Tiny pure-arithmetic fixture for fission-dir's DIR/HIR end-to-end test:
 * a real compiled function (not hand-built p-code) with a genuine
 * conditional-return diamond and no memory/call touches, so it's fully
 * within Phase 1 interp's supported subset.
 *
 * Rebuild:
 *   zig cc -target x86_64-linux-musl -O0 -static -o max2.elf src_max2.c
 */
int max2(int a, int b) {
    if (a > b) {
        return a;
    }
    return b;
}

int main(void) {
    return max2(3, 5);
}
