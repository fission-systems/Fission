// SPDX-License-Identifier: Apache-2.0
/*
 * Owned semantic fixtures for Issue #1253 metamorphic decompilation tests.
 * Uses strictly defined unsigned integer arithmetic, bounded loops, and
 * initialized memory -- no undefined behavior or address-space assumptions
 * (except `addr_sensitive_tag`, which exists specifically as an inapplicable
 * layout-shift control).
 *
 * Reproducible build commands for checked-in ELFs (Homebrew LLVM + lld):
 *
 *   # x86-64 SysV variants:
 *   /opt/homebrew/opt/llvm/bin/clang --target=x86_64-linux-gnu -DFISSION_FREESTANDING \
 *     -O0 -fno-inline -fno-stack-protector -fno-pic -fno-vectorize -fno-slp-vectorize \
 *     -fno-unroll-loops -nostdlib -static -fuse-ld=/opt/homebrew/bin/ld.lld \
 *     -Wl,--build-id=none -Wl,-e,_start -o metamorphic_x64_o0.elf metamorphic_fixture.c
 *
 *   /opt/homebrew/opt/llvm/bin/clang --target=x86_64-linux-gnu -DFISSION_FREESTANDING \
 *     -O2 -fno-inline -fno-stack-protector -fno-pic -fno-vectorize -fno-slp-vectorize \
 *     -fno-unroll-loops -nostdlib -static -fuse-ld=/opt/homebrew/bin/ld.lld \
 *     -Wl,--build-id=none -Wl,-e,_start -o metamorphic_x64_o2.elf metamorphic_fixture.c
 *
 *   /opt/homebrew/opt/llvm/bin/clang --target=x86_64-linux-gnu -DFISSION_FREESTANDING \
 *     -O2 -DFISSION_LAYOUT_SHIFT=1 -fno-inline -fno-stack-protector -fno-pic \
 *     -fno-vectorize -fno-slp-vectorize -fno-unroll-loops -nostdlib -static \
 *     -fuse-ld=/opt/homebrew/bin/ld.lld -Wl,--build-id=none -Wl,-e,_start \
 *     -o metamorphic_x64_layout.elf metamorphic_fixture.c
 *
 *   # AArch64 SysV variants:
 *   /opt/homebrew/opt/llvm/bin/clang --target=aarch64-linux-gnu -DFISSION_FREESTANDING \
 *     -O0 -fno-inline -fno-stack-protector -fno-pic -fno-vectorize -fno-slp-vectorize \
 *     -fno-unroll-loops -nostdlib -static -fuse-ld=/opt/homebrew/bin/ld.lld \
 *     -Wl,--build-id=none -Wl,-e,_start -o metamorphic_aarch64_o0.elf metamorphic_fixture.c
 *
 *   /opt/homebrew/opt/llvm/bin/clang --target=aarch64-linux-gnu -DFISSION_FREESTANDING \
 *     -O2 -fno-inline -fno-stack-protector -fno-pic -fno-vectorize -fno-slp-vectorize \
 *     -fno-unroll-loops -nostdlib -static -fuse-ld=/opt/homebrew/bin/ld.lld \
 *     -Wl,--build-id=none -Wl,-e,_start -o metamorphic_aarch64_o2.elf metamorphic_fixture.c
 *
 *   /opt/homebrew/opt/llvm/bin/clang --target=aarch64-linux-gnu -DFISSION_FREESTANDING \
 *     -O2 -DFISSION_LAYOUT_SHIFT=1 -fno-inline -fno-stack-protector -fno-pic \
 *     -fno-vectorize -fno-slp-vectorize -fno-unroll-loops -nostdlib -static \
 *     -fuse-ld=/opt/homebrew/bin/ld.lld -Wl,--build-id=none -Wl,-e,_start \
 *     -o metamorphic_aarch64_layout.elf metamorphic_fixture.c
 */

#include <stdint.h>

#if defined(__clang__) || defined(__GNUC__)
#define NOINLINE __attribute__((noinline, used))
#else
#define NOINLINE
#endif

#ifdef FISSION_LAYOUT_SHIFT
static const uint64_t g_layout_pad_data[16] = {
    0x1111111122222222ULL, 0x3333333344444444ULL, 0x5555555566666666ULL,
    0x7777777788888888ULL, 0x99999999aaaaaaaaULL, 0xbbbbbbbbccccccccULL,
    0xddddddddeeeeeeeeULL, 0xffffffff01020304ULL, 0x05060708090a0b0cULL,
    0x0d0e0f1011121314ULL, 0x15161718191a1b1cULL, 0x1d1e1f2021222324ULL,
    0x25262728292a2b2cULL, 0x2d2e2f3031323334ULL, 0x35363738393a3b3cULL,
    0x3d3e3f4041424344ULL,
};

NOINLINE uint32_t layout_shift_anchor_pad(uint32_t x)
{
    uint32_t acc = x ^ 0xA5A55A5Au;
    for (uint32_t i = 0; i < 4u; i++) {
        acc = (acc * 1664525u) + (uint32_t)g_layout_pad_data[(x + i) & 15u];
    }
    return acc;
}
#endif

static const uint32_t g_anchor = 0x13572468u;

