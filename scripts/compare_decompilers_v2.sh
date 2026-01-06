#!/bin/bash
# Compare Ghidra vs Fission Decompiler (Enhanced Version with Text Extraction)
# Usage: ./compare_decompilers_v2.sh <binary> <address> [output.json]
#        ./compare_decompilers_v2.sh -m <binary> <address_file> <output_dir>

set -e

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Default values
BATCH_MODE=false
GENERATE_HTML=false
TIMEOUT_SECONDS=600  # Increased to 10 minutes for FID database loading
MAX_RETRIES=2

# Parse options
while getopts "mht:" opt; do
    case $opt in
        m) BATCH_MODE=true ;;
        h) GENERATE_HTML=true ;;
        t) TIMEOUT_SECONDS="$OPTARG" ;;
        \?) echo "Invalid option: -$OPTARG" >&2; exit 1 ;;
    esac
done
shift $((OPTIND-1))

BINARY="$1"
ADDRESS_OR_FILE="$2"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_BIN="python3"
if [ -x "$SCRIPT_DIR/../.venv/bin/python" ]; then
    PYTHON_BIN="$SCRIPT_DIR/../.venv/bin/python"
fi
LIBDECOMP_DIR="$SCRIPT_DIR/../ghidra_decompiler/build"
DYLD_PATH_PREFIX="DYLD_LIBRARY_PATH=$LIBDECOMP_DIR"
if [ -n "$DYLD_LIBRARY_PATH" ]; then
    DYLD_PATH_PREFIX="DYLD_LIBRARY_PATH=$LIBDECOMP_DIR:$DYLD_LIBRARY_PATH"
fi

# Check if fission_cli is built, if so use it directly for faster execution
# Prefer debug build for now as release build has decompiler issues
FISSION_BIN="$SCRIPT_DIR/../target/debug/fission_cli"
if [ ! -f "$FISSION_BIN" ]; then
    FISSION_BIN="$SCRIPT_DIR/../target/release/fission_cli"
fi
if [ -f "$FISSION_BIN" ]; then
    FISSION_CMD="$FISSION_BIN"
else
    FISSION_CMD="cargo run --quiet --bin fission_cli --"
fi

# Create timestamped result folder if output not specified
if [ -z "$3" ]; then
    TIMESTAMP=$(date +%Y%m%d%H%M)
    RESULT_DIR="$SCRIPT_DIR/result/${TIMESTAMP}_result"
    mkdir -p "$RESULT_DIR"
    OUTPUT="$RESULT_DIR/comparison.json"
    echo -e "${GREEN}Created result folder: $RESULT_DIR${NC}"
else
    OUTPUT="$3"
    mkdir -p "$(dirname "$OUTPUT")"
fi

# Show usage
if [ -z "$BINARY" ] || [ -z "$ADDRESS_OR_FILE" ]; then
    echo -e "${CYAN}Usage:${NC}"
    echo "  Single function:  $0 <binary> <address> [output.json]"
    echo "  Batch mode:       $0 -m <binary> <address_file> <output_dir>"
    echo ""
    echo -e "${CYAN}Options:${NC}"
    echo "  -m    Batch mode: process multiple addresses from file"
    echo "  -h    Generate HTML report (batch mode only)"
    echo "  -t N  Set timeout in seconds (default: 300)"
    echo ""
    echo -e "${CYAN}Examples:${NC}"
    echo "  $0 test.exe 0x401000"
    echo "  $0 -m test.exe addresses.txt results/"
    echo "  $0 -m -h test.exe addresses.txt results/"
    exit 1
fi

if [ ! -f "$BINARY" ]; then
    echo -e "${RED}Error: Binary file not found: $BINARY${NC}"
    exit 1
fi

    # Function to run command with timeout and retry
