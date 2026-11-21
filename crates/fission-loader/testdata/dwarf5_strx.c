/* DWARF 5 fixture whose names are DW_FORM_strx* indices into
 * .debug_str_offsets, as clang emits them by default.
 *
 *   clang -target x86_64-linux-gnu -gdwarf-5 -O0 -ffreestanding -nostdlib \
 *       -fuse-ld=lld -static -Wl,--build-id=none -Wl,-e,entry \
 *       dwarf5_strx.c -o x64_dwarf5_strx.elf
 */
struct point {
    int horizontal;
    long vertical;
};

long manhattan(const struct point *where, int scale) {
    return (where->horizontal + where->vertical) * scale;
}

void entry(void) {
    struct point origin = {3, 4};
    volatile long sink = manhattan(&origin, 2);
    (void)sink;
    for (;;) {
    }
}
