/* Repository-owned indirect-dispatch fixture (ADR 0020, issue #1368).
 *
 * Both entries of `ops` are feasible at both dispatch sites; argc selects one.
 * One run therefore observes exactly one of two targets. Libc-free, and only
 * ever executed inside Fission's emulator.
 */
typedef long (*op_t)(long);

__attribute__((noinline)) long op_increment(long x) { return x + 1; }
__attribute__((noinline)) long op_triple(long x) { return x * 3; }

static volatile const op_t ops[2] = {op_increment, op_triple};

/* An indirect call: the addition keeps it from becoming a tail call. */
__attribute__((noinline)) long dispatch_call(unsigned long selector, long x) {
  return ops[selector & 1](x) + 7;
}

/* An indirect jump: the same choice as a tail call. */
__attribute__((noinline)) long dispatch_jump(unsigned long selector, long x) {
  return ops[selector & 1](x);
}

__attribute__((noreturn, used)) void fixture_main(unsigned long argc) {
  unsigned long selector = argc - 1;
  long status = dispatch_call(selector, 10) + dispatch_jump(selector, 20);
#if defined(__x86_64__)
  __asm__ volatile("syscall" : : "a"(60L), "D"(status) : "rcx", "r11", "memory");
#elif defined(__aarch64__)
  register long x8 __asm__("x8") = 93;
  register long x0 __asm__("x0") = status;
  __asm__ volatile("svc #0" : : "r"(x8), "r"(x0) : "memory");
#endif
  __builtin_unreachable();
}

#if defined(__x86_64__)
__asm__(".text\n.globl _start\n_start:\n"
        "  movq (%rsp), %rdi\n  andq $-16, %rsp\n  callq fixture_main\n");
#elif defined(__aarch64__)
__asm__(".text\n.globl _start\n_start:\n"
        "  ldr x0, [sp]\n  bl fixture_main\n");
#endif
