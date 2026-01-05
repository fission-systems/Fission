#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Test 1: Simple arithmetic
int add(int a, int b) {
    return a + b;
}

int multiply(int x, int y) {
    return x * y;
}

// Test 2: External function calls
void print_message(const char* msg) {
    printf("Message: %s\n", msg);
}

// Test 3: Struct operations
typedef struct {
    int id;
    char name[32];
    double value;
} Item;

void init_item(Item* item, int id, const char* name, double value) {
    if (item != NULL) {
        item->id = id;
        strncpy(item->name, name, 31);
        item->name[31] = '\0';
        item->value = value;
    }
}

void print_item(Item* item) {
    if (item != NULL) {
        printf("Item ID: %d\n", item->id);
        printf("Name: %s\n", item->name);
        printf("Value: %.2f\n", item->value);
    }
}

// Test 4: Memory allocation
Item* create_item(int id, const char* name, double value) {
    Item* item = (Item*)malloc(sizeof(Item));
    if (item != NULL) {
        init_item(item, id, name, value);
    }
    return item;
}

void destroy_item(Item* item) {
    if (item != NULL) {
        free(item);
    }
}

// Test 5: Control flow
int calculate_discount(int age, double price) {
    if (age < 18) {
        return (int)(price * 0.5);  // 50% discount for kids
    } else if (age >= 65) {
        return (int)(price * 0.7);  // 30% discount for seniors
    } else {
        return (int)price;          // No discount
    }
}

// Test 6: Loops
int sum_array(int* arr, int size) {
    int sum = 0;
    for (int i = 0; i < size; i++) {
        sum += arr[i];
    }
    return sum;
}

// Main function
int main() {
    printf("=== Fission Decompiler Comparison Test ===\n\n");
    
    // Test arithmetic
    int result1 = add(10, 20);
    int result2 = multiply(5, 6);
    printf("Add: %d, Multiply: %d\n", result1, result2);
    
    // Test struct operations
    Item* item = create_item(1001, "TestItem", 49.99);
    if (item != NULL) {
        print_item(item);
        destroy_item(item);
    }
    
    // Test control flow
    int discount1 = calculate_discount(15, 100.0);
    int discount2 = calculate_discount(70, 100.0);
    printf("Kid price: %d, Senior price: %d\n", discount1, discount2);
    
    // Test loops
    int numbers[] = {1, 2, 3, 4, 5};
    int total = sum_array(numbers, 5);
    printf("Sum: %d\n", total);
    
    return 0;
}
