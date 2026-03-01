#!/usr/bin/env bash
# build_test_binaries.sh
# Builds all test sources into Mach-O (ARM64 + x86_64), ELF (x86_64), PE (x86_64)
# Optimization levels: O0 (debug-like) and O2 (release-like)
#
# Requirements:
#   - clang (native, for Mach-O)
#   - x86_64-linux-musl-g++ (brew install musl-cross, for ELF)
#   - x86_64-w64-mingw32-g++ (brew install mingw-w64, for PE)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
SRC_DIR="$ROOT_DIR/examples/sources"
SAMPLE_DIR="$ROOT_DIR/samples"

SOURCES=(
    test_arithmetic_idioms
    test_control_flow
    test_structs_classes
    test_calling_conventions
    test_string_memory
    test_advanced_patterns
    test_real_world_algorithms
)

OPT_LEVELS=("O0" "O2")

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

ok()   { echo -e "${GREEN}[OK]${NC}   $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; }
info() { echo -e "${YELLOW}[INFO]${NC} $1"; }

# Ensure output directories exist
mkdir -p "$SAMPLE_DIR/macos/arm64"
mkdir -p "$SAMPLE_DIR/macos/x64"
mkdir -p "$SAMPLE_DIR/linux/x64"
mkdir -p "$SAMPLE_DIR/windows/x64"

TOTAL=0
SUCCESS=0
FAILED=0

compile() {
    local compiler="$1"
    local src="$2"
    local out="$3"
    local opt="$4"
    shift 4
    local extra_flags=("$@")

    TOTAL=$((TOTAL + 1))
    if "$compiler" "-$opt" "${extra_flags[@]}" -o "$out" "$src" 2>&1; then
        ok "$out"
        SUCCESS=$((SUCCESS + 1))
    else
        fail "$out (exit $?)"
        FAILED=$((FAILED + 1))
    fi
}

echo "========================================"
echo "  Fission Test Binary Builder"
echo "========================================"
echo ""

# ───── Mach-O ARM64 (native) ─────
info "Building Mach-O ARM64..."
for src_name in "${SOURCES[@]}"; do
    for opt in "${OPT_LEVELS[@]}"; do
        local_flags=(-arch arm64 -std=c++17 -fno-exceptions -Wall)
        # Only add -fno-rtti for tests that don't use RTTI/virtual
        if [[ "$src_name" != "test_structs_classes" ]]; then
            local_flags+=(-fno-rtti)
        fi
        compile clang++ \
            "$SRC_DIR/${src_name}.cpp" \
            "$SAMPLE_DIR/macos/arm64/${src_name}_arm64_${opt}" \
            "$opt" \
            "${local_flags[@]}"
    done
done
echo ""

# ───── Mach-O x86_64 (cross on Apple Silicon) ─────
info "Building Mach-O x86_64..."
for src_name in "${SOURCES[@]}"; do
    for opt in "${OPT_LEVELS[@]}"; do
        local_flags=(-arch x86_64 -std=c++17 -fno-exceptions -Wall)
        if [[ "$src_name" != "test_structs_classes" ]]; then
            local_flags+=(-fno-rtti)
        fi
        compile clang++ \
            "$SRC_DIR/${src_name}.cpp" \
            "$SAMPLE_DIR/macos/x64/${src_name}_x64_${opt}" \
            "$opt" \
            "${local_flags[@]}"
    done
done
echo ""

# ───── ELF x86_64 (musl-cross) ─────
if command -v x86_64-linux-musl-g++ &>/dev/null; then
    info "Building ELF x86_64 (musl-static)..."
    for src_name in "${SOURCES[@]}"; do
        for opt in "${OPT_LEVELS[@]}"; do
            local_flags=(-static -std=c++17 -fno-exceptions -Wall)
            if [[ "$src_name" != "test_structs_classes" ]]; then
                local_flags+=(-fno-rtti)
            fi
            compile x86_64-linux-musl-g++ \
                "$SRC_DIR/${src_name}.cpp" \
                "$SAMPLE_DIR/linux/x64/${src_name}_x64_${opt}" \
                "$opt" \
                "${local_flags[@]}"
        done
    done
else
    info "SKIP ELF x86_64 — x86_64-linux-musl-g++ not found"
    info "Install: brew install FiloSottile/musl-cross/musl-cross"
fi
echo ""

# ───── PE x86_64 (MinGW) ─────
if command -v x86_64-w64-mingw32-g++ &>/dev/null; then
    info "Building PE x86_64 (MinGW)..."
    for src_name in "${SOURCES[@]}"; do
        for opt in "${OPT_LEVELS[@]}"; do
            local_flags=(-static -std=c++17 -fno-exceptions -Wall)
            if [[ "$src_name" != "test_structs_classes" ]]; then
                local_flags+=(-fno-rtti)
            fi
            compile x86_64-w64-mingw32-g++ \
                "$SRC_DIR/${src_name}.cpp" \
                "$SAMPLE_DIR/windows/x64/${src_name}_x64_${opt}.exe" \
                "$opt" \
                "${local_flags[@]}"
        done
    done
else
    info "SKIP PE x86_64 — x86_64-w64-mingw32-g++ not found"
    info "Install: brew install mingw-w64"
fi
echo ""

echo "========================================"
echo "  Results: $SUCCESS / $TOTAL succeeded, $FAILED failed"
echo "========================================"

# ───── Summary ─────
echo ""
info "Binary inventory:"
echo "  Mach-O ARM64: $(ls "$SAMPLE_DIR/macos/arm64/" 2>/dev/null | wc -l | tr -d ' ') files"
echo "  Mach-O x64:   $(ls "$SAMPLE_DIR/macos/x64/" 2>/dev/null | wc -l | tr -d ' ') files"
echo "  ELF x64:      $(ls "$SAMPLE_DIR/linux/x64/" 2>/dev/null | wc -l | tr -d ' ') files"
echo "  PE x64:       $(ls "$SAMPLE_DIR/windows/x64/" 2>/dev/null | wc -l | tr -d ' ') files"
