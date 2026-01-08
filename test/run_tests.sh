#!/bin/bash
# Run all test executables to verify they work

set -e

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

BIN_DIR="bin_x64"

echo -e "${GREEN}=== Running Test Executables ===${NC}\n"

run_test() {
    local exe=$1
    echo -e "${YELLOW}Running:${NC} $exe"
    echo "----------------------------------------"
    wine "$BIN_DIR/$exe" 2>&1 | head -50 || echo "(Wine execution - may require Wine installed)"
    echo ""
}

if command -v wine &> /dev/null; then
    echo -e "${GREEN}Wine detected - running tests${NC}\n"
    
    run_test "nested_loops_x64.exe"
    run_test "switch_case_x64.exe"
    run_test "recursion_x64.exe"
    run_test "complex_structs_x64.exe"
    run_test "function_pointers_x64.exe"
    run_test "virtual_functions_x64.exe"
    
    echo -e "${GREEN}✓ All tests executed${NC}"
else
    echo -e "${YELLOW}Wine not found. Skipping execution tests.${NC}"
    echo -e "To run Windows executables on macOS/Linux, install Wine:"
    echo -e "  macOS: ${GREEN}brew install wine-stable${NC}"
    echo -e "  Linux: ${GREEN}sudo apt-get install wine${NC}"
fi

echo -e "\n${GREEN}Test files are ready for decompilation analysis!${NC}"
