#!/bin/bash
# Quick FID test - Compile sample programs for FID generation

set -e

echo "🔧 Building GCC/MinGW Test Programs for FID Generation"
echo "========================================================="

# Create output directory
mkdir -p fid_test_binaries/{gcc,mingw}/{x64,x86}/{debug,release}

# Test program
TEST_PROG="test_fid_runtime.c"

cat > "$TEST_PROG" << 'EOF'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <time.h>

// String manipulation tests
void test_strings() {
    char buffer[256];
    strcpy(buffer, "Hello, World!");
    strcat(buffer, " - Testing FID");
    
    char *found = strstr(buffer, "World");
    if (found) {
        printf("Found at: %p\n", (void*)found);
    }
    
    int len = strlen(buffer);
    printf("Length: %d\n", len);
    
    char dest[100];
    strncpy(dest, buffer, 50);
    dest[50] = '\0';
}

// Memory tests
void test_memory() {
    void *ptr1 = malloc(1024);
    void *ptr2 = calloc(10, sizeof(int));
    void *ptr3 = realloc(ptr1, 2048);
    
    if (ptr3) {
        memset(ptr3, 0, 2048);
        memcpy(ptr3, "test", 4);
        int cmp = memcmp(ptr3, "test", 4);
        printf("memcmp result: %d\n", cmp);
    }
    
    free(ptr2);
    free(ptr3);
}

// Math tests
void test_math() {
    double values[] = {16.0, 2.5, 3.14159, -5.0};
    
    for (int i = 0; i < 4; i++) {
        printf("sqrt(%f) = %f\n", values[i], sqrt(fabs(values[i])));
        printf("sin(%f) = %f\n", values[i], sin(values[i]));
        printf("cos(%f) = %f\n", values[i], cos(values[i]));
        printf("exp(%f) = %f\n", values[i], exp(values[i]));
        printf("log(%f) = %f\n", fabs(values[i]), log(fabs(values[i])));
    }
    
    double result = pow(2.0, 8.0);
    printf("pow(2, 8) = %f\n", result);
}

// File I/O tests
void test_files() {
    FILE *fp = fopen("test.txt", "w");
    if (fp) {
        fprintf(fp, "Line 1\n");
        fprintf(fp, "Line 2: %d\n", 42);
        fclose(fp);
    }
    
    fp = fopen("test.txt", "r");
    if (fp) {
        char line[256];
        while (fgets(line, sizeof(line), fp)) {
            printf("%s", line);
        }
        fclose(fp);
        remove("test.txt");
    }
}

// Array sorting test
int compare_ints(const void *a, const void *b) {
    return (*(int*)a - *(int*)b);
}

void test_sorting() {
    int arr[] = {5, 2, 8, 1, 9, 3, 7, 4, 6};
    int n = sizeof(arr) / sizeof(arr[0]);
    
    printf("Before sorting: ");
    for (int i = 0; i < n; i++) {
        printf("%d ", arr[i]);
    }
    printf("\n");
    
    qsort(arr, n, sizeof(int), compare_ints);
    
    printf("After sorting: ");
    for (int i = 0; i < n; i++) {
        printf("%d ", arr[i]);
    }
    printf("\n");
}

// Time functions
void test_time() {
    time_t now = time(NULL);
    printf("Current time: %ld\n", (long)now);
    
    struct tm *tm_info = localtime(&now);
    char time_str[100];
    strftime(time_str, sizeof(time_str), "%Y-%m-%d %H:%M:%S", tm_info);
    printf("Formatted: %s\n", time_str);
}

int main(int argc, char *argv[]) {
    printf("=== FID Test Program ===\n");
    printf("Testing common runtime functions\n\n");
    
    test_strings();
    test_memory();
    test_math();
    test_files();
    test_sorting();
    test_time();
    
    printf("\nAll tests completed!\n");
    return 0;
}
EOF

echo ""
echo "📝 Test program created: $TEST_PROG"
echo ""

# GCC x64 builds
if command -v gcc &> /dev/null; then
    echo "🔨 Building GCC x64 binaries..."
    
    # Debug
    gcc -m64 -g -O0 -o fid_test_binaries/gcc/x64/debug/test_runtime.out "$TEST_PROG" -lm
    echo "   ✓ GCC x64 Debug"
    
    # Release
    gcc -m64 -O2 -o fid_test_binaries/gcc/x64/release/test_runtime.out "$TEST_PROG" -lm
    echo "   ✓ GCC x64 Release"
fi

# MinGW x64 builds
if command -v x86_64-w64-mingw32-gcc &> /dev/null; then
    echo "🔨 Building MinGW x64 binaries..."
    
    # Debug
    x86_64-w64-mingw32-gcc -g -O0 -o fid_test_binaries/mingw/x64/debug/test_runtime.exe "$TEST_PROG"
    echo "   ✓ MinGW x64 Debug"
    
    # Release
    x86_64-w64-mingw32-gcc -O2 -o fid_test_binaries/mingw/x64/release/test_runtime.exe "$TEST_PROG"
    echo "   ✓ MinGW x64 Release"
fi

# MinGW x86 builds
if command -v i686-w64-mingw32-gcc &> /dev/null; then
    echo "🔨 Building MinGW x86 binaries..."
    
    # Debug
    i686-w64-mingw32-gcc -g -O0 -o fid_test_binaries/mingw/x86/debug/test_runtime.exe "$TEST_PROG"
    echo "   ✓ MinGW x86 Debug"
    
    # Release
    i686-w64-mingw32-gcc -O2 -o fid_test_binaries/mingw/x86/release/test_runtime.exe "$TEST_PROG"
    echo "   ✓ MinGW x86 Release"
fi

echo ""
echo "========================================================="
echo "✅ Build complete!"
echo ""
echo "Generated binaries:"
find fid_test_binaries -type f -name "*.exe" -o -name "*.out" | while read file; do
    size=$(ls -lh "$file" | awk '{print $5}')
    echo "   $file ($size)"
done
echo ""
echo "📚 Next steps:"
echo "1. Open Ghidra"
echo "2. Tools → Function ID → Create FidDb..."
echo "3. Tools → Function ID → Populate FidDb from programs..."
echo "4. Select common_symbols_gcc_x64.txt or common_symbols_gcc_x86.txt"
echo "5. Add all binaries from fid_test_binaries/"
echo "6. Export to .fidbf format"
echo "7. Copy to ghidra/funtionID/"
echo ""
echo "See BUILDING_GCC_FID.md for detailed instructions"
