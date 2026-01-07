#!/bin/bash
# Compare Ghidra vs Fission Decompiler
# Usage: ./compare_decompilers.sh <binary> <address>

set -e

BINARY="$1"
ADDRESS="$2"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

if [ -z "$BINARY" ] || [ -z "$ADDRESS" ]; then
    echo "Usage: $0 <binary> <address>"
    echo "Example: $0 test/struct_test 0x1000004b0"
    exit 1
fi

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary file not found: $BINARY"
    exit 1
fi

echo "=========================================="
echo "Decompiler Comparison"
echo "Binary: $BINARY"
echo "Address: $ADDRESS"
echo "=========================================="
echo ""

echo "[1/3] Running Ghidra Decompiler (PyGhidra)..."
echo "---"
python3 "$SCRIPTS_DIR/ghidra/pyghidra_decompile.py" "$BINARY" "$ADDRESS"

echo ""
echo ""
echo "[2/3] Running Fission Disassembler..."
echo "---"
cargo run --quiet --bin fission_cli -- "$BINARY" --asm "$ADDRESS" -n 50 2>/dev/null

echo ""
echo ""
echo "[3/3] Running Fission Decompiler..."
echo "---"
cargo run --quiet --bin fission_cli -- "$BINARY" --decomp "$ADDRESS" 2>/dev/null

echo ""
echo "=========================================="
echo "Comparison Complete"
echo "=========================================="
