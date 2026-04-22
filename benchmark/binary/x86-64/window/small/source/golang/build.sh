#!/bin/bash

# Build script for Go decompilation test binaries
# Uses MinGW-w64 for cross-compilation

set -e

LANG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_DIR="$(cd "${LANG_DIR}/../.." && pwd)/binary/golang"
SOURCE_SUBDIR="${LANG_DIR}"

# Create binary directory if it doesn't exist
mkdir -p "$BINARY_DIR"

# Check if Go compiler is available
if ! command -v go &> /dev/null; then
    echo "Error: Go compiler not found"
    echo "Please install Go from https://golang.org/dl/"
    exit 1
fi

# Go version
echo "Go version: $(go version)"

# Find all .go files in source directory and compile them
echo "Building Go binaries from source files in $SOURCE_SUBDIR"
echo "==========================================="

# Go requires main.go or separate build approach
find "$SOURCE_SUBDIR" -maxdepth 1 -name "*.go" | sort | while read source_file; do
    # Extract filename without extension
    filename=$(basename "$source_file" .go)
    output_binary="${BINARY_DIR}/${filename}.exe"
    
    echo "Building: $filename"
    echo "  Source: $source_file"
    echo "  Output: $output_binary"
    
    # Cross-compile for Windows x86-64
    GOOS=windows GOARCH=amd64 CGO_ENABLED=1 CC=x86_64-w64-mingw32-gcc \
        go build -o "$output_binary" "$source_file"
    
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
echo "Go Build complete! Binaries:"
ls -lh "$BINARY_DIR"/*.exe 2>/dev/null || echo "No Go binaries found"
