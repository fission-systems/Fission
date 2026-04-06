#!/bin/bash
# Test FID database loading

set -e

cd "$(dirname "$0")"/../..

echo "Testing FID Database Loading"
echo "============================="
echo ""

# Check if FID databases exist
FID_DIR="utils/signatures/fid"

if [ ! -d "$FID_DIR" ]; then
    echo "ERROR: FID directory not found: $FID_DIR"
    exit 1
fi

echo "Available FID databases:"
ls -lh "$FID_DIR"/*.fidbf 2>/dev/null || echo "  None found"
echo ""

# Build test binary
echo "Building Fission CLI..."
cargo build --release --bin fission_cli

echo ""
echo "FID databases ready for testing!"
echo ""
echo "To test FID matching, use:"
echo "  cargo run --release --bin fission_cli -- \\"
echo "    --headless examples/your_binary.exe 0x140001000"
