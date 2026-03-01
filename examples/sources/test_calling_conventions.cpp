/**
 * test_calling_conventions.cpp
 *
 * Tests decompiler calling convention detection:
 *  - cdecl / stdcall / fastcall / thiscall (x86 only via attributes)
 *  - System V AMD64 ABI (RDI, RSI, RDX, RCX, R8, R9)
 *  - Microsoft x64 ABI (RCX, RDX, R8, R9)
 *  - Variadic functions (va_list)
 *  - Functions with many parameters (stack spill)
 *  - Struct return (hidden pointer parameter)
 *  - Tail call optimization
 */
#include <cstdio>
#include <cstdlib>
#include <cstdarg>
#include <cstring>

// ---- Basic parameter passing ----
int one_param(int a) { return a + 1; }
int two_params(int a, int b) { return a + b; }
int three_params(int a, int b, int c) { return a + b + c; }
int four_params(int a, int b, int c, int d) { return a + b + c + d; }
int six_params(int a, int b, int c, int d, int e, int f) {
    return a + b + c + d + e + f;
}

// ---- Stack-spilled parameters (> 6 on SysV, > 4 on MSVC) ----
int eight_params(int a, int b, int c, int d, int e, int f, int g, int h) {
    return a + b + c + d + e + f + g + h;
}

int ten_params(int a, int b, int c, int d, int e, int f,
               int g, int h, int i, int j) {
    return a + b + c + d + e + f + g + h + i + j;
}

// ---- Mixed types in parameters ----
double mixed_params(int a, double b, int c, double d) {
    return (double)a * b + (double)c * d;
}

float float_params(float a, float b, float c, float d, float e) {
    return a + b + c + d + e;
}

// ---- Struct return (compiler uses hidden pointer param) ----
struct BigResult {
    int values[8];
    char name[32];
};

BigResult make_result(int seed) {
    BigResult r;
    for (int i = 0; i < 8; i++) r.values[i] = seed * (i + 1);
    snprintf(r.name, 32, "result_%d", seed);
    return r;
}

struct SmallResult {
    int a, b;
};

SmallResult make_small(int x) {
    SmallResult r;
    r.a = x;
    r.b = x * 2;
    return r;
}

// ---- Variadic function ----
int sum_variadic(int count, ...) {
    va_list args;
    va_start(args, count);
    int sum = 0;
    for (int i = 0; i < count; i++) {
        sum += va_arg(args, int);
    }
    va_end(args);
    return sum;
}

void log_message(const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    printf("[LOG] ");
    vprintf(fmt, args);
    printf("\n");
    va_end(args);
}

// ---- Function pointers ----
typedef int (*BinaryOp)(int, int);

int apply_op(BinaryOp op, int a, int b) {
    return op(a, b);
}

int add(int a, int b) { return a + b; }
int sub(int a, int b) { return a - b; }
int mul(int a, int b) { return a * b; }

int dispatch_op(int op_code, int a, int b) {
    BinaryOp ops[] = { add, sub, mul };
    if (op_code >= 0 && op_code < 3) {
        return apply_op(ops[op_code], a, b);
    }
    return 0;
}

// ---- Tail call optimization ----
int factorial_tail(int n, int acc) {
    if (n <= 1) return acc;
    return factorial_tail(n - 1, n * acc);  // Tail call
}

int factorial(int n) {
    return factorial_tail(n, 1);
}

// ---- Callback pattern ----
typedef void (*Callback)(void *ctx, int value);

void process_array(const int *arr, int len, Callback cb, void *ctx) {
    for (int i = 0; i < len; i++) {
        cb(ctx, arr[i]);
    }
}

struct SumCtx { int total; };

void sum_callback(void *ctx, int value) {
    ((SumCtx *)ctx)->total += value;
}

// ---- main as argc/argv test ----
int main(int argc, char **argv) {
    printf("one_param(5) = %d\n", one_param(5));
    printf("six_params = %d\n", six_params(1, 2, 3, 4, 5, 6));
    printf("eight_params = %d\n", eight_params(1, 2, 3, 4, 5, 6, 7, 8));
    printf("ten_params = %d\n", ten_params(1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
    printf("mixed_params = %f\n", mixed_params(3, 2.5, 4, 1.5));
    
    BigResult br = make_result(7);
    printf("big result: %s, [0]=%d [7]=%d\n", br.name, br.values[0], br.values[7]);
    
    SmallResult sr = make_small(42);
    printf("small result: %d, %d\n", sr.a, sr.b);
    
    printf("sum_variadic = %d\n", sum_variadic(5, 10, 20, 30, 40, 50));
    log_message("test %d %s", 42, "hello");
    
    printf("dispatch_op(0, 10, 3) = %d\n", dispatch_op(0, 10, 3));
    printf("dispatch_op(2, 10, 3) = %d\n", dispatch_op(2, 10, 3));
    printf("factorial(10) = %d\n", factorial(10));
    
    int data[] = {1, 2, 3, 4, 5};
    SumCtx ctx = {0};
    process_array(data, 5, sum_callback, &ctx);
    printf("callback sum = %d\n", ctx.total);
    
    // argc/argv semantic naming test
    if (argc > 1) {
        printf("arg1 = %s (len=%zu)\n", argv[1], strlen(argv[1]));
    }
    
    return 0;
}
