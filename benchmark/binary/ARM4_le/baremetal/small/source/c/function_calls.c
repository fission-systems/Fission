typedef unsigned int u32;

volatile u32 func_sink = 0;

u32 recursive_fib(u32 n) {
    if (n <= 1) return n;
    return recursive_fib(n - 1) + recursive_fib(n - 2);
}

u32 op_add(u32 a, u32 b) { return a + b; }
u32 op_sub(u32 a, u32 b) { return a >= b ? a - b : b - a; }
u32 op_mul(u32 a, u32 b) { return a * b; }

typedef u32 (*op_func)(u32, u32);

u32 apply_op(op_func f, u32 a, u32 b) {
    return f(a, b);
}

void run_function_calls(u32 seed) {
    op_func funcs[3] = {op_add, op_sub, op_mul};
    u32 result = 0;
    
    result += recursive_fib(seed % 10);
    
    for (int i = 0; i < 5; ++i) {
        result = apply_op(funcs[i % 3], result, seed + i);
    }
    
    func_sink = result;
}
