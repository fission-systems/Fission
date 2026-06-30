#!/bin/bash

# Master build script for all Windows x86-64 Medium decompilation test binaries
# Supports: C, C++, Go, Rust

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_DIR="${SCRIPT_DIR}/source"
BINARY_DIR="${SCRIPT_DIR}/binary"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "============================================"
echo "Fission Medium Binary Build System"
echo "Windows x86-64 Test Binaries"
echo "============================================"
echo ""

# Function to print section header
print_section() {
    echo -e "${YELLOW}>>> $1${NC}"
}

# Function to print success
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

# Function to print error
print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Check which languages to build
build_c=false
build_cpp=false
build_go=false
build_rust=false

if [ $# -eq 0 ]; then
    # Build all by default
    build_c=true
    build_cpp=true
    build_go=true
    build_rust=true
else
    case "$1" in
        all)
            build_c=true
            build_cpp=true
            build_go=true
            build_rust=true
            ;;
        c)
            build_c=true
            ;;
        cpp|c++)
            build_cpp=true
            ;;
        go|golang)
            build_go=true
            ;;
        rust|rustlang)
            build_rust=true
            ;;
        *)
            print_error "Unknown language: $1"
            echo "Usage: $0 [c|cpp|go|rust|all]"
            exit 1
            ;;
    esac
fi

# Create binary directories
mkdir -p "$BINARY_DIR"/{c,cpp,golang,rustlang}

# Build C
if [ "$build_c" = true ]; then
    print_section "Building C binaries..."
    if [ -f "${SOURCE_DIR}/c/build.sh" ]; then
        bash "${SOURCE_DIR}/c/build.sh"
        print_success "C build complete"
    else
        print_error "C build script not found"
    fi
    echo ""
fi

# Build C++
if [ "$build_cpp" = true ]; then
    print_section "Building C++ binaries..."
    if [ -f "${SOURCE_DIR}/cpp/build.sh" ]; then
        bash "${SOURCE_DIR}/cpp/build.sh"
        print_success "C++ build complete"
    else
        print_error "C++ build script not found"
    fi
    echo ""
fi

# Build Go
if [ "$build_go" = true ]; then
    print_section "Building Go binaries..."
    if [ -f "${SOURCE_DIR}/golang/build.sh" ]; then
        bash "${SOURCE_DIR}/golang/build.sh"
        print_success "Go build complete"
    else
        print_error "Go build script not found"
    fi
    echo ""
fi

# Build Rust
if [ "$build_rust" = true ]; then
    print_section "Building Rust binaries..."
    if [ -f "${SOURCE_DIR}/rustlang/build.sh" ]; then
        bash "${SOURCE_DIR}/rustlang/build.sh"
        print_success "Rust build complete"
    else
        print_error "Rust build script not found"
    fi
    echo ""
fi

# Summary
print_section "Build Summary"
echo ""
echo "Binary Output Directory: $BINARY_DIR"
echo ""
echo "Compiled Binaries:"

for lang_dir in c cpp golang rustlang; do
    if [ -d "$BINARY_DIR/$lang_dir" ]; then
        count=$(find "$BINARY_DIR/$lang_dir" -maxdepth 1 -name "*.exe" 2>/dev/null | wc -l)
        if [ $count -gt 0 ]; then
            total_size=$(du -sh "$BINARY_DIR/$lang_dir" 2>/dev/null | awk '{print $1}')
            echo ""
            echo "  $lang_dir:"
            find "$BINARY_DIR/$lang_dir" -maxdepth 1 -name "*.exe" -exec ls -lh {} \; | awk '{print "    " $9 " (" $5 ")"}'
            echo "    Total: $total_size"
        fi
    fi
done

echo ""
print_success "All requested builds completed!"
echo ""
echo "Next steps:"
echo "  1. Review binaries in: $BINARY_DIR"
echo "  2. Test with Fission CLI: fission_cli decomp binary/<language>/<binary>.exe"
echo "  3. Run benchmarks: cargo bench -p fission-analysis --bench benchmark"