run_with_timeout() {
    local cmd="$1"
    local output_file="$2"
    local desc="$3"
    local retry=0
    
    while [ $retry -le $MAX_RETRIES ]; do
        if [ $retry -gt 0 ]; then
            echo -e "${YELLOW}  Retry $retry/$MAX_RETRIES...${NC}"
        fi
        
        if "$PYTHON_BIN" - "$TIMEOUT_SECONDS" "$output_file" "$cmd" << 'PYEOF'
import subprocess
import sys

timeout = int(sys.argv[1])
output_path = sys.argv[2]
cmd = sys.argv[3]

with open(output_path, "w") as output:
    try:
        completed = subprocess.run(
            cmd,
            shell=True,
            stdout=output,
            stderr=subprocess.STDOUT,
            timeout=timeout,
        )
        sys.exit(completed.returncode)
    except subprocess.TimeoutExpired:
        sys.exit(124)
PYEOF
        then
            return 0
        else
            local exit_code=$?
            if [ $exit_code -eq 142 ] || [ $exit_code -eq 124 ]; then
                echo -e "${RED}  Timeout after ${TIMEOUT_SECONDS}s${NC}"
            else
                echo -e "${RED}  Failed with exit code $exit_code${NC}"
            fi
            retry=$((retry + 1))
        fi
    done
    
    echo -e "${RED}  All retries exhausted for: $desc${NC}"
    return 1
}

