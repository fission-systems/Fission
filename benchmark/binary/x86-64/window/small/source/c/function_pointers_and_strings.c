#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

// Function pointer types
typedef int (*compare_func)(const void *, const void *);
typedef int (*transform_func)(int);
typedef void (*callback_func)(int);

// Comparison functions
int compare_int_ascending(const void *a, const void *b) {
    return *(int *)a - *(int *)b;
}

int compare_int_descending(const void *a, const void *b) {
    return *(int *)b - *(int *)a;
}

int compare_abs_value(const void *a, const void *b) {
    int abs_a = abs(*(int *)a);
    int abs_b = abs(*(int *)b);
    return abs_a - abs_b;
}

// Transform functions
int square(int x) {
    return x * x;
}

int cube(int x) {
    return x * x * x;
}

int negate(int x) {
    return -x;
}

int absolute(int x) {
    return x < 0 ? -x : x;
}

// Apply transform function to array
void apply_transform(int *arr, int len, transform_func func) {
    for (int i = 0; i < len; i++) {
        arr[i] = func(arr[i]);
    }
}

// Apply function pointer callback
void for_each(int *arr, int len, callback_func cb) {
    for (int i = 0; i < len; i++) {
        cb(arr[i]);
    }
}

// Callback implementation
void print_value(int val) {
    printf("%d ", val);
}

void accumulate_callback(int val) {
    static int sum = 0;
    sum += val;
}

// Custom sort with function pointer
void custom_sort(int *arr, int len, compare_func cmp) {
    for (int i = 0; i < len - 1; i++) {
        for (int j = 0; j < len - i - 1; j++) {
            if (cmp(&arr[j], &arr[j + 1]) > 0) {
                int temp = arr[j];
                arr[j] = arr[j + 1];
                arr[j + 1] = temp;
            }
        }
    }
}

// String operations
int string_length(const char *str) {
    int len = 0;
    while (*str != '\0') {
        len++;
        str++;
    }
    return len;
}

char* string_reverse(char *str) {
    if (str == NULL) return NULL;
    
    char *start = str;
    char *end = str + string_length(str) - 1;
    
    while (start < end) {
        char temp = *start;
        *start = *end;
        *end = temp;
        start++;
        end--;
    }
    
    return str;
}

char* string_upper(char *str) {
    if (str == NULL) return NULL;
    
    for (int i = 0; str[i] != '\0'; i++) {
        str[i] = toupper(str[i]);
    }
    
    return str;
}

char* string_lower(char *str) {
    if (str == NULL) return NULL;
    
    for (int i = 0; str[i] != '\0'; i++) {
        str[i] = tolower(str[i]);
    }
    
    return str;
}

int string_compare(const char *s1, const char *s2) {
    while (*s1 != '\0' && *s2 != '\0') {
        if (*s1 != *s2) {
            return *s1 - *s2;
        }
        s1++;
        s2++;
    }
    return *s1 - *s2;
}

char* string_concat(char *dest, const char *src, int max_len) {
    int i = 0;
    while (dest[i] != '\0' && i < max_len) {
        i++;
    }
    
    int j = 0;
    while (src[j] != '\0' && i < max_len - 1) {
        dest[i] = src[j];
        i++;
        j++;
    }
    dest[i] = '\0';
    
    return dest;
}

int string_find_char(const char *str, char ch) {
    for (int i = 0; str[i] != '\0'; i++) {
        if (str[i] == ch) {
            return i;
        }
    }
    return -1;
}

// Main function
int main() {
    // Function pointers and transforms
    int arr[] = {3, 1, 4, 1, 5, 9, 2, 6};
    int len = sizeof(arr) / sizeof(arr[0]);
    
    // Test comparisons
    int copy1[8];
    int copy2[8];
    int copy3[8];
    memcpy(copy1, arr, sizeof(arr));
    memcpy(copy2, arr, sizeof(arr));
    memcpy(copy3, arr, sizeof(arr));
    
    custom_sort(copy1, len, compare_int_ascending);
    custom_sort(copy2, len, compare_int_descending);
    custom_sort(copy3, len, compare_abs_value);
    
    // Test transforms
    int copy_square[] = {1, 2, 3, 4, 5};
    apply_transform(copy_square, 5, square);
    
    // String operations
    char str1[] = "Hello";
    char str2[] = "World";
    char result[32] = "Test";
    
    char str_copy[] = "TestString";
    string_reverse(str_copy);
    
    char str_upper[] = "hello";
    string_upper(str_upper);
    
    int cmp_result = string_compare("abc", "abd");
    int find_result = string_find_char("Hello World", 'o');
    
    string_concat(result, " String", sizeof(result));
    
    printf("Operations completed\n");
    
    return 0;
}
