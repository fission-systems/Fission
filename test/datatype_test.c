#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Test various data types and their operations

// Test 1: Integer overflow and boundary conditions
void test_integer_boundaries() {
    printf("=== Integer Boundary Tests ===\n");
    
    // Signed integer boundaries
    signed char sc_max = 127;
    signed char sc_min = -128;
    printf("signed char: %d to %d\n", sc_min, sc_max);
    
    short s_max = 32767;
    short s_min = -32768;
    printf("short: %d to %d\n", s_min, s_max);
    
    int i_max = 2147483647;
    int i_min = -2147483648;
    printf("int: %d to %d\n", i_min, i_max);
    
    // Unsigned boundaries
    unsigned char uc_max = 255;
    printf("unsigned char: 0 to %u\n", uc_max);
    
    unsigned short us_max = 65535;
    printf("unsigned short: 0 to %u\n", us_max);
    
    unsigned int ui_max = 4294967295U;
    printf("unsigned int: 0 to %u\n", ui_max);
    
    // Overflow test
    int overflow_test = i_max + 1;
    printf("Overflow: %d + 1 = %d\n", i_max, overflow_test);
}

// Test 2: Floating point precision
void test_floating_point() {
    printf("\n=== Floating Point Tests ===\n");
    
    float f1 = 0.1f;
    float f2 = 0.2f;
    float f3 = f1 + f2;
    printf("float: 0.1 + 0.2 = %.10f\n", f3);
    
    double d1 = 0.1;
    double d2 = 0.2;
    double d3 = d1 + d2;
    printf("double: 0.1 + 0.2 = %.17f\n", d3);
    
    // Division by zero behavior
    float zero = 0.0f;
    float inf = 1.0f / zero;
    printf("1.0 / 0.0 = %f\n", inf);
    
    // NaN generation
    float nan = 0.0f / zero;
    printf("0.0 / 0.0 = %f\n", nan);
}

// Test 3: Type casting and conversion
void test_type_conversion() {
    printf("\n=== Type Conversion Tests ===\n");
    
    // Implicit conversion
    int i = 42;
    float f = i;  // int to float
    printf("int %d -> float %.2f\n", i, f);
    
    float f2 = 3.14f;
    int i2 = (int)f2;  // float to int (truncation)
    printf("float %.2f -> int %d\n", f2, i2);
    
    // Sign conversion
    unsigned int ui = 4294967295U;
    int si = (int)ui;  // unsigned to signed
    printf("unsigned %u -> signed %d\n", ui, si);
    
    // Pointer to integer
    void* ptr = (void*)0x12345678;
    size_t addr = (size_t)ptr;
    printf("pointer %p -> integer 0x%zx\n", ptr, addr);
}

// Test 4: Struct packing and alignment
#pragma pack(push, 1)
typedef struct {
    char c;      // 1 byte
    int i;       // 4 bytes
    short s;     // 2 bytes
} PackedStruct;  // Total: 7 bytes (packed)
#pragma pack(pop)

typedef struct {
    char c;      // 1 byte + 3 padding
    int i;       // 4 bytes
    short s;     // 2 bytes + 2 padding
} UnpackedStruct;  // Total: 12 bytes (natural alignment)

void test_struct_alignment() {
    printf("\n=== Struct Alignment Tests ===\n");
    printf("PackedStruct size: %zu bytes\n", sizeof(PackedStruct));
    printf("UnpackedStruct size: %zu bytes\n", sizeof(UnpackedStruct));
    
    PackedStruct ps = {1, 0x12345678, 0x9ABC};
    printf("PackedStruct: c=%d, i=0x%X, s=0x%X\n", ps.c, ps.i, ps.s);
    
    // Show memory layout
    unsigned char* bytes = (unsigned char*)&ps;
    printf("Memory layout: ");
    for (size_t i = 0; i < sizeof(PackedStruct); i++) {
        printf("%02X ", bytes[i]);
    }
    printf("\n");
}

// Test 5: Union type punning
typedef union {
    int i;
    float f;
    unsigned char bytes[4];
} TypePun;

