#!/bin/bash

# Build script for C++ decompilation test binaries
# Uses MinGW-w64 cross-compiler on Linux/macOS

set -e

LANG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_DIR="$(cd "${LANG_DIR}/../.." && pwd)/binary/cpp"
SOURCE_SUBDIR="${LANG_DIR}"

# Create binary directory if it doesn't exist
mkdir -p "$BINARY_DIR"

# MinGW-w64 cross-compiler
MINGW_CXX="${MINGW_CXX:-x86_64-w64-mingw32-g++}"

# Check if MinGW compiler is available
if ! command -v "$MINGW_CXX" &> /dev/null; then
    echo "Error: MinGW C++ compiler not found: $MINGW_CXX"
    echo "Please install MinGW-w64:"
    echo "  macOS: brew install mingw-w64"
    echo "  Linux: apt-get install mingw-w64"
    exit 1
fi

# Compilation flags
CXXFLAGS="-O2 -g -m64 -static -lm -std=c++17"

# Find all .cpp files in source directory and compile them
echo "Building C++ binaries from source files in $SOURCE_SUBDIR"
echo "==========================================="

find "$SOURCE_SUBDIR" -maxdepth 1 -name "*.cpp" | sort | while read source_file; do
    # Extract filename without extension
    filename=$(basename "$source_file" .cpp)
    output_binary="${BINARY_DIR}/${filename}.exe"
    
    echo "Compiling: $filename"
    echo "  Source: $source_file"
    echo "  Output: $output_binary"
    
    "$MINGW_CXX" $CXXFLAGS "$source_file" -o "$output_binary"
    
    if [ -f "$output_binary" ]; then
        size=$(ls -lh "$output_binary" | awk '{print $5}')
        echo "  ✓ Build successful ($size)"
    else
        echo "  ✗ Build failed"
        exit 1
    fi
    echo ""
done

echo "==========================================="
echo "C++ Build complete! Binaries:"
ls -lh "$BINARY_DIR"/*.exe 2>/dev/null || echo "No C++ binaries found"
