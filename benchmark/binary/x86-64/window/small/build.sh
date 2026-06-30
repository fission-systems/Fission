#!/bin/bash

# Master build script for all Windows x86-64 decompilation test binaries
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
echo "Fission Multi-Language Build System"
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
    # Build only specified languages
    for lang in "$@"; do
        case "$lang" in
            c|C|clang|Clang) build_c=true ;;
            cpp|C++|c++|clang++) build_cpp=true ;;
            go|Go|golang) build_go=true ;;
            rust|Rust|rustlang) build_rust=true ;;
            all) 
                build_c=true
                build_cpp=true
                build_go=true
                build_rust=true
                ;;
            *)
                echo "Unknown language: $lang"
                echo "Usage: $0 [c|cpp|go|rust|all]"
                exit 1
                ;;
        esac
    done
fi

# Build C
if [ "$build_c" = true ]; then
    print_section "Building C (Clang)"
    if [ -f "${SOURCE_DIR}/c/build.sh" ]; then
        bash "${SOURCE_DIR}/c/build.sh"
        print_success "C build completed"
    else
        print_error "C build script not found"
    fi
    echo ""
fi

# Build C++
if [ "$build_cpp" = true ]; then
    print_section "Building C++ (Clang++)"
    if [ -f "${SOURCE_DIR}/cpp/build.sh" ]; then
        bash "${SOURCE_DIR}/cpp/build.sh"
        print_success "C++ build completed"
    else
        print_error "C++ build script not found"
    fi
    echo ""
fi

# Build Go
if [ "$build_go" = true ]; then
    print_section "Building Go"
    if [ -f "${SOURCE_DIR}/golang/build.sh" ]; then
        bash "${SOURCE_DIR}/golang/build.sh"
        print_success "Go build completed"
    else
        print_error "Go build script not found"
    fi
    echo ""
fi

# Build Rust
if [ "$build_rust" = true ]; then
    print_section "Building Rust"
    if [ -f "${SOURCE_DIR}/rustlang/build.sh" ]; then
        bash "${SOURCE_DIR}/rustlang/build.sh"
        print_success "Rust build completed"
    else
        print_error "Rust build script not found"
    fi
    echo ""
fi

# Summary
print_section "Build Summary"
echo "Source directory: ${SOURCE_DIR}"
echo "Binary directory: ${BINARY_DIR}"
echo ""

for lang_dir in c cpp golang rustlang; do
    if [ -d "${BINARY_DIR}/${lang_dir}" ]; then
        count=$(find "${BINARY_DIR}/${lang_dir}" -type f | wc -l)
        if [ "$count" -gt 0 ]; then
            size=$(du -sh "${BINARY_DIR}/${lang_dir}" | awk '{print $1}')
            echo "  ${lang_dir}: ${count} file(s), ${size}"
        fi
    fi
done

echo ""
print_success "All builds completed!"
