/**
 * Test: Complex Control Flow - Nested Loops
 * Category: Control Flow
 * Difficulty: Medium
 */

#include <stdio.h>

// Test 1: Double nested loop with break
int find_pair(int arr[], int size, int target) {
    for (int i = 0; i < size; i++) {
        for (int j = i + 1; j < size; j++) {
            if (arr[i] + arr[j] == target) {
                printf("Found pair: %d + %d = %d\n", arr[i], arr[j], target);
                return 1;
            }
        }
    }
    return 0;
}

// Test 2: Triple nested loop with continue
void print_3d_matrix(int depth, int rows, int cols) {
    for (int d = 0; d < depth; d++) {
        printf("Layer %d:\n", d);
        for (int r = 0; r < rows; r++) {
            for (int c = 0; c < cols; c++) {
                if (c == 0 && r > 0) {
                    continue;  // Skip first column after first row
                }
                printf("%d ", d * rows * cols + r * cols + c);
            }
            printf("\n");
        }
    }
}

// Test 3: Nested loop with labeled break (simulated with goto)
int find_in_matrix(int matrix[][5], int rows, int target) {
    int found = 0;
    for (int i = 0; i < rows; i++) {
        for (int j = 0; j < 5; j++) {
            if (matrix[i][j] == target) {
                printf("Found %d at [%d][%d]\n", target, i, j);
                found = 1;
                goto exit_loops;
            }
        }
    }
exit_loops:
    return found;
}

// Test 4: While inside for loop
void complex_iteration(int n) {
    for (int i = 0; i < n; i++) {
        int j = i;
        while (j > 0) {
            printf("%d ", j);
            j /= 2;
        }
        printf("\n");
    }
}

int main() {
    printf("=== Nested Loops Test ===\n\n");
    
    // Test 1
    int arr[] = {1, 4, 7, 2, 9, 5};
    find_pair(arr, 6, 11);
    
    // Test 2
    print_3d_matrix(2, 3, 4);
    
    // Test 3
    int matrix[3][5] = {
        {1, 2, 3, 4, 5},
        {6, 7, 8, 9, 10},
        {11, 12, 13, 14, 15}
    };
    find_in_matrix(matrix, 3, 8);
    
    // Test 4
    complex_iteration(5);
    
    return 0;
}
