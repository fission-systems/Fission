/**
 * Test: Complex Pointer Operations - Function Pointers
 * Category: Pointers
 * Difficulty: Hard
 */

#include <stdio.h>
#include <stdlib.h>

// Test 1: Simple function pointer
typedef int (*BinaryOp)(int, int);

int add(int a, int b) { return a + b; }
int subtract(int a, int b) { return a - b; }
int multiply(int a, int b) { return a * b; }
int divide(int a, int b) { return b != 0 ? a / b : 0; }

int apply_operation(BinaryOp op, int x, int y) {
    return op(x, y);
}

// Test 2: Array of function pointers
BinaryOp get_operation(char op_char) {
    static BinaryOp operations[4] = {add, subtract, multiply, divide};
    switch (op_char) {
        case '+': return operations[0];
        case '-': return operations[1];
        case '*': return operations[2];
        case '/': return operations[3];
        default: return NULL;
    }
}

// Test 3: Function pointer as struct member
typedef int (*FilterFunc)(int);

typedef struct {
    int* data;
    int size;
    FilterFunc filter;
} FilteredArray;

int is_even(int n) { return n % 2 == 0; }
int is_positive(int n) { return n > 0; }
int is_large(int n) { return n > 100; }

int count_filtered(FilteredArray* arr) {
    int count = 0;
    for (int i = 0; i < arr->size; i++) {
        if (arr->filter(arr->data[i])) {
            count++;
        }
    }
    return count;
}

// Test 4: Callback pattern
typedef void (*EventCallback)(void* user_data, int event_type);

typedef struct {
    EventCallback on_start;
    EventCallback on_update;
    EventCallback on_end;
    void* user_data;
} EventSystem;

void register_callbacks(EventSystem* sys, 
                       EventCallback start,
                       EventCallback update,
                       EventCallback end,
                       void* data) {
    sys->on_start = start;
    sys->on_update = update;
    sys->on_end = end;
    sys->user_data = data;
}

void trigger_event(EventSystem* sys, EventCallback callback, int event_type) {
    if (callback) {
        callback(sys->user_data, event_type);
    }
}

// Test 5: Function returning function pointer
typedef double (*MathFunc)(double);

double square(double x) { return x * x; }
double cube(double x) { return x * x * x; }
double sqrt_approx(double x) { return x / 2.0; }  // Simplified

MathFunc get_math_function(int func_id) {
    switch (func_id) {
        case 1: return square;
        case 2: return cube;
        case 3: return sqrt_approx;
        default: return NULL;
    }
}

// Test 6: Complex: pointer to array of function pointers
typedef int (*CompareFunc)(int, int);

int compare_less(int a, int b) { return a < b; }
int compare_greater(int a, int b) { return a > b; }
int compare_equal(int a, int b) { return a == b; }

typedef CompareFunc (*CompareFuncGetter)(void);

CompareFunc get_less_comparator(void) { return compare_less; }
CompareFunc get_greater_comparator(void) { return compare_greater; }
CompareFunc get_equal_comparator(void) { return compare_equal; }

void test_comparators(void) {
    CompareFuncGetter getters[] = {
        get_less_comparator,
        get_greater_comparator,
        get_equal_comparator
    };
    
    const char* names[] = {"less", "greater", "equal"};
    
    for (int i = 0; i < 3; i++) {
        CompareFunc cmp = getters[i]();
        printf("%s(5, 3): %d\n", names[i], cmp(5, 3));
    }
}

// Callback implementations
void on_start_handler(void* data, int type) {
    printf("Event started (type: %d, data: %p)\n", type, data);
}

void on_update_handler(void* data, int type) {
    int* counter = (int*)data;
    (*counter)++;
    printf("Event update %d (type: %d)\n", *counter, type);
}

void on_end_handler(void* data, int type) {
    printf("Event ended (type: %d)\n", type);
}

int main() {
    printf("=== Function Pointers Test ===\n\n");
    
    // Test 1: Simple function pointer
    printf("add(10, 5) = %d\n", apply_operation(add, 10, 5));
    printf("multiply(10, 5) = %d\n", apply_operation(multiply, 10, 5));
    
    // Test 2: Array of function pointers
    BinaryOp op = get_operation('+');
    if (op) {
        printf("10 + 5 = %d\n", op(10, 5));
    }
    
    // Test 3: Filter with function pointer
    int data[] = {-5, 10, -3, 8, 15, 2, -1, 7};
    FilteredArray arr = {data, 8, is_positive};
    printf("Positive numbers: %d\n", count_filtered(&arr));
    
    arr.filter = is_even;
    printf("Even numbers: %d\n", count_filtered(&arr));
    
    // Test 4: Callback pattern
    EventSystem system;
    int update_count = 0;
    register_callbacks(&system, 
                      on_start_handler,
                      on_update_handler,
                      on_end_handler,
                      &update_count);
    
    trigger_event(&system, system.on_start, 1);
    trigger_event(&system, system.on_update, 2);
    trigger_event(&system, system.on_update, 2);
    trigger_event(&system, system.on_end, 3);
    
    // Test 5: Function returning function pointer
    MathFunc func = get_math_function(1);
    if (func) {
        printf("square(5.0) = %.2f\n", func(5.0));
    }
    
    func = get_math_function(2);
    if (func) {
        printf("cube(3.0) = %.2f\n", func(3.0));
    }
    
    // Test 6: Complex comparators
    test_comparators();
    
    return 0;
}
