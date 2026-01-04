#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>

// Test 1: Control Flow - Switch Statement
int classify_number(int n) {
    switch (n % 5) {
        case 0:
            return 100;
        case 1:
            return 10;
        case 2:
            return 20;
        case 3:
            return 30;
        case 4:
            return 40;
        default:
            return -1;
    }
}

// Test 2: Nested Loops
void print_matrix(int rows, int cols) {
    for (int i = 0; i < rows; i++) {
        for (int j = 0; j < cols; j++) {
            printf("%d ", i * cols + j);
        }
        printf("\n");
    }
}

// Test 3: Recursive Function
int fibonacci(int n) {
    if (n <= 1) {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

// Test 4: Tail Recursion
int factorial_tail(int n, int accumulator) {
    if (n <= 1) {
        return accumulator;
    }
    return factorial_tail(n - 1, n * accumulator);
}

int factorial(int n) {
    return factorial_tail(n, 1);
}

// Test 5: Complex Conditional Logic
const char* check_range(int value) {
    if (value < 0) {
        return "negative";
    } else if (value == 0) {
        return "zero";
    } else if (value < 10) {
        return "small";
    } else if (value < 100) {
        return "medium";
    } else if (value < 1000) {
        return "large";
    } else {
        return "very large";
    }
}

// Test 6: Bitwise Operations
unsigned int count_bits(unsigned int n) {
    unsigned int count = 0;
    while (n) {
        count += n & 1;
        n >>= 1;
    }
    return count;
}

// Test 7: Pointer Arithmetic
void reverse_array(int* arr, int size) {
    int* left = arr;
    int* right = arr + size - 1;
    
    while (left < right) {
        int temp = *left;
        *left = *right;
        *right = temp;
        left++;
        right--;
    }
}

// Test 8: Function Pointers
typedef int (*operation_func)(int, int);

int add(int a, int b) { return a + b; }
int subtract(int a, int b) { return a - b; }
int multiply(int a, int b) { return a * b; }
int divide(int a, int b) { return b != 0 ? a / b : 0; }

int calculate(operation_func op, int a, int b) {
    return op(a, b);
}

// Test 9: Variadic Function
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

// Test 10: Inline Assembly (x86/x64 specific)
// Note: Assembly disabled for portability
int add_asm(int a, int b) {
    // Simple fallback implementation
    return a + b;
}

// Test 11: String Manipulation
char* string_duplicate(const char* str) {
    if (!str) return NULL;
    
    size_t len = strlen(str);
    char* result = (char*)malloc(len + 1);
    
    if (result) {
        memcpy(result, str, len + 1);
    }
    
    return result;
}

// Test 12: Array of Structures
typedef struct {
    int id;
    char name[32];
    float score;
} Student;

void sort_students(Student* students, int count) {
    // Simple bubble sort
    for (int i = 0; i < count - 1; i++) {
        for (int j = 0; j < count - i - 1; j++) {
            if (students[j].score < students[j + 1].score) {
                Student temp = students[j];
                students[j] = students[j + 1];
                students[j + 1] = temp;
            }
        }
    }
}

int main(int argc, char* argv[]) {
    printf("=== Fission Decompilation Test Suite ===\n\n");
    
    // Test 1: Switch
    printf("1. Switch: classify_number(7) = %d\n", classify_number(7));
    
    // Test 2: Nested Loops
    printf("\n2. Matrix:\n");
    print_matrix(3, 4);
    
    // Test 3: Recursion
    printf("\n3. Fibonacci(10) = %d\n", fibonacci(10));
    
    // Test 4: Tail Recursion
    printf("4. Factorial(5) = %d\n", factorial(5));
    
    // Test 5: Conditional Logic
    printf("\n5. Range check:\n");
    printf("   -5: %s\n", check_range(-5));
    printf("    0: %s\n", check_range(0));
    printf("    5: %s\n", check_range(5));
    printf("   50: %s\n", check_range(50));
    printf("  500: %s\n", check_range(500));
    printf(" 5000: %s\n", check_range(5000));
    
    // Test 6: Bitwise
    printf("\n6. Bit count of 255: %u\n", count_bits(255));
    
    // Test 7: Pointer Arithmetic
    int arr[] = {1, 2, 3, 4, 5};
    printf("\n7. Array before reverse: ");
    for (int i = 0; i < 5; i++) printf("%d ", arr[i]);
    reverse_array(arr, 5);
    printf("\n   Array after reverse:  ");
    for (int i = 0; i < 5; i++) printf("%d ", arr[i]);
    printf("\n");
    
    // Test 8: Function Pointers
    printf("\n8. Function pointers:\n");
    printf("   10 + 5 = %d\n", calculate(add, 10, 5));
    printf("   10 - 5 = %d\n", calculate(subtract, 10, 5));
    printf("   10 * 5 = %d\n", calculate(multiply, 10, 5));
    printf("   10 / 5 = %d\n", calculate(divide, 10, 5));
    
    // Test 9: Variadic
    printf("\n9. Sum(1,2,3,4,5) = %d\n", sum_variadic(5, 1, 2, 3, 4, 5));
    
    // Test 10: Assembly
    printf("\n10. Assembly add(15, 27) = %d\n", add_asm(15, 27));
    
    // Test 11: String Manipulation
    const char* original = "Hello, Fission!";
    char* duplicate = string_duplicate(original);
    printf("\n11. String duplicate: '%s'\n", duplicate ? duplicate : "NULL");
    free(duplicate);
    
    // Test 12: Structures
    printf("\n12. Student sorting:\n");
    Student students[3] = {
        {1, "Alice", 85.5f},
        {2, "Bob", 92.0f},
        {3, "Charlie", 78.3f}
    };
    
    printf("   Before sort:\n");
    for (int i = 0; i < 3; i++) {
        printf("      %s: %.1f\n", students[i].name, students[i].score);
    }
    
    sort_students(students, 3);
    
    printf("   After sort:\n");
    for (int i = 0; i < 3; i++) {
        printf("      %s: %.1f\n", students[i].name, students[i].score);
    }
    
    printf("\n=== All tests completed ===\n");
    return 0;
}
