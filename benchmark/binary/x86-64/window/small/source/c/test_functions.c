#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Simple arithmetic function
int add(int a, int b) {
    return a + b;
}

// Function with control flow
int max(int a, int b) {
    if (a > b) {
        return a;
    }
    return b;
}

// Recursive function
int fibonacci(int n) {
    if (n <= 1) {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

// Function with loop
int sum_array(int *arr, int len) {
    int sum = 0;
    for (int i = 0; i < len; i++) {
        sum += arr[i];
    }
    return sum;
}

// Function with switch
int process_code(int code) {
    switch (code) {
        case 1:
            return 10;
        case 2:
            return 20;
        case 3:
            return 30;
        default:
            return 0;
    }
}

// Function with nested loops
void fill_matrix(int *matrix, int rows, int cols, int value) {
    for (int i = 0; i < rows; i++) {
        for (int j = 0; j < cols; j++) {
            matrix[i * cols + j] = value;
        }
    }
}

// Function with pointer manipulation
void swap(int *a, int *b) {
    int temp = *a;
    *a = *b;
    *b = temp;
}

// Main function
int main() {
    int x = 5;
    int y = 10;
    
    int result1 = add(x, y);
    int result2 = max(x, y);
    int result3 = fibonacci(10);
    
    int arr[] = {1, 2, 3, 4, 5};
    int arr_sum = sum_array(arr, 5);
    
    int code_result = process_code(2);
    
    swap(&x, &y);
    
    printf("Results: %d %d %d %d %d\n", result1, result2, result3, arr_sum, code_result);
    
    return 0;
}
