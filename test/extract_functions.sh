#!/bin/bash
# Extract function addresses from test executables

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

BIN_DIR="bin_x64"

echo -e "${GREEN}=== Extracting Function Addresses ===${NC}\n"

# Function to extract addresses using objdump
extract_addresses() {
    local exe=$1
    local output=$2
    
    echo -e "${YELLOW}Processing:${NC} $exe"
    
    # Use objdump to get symbol table
    # Look for functions (ty   20) in .text section (sec  1)
    x86_64-w64-mingw32-objdump -t "$BIN_DIR/$exe" | \
        grep "(ty   20)" | \
        grep "(sec  1)" | \
        grep -v "__mingw" | \
        grep -v "__gcc" | \
        grep -v "__do_global" | \
        grep -v "__main" | \
        grep -v "CRTStartup" | \
        grep -v "_setargv" | \
        grep -v "__dyn_tls" | \
        grep -v "__tlregdtor" | \
        grep -v "_matherr" | \
        grep -v "atexit" | \
        awk '{print $8}' > "$output"
    
    echo "  Extracted $(wc -l < "$output") functions to $output"
}

# Extract for each test executable
mkdir -p addresses

extract_addresses "nested_loops_x64.exe" "addresses/nested_loops_addrs.txt"
extract_addresses "switch_case_x64.exe" "addresses/switch_case_addrs.txt"
extract_addresses "recursion_x64.exe" "addresses/recursion_addrs.txt"
extract_addresses "complex_structs_x64.exe" "addresses/complex_structs_addrs.txt"
extract_addresses "function_pointers_x64.exe" "addresses/function_pointers_addrs.txt"
extract_addresses "virtual_functions_x64.exe" "addresses/virtual_functions_addrs.txt"

echo -e "\n${GREEN}✓ Address extraction complete${NC}"
echo -e "\nAddress files created in: ${YELLOW}addresses/${NC}"
