/**
 * test_arithmetic_idioms.cpp
 * 
 * Tests decompiler recovery of compiler-generated arithmetic patterns:
 *  - Magic number division (unsigned/signed)
 *  - Power-of-two modulo via AND mask
 *  - Shift-based multiply/divide
 *  - Sign extraction / absolute value
 *  - Float negation via XOR sign bit
 *
 * Compile with -O2 to trigger these patterns.
 */
#include <cstdint>
#include <cstdio>
#include <cstdlib>

// ---- Magic Division (unsigned) ----
// Compiler replaces `/ 3`, `/ 7`, `/ 10` etc. with multiply-high + shift
uint32_t divide_by_3(uint32_t x) { return x / 3; }
uint32_t divide_by_7(uint32_t x) { return x / 7; }
uint32_t divide_by_10(uint32_t x) { return x / 10; }
uint32_t divide_by_100(uint32_t x) { return x / 100; }

// ---- Magic Division (signed) ----
// Adds sign-correction: + (x >> 31)
int32_t signed_div_3(int32_t x) { return x / 3; }
int32_t signed_div_5(int32_t x) { return x / 5; }
int32_t signed_div_7(int32_t x) { return x / 7; }

// ---- Modulo via AND mask ----
// x % 2^N  =>  x & (2^N - 1) for unsigned
uint32_t mod_2(uint32_t x) { return x % 2; }
uint32_t mod_4(uint32_t x) { return x % 4; }
uint32_t mod_8(uint32_t x) { return x % 8; }
uint32_t mod_16(uint32_t x) { return x % 16; }
uint32_t mod_256(uint32_t x) { return x % 256; }

// ---- Shift-based multiply/divide ----
uint32_t multiply_by_2(uint32_t x) { return x * 2; }
uint32_t multiply_by_4(uint32_t x) { return x * 4; }
uint32_t multiply_by_8(uint32_t x) { return x * 8; }
uint32_t unsigned_div_2(uint32_t x) { return x / 2; }
uint32_t unsigned_div_4(uint32_t x) { return x / 4; }

// ---- Signed modulo power-of-two ----
// Compiler generates (x + (x >> 31)) & mask - (x >> 31)
int32_t signed_mod_2(int32_t x) { return x % 2; }
int32_t signed_mod_4(int32_t x) { return x % 4; }
int32_t signed_mod_8(int32_t x) { return x % 8; }

// ---- Absolute value ----
// Compiler generates XOR-subtract: (x ^ (x >> 31)) - (x >> 31)
int32_t absolute_value(int32_t x) { return abs(x); }

// ---- Combined: division + modulo in one function ----
void divmod(uint32_t a, uint32_t b, uint32_t *quotient, uint32_t *remainder) {
    *quotient = a / b;
    *remainder = a % b;
}

// ---- Float negation via XOR sign bit ----
float negate_float(float x) {
    // Compiler may generate: *(int*)&x ^= 0x80000000
    return -x;
}

double negate_double(double x) { return -x; }

// ---- Combined expression: magic div + modulo + shift ----
uint32_t complex_arith(uint32_t x) {
    return (x / 10) + (x % 16) + (x * 8);
}

// ---- 64-bit magic division ----
uint64_t div64_by_3(uint64_t x) { return x / 3; }
uint64_t div64_by_1000(uint64_t x) { return x / 1000; }

// ---- Strength reduction: multiply by constant ----
uint32_t multiply_by_3(uint32_t x) { return x * 3; }   // lea eax, [rdi+rdi*2]
uint32_t multiply_by_5(uint32_t x) { return x * 5; }   // lea eax, [rdi+rdi*4]
uint32_t multiply_by_7(uint32_t x) { return x * 7; }   // (x<<3) - x
uint32_t multiply_by_15(uint32_t x) { return x * 15; }  // (x<<4) - x

int main(int argc, char **argv) {
    uint32_t val = argc > 1 ? (uint32_t)atoi(argv[1]) : 12345;
    
    printf("divide_by_3(%u) = %u\n", val, divide_by_3(val));
    printf("divide_by_7(%u) = %u\n", val, divide_by_7(val));
    printf("divide_by_10(%u) = %u\n", val, divide_by_10(val));
    printf("signed_div_3(%d) = %d\n", (int)val, signed_div_3((int)val));
    printf("mod_16(%u) = %u\n", val, mod_16(val));
    printf("mod_256(%u) = %u\n", val, mod_256(val));
    printf("absolute_value(%d) = %d\n", -42, absolute_value(-42));
    printf("complex_arith(%u) = %u\n", val, complex_arith(val));
    printf("negate_float(3.14) = %f\n", negate_float(3.14f));
    printf("multiply_by_7(%u) = %u\n", val, multiply_by_7(val));
    printf("div64_by_3(%llu) = %llu\n", (unsigned long long)val, 
           (unsigned long long)div64_by_3(val));
    
    return 0;
}
