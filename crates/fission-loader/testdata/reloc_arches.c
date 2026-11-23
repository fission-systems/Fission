/* Relocatable objects for the ELF loader's per-architecture relocation
 * tests (`loader::elf` tests: `*_relocatable_*`). From this directory:
 *
 *   F="-c -O1 -fno-asynchronous-unwind-tables -fno-unwind-tables -ffreestanding -fno-pic"
 *   clang $F --target=armeb-none-eabi -march=armv4t -marm reloc_arches.c -o armv4t_be_reloc_arches.o
 *   clang $F --target=thumbv8m.main-none-eabi        reloc_arches.c -o armv8m_reloc_arches.o
 *   clang $F --target=loongarch64-unknown-elf        reloc_arches.c -o loongarch64_reloc_arches.o
 *   clang $F --target=loongarch32-unknown-elf        reloc_arches.c -o loongarch32_reloc_arches.o
 *   clang $F --target=powerpc64-unknown-linux-gnu -mabi=elfv1 reloc_arches.c -o ppc64_be_reloc_arches.o
 *   clang $F --target=x86_64-unknown-elf             reloc_arches.c -o x64_reloc_arches.o
 *
 * Built with Homebrew clang 22.1.8. The tests pin byte offsets, so a rebuild
 * with another compiler means re-deriving them (llvm-readelf -r lists the
 * relocation sites).
 */
int math_sink;

int op_add(int a, int b) { return a + b; }

int recursive_fib(int n)
{
    if (n < 2)
        return n;
    return recursive_fib(n - 1) + recursive_fib(n - 2);
}

int count_down(int n)
{
    int total = 0;
    while (n > 0)
        total += n--;
    return total;
}

void store_sink(int v) { math_sink = v; }

/* Arms with different side effects, so the switch lowers to a jump table
 * rather than a lookup table: on x86-64 (non-PIC) that is a .rodata table
 * of absolute addresses, one R_X86_64_64 each. */
void dispatch(int v)
{
    switch (v) {
    case 0: math_sink += 3; break;
    case 1: math_sink ^= 0x55; break;
    case 2: math_sink = op_add(math_sink, 9); break;
    case 3: math_sink = recursive_fib(math_sink & 7); break;
    case 4: math_sink -= 17; break;
    default: math_sink = 0; break;
    }
}