void test_union_punning() {
    printf("\n=== Union Type Punning Tests ===\n");
    
    TypePun tp;
    tp.i = 0x42280000;
    printf("As int: 0x%08X\n", tp.i);
    printf("As float: %f\n", tp.f);
    printf("As bytes: %02X %02X %02X %02X\n", 
           tp.bytes[0], tp.bytes[1], tp.bytes[2], tp.bytes[3]);
    
    // Test endianness
    tp.i = 0x12345678;
    printf("\nEndianness test (0x12345678):\n");
    printf("bytes[0] = 0x%02X (", tp.bytes[0]);
    if (tp.bytes[0] == 0x78) {
        printf("Little Endian)\n");
    } else if (tp.bytes[0] == 0x12) {
        printf("Big Endian)\n");
    } else {
        printf("Unknown)\n");
    }
}

// Test 6: Bitfields
typedef struct {
    unsigned int flag1 : 1;   // 1 bit
    unsigned int flag2 : 1;   // 1 bit
    unsigned int value : 6;   // 6 bits
    unsigned int type : 4;    // 4 bits
    unsigned int reserved : 20;  // 20 bits
} BitField;  // Total: 32 bits = 4 bytes

void test_bitfields() {
    printf("\n=== Bitfield Tests ===\n");
    printf("BitField size: %zu bytes\n", sizeof(BitField));
    
    BitField bf = {0};
    bf.flag1 = 1;
    bf.flag2 = 0;
    bf.value = 42;
    bf.type = 7;
    
    printf("flag1=%u, flag2=%u, value=%u, type=%u\n",
           bf.flag1, bf.flag2, bf.value, bf.type);
    
    // Show raw memory
    unsigned int* raw = (unsigned int*)&bf;
    printf("Raw value: 0x%08X\n", *raw);
}

// Test 7: Array decay and pointer equivalence
void process_array(int arr[], int size) {
    printf("\n=== Array Decay Test ===\n");
    printf("In function: sizeof(arr) = %zu (pointer size)\n", sizeof(arr));
    printf("Array elements: ");
    for (int i = 0; i < size; i++) {
        printf("%d ", arr[i]);
    }
    printf("\n");
}

// Test 8: Volatile qualifier
void test_volatile() {
    printf("\n=== Volatile Qualifier Test ===\n");
    
    volatile int counter = 0;
    for (int i = 0; i < 5; i++) {
        counter++;  // Should not be optimized away
    }
    printf("Counter: %d\n", counter);
    
    // Memory-mapped I/O simulation
    volatile unsigned int* mmio_register = (volatile unsigned int*)0x1000;
    // Note: This would crash in real code, just for decompilation test
    // *mmio_register = 0x42;
    printf("MMIO register address: %p\n", (void*)mmio_register);
}

// Test 9: Const correctness
const char* get_message(int code) {
    static const char* messages[] = {
        "Success",
        "Error",
        "Warning",
        "Info"
    };
    
    if (code >= 0 && code < 4) {
        return messages[code];
    }
    return "Unknown";
}

void test_const() {
    printf("\n=== Const Correctness Test ===\n");
    
    const char* msg = get_message(0);
    printf("Message: %s\n", msg);
    
    // const pointer vs pointer to const
    int value1 = 10, value2 = 20;
    const int* ptr_to_const = &value1;  // Can change pointer, not value
    int* const const_ptr = &value1;      // Can change value, not pointer
    
    printf("ptr_to_const points to: %d\n", *ptr_to_const);
    ptr_to_const = &value2;  // OK
    printf("ptr_to_const now points to: %d\n", *ptr_to_const);
    
    printf("const_ptr points to: %d\n", *const_ptr);
    *const_ptr = 30;  // OK
    printf("const_ptr value changed to: %d\n", *const_ptr);
}

// Test 10: Static variables
int count_calls() {
    static int call_count = 0;
    return ++call_count;
}

void test_static() {
    printf("\n=== Static Variable Test ===\n");
    printf("Call 1: %d\n", count_calls());
    printf("Call 2: %d\n", count_calls());
    printf("Call 3: %d\n", count_calls());
}

int main(int argc, char* argv[]) {
    printf("=== Fission Data Type Test Suite ===\n\n");
    
    test_integer_boundaries();
    test_floating_point();
    test_type_conversion();
    test_struct_alignment();
    test_union_punning();
    test_bitfields();
    
    int arr[] = {1, 2, 3, 4, 5};
    printf("\nIn main: sizeof(arr) = %zu (full array)\n", sizeof(arr));
    process_array(arr, 5);
    
    test_volatile();
    test_const();
    test_static();
    
    printf("\n=== All data type tests completed ===\n");
    return 0;
}
