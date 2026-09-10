// Throughput of Unicorn (QEMU's TCG as a library) on the exact byte sequences
// fission-emulator's throughput_bench runs. Same code, same instruction
// counts, same two-point method so translation falls out of the difference.
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unicorn/unicorn.h>

#define CODE_BASE 0x10000000ULL
#define STACK_TOP 0x20000000ULL

static const unsigned char LOOP_CODE[] = {0x83,0xC0,0x01, 0x83,0xE9,0x01, 0xEB,0xF8};
static const unsigned char MEM_CODE[]  = {0x48,0x89,0x45,0x00, 0x48,0x8B,0x45,0x00,
                                          0x83,0xE9,0x01, 0xEB,0xF3};

static double run(const unsigned char *code, size_t len, uint64_t count) {
    uc_engine *uc;
    if (uc_open(UC_ARCH_X86, UC_MODE_64, &uc) != UC_ERR_OK) return -1;
    uc_mem_map(uc, CODE_BASE, 0x1000, UC_PROT_ALL);
    uc_mem_map(uc, STACK_TOP - 0x10000, 0x10000, UC_PROT_ALL);
    uc_mem_write(uc, CODE_BASE, code, len);
    uint64_t rcx = count, rbp = STACK_TOP - 0x1000, rsp = STACK_TOP - 0x2000, rax = 0;
    uc_reg_write(uc, UC_X86_REG_RCX, &rcx);
    uc_reg_write(uc, UC_X86_REG_RBP, &rbp);
    uc_reg_write(uc, UC_X86_REG_RSP, &rsp);
    uc_reg_write(uc, UC_X86_REG_RAX, &rax);

    struct timespec a, b;
    clock_gettime(CLOCK_MONOTONIC, &a);
    uc_err e = uc_emu_start(uc, CODE_BASE, 0, 0, count);
    clock_gettime(CLOCK_MONOTONIC, &b);
    if (e != UC_ERR_OK) fprintf(stderr, "  (uc: %s)\n", uc_strerror(e));
    uc_close(uc);
    return (b.tv_sec - a.tv_sec) + (b.tv_nsec - a.tv_nsec) / 1e9;
}

int main(void) {
    struct { const char *name; const unsigned char *code; size_t len; } w[] = {
        {"register loop", LOOP_CODE, sizeof LOOP_CODE},
        {"memory loop",   MEM_CODE,  sizeof MEM_CODE},
    };
    for (int i = 0; i < 2; i++) {
        uint64_t n1 = 2000000, n2 = 20000000;
        double t1 = run(w[i].code, w[i].len, n1);
        double t2 = run(w[i].code, w[i].len, n2);
        printf("%-16s %llu in %.3fs, %llu in %.3fs  ->  marginal %.2fM inst/s\n",
               w[i].name, (unsigned long long)n1, t1, (unsigned long long)n2, t2,
               (double)(n2 - n1) / (t2 - t1) / 1e6);
    }
    return 0;
}
