typedef unsigned char u8;
typedef unsigned int u32;
typedef unsigned long long u64;

volatile u64 llvm_smoke_sink = 0;

static u32 llvm_mix(u32 a, u32 b) {
    u32 x = (a ^ b) + 0x13579bdu;
    for (u32 i = 0; i < 4; ++i) {
        x = (x << 3) ^ (x >> 2) ^ (b + i * 17u);
    }
    return x;
}

u64 llvm_smoke(u32 seed) {
    u64 acc = 0x1020304050607080ULL ^ (u64)seed;
    for (u32 i = 0; i < 8; ++i) {
        u32 part = llvm_mix(seed + i, (u32)(acc >> (i & 7u)));
        acc ^= ((u64)part << ((i & 3u) * 8u));
        acc += (u64)(seed * (i + 1u));
    }
    llvm_smoke_sink = acc;
    return acc;
}