/*
 * Baseline positive fixture 1: conditional branch over defined 32-bit
 * unsigned arithmetic. Avoids sub-byte %dil / signed-right-shift lowering paths
 * tracked separately in `diag_narrow_param_mix`.
 */
NOINLINE uint32_t u32_branch_mix(uint32_t a, uint32_t b)
{
    uint32_t base = (a * 2654435761u) ^ (b + 0x9e3779b9u);
    if ((a ^ b) & 1u) {
        return ((base & 0x7fffu) << 2) + (b ^ 0x55u);
    } else {
        return (base + (a & 0x1ffu)) ^ 0x3cu;
    }
}

/*
 * Baseline positive fixture 2: bounded loop reading an initialized 4-byte
 * buffer (len bounded to [0, 3]). Compiled with -fno-unroll-loops so -O2
 * preserves a real loop instead of fixed-offset byte accesses.
 */
NOINLINE uint32_t u32_bounded_crc(const uint8_t *buf, uint32_t len)
{
    uint32_t bound = len & 3u;
    uint32_t acc = 0x811c9dc5u;
    for (uint32_t i = 0; i < bound; i++) {
        acc ^= (uint32_t)buf[i];
        acc *= 16777619u;
    }
    return acc;
}

/*
 * Baseline positive fixture 3: indexed byte read + update on initialized
 * 4-byte buffer, returning an unsigned mixed result.
 */
NOINLINE uint32_t u32_write_byte(uint8_t *buf, uint32_t key)
{
    uint32_t idx = key & 3u;
    uint8_t v = (uint8_t)(buf[idx] ^ (uint8_t)(key & 0xffu) ^ 0x5au);
    buf[idx] = v;
    return ((uint32_t)v) + (key * 3u + 1u);
}

/*
 * Diagnostic fixture 1: triggers -O2 sub-register narrowing (`movzbl %dil`)
 * + signed right-shift divergence in Fission HIR despite equivalent -O0/-O2
 * machine semantics.
 */
NOINLINE uint32_t diag_narrow_param_mix(uint32_t a, uint32_t b)
{
    uint32_t base = (a * 2654435761u) ^ (b + 0x9e3779b9u);
    if ((a ^ b) & 1u) {
        return (base >> 3) + (b ^ 0x55u);
    } else {
        return (base + (a & 0xffu)) ^ 0x3cu;
    }
}

/*
 * Diagnostic fixture 2: triggers `slot_1..slot_3` initializer desync
 * (`param_1` left un-renamed when param 0 renames to `ptr`).
 */
NOINLINE uint32_t diag_multi_slot_buf(uint8_t *buf, uint32_t key)
{
    uint8_t b0 = buf[0];
    uint8_t b1 = buf[1];
    uint8_t b2 = buf[2];
    uint8_t b3 = buf[3];
    buf[0] = (uint8_t)(b0 ^ (uint8_t)(key & 0xffu));
    buf[1] = (uint8_t)(b1 + (uint8_t)((key >> 8) & 0xffu) + 1u);
    buf[2] = (uint8_t)(b2 ^ b0 ^ 0x5au);
    buf[3] = (uint8_t)(b3 + (uint8_t)((key >> 16) & 0x1fu));
    return ((uint32_t)buf[0]) | (((uint32_t)buf[1]) << 8)
         | (((uint32_t)buf[2]) << 16) | (((uint32_t)buf[3]) << 24);
}

/*
 * Intentionally address-sensitive function: layout shifts change the address
 * of `g_anchor` / `buf`, so LayoutShift is inapplicable to this fixture.
 */
NOINLINE uint32_t addr_sensitive_tag(const uint8_t *buf)
{
    return (uint32_t)(((uintptr_t)&g_anchor) ^ ((uintptr_t)buf) ^ 0xa5a5a5a5u);
}

#ifdef FISSION_FREESTANDING
void _start(void)
{
    for (;;) {
    }
}
#else
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv)
{
    if (argc != 8) {
        return 2;
    }
    const char *fn = argv[1];
    uint32_t a0 = (uint32_t)strtoul(argv[2], NULL, 10);
    uint32_t a1 = (uint32_t)strtoul(argv[3], NULL, 10);
    uint8_t buf[4];
    for (int i = 0; i < 4; i++) {
        buf[i] = (uint8_t)strtoul(argv[4 + i], NULL, 10);
    }

    uint32_t ret = 0;
    if (strcmp(fn, "u32_branch_mix") == 0) {
        ret = u32_branch_mix(a0, a1);
    } else if (strcmp(fn, "u32_bounded_crc") == 0) {
        ret = u32_bounded_crc(buf, a0 & 3u);
    } else if (strcmp(fn, "u32_write_byte") == 0) {
        ret = u32_write_byte(buf, a0);
    } else if (strcmp(fn, "diag_narrow_param_mix") == 0) {
        ret = diag_narrow_param_mix(a0, a1);
    } else if (strcmp(fn, "diag_multi_slot_buf") == 0) {
        ret = diag_multi_slot_buf(buf, a0);
    } else if (strcmp(fn, "addr_sensitive_tag") == 0) {
        ret = addr_sensitive_tag(buf);
    } else {
        return 2;
    }

    printf("[%" PRIu32 ",[%u,%u,%u,%u]]\n", ret, buf[0], buf[1], buf[2], buf[3]);
    return 0;
}
#endif
