#!/bin/bash
# Fission Development Tools Usage Guide
# Run: ./scripts/build/dev-tools.sh <command>

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

show_help() {
    cat << EOF
🔬 Fission Development Tools

Usage: $0 <command>

Commands:
  profile     - Profile the application with samply
  expand      - Expand macros in fission-ffi
  fuzz        - Run fuzz testing on parsers
  asan        - Build with AddressSanitizer (requires nightly)
  clang-tidy  - Run clang-tidy on C++ code
  sccache     - Show sccache statistics

Examples:
  $0 profile              # Profile the default binary
  $0 fuzz pe 60           # Fuzz PE parser for 60 seconds
  $0 asan                 # Build with ASAN enabled
EOF
}

cmd_profile() {
    echo "📊 Profiling Fission with samply..."
    echo "   Opening Firefox Profiler UI when complete."
    
    # Build with profiling symbols
    cargo build --profile profiling
    
    # Run with samply
    samply record ./target/profiling/fission "$@"
}

cmd_expand() {
    echo "🔍 Expanding macros in fission-ffi..."
    cargo expand -p fission-ffi "${@:-}" 2>&1 | head -500
}

cmd_fuzz() {
    local target="${1:-pe}"
    local duration="${2:-60}"
    
    echo "🐛 Fuzzing ${target} parser for ${duration} seconds..."
    cd "$PROJECT_ROOT/crates/fission-loader"
    
    # Install cargo-fuzz if not present
    if ! command -v cargo-fuzz &> /dev/null; then
        echo "Installing cargo-fuzz..."
        cargo install cargo-fuzz
    fi
    
    cargo +nightly fuzz run "fuzz_${target}_parser" -- -max_total_time="${duration}"
}

cmd_asan() {
    echo "🔍 Building with AddressSanitizer..."
    echo "   Note: Requires rustup +nightly"
    
    # Detect architecture
    ARCH=$(uname -m)
    if [ "$ARCH" = "arm64" ]; then
        TARGET="aarch64-apple-darwin"
    else
        TARGET="x86_64-apple-darwin"
    fi
    
    RUSTFLAGS="-Zsanitizer=address" cargo +nightly build --target "$TARGET" "$@"
    
    echo ""
    echo "✅ ASAN build complete. Binary location:"
    echo "   ./target/${TARGET}/debug/fission"
    echo ""
    echo "Run with: ./target/${TARGET}/debug/fission"
}

cmd_clang_tidy() {
    echo "🧹 Running clang-tidy on C++ code..."
    cd "$PROJECT_ROOT/ghidra_decompiler"
    
    # Find all .cpp files and run clang-tidy
    find src -name "*.cpp" -exec clang-tidy {} -- \
        -I include \
        -I /usr/local/include \
        -std=c++17 \;
}

cmd_sccache() {
    echo "📦 sccache Statistics:"
    sccache --show-stats
}

# Main dispatch
case "${1:-help}" in
    profile)
        shift
        cmd_profile "$@"
        ;;
    expand)
        shift
        cmd_expand "$@"
        ;;
    fuzz)
        shift
        cmd_fuzz "$@"
        ;;
    asan)
        shift
        cmd_asan "$@"
        ;;
    clang-tidy)
        cmd_clang_tidy
        ;;
    sccache)
        cmd_sccache
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        echo "Unknown command: $1"
        show_help
        exit 1
        ;;
esac
