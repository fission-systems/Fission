#!/bin/bash

# Build script for Medium Rust decompilation test binaries

set -e

LANG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_DIR="$(cd "${LANG_DIR}/../.." && pwd)/binary/rustlang"
SOURCE_SUBDIR="${LANG_DIR}"

mkdir -p "$BINARY_DIR"

if ! command -v rustc &> /dev/null; then
    echo "Error: Rust compiler not found"
    echo "Please install Rust from https://rustup.rs/"
    exit 1
fi

echo "Rust version: $(rustc --version)"

if ! rustup target list | grep -q "x86_64-pc-windows-gnu (installed)"; then
    echo "Installing x86_64-pc-windows-gnu target..."
    rustup target add x86_64-pc-windows-gnu
fi

echo "Building Medium Rust binaries from source files in $SOURCE_SUBDIR"
echo "=============================================================="

if [ -f "${SOURCE_SUBDIR}/Cargo.toml" ]; then
    echo "Found Cargo.toml - building Rust project"
    cd "$SOURCE_SUBDIR"
    cargo build --release --target x86_64-pc-windows-gnu
    echo "✓ Build successful"
else
    echo "Building individual Rust files with rustc"
    
    find "$SOURCE_SUBDIR" -maxdepth 1 -name "*.rs" | sort | while read source_file; do
        filename=$(basename "$source_file" .rs)
        output_binary="${BINARY_DIR}/${filename}.exe"
        
        echo "Compiling: $filename"
        echo "  Source: $source_file"
        echo "  Output: $output_binary"
        
        rustc -O --target x86_64-pc-windows-gnu "$source_file" -o "$output_binary"
        
        if [ -f "$output_binary" ]; then
            size=$(ls -lh "$output_binary" | awk '{print $5}')
            echo "  ✓ Build successful ($size)"
        else
            echo "  ✗ Build failed"
            exit 1
        fi
        echo ""
    done
fi

echo "=============================================================="
echo "Build complete! Medium Rust binaries:"
ls -lh "$BINARY_DIR"/*.exe 2>/dev/null || echo "No binaries found"
