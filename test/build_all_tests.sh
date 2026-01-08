#!/bin/bash
# Build all test cases for x86 (32-bit) using MinGW

set -e  # Exit on error

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Compiler settings
CC="x86_64-w64-mingw32-gcc"
CXX="x86_64-w64-mingw32-g++"
CFLAGS="-O0 -g -Wall -Wextra"
CXXFLAGS="-O0 -g -Wall -Wextra -std=c++11"

# Output directory
OUTPUT_DIR="bin_x64"
mkdir -p "$OUTPUT_DIR"

echo -e "${GREEN}=== Building Test Cases (x86_64 64-bit) ===${NC}\n"

# Function to compile a C file
compile_c() {
    local source=$1
    local output=$2
    local extra_flags=$3
    
    echo -e "${YELLOW}Compiling:${NC} $source"
    if $CC $CFLAGS $extra_flags "$source" -o "$output"; then
        echo -e "${GREEN}✓${NC} Built: $output"
        return 0
    else
        echo -e "${RED}✗${NC} Failed: $source"
        return 1
    fi
}

# Function to compile a C++ file
compile_cpp() {
    local source=$1
    local output=$2
    local extra_flags=$3
    
    echo -e "${YELLOW}Compiling:${NC} $source"
    if $CXX $CXXFLAGS $extra_flags "$source" -o "$output"; then
        echo -e "${GREEN}✓${NC} Built: $output"
        return 0
    else
        echo -e "${RED}✗${NC} Failed: $source"
        return 1
    fi
}

# Counter for statistics
TOTAL=0
SUCCESS=0
FAILED=0

# Category 1: Control Flow
echo -e "\n${GREEN}[1/5] Control Flow Tests${NC}"
cd control_flow
compile_c "nested_loops.c" "../$OUTPUT_DIR/nested_loops_x64.exe" && ((SUCCESS++)) || ((FAILED++)); ((TOTAL++))
compile_c "switch_case.c" "../$OUTPUT_DIR/switch_case_x64.exe" && ((SUCCESS++)) || ((FAILED++)); ((TOTAL++))
compile_c "recursion.c" "../$OUTPUT_DIR/recursion_x64.exe" && ((SUCCESS++)) || ((FAILED++)); ((TOTAL++))
cd ..

# Category 2: Data Structures
echo -e "\n${GREEN}[2/5] Data Structures Tests${NC}"
cd data_structures
compile_c "complex_structs.c" "../$OUTPUT_DIR/complex_structs_x64.exe" && ((SUCCESS++)) || ((FAILED++)); ((TOTAL++))
cd ..

# Category 3: Pointers
echo -e "\n${GREEN}[3/5] Pointer Tests${NC}"
cd pointers
compile_c "function_pointers.c" "../$OUTPUT_DIR/function_pointers_x64.exe" && ((SUCCESS++)) || ((FAILED++)); ((TOTAL++))
cd ..

# Category 4: C++ Features
echo -e "\n${GREEN}[4/5] C++ Features Tests${NC}"
cd cpp_features
compile_cpp "virtual_functions.cpp" "../$OUTPUT_DIR/virtual_functions_x64.exe" && ((SUCCESS++)) || ((FAILED++)); ((TOTAL++))
cd ..

# Summary
echo -e "\n${GREEN}=== Build Summary ===${NC}"
echo -e "Total tests: $TOTAL"
echo -e "${GREEN}Success: $SUCCESS${NC}"
if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Failed: $FAILED${NC}"
fi

# List built executables
if [ $SUCCESS -gt 0 ]; then
    echo -e "\n${GREEN}Built executables:${NC}"
    ls -lh "$OUTPUT_DIR/"*.exe 2>/dev/null || echo "No executables found"
fi

# Exit with error if any build failed
if [ $FAILED -gt 0 ]; then
    exit 1
fi

echo -e "\n${GREEN}✓ All tests built successfully!${NC}"
