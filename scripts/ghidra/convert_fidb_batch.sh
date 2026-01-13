#!/bin/bash
# Batch convert all .fidb files to .fidbf format using Ghidra headless
# Usage: ./convert_fidb_batch.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FISSION_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GHIDRA_HOME="${GHIDRA_HOME:-$FISSION_ROOT/vendor/ghidra/ghidra_11.4.2_PUBLIC}"

# Input/output directories
FIDB_INPUT_DIR="$FISSION_ROOT/utils/signatures/fid"
FIDBF_OUTPUT_DIR="$FISSION_ROOT/utils/signatures/fid"

# Temp project for headless analysis
TEMP_PROJECT="/tmp/fission_fidb_convert"
rm -rf "$TEMP_PROJECT"
mkdir -p "$TEMP_PROJECT"

# Script path
CONVERT_SCRIPT="$SCRIPT_DIR/ConvertFidbToFidbf.java"

echo "=========================================="
echo "FIDB to FIDBF Batch Converter"
echo "=========================================="
echo "Ghidra: $GHIDRA_HOME"
echo "Input:  $FIDB_INPUT_DIR"
echo "Output: $FIDBF_OUTPUT_DIR"
echo "=========================================="

# Check Ghidra exists
if [ ! -d "$GHIDRA_HOME" ]; then
    echo "ERROR: Ghidra not found at $GHIDRA_HOME"
    echo "Set GHIDRA_HOME environment variable or install Ghidra"
    exit 1
fi

# Find all .fidb files
FIDB_FILES=$(find "$FIDB_INPUT_DIR" -name "*.fidb" -type f 2>/dev/null || true)

if [ -z "$FIDB_FILES" ]; then
    echo "No .fidb files found in $FIDB_INPUT_DIR"
    exit 0
fi

COUNT=0
SUCCESS=0
FAILED=0

for fidb_file in $FIDB_FILES; do
    COUNT=$((COUNT + 1))
    basename=$(basename "$fidb_file" .fidb)
    output_file="$FIDBF_OUTPUT_DIR/${basename}.fidbf"
    
    # Skip if already converted
    if [ -f "$output_file" ]; then
        echo "[$COUNT] SKIP: $basename.fidbf already exists"
        continue
    fi
    
    echo "[$COUNT] Converting: $basename.fidb -> $basename.fidbf"
    
    # Run Ghidra headless with the conversion script
    "$GHIDRA_HOME/support/analyzeHeadless" \
        "$TEMP_PROJECT" temp_project \
        -noanalysis \
        -scriptPath "$SCRIPT_DIR" \
        -postScript ConvertFidbToFidbf.java "$fidb_file" "$output_file" \
        2>&1 | grep -E "(Converting|SUCCESS|FAILED|ERROR)" || true
    
    if [ -f "$output_file" ]; then
        SUCCESS=$((SUCCESS + 1))
        echo "  -> SUCCESS"
    else
        FAILED=$((FAILED + 1))
        echo "  -> FAILED"
    fi
done

# Cleanup
rm -rf "$TEMP_PROJECT"

echo "=========================================="
echo "Conversion Complete"
echo "Total: $COUNT, Success: $SUCCESS, Failed: $FAILED"
echo "=========================================="
