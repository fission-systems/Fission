/* Test fixture: three ARM and three Thumb functions in one ELF32 image.
 *
 *   clang -target armv7-none-eabi -mfloat-abi=soft -O1 -c arm_mixed_mode.c
 *   ld.lld -static -nostdlib -e _start --image-base=0x10000 arm_mixed_mode.o -o arm_mixed_mode.elf
 *   llvm-strip --strip-all -o arm_mixed_mode_stripped.elf arm_mixed_mode.elf
 */
volatile int sink;

__attribute__((noinline, target("arm"))) int arm_a(int x, int y) { int s = 0; for (int i = 0; i < x; i++) s += i * y + sink; return s; }
__attribute__((noinline, target("thumb"))) int thumb_a(int x, int y) { int s = 0; for (int i = 0; i < x; i++) s += i ^ y; sink = s; return s; }
__attribute__((noinline, target("arm"))) int arm_b(int x) { if (x > 3) { sink = x; return x * 3; } return sink + x; }
__attribute__((noinline, target("thumb"))) int thumb_b(int x) { if (x > 5) { sink = x * 7; return x - 1; } return sink - x; }
__attribute__((noinline, target("thumb"))) int thumb_c(int x, int y) { return thumb_a(x, y) + thumb_b(y) + sink; }
__attribute__((noinline, target("arm"))) int arm_c(int x, int y) { return arm_a(x, y) + arm_b(y) + thumb_c(x, y); }

__attribute__((target("arm"))) void _start(void) { sink = arm_c(4, 5) + thumb_c(2, 3); for (;;) {} }
