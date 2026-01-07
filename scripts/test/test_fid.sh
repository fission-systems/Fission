#!/bin/bash
# Test FID database loading

set -e

cd "$(dirname "$0")"/../..

echo "Testing FID Database Loading"
echo "============================="
echo ""

# Check if FID databases exist
FID_DIR="ghidra/funtionID"

if [ ! -d "$FID_DIR" ]; then
    echo "ERROR: FID directory not found: $FID_DIR"
    exit 1
fi

echo "Available FID databases:"
ls -lh "$FID_DIR"/*.fidbf 2>/dev/null || echo "  None found"
echo ""

# Build test binary
echo "Building Fission with native decompiler..."
cargo build --release --features native_decomp

echo ""
echo "FID databases ready for testing!"
echo ""
echo "To test FID matching, use:"
echo "  cargo run --release --features native_decomp --bin fission -- \\"
echo "    --headless test/your_binary.exe 0x140001000"
