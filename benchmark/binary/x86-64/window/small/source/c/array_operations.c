#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Array operations
int find_max(int *arr, int len) {
    if (len <= 0) return 0;
    
    int max = arr[0];
    for (int i = 1; i < len; i++) {
        if (arr[i] > max) {
            max = arr[i];
        }
    }
    return max;
}

int find_min(int *arr, int len) {
    if (len <= 0) return 0;
    
    int min = arr[0];
    for (int i = 1; i < len; i++) {
        if (arr[i] < min) {
            min = arr[i];
        }
    }
    return min;
}

int array_sum(int *arr, int len) {
    int sum = 0;
    for (int i = 0; i < len; i++) {
        sum += arr[i];
    }
    return sum;
}

double array_average(int *arr, int len) {
    if (len <= 0) return 0.0;
    return (double)array_sum(arr, len) / len;
}

// Binary search
int binary_search(int *arr, int len, int target) {
    int left = 0;
    int right = len - 1;
    
    while (left <= right) {
        int mid = left + (right - left) / 2;
        
        if (arr[mid] == target) {
            return mid;
        }
        else if (arr[mid] < target) {
            left = mid + 1;
        }
        else {
            right = mid - 1;
        }
    }
    
    return -1;
}

// Linear search
int linear_search(int *arr, int len, int target) {
    for (int i = 0; i < len; i++) {
        if (arr[i] == target) {
            return i;
        }
    }
    return -1;
}

// Merge two sorted arrays
int* merge_arrays(int *arr1, int len1, int *arr2, int len2) {
    int *result = (int *)malloc((len1 + len2) * sizeof(int));
    
    int i = 0, j = 0, k = 0;
    
    while (i < len1 && j < len2) {
        if (arr1[i] <= arr2[j]) {
            result[k++] = arr1[i++];
        }
        else {
            result[k++] = arr2[j++];
        }
    }
    
    while (i < len1) {
        result[k++] = arr1[i++];
    }
    
    while (j < len2) {
        result[k++] = arr2[j++];
    }
    
    return result;
}

// Rotate array
void rotate_array(int *arr, int len, int k) {
    if (len <= 0 || k == 0) return;
    
    k = k % len;
    
    // Reverse entire array
    for (int i = 0; i < len / 2; i++) {
        int temp = arr[i];
        arr[i] = arr[len - 1 - i];
        arr[len - 1 - i] = temp;
    }
    
    // Reverse first k elements
    for (int i = 0; i < k / 2; i++) {
        int temp = arr[i];
        arr[i] = arr[k - 1 - i];
        arr[k - 1 - i] = temp;
    }
    
    // Reverse remaining elements
    for (int i = 0; i < (len - k) / 2; i++) {
        int temp = arr[k + i];
        arr[k + i] = arr[len - 1 - i];
        arr[len - 1 - i] = temp;
    }
}

// Remove duplicates from sorted array
int remove_duplicates(int *arr, int len) {
    if (len <= 1) return len;
    
    int write_idx = 0;
    for (int i = 1; i < len; i++) {
        if (arr[i] != arr[write_idx]) {
            write_idx++;
            arr[write_idx] = arr[i];
        }
    }
    
    return write_idx + 1;
}

// Partition array (for quicksort)
int partition(int *arr, int low, int high) {
    int pivot = arr[high];
    int i = low - 1;
    
    for (int j = low; j < high; j++) {
        if (arr[j] < pivot) {
            i++;
            int temp = arr[i];
            arr[i] = arr[j];
            arr[j] = temp;
        }
    }
    
    int temp = arr[i + 1];
    arr[i + 1] = arr[high];
    arr[high] = temp;
    
    return i + 1;
}

// Quick sort
void quicksort(int *arr, int low, int high) {
    if (low < high) {
        int pi = partition(arr, low, high);
        quicksort(arr, low, pi - 1);
        quicksort(arr, pi + 1, high);
    }
}

// Bubble sort
void bubble_sort(int *arr, int len) {
    for (int i = 0; i < len - 1; i++) {
        for (int j = 0; j < len - i - 1; j++) {
            if (arr[j] > arr[j + 1]) {
                int temp = arr[j];
                arr[j] = arr[j + 1];
                arr[j + 1] = temp;
            }
        }
    }
}

// Insertion sort
void insertion_sort(int *arr, int len) {
    for (int i = 1; i < len; i++) {
        int key = arr[i];
        int j = i - 1;
        
        while (j >= 0 && arr[j] > key) {
            arr[j + 1] = arr[j];
            j--;
        }
        arr[j + 1] = key;
    }
}

// 2D array operations
int matrix_sum(int matrix[][5], int rows, int cols) {
    int sum = 0;
    for (int i = 0; i < rows; i++) {
        for (int j = 0; j < cols; j++) {
            sum += matrix[i][j];
        }
    }
    return sum;
}

// Transpose matrix
void transpose_matrix(int src[][5], int dst[][5], int rows, int cols) {
    for (int i = 0; i < rows; i++) {
        for (int j = 0; j < cols; j++) {
            dst[j][i] = src[i][j];
        }
    }
}

// Main function
int main() {
    // Basic array operations
    int arr[] = {3, 7, 2, 9, 1, 5};
    int len = sizeof(arr) / sizeof(arr[0]);
    
    int max_val = find_max(arr, len);
    int min_val = find_min(arr, len);
    int sum = array_sum(arr, len);
    double avg = array_average(arr, len);
    
    // Sorting tests
    int arr_bubble[] = {5, 2, 8, 1, 9};
    int arr_insertion[] = {5, 2, 8, 1, 9};
    int arr_quick[] = {5, 2, 8, 1, 9};
    
    bubble_sort(arr_bubble, 5);
    insertion_sort(arr_insertion, 5);
    quicksort(arr_quick, 0, 4);
    
    // Binary search test
    int sorted_arr[] = {1, 2, 3, 4, 5, 6, 7, 8, 9};
    int search_result = binary_search(sorted_arr, 9, 5);
    
    // Merge arrays
    int arr1[] = {1, 3, 5};
    int arr2[] = {2, 4, 6};
    int *merged = merge_arrays(arr1, 3, arr2, 3);
    
    // Rotate array
    int arr_rotate[] = {1, 2, 3, 4, 5};
    rotate_array(arr_rotate, 5, 2);
    
    // Remove duplicates
    int arr_dup[] = {1, 1, 2, 2, 3, 3, 3};
    int new_len = remove_duplicates(arr_dup, 7);
    
    // 2D array operations
    int matrix[3][5] = {
        {1, 2, 3, 4, 5},
        {6, 7, 8, 9, 10},
        {11, 12, 13, 14, 15}
    };
    int matrix_total = matrix_sum(matrix, 3, 5);
    
    free(merged);
    
    printf("Array operations completed: %d %d %d\n", max_val, min_val, new_len);
    
    return 0;
}
