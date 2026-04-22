#!/bin/bash

# Build script for Rust decompilation test binaries
# Uses MinGW-w64 for cross-compilation

set -e

LANG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_DIR="$(cd "${LANG_DIR}/../.." && pwd)/binary/rustlang"
SOURCE_SUBDIR="${LANG_DIR}"

# Create binary directory if it doesn't exist
mkdir -p "$BINARY_DIR"

# Check if Rust compiler is available
if ! command -v rustc &> /dev/null; then
    echo "Error: Rust compiler not found"
    echo "Please install Rust from https://rustup.rs/"
    exit 1
fi

# Rust version
echo "Rust version: $(rustc --version)"

# Check if x86_64-pc-windows-gnu target is installed
if ! rustup target list | grep -q "x86_64-pc-windows-gnu (installed)"; then
    echo "Installing x86_64-pc-windows-gnu target..."
    rustup target add x86_64-pc-windows-gnu
fi

# Find all Rust projects or files
echo "Building Rust binaries from source files in $SOURCE_SUBDIR"
echo "==========================================="

# Look for Cargo.toml files first (Rust projects)
if [ -f "${SOURCE_SUBDIR}/Cargo.toml" ]; then
    echo "Found Cargo.toml - building Rust project"
    cd "$SOURCE_SUBDIR"
    cargo build --release --target x86_64-pc-windows-gnu
    
    # Copy built binary to binary directory
    binary_name=$(grep '^name' Cargo.toml | head -1 | cut -d'"' -f2 | tr '-' '_')
    src_binary="target/x86_64-pc-windows-gnu/release/${binary_name}.exe"
    
    if [ -f "$src_binary" ]; then
        cp "$src_binary" "${BINARY_DIR}/${binary_name}.exe"
        size=$(ls -lh "${BINARY_DIR}/${binary_name}.exe" | awk '{print $5}')
        echo "  ✓ Build successful ($size)"
    fi
else
    # Single file compilation
    find "$SOURCE_SUBDIR" -maxdepth 1 -name "*.rs" -not -name "main.rs" | sort | while read source_file; do
        # Extract filename without extension
        filename=$(basename "$source_file" .rs)
        output_binary="${BINARY_DIR}/${filename}.exe"
        
        echo "Compiling: $filename"
        echo "  Source: $source_file"
        echo "  Output: $output_binary"
        
        rustc --edition 2021 -O --target x86_64-pc-windows-gnu \
            "$source_file" -o "$output_binary"
        
        if [ -f "$output_binary" ]; then
            size=$(ls -lh "$output_binary" | awk '{print $5}')
            echo "  ✓ Build successful ($size)"
        else
            echo "  ✗ Build failed"
        fi
        echo ""
    done
fi

echo "==========================================="
echo "Rust Build complete! Binaries:"
ls -lh "$BINARY_DIR"/*.exe 2>/dev/null || echo "No Rust binaries found"
