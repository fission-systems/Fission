/* ARM inline jump tables for the Rust-Sleigh decode tests
 * (`pipeline::rust_sleigh::decode` tests). Big-endian ARM-mode objects; the
 * switch lowers to `ldr pc, [r2, r0, lsl #2]` with its four-word table placed
 * inline in .text, and the two range checks return conditionally
 * (`movmi pc, lr` on ARMv4, `bxmi lr` on ARMv5). From this directory:
 *
 *   F="-c -O2 -fno-asynchronous-unwind-tables -fno-unwind-tables -ffreestanding -fno-pic"
 *   clang $F --target=armeb-none-eabi -march=armv4 -marm arm_switch.c -o armv4_be_switch.o
 *   clang $F --target=armeb-none-eabi -march=armv5te -marm arm_switch.c -o armv5_be_switch.o
 *
 * Built with Homebrew clang 22.1.8; the tests pin the addresses `llvm-objdump
 * -d` shows (table at .text+0x24..0x34, cases at 0x34/0x44/0x54/0x60).
 */
volatile int sink;

int test_switch(int v)
{
    if (v < 0)
        return 0;
    switch (v) {
    case 0: sink = 10; break;
    case 1: sink += 20; break;
    case 2: sink ^= 30; break;
    case 3: sink -= 40; break;
    default: return 1;
    }
    return sink;
}