# Function to compare single address
compare_single() {
    local binary="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"  # Convert to absolute path
    local address="$2"
    local output_json="$3"
    
    echo -e "${CYAN}==========================================${NC}"
    echo -e "${CYAN}Decompiler Comparison${NC}"
    echo -e "  Binary:  ${GREEN}$binary${NC}"
    echo -e "  Address: ${GREEN}$address${NC}"
    echo -e "  Output:  ${GREEN}$output_json${NC}"
    echo -e "${CYAN}==========================================${NC}"
    echo ""
    
    # Temporary files for capturing output
    local GHIDRA_OUT=$(mktemp)
    local FISSION_ASM_OUT=$(mktemp)
    local FISSION_DECOMP_OUT=$(mktemp)
    
    # Cleanup on exit
    trap "rm -f $GHIDRA_OUT $FISSION_ASM_OUT $FISSION_DECOMP_OUT" RETURN
    
    # Run decompilers
    echo -e "${BLUE}[1/3] Running Ghidra Decompiler (PyGhidra)...${NC}"
    if ! run_with_timeout \
        "python3 '$SCRIPT_DIR/pyghidra_decompile.py' '$binary' '$address'" \
        "$GHIDRA_OUT" \
        "Ghidra"; then
        echo "ERROR: Ghidra decompilation failed" > "$GHIDRA_OUT"
    fi
    
    echo -e "${BLUE}[2/3] Running Fission Disassembler...${NC}"
    if ! run_with_timeout \
        "$DYLD_PATH_PREFIX $FISSION_CMD '$binary' --disasm-function '$address' 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep -v '^╔\|^║\|^╚\|^Usage:\|^📊\|^🔍\|^⚙️\|^💾\|^📚\|^Examples:\|^  -\|^  fission\|^Information:\|^Analysis:\|^Decompilation:\|^Output:' | sed '/^\$/N;/^\n\$/d'" \
        "$FISSION_ASM_OUT" \
        "Fission ASM"; then
        echo "ERROR: Fission disassembly failed" > "$FISSION_ASM_OUT"
    fi
    
    
    echo -e "${BLUE}[3/3] Running Fission Decompiler...${NC}"
    # Run Fission decompiler from project root directory
    local FISSION_DECOMP_RAW=$(mktemp)
    local PROJECT_ROOT="$SCRIPT_DIR/.."
    if run_with_timeout \
        "cd '$PROJECT_ROOT' && $DYLD_PATH_PREFIX $FISSION_CMD '$binary' --decomp '$address'" \
        "$FISSION_DECOMP_RAW" \
        "Fission Decomp"; then
        # Extract just the decompiled code
        awk '/^\/\/ ====/{flag=1} flag; /^}/{flag=0}' "$FISSION_DECOMP_RAW" > "$FISSION_DECOMP_OUT" 2>&1
    else
        echo "ERROR: Fission decompilation failed" > "$FISSION_DECOMP_OUT"
    fi
    rm -f "$FISSION_DECOMP_RAW"
    
    # Generate JSON and text extracts
    echo ""
    echo -e "${BLUE}Generating JSON output and text extracts...${NC}"
    
    # Create output directory if it doesn't exist
    mkdir -p "$(dirname "$output_json")"
    
    # Export variables for Python script
    export GHIDRA_OUT FISSION_ASM_OUT FISSION_DECOMP_OUT
    export OUTPUT_JSON="$output_json"
    export BINARY="$binary"
    export ADDRESS="$address"
    
    python3 << 'PYEOF'
import json
import sys
import os
from datetime import datetime
import difflib
import re

# Get environment variables
ghidra_out = os.environ['GHIDRA_OUT']
fission_asm_out = os.environ['FISSION_ASM_OUT']
fission_decomp_out = os.environ['FISSION_DECOMP_OUT']
output_json = os.environ['OUTPUT_JSON']
binary = os.environ['BINARY']
address = os.environ['ADDRESS']

# Read outputs
try:
    with open(ghidra_out, "r", encoding='utf-8', errors='replace') as f:
        ghidra_full = f.read()
except:
    ghidra_full = "Error reading Ghidra output"

try:
    with open(fission_asm_out, "r", encoding='utf-8', errors='replace') as f:
        fission_asm = f.read()
except:
    fission_asm = "Error reading Fission disassembly"

try:
    with open(fission_decomp_out, "r", encoding='utf-8', errors='replace') as f:
        fission_decomp = f.read()
except:
    fission_decomp = "Error reading Fission decompilation"

# Strip any remaining ANSI codes
ansi_escape = re.compile(r'\x1b\[[0-9;]*m')
ghidra_full = ansi_escape.sub('', ghidra_full).strip()
fission_asm = ansi_escape.sub('', fission_asm).strip()
fission_decomp = ansi_escape.sub('', fission_decomp).strip()

# Parse Ghidra output to separate assembly and decompilation
ghidra_asm = ""
ghidra_decomp = ""

if "--- Assembly Listing ---" in ghidra_full and "--- Decompiled Code ---" in ghidra_full:
    parts = ghidra_full.split("--- Assembly Listing ---")
    if len(parts) > 1:
        asm_and_rest = parts[1].split("--- Decompiled Code ---")
        ghidra_asm = asm_and_rest[0].strip()
        if len(asm_and_rest) > 1:
            ghidra_decomp = asm_and_rest[1].strip()
else:
    # Fallback: treat entire output as decompilation
    ghidra_decomp = ghidra_full
    ghidra_asm = "Assembly not available"

# Calculate metrics
def analyze_code(code):
    lines = code.count('\n') + 1
    chars = len(code)
    functions = code.count('(')
    branches = sum([code.count(kw) for kw in ['if', 'while', 'for', 'switch']])
    return {"lines": lines, "chars": chars, "functions": functions, "branches": branches}

ghidra_metrics = analyze_code(ghidra_decomp)
fission_metrics = analyze_code(fission_decomp)

# Calculate similarity
ghidra_lines = ghidra_decomp.splitlines()
fission_lines = fission_decomp.splitlines()
similarity = difflib.SequenceMatcher(None, ghidra_lines, fission_lines).ratio()

# Create JSON structure
result = {
    "comparison_info": {
        "binary": binary,
        "address": address,
        "timestamp": datetime.utcnow().isoformat() + "Z",
        "metrics": {
            "ghidra": ghidra_metrics,
            "fission": fission_metrics
        },
        "similarity": round(similarity * 100, 2)
    },
    "ghidra_assembly": ghidra_asm,
    "ghidra_decompilation": ghidra_decomp,
    "fission_assembly": fission_asm,
    "fission_decompilation": fission_decomp
}

# Write JSON with proper formatting
try:
    with open(output_json, "w", encoding='utf-8') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
    print(f"✅ JSON file created successfully ({len(json.dumps(result))} bytes)")
except Exception as e:
    print(f"❌ Error writing JSON: {e}")
    sys.exit(1)

# Extract individual text files
base_path = output_json.rsplit('.', 1)[0]  # Remove .json extension

try:
    # Ghidra Assembly
    with open(f"{base_path}_ghidra_asm.txt", "w", encoding='utf-8') as f:
        f.write(ghidra_asm)
    
    # Ghidra Decompilation
    with open(f"{base_path}_ghidra_decomp.txt", "w", encoding='utf-8') as f:
        f.write(ghidra_decomp)
    
    # Fission Assembly
    with open(f"{base_path}_fission_asm.txt", "w", encoding='utf-8') as f:
        f.write(fission_asm)
    
    # Fission Decompilation
    with open(f"{base_path}_fission_decomp.txt", "w", encoding='utf-8') as f:
        f.write(fission_decomp)
    
    print(f"✅ Text extracts created successfully")
except Exception as e:
    print(f"⚠️  Warning: Could not create text extracts: {e}")

PYEOF
    
    # Display summary
    echo ""
    echo -e "${GREEN}=========================================="
    echo "✅ Comparison Complete"
    echo -e "==========================================${NC}"
    
    if [ -f "$output_json" ]; then
        local file_size=$(du -h "$output_json" | cut -f1)
        echo -e "  JSON: ${CYAN}$output_json${NC} ($file_size)"
        
        # List text extracts
        local base="${output_json%.json}"
        echo -e "  ${CYAN}Text Extracts:${NC}"
        echo -e "    • ${base}_ghidra_asm.txt"
        echo -e "    • ${base}_ghidra_decomp.txt"
        echo -e "    • ${base}_fission_asm.txt"
        echo -e "    • ${base}_fission_decomp.txt"
        
        # Extract and display metadata
        if command -v jq &> /dev/null; then
            echo ""
            echo -e "${YELLOW}Metrics:${NC}"
            jq -r '.comparison_info.metrics | 
                "  Ghidra:  \(.ghidra.lines) lines, \(.ghidra.branches) branches\n  Fission: \(.fission.lines) lines, \(.fission.branches) branches"' "$output_json"
            
            local similarity=$(jq -r '.comparison_info.similarity' "$output_json")
            echo -e "  ${YELLOW}Similarity: ${similarity}%${NC}"
        fi
    else
        echo -e "${RED}❌ Error: Output file not created${NC}"
        return 1
    fi
    
    return 0
}

# Main execution (single mode only for now)
if [ "$BATCH_MODE" = true ]; then
    echo -e "${RED}Batch mode coming soon - use original compare_decompilers.sh${NC}"
    exit 1
else
    compare_single "$BINARY" "$ADDRESS_OR_FILE" "$OUTPUT"
    
    # Show helpful commands
    if [ -f "$OUTPUT" ]; then
        echo ""
        echo -e "${CYAN}Quick Commands:${NC}"
        BASE_NAME="${OUTPUT%.json}"
        echo "  # View decompilation"
        echo "  cat ${BASE_NAME}_ghidra_decomp.txt"
        echo "  cat ${BASE_NAME}_fission_decomp.txt"
        echo ""
        echo "  # Compare decompilation"
        echo "  diff ${BASE_NAME}_ghidra_decomp.txt ${BASE_NAME}_fission_decomp.txt"
        echo ""
        echo "  # View assembly"
        echo "  cat ${BASE_NAME}_ghidra_asm.txt"
        echo "  cat ${BASE_NAME}_fission_asm.txt"
    fi
fi
