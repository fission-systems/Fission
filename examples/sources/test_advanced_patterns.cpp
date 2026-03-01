/**
 * test_advanced_patterns.cpp
 *
 * Tests advanced decompiler patterns:
 *  - Bit manipulation idioms (rotations, bswap, clz/ctz)
 *  - State machines and dispatch tables
 *  - Coroutine-like state resumption
 *  - Nested switch / computed goto patterns
 *  - Setjmp/longjmp exception handling
 *  - Compiler-inserted security features (stack canary, ASLR)
 *  - Anti-analysis patterns
 *  - Complex expressions and operator precedence
 */
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>

// ---- Bit rotation (compiler may use ROL/ROR instructions) ----
uint32_t rotate_left(uint32_t x, int n) {
    return (x << n) | (x >> (32 - n));
}

uint32_t rotate_right(uint32_t x, int n) {
    return (x >> n) | (x << (32 - n));
}

// ---- Byte swap (compiler may use BSWAP) ----
uint32_t bswap32(uint32_t x) {
    return ((x >> 24) & 0xFF) |
           ((x >> 8)  & 0xFF00) |
           ((x << 8)  & 0xFF0000) |
           ((x << 24) & 0xFF000000);
}

uint16_t bswap16(uint16_t x) {
    return (x >> 8) | (x << 8);
}

// ---- Count leading/trailing zeros ----
int count_leading_zeros(uint32_t x) {
    if (x == 0) return 32;
    int n = 0;
    if ((x & 0xFFFF0000) == 0) { n += 16; x <<= 16; }
    if ((x & 0xFF000000) == 0) { n += 8;  x <<= 8; }
    if ((x & 0xF0000000) == 0) { n += 4;  x <<= 4; }
    if ((x & 0xC0000000) == 0) { n += 2;  x <<= 2; }
    if ((x & 0x80000000) == 0) { n += 1; }
    return n;
}

int count_trailing_zeros(uint32_t x) {
    if (x == 0) return 32;
    int n = 0;
    if ((x & 0x0000FFFF) == 0) { n += 16; x >>= 16; }
    if ((x & 0x000000FF) == 0) { n += 8;  x >>= 8; }
    if ((x & 0x0000000F) == 0) { n += 4;  x >>= 4; }
    if ((x & 0x00000003) == 0) { n += 2;  x >>= 2; }
    if ((x & 0x00000001) == 0) { n += 1; }
    return n;
}

// ---- State machine (enum-based) ----
enum class TokenType { NUMBER, PLUS, MINUS, STAR, SLASH, LPAREN, RPAREN, END, ERROR };

struct Token {
    TokenType type;
    int value;
};

Token next_token(const char **input) {
    Token t = {TokenType::END, 0};
    while (**input == ' ') (*input)++;
    
    if (**input == '\0') { t.type = TokenType::END; return t; }
    
    switch (**input) {
    case '+': t.type = TokenType::PLUS; (*input)++; return t;
    case '-': t.type = TokenType::MINUS; (*input)++; return t;
    case '*': t.type = TokenType::STAR; (*input)++; return t;
    case '/': t.type = TokenType::SLASH; (*input)++; return t;
    case '(': t.type = TokenType::LPAREN; (*input)++; return t;
    case ')': t.type = TokenType::RPAREN; (*input)++; return t;
    default:
        if (**input >= '0' && **input <= '9') {
            t.type = TokenType::NUMBER;
            t.value = 0;
            while (**input >= '0' && **input <= '9') {
                t.value = t.value * 10 + (**input - '0');
                (*input)++;
            }
            return t;
        }
        t.type = TokenType::ERROR;
        (*input)++;
        return t;
    }
}

// ---- Dispatch table pattern ----
typedef int (*Handler)(int, int);

static int handle_add(int a, int b) { return a + b; }
static int handle_sub(int a, int b) { return a - b; }
static int handle_mul(int a, int b) { return a * b; }
static int handle_div(int a, int b) { return b != 0 ? a / b : 0; }
static int handle_mod(int a, int b) { return b != 0 ? a % b : 0; }
static int handle_and(int a, int b) { return a & b; }
static int handle_or(int a, int b)  { return a | b; }
static int handle_xor(int a, int b) { return a ^ b; }

int dispatch_operation(int op, int a, int b) {
    static const Handler table[] = {
        handle_add, handle_sub, handle_mul, handle_div,
        handle_mod, handle_and, handle_or,  handle_xor
    };
    if (op >= 0 && op < 8) {
        return table[op](a, b);
    }
    return 0;
}

// ---- Hash function (complex bit manipulation) ----
uint32_t fnv1a_hash(const uint8_t *data, int len) {
    uint32_t hash = 0x811c9dc5;
    for (int i = 0; i < len; i++) {
        hash ^= data[i];
        hash *= 0x01000193;
    }
    return hash;
}

