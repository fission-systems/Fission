/*
 * x86-64 compiler-option matrix sample for SLEIGH/raw P-code coverage.
 *
 * The source is intentionally freestanding-friendly and libc-free so it can be
 * built as PE executables and ELF/COFF/Mach-O objects across local toolchains.
 */
#include <stdint.h>

#if defined(_WIN32)
#define FISSION_EXPORT __declspec(dllexport)
#else
#define FISSION_EXPORT __attribute__((visibility("default")))
#endif

#if defined(__GNUC__) || defined(__clang__)
#define FISSION_NOINLINE __attribute__((noinline))
#else
#define FISSION_NOINLINE
#endif

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;
typedef int32_t i32;

struct MatrixNode {
    u32 tag;
    i32 delta;
    u64 payload;
    struct MatrixNode *next;
};

volatile u64 fission_option_sink = 0;
static u32 global_table[16] = {
    0x13579bdfu, 0x2468ace0u, 0x10203040u, 0x55667788u,
    0xdeadbeefu, 0xcafebabeu, 0x0badf00du, 0x31415926u,
    0x27182818u, 0x11223344u, 0xa5a5a5a5u, 0x5a5a5a5au,
    0x01010101u, 0x80808080u, 0xffffffffu, 0x00010001u,
};

FISSION_NOINLINE static u64 mix64(u64 x, u64 y) {
    x ^= y + 0x9e3779b97f4a7c15ULL + (x << 6) + (x >> 2);
    x = (x << 13) | (x >> 51);
    return x * 0xff51afd7ed558ccdULL;
}

FISSION_NOINLINE static u32 branch_flags(u32 a, i32 b) {
    u32 out = 0;
    if ((i32)a <= b) {
        out ^= a + (u32)b;
    } else {
        out ^= a - (u32)b;
    }
    if ((a & 7u) == 3u) {
        out += 0x55u;
    }
    if ((i32)(a ^ 0x80000000u) < b) {
        out ^= 0xaa00aa00u;
    }
    return out;
}

FISSION_NOINLINE static u64 pointer_walk(struct MatrixNode *node, u32 rounds) {
    u64 acc = 0x123456789abcdef0ULL;
    for (u32 i = 0; i < rounds; ++i) {
        if (node == 0) {
            break;
        }
        acc = mix64(acc ^ node->payload, (u64)node->tag + (u32)node->delta);
        node = node->next;
    }
    return acc;
}

FISSION_NOINLINE static u32 table_switch(u32 selector, u32 seed) {
    switch (selector & 7u) {
    case 0:
        return seed + global_table[selector & 15u];
    case 1:
        return seed - 7u;
    case 2:
        return seed * 33u;
    case 3:
        return seed ^ 0x7f4a7c15u;
    case 4:
        return (seed << 5) | (seed >> 27);
    case 5:
        return (seed >> 3) ^ (selector * 17u);
    case 6:
        return branch_flags(seed, -20);
    default:
        return seed + 0x80000001u;
    }
}

FISSION_NOINLINE static u64 stack_pressure(u32 seed) {
    u64 local[8];
    for (u32 i = 0; i < 8; ++i) {
        local[i] = mix64((u64)seed + i, (u64)global_table[(seed + i) & 15u]);
    }

    u64 acc = 0;
    for (u32 i = 0; i < 8; ++i) {
        acc ^= local[(i * 3u) & 7u] + i;
    }
    return acc;
}

typedef u32 (*matrix_op)(u32, u32);

FISSION_NOINLINE static u32 op_add(u32 a, u32 b) {
    return a + b;
}

FISSION_NOINLINE static u32 op_xor(u32 a, u32 b) {
    return a ^ b;
}

FISSION_NOINLINE static u32 indirect_call(u32 a, u32 b, u32 selector) {
    static matrix_op ops[2] = {op_add, op_xor};
    return ops[selector & 1u](a, b);
}

FISSION_EXPORT FISSION_NOINLINE u64 fission_option_matrix(u32 seed) {
    struct MatrixNode nodes[3];
    nodes[0].tag = seed;
    nodes[0].delta = -7;
    nodes[0].payload = mix64(seed, 0x1111222233334444ULL);
    nodes[0].next = &nodes[1];
    nodes[1].tag = seed + 1u;
    nodes[1].delta = 13;
    nodes[1].payload = mix64(seed, 0x5555666677778888ULL);
    nodes[1].next = &nodes[2];
    nodes[2].tag = seed + 2u;
    nodes[2].delta = -20;
    nodes[2].payload = mix64(seed, 0x9999aaaabbbbccccULL);
    nodes[2].next = 0;

    u64 acc = pointer_walk(nodes, 3);
    for (u32 i = 0; i < 12; ++i) {
        u32 selected = table_switch(seed + i, (u32)acc);
        acc ^= indirect_call(selected, branch_flags(seed + i, (i32)i - 7), i);
        acc += stack_pressure(seed ^ i);
    }

    fission_option_sink = acc;
    return acc;
}

int main(void) {
    return (int)(fission_option_matrix(0x1234u) & 0xffu);
}
