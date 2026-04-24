typedef unsigned int u32;

volatile u32 math_sink = 0;

u32 run_mathematics(u32 seed) {
    u32 a = seed;
    u32 b = seed ^ 0xDEADBEEF;
    
    // bitwise ops
    u32 c = (a << 5) | (b >> 27);
    u32 d = (a & 0xFF00FF00) ^ (b & 0x00FF00FF);
    
    // arithmetic ops
    u32 e = a * 1337 + b / (a % 10 + 1);
    
    // mixed
    u32 f = (c + d) * (e ^ a);
    
    math_sink = f;
    return f;
}