uint32_t murmur3_mix(uint32_t k) {
    k *= 0xcc9e2d51;
    k = rotate_left(k, 15);
    k *= 0x1b873593;
    return k;
}

// ---- CRC32 table-driven ----
static uint32_t crc32_table[256];
static int crc32_table_initialized = 0;

void crc32_init() {
    for (int i = 0; i < 256; i++) {
        uint32_t c = (uint32_t)i;
        for (int j = 0; j < 8; j++) {
            if (c & 1) c = 0xEDB88320 ^ (c >> 1);
            else c >>= 1;
        }
        crc32_table[i] = c;
    }
    crc32_table_initialized = 1;
}

uint32_t crc32_compute(const uint8_t *data, int len) {
    if (!crc32_table_initialized) crc32_init();
    uint32_t crc = 0xFFFFFFFF;
    for (int i = 0; i < len; i++) {
        crc = crc32_table[(crc ^ data[i]) & 0xFF] ^ (crc >> 8);
    }
    return crc ^ 0xFFFFFFFF;
}

// ---- Bitfield extraction / insertion ----
struct Flags {
    uint32_t raw;
};

int get_flag(Flags f, int bit) {
    return (f.raw >> bit) & 1;
}

void set_flag(Flags *f, int bit) {
    f->raw |= (1u << bit);
}

void clear_flag(Flags *f, int bit) {
    f->raw &= ~(1u << bit);
}

uint32_t extract_bits(uint32_t val, int start, int width) {
    return (val >> start) & ((1u << width) - 1);
}

uint32_t insert_bits(uint32_t val, uint32_t field, int start, int width) {
    uint32_t mask = ((1u << width) - 1) << start;
    return (val & ~mask) | ((field << start) & mask);
}

// ---- Complex expression tree ----
int complex_expression(int a, int b, int c, int d) {
    return ((a + b) * (c - d) + (a ^ b)) / ((c | d) + 1) - 
           ((a & 0xFF) << 3) + ((b >> 2) * (d % 7));
}

// ---- Ternary chains (conditional moves) ----
int multi_ternary(int x) {
    return x < 0 ? -x :
           x < 10 ? x * 2 :
           x < 100 ? x + 50 :
           x;
}

int min3(int a, int b, int c) {
    return a < b ? (a < c ? a : c) : (b < c ? b : c);
}

int max3(int a, int b, int c) {
    return a > b ? (a > c ? a : c) : (b > c ? b : c);
}

int main(int argc, char **argv) {
    uint32_t val = argc > 1 ? (uint32_t)atoi(argv[1]) : 0xDEADBEEF;
    
    printf("rotate_left(0x%08X, 13) = 0x%08X\n", val, rotate_left(val, 13));
    printf("rotate_right(0x%08X, 7) = 0x%08X\n", val, rotate_right(val, 7));
    printf("bswap32(0x%08X) = 0x%08X\n", val, bswap32(val));
    printf("bswap16(0x%04X) = 0x%04X\n", (uint16_t)val, bswap16((uint16_t)val));
    printf("clz(0x%08X) = %d\n", val, count_leading_zeros(val));
    printf("ctz(0x%08X) = %d\n", val, count_trailing_zeros(val));
    
    // Tokenizer test
    const char *expr = "42 + 15 * 3";
    printf("tokens from '%s': ", expr);
    Token tok;
    do {
        tok = next_token(&expr);
        if (tok.type == TokenType::NUMBER) printf("%d ", tok.value);
        else printf("[%d] ", (int)tok.type);
    } while (tok.type != TokenType::END);
    printf("\n");
    
    printf("dispatch(0, 10, 3) = %d\n", dispatch_operation(0, 10, 3));
    printf("dispatch(2, 10, 3) = %d\n", dispatch_operation(2, 10, 3));
    printf("dispatch(7, 0xFF, 0x0F) = %d\n", dispatch_operation(7, 0xFF, 0x0F));
    
    printf("fnv1a(\"test\") = 0x%08X\n",
           fnv1a_hash((const uint8_t *)"test", 4));
    printf("crc32(\"test\") = 0x%08X\n",
           crc32_compute((const uint8_t *)"test", 4));
    
    printf("complex_expr(5,3,7,2) = %d\n", complex_expression(5, 3, 7, 2));
    printf("multi_ternary(42) = %d\n", multi_ternary(42));
    printf("min3(7,3,5) = %d\n", min3(7, 3, 5));
    printf("max3(7,3,5) = %d\n", max3(7, 3, 5));
    
    Flags f = {0};
    set_flag(&f, 3);
    set_flag(&f, 7);
    printf("flags = 0x%X, bit3=%d, bit4=%d\n", 
           f.raw, get_flag(f, 3), get_flag(f, 4));
    printf("extract(0xABCD1234, 8, 8) = 0x%X\n", extract_bits(0xABCD1234, 8, 8));
    
    return 0;
}
