/**
 * test_control_flow.cpp
 *
 * Tests decompiler control flow recovery:
 *  - Nested if/else → switch reconstruction
 *  - BST-style dispatch patterns
 *  - Loop types: for, while, do-while, nested
 *  - while(true) → for loop conversion
 *  - while(true) → while(cond) conversion
 *  - Early return (if-return-else simplification)
 *  - noreturn function handling
 *  - Computed goto / indirect jump tables
 */
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>

// ---- Switch with many cases (jump table) ----
const char* day_name(int day) {
    switch (day) {
    case 0: return "Sunday";
    case 1: return "Monday";
    case 2: return "Tuesday";
    case 3: return "Wednesday";
    case 4: return "Thursday";
    case 5: return "Friday";
    case 6: return "Saturday";
    default: return "Unknown";
    }
}

// ---- Sparse switch (BST dispatch) ----
int sparse_switch(int code) {
    switch (code) {
    case 1:    return 10;
    case 5:    return 50;
    case 10:   return 100;
    case 50:   return 500;
    case 100:  return 1000;
    case 500:  return 5000;
    case 1000: return 10000;
    default:   return -1;
    }
}

// ---- Nested if-else chain (compiler may BST-ify) ----
int classify_temperature(int temp) {
    if (temp < -20) return 0;        // Extreme cold
    else if (temp < 0) return 1;     // Cold
    else if (temp < 10) return 2;    // Cool
    else if (temp < 20) return 3;    // Mild
    else if (temp < 30) return 4;    // Warm
    else if (temp < 40) return 5;    // Hot
    else return 6;                   // Extreme hot
}

// ---- Simple for loop ----
int sum_range(int start, int end) {
    int sum = 0;
    for (int i = start; i < end; i++) {
        sum += i;
    }
    return sum;
}

// ---- while loop with complex condition ----
int count_digits(int n) {
    int count = 0;
    if (n < 0) n = -n;
    while (n > 0) {
        n /= 10;
        count++;
    }
    return count == 0 ? 1 : count;
}

// ---- do-while loop ----
int find_first_set(uint32_t x) {
    if (x == 0) return -1;
    int pos = 0;
    do {
        if (x & 1) return pos;
        x >>= 1;
        pos++;
    } while (x != 0);
    return -1;
}

// ---- Nested loops (2D array operation) ----
void matrix_multiply(const int A[4][4], const int B[4][4], int C[4][4]) {
    for (int i = 0; i < 4; i++) {
        for (int j = 0; j < 4; j++) {
            C[i][j] = 0;
            for (int k = 0; k < 4; k++) {
                C[i][j] += A[i][k] * B[k][j];
            }
        }
    }
}

// ---- while(true) with if-break (→ while(cond)) ----
int read_until_sentinel(const int *data, int sentinel) {
    int sum = 0;
    int idx = 0;
    while (1) {
        if (data[idx] == sentinel) break;
        sum += data[idx];
        idx++;
    }
    return sum;
}

// ---- while(true) with init + update (→ for loop) ----
int count_set_bits_loop(uint32_t x) {
    int count = 0;
    uint32_t mask = 1;
    while (1) {
        if (mask == 0) break;
        if (x & mask) count++;
        mask <<= 1;
    }
    return count;
}

// ---- Early return pattern (if-return-else) ----
int safe_divide(int a, int b) {
    if (b == 0) {
        return -1;  // Error
    } else {
        return a / b;
    }
}

int clamp(int value, int min_val, int max_val) {
    if (value < min_val) {
        return min_val;
    } else if (value > max_val) {
        return max_val;
    } else {
        return value;
    }
}

// ---- noreturn function usage ----
__attribute__((noreturn)) void fatal_error(const char *msg) {
    fprintf(stderr, "FATAL: %s\n", msg);
    abort();
}

int process_command(int cmd) {
    if (cmd < 0) {
        fatal_error("negative command");
    }
    if (cmd > 100) {
        fatal_error("command overflow");
    }
    return cmd * 2 + 1;
}

// ---- Constant condition (dead branch) ----
int constant_condition_test(int x) {
    if (0) {
        // Dead code - should be removed
        return -999;
    }
    if (1) {
        return x + 1;
    }
    return x;  // Unreachable but compiler may keep it
}

// ---- Loop unrolling residual ----
void memzero_manual(char *buf, int len) {
    int i = 0;
    for (; i + 4 <= len; i += 4) {
        buf[i] = 0;
        buf[i+1] = 0;
        buf[i+2] = 0;
        buf[i+3] = 0;
    }
    for (; i < len; i++) {
        buf[i] = 0;
    }
}

// ---- Recursive with multiple base cases ----
int fibonacci(int n) {
    if (n <= 0) return 0;
    if (n == 1) return 1;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

int main(int argc, char **argv) {
    int val = argc > 1 ? atoi(argv[1]) : 42;
    
    printf("day_name(3) = %s\n", day_name(3));
    printf("sparse_switch(50) = %d\n", sparse_switch(50));
    printf("classify_temperature(25) = %d\n", classify_temperature(25));
    printf("sum_range(1, %d) = %d\n", val, sum_range(1, val));
    printf("count_digits(%d) = %d\n", val, count_digits(val));
    printf("find_first_set(%u) = %d\n", (unsigned)val, find_first_set((uint32_t)val));
    printf("safe_divide(%d, 3) = %d\n", val, safe_divide(val, 3));
    printf("clamp(%d, 0, 100) = %d\n", val, clamp(val, 0, 100));
    printf("fibonacci(10) = %d\n", fibonacci(10));
    printf("process_command(%d) = %d\n", val % 50, process_command(val % 50));
    
    int data[] = {1, 2, 3, 4, 5, -1};
    printf("read_until_sentinel = %d\n", read_until_sentinel(data, -1));
    printf("count_set_bits_loop(%u) = %d\n", (unsigned)val, count_set_bits_loop((uint32_t)val));
    
    return 0;
}
