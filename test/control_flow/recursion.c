/**
 * Test: Complex Control Flow - Recursion
 * Category: Control Flow
 * Difficulty: Medium-Hard
 */

#include <stdio.h>
#include <string.h>

// Test 1: Simple tail recursion
int factorial(int n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

// Test 2: Multiple recursive calls (binary tree)
int fibonacci(int n) {
    if (n <= 1) {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

// Test 3: Mutual recursion
int is_even(int n);
int is_odd(int n);

int is_even(int n) {
    if (n == 0) return 1;
    if (n == 1) return 0;
    return is_odd(n - 1);
}

int is_odd(int n) {
    if (n == 0) return 0;
    if (n == 1) return 1;
    return is_even(n - 1);
}

// Test 4: String recursion with pointer manipulation
void reverse_print(const char* str) {
    if (*str == '\0') return;
    reverse_print(str + 1);
    putchar(*str);
}

// Test 5: Complex recursion with multiple base cases
int ackermann(int m, int n) {
    if (m == 0) {
        return n + 1;
    } else if (n == 0) {
        return ackermann(m - 1, 1);
    } else {
        return ackermann(m - 1, ackermann(m, n - 1));
    }
}

// Test 6: Recursion with static variable (memoization attempt)
int count_calls(int n) {
    static int call_count = 0;
    call_count++;
    
    if (n <= 0) {
        int result = call_count;
        call_count = 0;  // Reset for next test
        return result;
    }
    
    return count_calls(n - 1);
}

// Test 7: Tree traversal simulation
typedef struct Node {
    int value;
    struct Node* left;
    struct Node* right;
} Node;

int sum_tree(Node* node) {
    if (node == NULL) {
        return 0;
    }
    return node->value + sum_tree(node->left) + sum_tree(node->right);
}

int main() {
    printf("=== Recursion Test ===\n\n");
    
    // Test 1
    printf("Factorial of 5: %d\n", factorial(5));
    printf("Factorial of 10: %d\n", factorial(10));
    
    // Test 2
    printf("Fibonacci(7): %d\n", fibonacci(7));
    
    // Test 3
    printf("Is 4 even? %d\n", is_even(4));
    printf("Is 7 odd? %d\n", is_odd(7));
    
    // Test 4
    printf("Reverse print 'Hello': ");
    reverse_print("Hello");
    printf("\n");
    
    // Test 5 (small values only - Ackermann grows very fast!)
    printf("Ackermann(1, 2): %d\n", ackermann(1, 2));
    printf("Ackermann(2, 2): %d\n", ackermann(2, 2));
    
    // Test 6
    printf("Call count for n=5: %d\n", count_calls(5));
    
    // Test 7
    Node n1 = {1, NULL, NULL};
    Node n2 = {2, NULL, NULL};
    Node n3 = {3, &n1, &n2};
    Node n4 = {4, NULL, NULL};
    Node root = {5, &n3, &n4};
    printf("Sum of tree: %d\n", sum_tree(&root));
    
    return 0;
}
