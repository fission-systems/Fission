#!/bin/bash

# Build script for Medium C decompilation test binaries
# Uses MinGW-w64 cross-compiler on Linux/macOS
# For medium-sized PE binaries with more complex algorithms

set -e

LANG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_DIR="$(cd "${LANG_DIR}/../.." && pwd)/binary/c"
SOURCE_SUBDIR="${LANG_DIR}"

# Create binary directory if it doesn't exist
mkdir -p "$BINARY_DIR"

# MinGW-w64 cross-compiler
MINGW_CC="${MINGW_CC:-x86_64-w64-mingw32-gcc}"

# Check if MinGW compiler is available
if ! command -v "$MINGW_CC" &> /dev/null; then
    echo "Error: MinGW compiler not found: $MINGW_CC"
    echo "Please install MinGW-w64:"
    echo "  macOS: brew install mingw-w64"
    echo "  Linux: apt-get install mingw-w64"
    exit 1
fi

# Compilation flags
# -O2: Optimization level 2
# -g: Include debug symbols
# -m64: Target 64-bit architecture
# -static: Link statically
# -lm: Link math library
CFLAGS="-O2 -g -m64 -static -lm"

echo "Building Medium C binaries from source files in $SOURCE_SUBDIR"
echo "=============================================================="

# Build all .c files
find "$SOURCE_SUBDIR" -maxdepth 1 -name "*.c" | sort | while read source_file; do
    filename=$(basename "$source_file" .c)
    output_binary="${BINARY_DIR}/${filename}.exe"
    
    echo "Compiling: $filename"
    echo "  Source: $source_file"
    echo "  Output: $output_binary"
    
    "$MINGW_CC" $CFLAGS "$source_file" -o "$output_binary"
    
    if [ -f "$output_binary" ]; then
        size=$(ls -lh "$output_binary" | awk '{print $5}')
        echo "  ✓ Build successful ($size)"
    else
        echo "  ✗ Build failed"
        exit 1
    fi
    echo ""
done

echo "=============================================================="
echo "Build complete! Medium C binaries:"
ls -lh "$BINARY_DIR"/*.exe 2>/dev/null || echo "No binaries found"
